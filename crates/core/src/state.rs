use std::collections::HashMap;
use thymos_common::{
    ConnectionDirection, EventBatch, LateralChain, MachineIdentity, Mutation, MutationDimension,
    MutationStatus, NetworkEvent, PeerProfile, ToleranceContext, ToleranceEntry,
};
use thymos_detection::ImmuneEngine;
use thymos_detection::innate::PortScanDetector;
use thymos_detection::lateral::LateralDetector;
use thymos_detection::memory::ImmuneMemory;

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
    pub lateral_detector: LateralDetector,
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
            lateral_detector: LateralDetector::new(),
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

        for event in &batch.network_events {
            let machine_id = &batch.sensor_id;

            if !self.profiles.contains_key(machine_id) {
                self.profiles.insert(
                    machine_id.clone(),
                    MachineIdentity::new(machine_id.clone(), machine_id.clone()),
                );
            }

            if self.phase == Phase::Active {
                // Check port scan
                let is_scan =
                    self.scan_detector
                        .record(machine_id, event.dest_port, event.timestamp);

                if is_scan {
                    let mut mutation = Mutation::new(machine_id.clone());
                    mutation.risk_score = 0.9;
                    mutation.innate_score = 0.9;
                    mutation.dimensions = vec![MutationDimension::Relational];
                    mutation.details.push(thymos_common::MutationDetail {
                        dimension: MutationDimension::Relational,
                        description: format!("{machine_id} scanne plus de 10 ports en 60s"),
                        expected_value: "< 10 ports distincts".into(),
                        observed_value: "scan de ports détecté".into(),
                        deviation_sigma: 5.0,
                    });
                    tracing::warn!(machine = %machine_id, "port scan detected");
                    self.mutations.push(mutation);
                }

                // Normal immune detection
                let profile = &self.profiles[machine_id];
                if let Some(mut mutation) = self.engine.analyze_network_event(event, profile) {
                    // Check tolerance (immune tolerance)
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
                        continue;
                    }

                    // Check active contexts
                    let in_context = self
                        .contexts
                        .iter()
                        .any(|ctx| ctx.is_active() && ctx.affects_machine(machine_id));
                    if in_context {
                        mutation.risk_score *= 0.5;
                        if mutation.risk_score < 0.4 {
                            continue;
                        }
                    }

                    // Consult immune memory for accelerated response
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

                    // Feed to lateral detector
                    let dest_ips = vec![event.dest_ip.to_string()];
                    if let Some(chain) = self.lateral_detector.record_mutation(&mutation, dest_ips)
                    {
                        self.chains.push(chain);
                    }

                    self.mutations.push(mutation);
                }
            }

            self.update_profile(machine_id, event);
        }

        if self.phase == Phase::Thymus {
            self.check_auto_activate();
        }
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
        thymos_detection::clonal::ClonalSelection::optimize(&mut cells);
        self.memory.replace_cells(cells);
    }
}
