use chrono::{DateTime, Duration, Utc};
use thymus_common::{ChainLink, LateralChain, Mutation};
use uuid::Uuid;

const CORRELATION_WINDOW_MINUTES: i64 = 30;

pub struct LateralDetector {
    recent_mutations: Vec<RecentMutation>,
    pub chains: Vec<LateralChain>,
}

#[derive(Clone)]
struct RecentMutation {
    machine_id: String,
    mutation_id: Uuid,
    timestamp: DateTime<Utc>,
    dest_ips: Vec<String>,
    risk_score: f64,
}

impl LateralDetector {
    pub fn new() -> Self {
        Self {
            recent_mutations: Vec::new(),
            chains: Vec::new(),
        }
    }

    pub fn record_mutation(
        &mut self,
        mutation: &Mutation,
        dest_ips: Vec<String>,
    ) -> Option<LateralChain> {
        let cutoff = Utc::now() - Duration::minutes(CORRELATION_WINDOW_MINUTES);
        self.recent_mutations.retain(|m| m.timestamp > cutoff);

        // Collect predecessor data before mutating
        let predecessors: Vec<RecentMutation> = self
            .recent_mutations
            .iter()
            .filter(|m| {
                m.machine_id != mutation.machine_id
                    && m.dest_ips
                        .iter()
                        .any(|ip| is_same_machine(ip, &mutation.machine_id))
            })
            .cloned()
            .collect();

        let first_dest = dest_ips.first().cloned().unwrap_or_default();

        self.recent_mutations.push(RecentMutation {
            machine_id: mutation.machine_id.clone(),
            mutation_id: mutation.id,
            timestamp: mutation.detected_at,
            dest_ips,
            risk_score: mutation.risk_score,
        });

        if predecessors.is_empty() {
            return None;
        }

        let new_link = ChainLink {
            machine_id: mutation.machine_id.clone(),
            mutation_id: mutation.id,
            timestamp: mutation.detected_at,
            dest_ip: first_dest,
            risk_score: mutation.risk_score,
        };

        // Try to extend existing chain
        for chain in &mut self.chains {
            if chain.status != thymus_common::MutationStatus::Active {
                continue;
            }

            let last = chain.last_machine().unwrap_or_default().to_string();
            let is_continuation = predecessors.iter().any(|p| p.machine_id == last);

            if is_continuation {
                chain.add_link(new_link);
                tracing::warn!(
                    chain_id = %chain.id,
                    path = %chain.path_str(),
                    score = chain.chain_score,
                    "lateral chain extended"
                );
                return Some(chain.clone());
            }
        }

        // Create new chain from predecessor + current
        if let Some(pred) = predecessors.first() {
            let first_link = ChainLink {
                machine_id: pred.machine_id.clone(),
                mutation_id: pred.mutation_id,
                timestamp: pred.timestamp,
                dest_ip: pred.dest_ips.first().cloned().unwrap_or_default(),
                risk_score: pred.risk_score,
            };

            let mut chain = LateralChain::new(first_link);
            chain.add_link(new_link);

            tracing::warn!(
                chain_id = %chain.id,
                path = %chain.path_str(),
                score = chain.chain_score,
                "lateral movement detected"
            );

            let result = chain.clone();
            self.chains.push(chain);
            return Some(result);
        }

        None
    }

    pub fn active_chains(&self) -> Vec<&LateralChain> {
        self.chains
            .iter()
            .filter(|c| c.status == thymus_common::MutationStatus::Active)
            .collect()
    }
}

impl Default for LateralDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn is_same_machine(ip: &str, machine_id: &str) -> bool {
    machine_id.contains(ip) || ip.contains(machine_id)
}
