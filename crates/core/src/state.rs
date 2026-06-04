use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use thymus_common::{
    CollectionMode, ConnectionDirection, Discovery, EventBatch, LateralChain, MachineIdentity,
    Mutation, MutationDimension, MutationStatus, NetworkEvent, PeerProfile, ToleranceContext,
    ToleranceEntry,
};
use thymus_detection::ImmuneEngine;
use thymus_detection::beacon::BeaconDetector;
use thymus_detection::innate::PortScanDetector;
use thymus_detection::lateral::LateralDetector;
use thymus_detection::memory::ImmuneMemory;

use crate::alerting::WebhookConfig;
use crate::profiler;

pub struct AppState {
    pub profiles: HashMap<String, MachineIdentity>,
    pub mutations: Vec<Mutation>,
    pub chains: Vec<LateralChain>,
    pub tolerances: Vec<ToleranceEntry>,
    pub contexts: Vec<ToleranceContext>,
    pub event_count: u64,
    pub engine: ImmuneEngine,
    pub memory: ImmuneMemory,
    pub phase: Phase,
    pub scan_detector: PortScanDetector,
    pub beacon_detector: BeaconDetector,
    pub lateral_detector: LateralDetector,
    pub webhook: Option<WebhookConfig>,
    /// IP-keyed devices whose reverse-DNS lookup failed; skipped on later passes.
    resolution_failed: HashSet<String>,
    batches_since_save: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Thymus,
    Active,
}

impl AppState {
    pub fn load_from_db(db: &crate::db::Db) -> Self {
        let profiles = db.load_profiles().unwrap_or_default();
        let mutations = db.load_mutations().unwrap_or_default();
        let event_count = db.load_event_count().unwrap_or(0);
        let phase = match db.load_phase().ok().flatten().as_deref() {
            Some("active") => Phase::Active,
            _ => Phase::Thymus,
        };

        let machine_count = profiles.len();
        let mutation_count = mutations.len();

        let memory_cells = db.load_memory_cells().unwrap_or_default();
        let cell_count = memory_cells.len();

        let tolerances = db.load_tolerances().unwrap_or_default();

        let state = Self {
            profiles,
            mutations,
            chains: Vec::new(),
            tolerances,
            contexts: Vec::new(),
            event_count,
            engine: ImmuneEngine::new(),
            memory: ImmuneMemory::load(memory_cells),
            phase,
            scan_detector: PortScanDetector::default(),
            beacon_detector: BeaconDetector::new(),
            lateral_detector: LateralDetector::new(),
            webhook: None,
            resolution_failed: HashSet::new(),
            batches_since_save: 0,
        };

        if machine_count > 0 {
            tracing::info!(
                machines = machine_count,
                mutations = mutation_count,
                memory_cells = cell_count,
                events = event_count,
                phase = ?phase,
                "restored state from db"
            );
        }

        state
    }

    pub fn save_to_db(&mut self, db: &crate::db::Db) {
        if let Err(e) = db.save_profiles(&self.profiles) {
            tracing::error!(error = %e, "failed to save profiles");
        }
        if let Err(e) = db.save_mutations(&self.mutations) {
            tracing::error!(error = %e, "failed to save mutations");
        }
        if let Err(e) = db.save_event_count(self.event_count) {
            tracing::error!(error = %e, "failed to save event count");
        }
        let phase_str = match self.phase {
            Phase::Thymus => "thymus",
            Phase::Active => "active",
        };
        if let Err(e) = db.save_phase(phase_str) {
            tracing::error!(error = %e, "failed to save phase");
        }
        if let Err(e) = db.save_memory_cells(self.memory.cells()) {
            tracing::error!(error = %e, "failed to save memory cells");
        }
        if let Err(e) = db.save_tolerances(&self.tolerances) {
            tracing::error!(error = %e, "failed to save tolerances");
        }
        self.batches_since_save = 0;
    }

    pub fn should_save(&self) -> bool {
        self.batches_since_save >= 3
    }

    pub fn ingest_batch(&mut self, batch: &EventBatch) {
        self.event_count += batch.event_count() as u64;
        self.batches_since_save += 1;

        match batch.mode {
            CollectionMode::Host => {
                for event in &batch.network_events {
                    let machine_id = batch.sensor_id.clone();
                    self.ensure_profile(&machine_id, Discovery::Agent);
                    if self.phase == Phase::Active {
                        self.detect_on_machine(&machine_id, event);
                    }
                    self.update_profile(&machine_id, event);
                }
            }
            CollectionMode::Passive => {
                for event in &batch.network_events {
                    self.ingest_passive_event(event);
                }
            }
        }

        if self.phase == Phase::Thymus {
            self.check_auto_activate();
        }
    }

    fn ensure_profile(&mut self, machine_id: &str, discovery: Discovery) {
        if !self.profiles.contains_key(machine_id) {
            let mut identity = MachineIdentity::new(machine_id.to_string(), machine_id.to_string());
            identity.discovery = discovery;
            self.profiles.insert(machine_id.to_string(), identity);
        }
    }

    /// Passive mode: a single sensor observes many devices. We attribute each
    /// flow to its endpoints by IP and only profile devices on the local network
    /// (RFC1918). The client (initiator) is profiled and analysed; the server is
    /// profiled too so it appears in the relational graph.
    fn ingest_passive_event(&mut self, event: &NetworkEvent) {
        let client_local = is_private(&event.source_ip);
        let server_local = is_private(&event.dest_ip);

        if client_local {
            let client_id = event.source_ip.to_string();
            self.ensure_profile(&client_id, Discovery::Passive);
            if self.phase == Phase::Active {
                self.detect_on_machine(&client_id, event);
            }
            self.update_profile(&client_id, event);
        }

        if server_local {
            let server_id = event.dest_ip.to_string();
            self.ensure_profile(&server_id, Discovery::Passive);
            self.update_profile_incoming(&server_id, event);
        }
    }

    fn push_mutation(&mut self, mutation: Mutation) {
        if let Some(ref wh) = self.webhook {
            wh.send_mutation(&mutation);
        }
        self.mutations.push(mutation);
    }

    /// Stateful innate detectors that don't depend on the machine profile.
    fn detect_stateful(&mut self, machine_id: &str, event: &NetworkEvent) {
        // Port scan
        if self
            .scan_detector
            .record(machine_id, event.dest_port, event.timestamp)
        {
            let mut mutation = Mutation::new(machine_id.to_string());
            mutation.risk_score = 0.9;
            mutation.innate_score = 0.9;
            mutation.dimensions = vec![MutationDimension::Relational];
            mutation.details.push(thymus_common::MutationDetail {
                dimension: MutationDimension::Relational,
                description: format!("{machine_id} scanne plus de 10 ports en 60s"),
                expected_value: "< 10 ports distincts".into(),
                observed_value: "scan de ports détecté".into(),
                deviation_sigma: 5.0,
            });
            tracing::warn!(machine = %machine_id, "port scan detected");
            self.push_mutation(mutation);
        }

        // C2 beaconing (periodic callback to the same destination)
        if let Some(hit) =
            self.beacon_detector
                .record(machine_id, event.dest_ip, event.dest_port, event.timestamp)
        {
            let mut mutation = Mutation::new(machine_id.to_string());
            mutation.risk_score = 0.7;
            mutation.innate_score = 0.7;
            mutation.dimensions = vec![MutationDimension::Temporal];
            mutation.details.push(thymus_common::MutationDetail {
                dimension: MutationDimension::Temporal,
                description: format!(
                    "Connexions régulières vers {}:{} toutes les {:.0}s (balise C2 probable)",
                    hit.dest_ip, hit.dest_port, hit.interval_secs
                ),
                expected_value: "trafic irrégulier".into(),
                observed_value: format!(
                    "{} connexions, jitter {:.0}%",
                    hit.samples,
                    hit.regularity * 100.0
                ),
                deviation_sigma: 4.0,
            });
            tracing::warn!(
                machine = %machine_id,
                dest = %hit.dest_ip,
                interval = hit.interval_secs,
                "beaconing detected"
            );
            self.push_mutation(mutation);
        }
    }

    /// The full immune detection pipeline for one (machine, event) pair.
    fn detect_on_machine(&mut self, machine_id: &str, event: &NetworkEvent) {
        self.detect_stateful(machine_id, event);

        // Dual-layer immune detection
        let profile = &self.profiles[machine_id];
        let Some(mut mutation) = self.engine.analyze_network_event(event, profile) else {
            return;
        };

        // Tolerance
        let dest_ip_str = event.dest_ip.to_string();
        let is_tolerated = self.tolerances.iter_mut().any(|t| {
            let m = t.matches(
                machine_id,
                &mutation.dimensions,
                mutation.risk_score,
                Some(&dest_ip_str),
            );
            if m {
                t.hits += 1;
            }
            m
        });
        if is_tolerated {
            return;
        }

        // Active contexts
        let in_context = self
            .contexts
            .iter()
            .any(|ctx| ctx.is_active() && ctx.affects_machine(machine_id));
        if in_context {
            mutation.risk_score *= 0.5;
            if mutation.risk_score < 0.4 {
                return;
            }
        }

        // Immune memory
        if let Some(mem_match) = self.memory.consult(&mutation) {
            tracing::info!(
                cell = %mem_match.cell_id,
                similarity = mem_match.similarity,
                "memory match — accelerated response"
            );
            mutation.response = mem_match.suggested_response;
        }

        tracing::warn!(
            machine = %mutation.machine_id,
            score = mutation.risk_score,
            "mutation detected"
        );
        if let Some(ref wh) = self.webhook {
            wh.send_mutation(&mutation);
        }

        // Lateral movement
        let dest_ips = vec![event.dest_ip.to_string()];
        if let Some(chain) = self.lateral_detector.record_mutation(&mutation, dest_ips) {
            if let Some(ref wh) = self.webhook {
                wh.send_chain(&chain);
            }
            self.chains.push(chain);
        }

        self.mutations.push(mutation);
    }

    fn update_profile(&mut self, machine_id: &str, event: &NetworkEvent) {
        let Some(profile) = self.profiles.get_mut(machine_id) else {
            return;
        };

        if profile.is_known_peer(&event.dest_ip) {
            if let Some(peer) = profile
                .relational
                .known_peers
                .iter_mut()
                .find(|p| p.peer_ip == event.dest_ip)
            {
                peer.last_seen = event.timestamp;
                peer.avg_daily_connections += 1.0;
                if !peer.ports.contains(&event.dest_port) {
                    peer.ports.push(event.dest_port);
                }
                peer.confidence = (peer.confidence + 0.01).min(1.0);
            }
        } else {
            profile.relational.known_peers.push(PeerProfile {
                peer_ip: event.dest_ip,
                peer_hostname: None,
                ports: vec![event.dest_port],
                protocols: vec![event.protocol],
                direction: ConnectionDirection::Outgoing,
                avg_daily_volume: event.bytes_sent + event.bytes_recv,
                avg_daily_connections: 1.0,
                first_seen: event.timestamp,
                last_seen: event.timestamp,
                confidence: 0.1,
            });
        }

        profiler::update_temporal_stats(profile, event);
        profiler::update_observation_days(profile);
        profiler::update_active_hours(profile);
        profiler::compute_maturity(profile);

        profile.last_updated = event.timestamp;
    }

    /// Server-side profiling for passive flows: the peer is the *source* (client),
    /// observed on the server's listening port.
    fn update_profile_incoming(&mut self, machine_id: &str, event: &NetworkEvent) {
        let Some(profile) = self.profiles.get_mut(machine_id) else {
            return;
        };

        if let Some(peer) = profile
            .relational
            .known_peers
            .iter_mut()
            .find(|p| p.peer_ip == event.source_ip)
        {
            peer.last_seen = event.timestamp;
            peer.avg_daily_connections += 1.0;
            if !peer.ports.contains(&event.dest_port) {
                peer.ports.push(event.dest_port);
            }
            peer.confidence = (peer.confidence + 0.01).min(1.0);
        } else {
            profile.relational.known_peers.push(PeerProfile {
                peer_ip: event.source_ip,
                peer_hostname: None,
                ports: vec![event.dest_port],
                protocols: vec![event.protocol],
                direction: ConnectionDirection::Incoming,
                avg_daily_volume: event.bytes_sent + event.bytes_recv,
                avg_daily_connections: 1.0,
                first_seen: event.timestamp,
                last_seen: event.timestamp,
                confidence: 0.1,
            });
        }

        profiler::update_temporal_stats(profile, event);
        profiler::update_observation_days(profile);
        profiler::compute_maturity(profile);
        profile.last_updated = event.timestamp;
    }

    fn check_auto_activate(&mut self) {
        if profiler::should_auto_activate(&self.profiles) {
            tracing::info!("all profiles mature — auto-activating immune detection");
            self.phase = Phase::Active;
        }
    }

    pub fn activate(&mut self) {
        self.phase = Phase::Active;
        tracing::info!("switched to ACTIVE mode");
    }

    pub fn active_mutations(&self) -> Vec<&Mutation> {
        self.mutations
            .iter()
            .filter(|m| m.status == MutationStatus::Active)
            .collect()
    }

    pub fn active_chains(&self) -> Vec<&LateralChain> {
        self.chains
            .iter()
            .filter(|c| c.status == MutationStatus::Active)
            .collect()
    }

    pub fn run_clonal_selection(&mut self) {
        let mut cells = self.memory.take_cells();
        thymus_detection::clonal::ClonalSelection::optimize(&mut cells);
        self.memory.replace_cells(cells);
    }

    /// IP-keyed devices (passive) still labelled by their raw IP and not yet
    /// marked as resolution-failed. Host devices (`machine_id` is a hostname, not
    /// an IP) are naturally excluded.
    pub fn unresolved_ips(&self, limit: usize) -> Vec<String> {
        self.profiles
            .values()
            .filter(|p| {
                p.hostname == p.machine_id
                    && p.machine_id.parse::<IpAddr>().is_ok()
                    && !self.resolution_failed.contains(&p.machine_id)
            })
            .take(limit)
            .map(|p| p.machine_id.clone())
            .collect()
    }

    /// Apply reverse-DNS results: set the hostname when resolved, otherwise record
    /// the failure so we don't keep retrying a device with no PTR record.
    pub fn apply_resolution(&mut self, results: Vec<(String, Option<String>)>) {
        for (ip, name) in results {
            match name {
                Some(hostname) => {
                    if let Some(profile) = self.profiles.get_mut(&ip) {
                        tracing::info!(ip = %ip, hostname = %hostname, "device hostname resolved");
                        profile.hostname = hostname;
                    }
                }
                None => {
                    self.resolution_failed.insert(ip);
                }
            }
        }
    }
}

/// Whether an address belongs to a local network we should profile as a device.
/// Uses RFC1918 private ranges for IPv4 (covers most LANs). IPv6 unique-local
/// detection is not yet stable in std, so v6 is treated as non-local for now.
fn is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private(),
        IpAddr::V6(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use thymus_common::Protocol;
    use uuid::Uuid;

    fn temp_state() -> AppState {
        let dir = std::env::temp_dir().join(format!("thymus-test-{}", Uuid::new_v4()));
        let db = crate::db::Db::open(&dir).unwrap();
        AppState::load_from_db(&db)
    }

    fn passive_event(sip: &str, dip: &str, dport: u16, sent: u64, recv: u64) -> NetworkEvent {
        NetworkEvent {
            id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            sensor_id: "span".into(),
            source_ip: sip.parse().unwrap(),
            source_port: 0,
            dest_ip: dip.parse().unwrap(),
            dest_port: dport,
            protocol: Protocol::Tcp,
            bytes_sent: sent,
            bytes_recv: recv,
            process_pid: 0,
            process_name: String::new(),
            process_user: String::new(),
        }
    }

    #[test]
    fn is_private_rfc1918() {
        assert!(is_private(&"192.168.1.1".parse().unwrap()));
        assert!(is_private(&"10.0.0.1".parse().unwrap()));
        assert!(is_private(&"172.16.5.5".parse().unwrap()));
        assert!(!is_private(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private(&"1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn passive_profiles_both_local_endpoints() {
        let mut s = temp_state();
        let mut batch = EventBatch::new_passive("span".into());
        batch.network_events.push(passive_event(
            "192.168.1.50",
            "192.168.1.10",
            443,
            1500,
            4000,
        ));
        s.ingest_batch(&batch);

        // Both endpoints profiled, keyed by IP
        assert!(s.profiles.contains_key("192.168.1.50"));
        assert!(s.profiles.contains_key("192.168.1.10"));

        // Client sees server as an outgoing peer on :443
        let client = &s.profiles["192.168.1.50"];
        assert_eq!(client.relational.known_peers.len(), 1);
        let cpeer = &client.relational.known_peers[0];
        assert_eq!(cpeer.peer_ip, Ipv4Addr::new(192, 168, 1, 10));
        assert_eq!(cpeer.direction, ConnectionDirection::Outgoing);
        assert!(cpeer.ports.contains(&443));

        // Server sees client as an incoming peer on its listening port :443
        let server = &s.profiles["192.168.1.10"];
        let speer = &server.relational.known_peers[0];
        assert_eq!(speer.peer_ip, Ipv4Addr::new(192, 168, 1, 50));
        assert_eq!(speer.direction, ConnectionDirection::Incoming);
        assert!(speer.ports.contains(&443));
    }

    #[test]
    fn passive_skips_external_endpoints() {
        let mut s = temp_state();
        let mut batch = EventBatch::new_passive("span".into());
        // Local device → public internet: only the local device is profiled.
        batch
            .network_events
            .push(passive_event("192.168.1.50", "8.8.8.8", 443, 100, 100));
        s.ingest_batch(&batch);

        assert!(s.profiles.contains_key("192.168.1.50"));
        assert!(!s.profiles.contains_key("8.8.8.8"));
    }

    #[test]
    fn host_mode_unchanged() {
        let mut s = temp_state();
        let mut batch = EventBatch::new("host-a".into());
        batch
            .network_events
            .push(passive_event("192.168.1.50", "192.168.1.10", 443, 100, 100));
        s.ingest_batch(&batch);

        // Host mode keys the profile by sensor_id, not by IP
        assert!(s.profiles.contains_key("host-a"));
        assert!(!s.profiles.contains_key("192.168.1.50"));
    }

    #[test]
    fn unresolved_ips_targets_only_passive_devices() {
        let mut s = temp_state();
        // A passive (IP-keyed) device and a host device in one go.
        let mut passive = EventBatch::new_passive("span".into());
        passive
            .network_events
            .push(passive_event("192.168.1.50", "192.168.1.10", 443, 100, 100));
        s.ingest_batch(&passive);

        let mut host = EventBatch::new("host-a".into());
        host.network_events
            .push(passive_event("192.168.1.50", "192.168.1.10", 443, 100, 100));
        s.ingest_batch(&host);

        let unresolved = s.unresolved_ips(50);
        // Only IP-keyed devices are candidates; "host-a" (a hostname) is excluded.
        assert!(unresolved.contains(&"192.168.1.50".to_string()));
        assert!(unresolved.contains(&"192.168.1.10".to_string()));
        assert!(!unresolved.contains(&"host-a".to_string()));
    }

    #[test]
    fn apply_resolution_updates_hostname_and_records_failures() {
        let mut s = temp_state();
        let mut batch = EventBatch::new_passive("span".into());
        batch.network_events.push(passive_event(
            "192.168.1.77",
            "192.168.1.10",
            9100,
            100,
            100,
        ));
        s.ingest_batch(&batch);

        // .77 resolves to a printer name, .10 has no PTR record
        s.apply_resolution(vec![
            ("192.168.1.77".into(), Some("imprimante-rh.local".into())),
            ("192.168.1.10".into(), None),
        ]);

        assert_eq!(s.profiles["192.168.1.77"].hostname, "imprimante-rh.local");
        // .10 keeps its IP as hostname and is no longer offered for resolution
        assert_eq!(s.profiles["192.168.1.10"].hostname, "192.168.1.10");

        let unresolved = s.unresolved_ips(50);
        assert!(!unresolved.contains(&"192.168.1.10".to_string()));
        // .77 is resolved (hostname != machine_id) so also no longer a candidate
        assert!(!unresolved.contains(&"192.168.1.77".to_string()));
    }

    #[test]
    fn beaconing_to_external_dest_flags_temporal_mutation() {
        use chrono::Duration;
        let mut s = temp_state();
        s.activate(); // enable detection

        // A local device beacons to an external C2 every 60s, 8 times.
        let t0 = chrono::Utc::now();
        for i in 0..8 {
            let mut ev = passive_event("192.168.1.50", "203.0.113.9", 443, 200, 200);
            ev.timestamp = t0 + Duration::seconds(60 * i);
            let mut batch = EventBatch::new_passive("span".into());
            batch.network_events.push(ev);
            s.ingest_batch(&batch);
        }

        let beacon = s
            .active_mutations()
            .into_iter()
            .find(|m| m.dimensions.contains(&MutationDimension::Temporal));
        let beacon = beacon.expect("beaconing should raise a temporal mutation");
        assert!(beacon.details[0].description.contains("balise C2"));
    }
}
