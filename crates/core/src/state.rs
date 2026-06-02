use anyhow::Result;
use std::collections::HashMap;
use thymos_common::{EventBatch, MachineIdentity, Mutation};
use thymos_detection::ImmuneEngine;

pub struct AppState {
    pub profiles: HashMap<String, MachineIdentity>,
    pub mutations: Vec<Mutation>,
    pub event_count: u64,
    pub engine: ImmuneEngine,
    pub phase: Phase,
    pub data_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Thymus,
    Active,
}

impl AppState {
    pub fn new(data_dir: &str) -> Result<Self> {
        Ok(Self {
            profiles: HashMap::new(),
            mutations: Vec::new(),
            event_count: 0,
            engine: ImmuneEngine::new(),
            phase: Phase::Thymus,
            data_dir: data_dir.to_string(),
        })
    }

    pub fn ingest_batch(&mut self, batch: EventBatch) {
        self.event_count += batch.event_count() as u64;

        for event in &batch.network_events {
            let machine_id = batch.sensor_id.clone();

            if !self.profiles.contains_key(&machine_id) {
                self.profiles.insert(
                    machine_id.clone(),
                    MachineIdentity::new(machine_id.clone(), machine_id.clone()),
                );
            }

            let profile = self.profiles.get(&machine_id).unwrap();

            if self.phase == Phase::Active {
                if let Some(mutation) = self.engine.analyze_network_event(event, profile) {
                    tracing::warn!(
                        machine = %mutation.machine_id,
                        score = mutation.risk_score,
                        "Mutation detected"
                    );
                    self.mutations.push(mutation);
                }
            }

            // Update profile with observed data
            self.update_profile(&machine_id, event);
        }
    }

    fn update_profile(&mut self, machine_id: &str, event: &thymos_common::NetworkEvent) {
        let profile = match self.profiles.get_mut(machine_id) {
            Some(p) => p,
            None => return,
        };

        if !profile.is_known_peer(&event.dest_ip) {
            profile.relational.known_peers.push(thymos_common::PeerProfile {
                peer_ip: event.dest_ip,
                peer_hostname: None,
                ports: vec![event.dest_port],
                protocols: vec![event.protocol],
                direction: thymos_common::ConnectionDirection::Outgoing,
                avg_daily_volume: event.bytes_sent + event.bytes_recv,
                avg_daily_connections: 1.0,
                first_seen: event.timestamp,
                last_seen: event.timestamp,
                confidence: 0.1,
            });
        } else {
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
        }

        profile.last_updated = event.timestamp;
    }

    pub fn activate(&mut self) {
        self.phase = Phase::Active;
        tracing::info!("Phase switched to ACTIVE — immune detection enabled");
    }

    pub fn active_mutations(&self) -> Vec<&Mutation> {
        self.mutations
            .iter()
            .filter(|m| m.status == thymos_common::MutationStatus::Active)
            .collect()
    }
}
