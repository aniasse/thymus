use serde::Serialize;
use thymus_common::{LateralChain, Mutation};

#[derive(Clone)]
pub struct WebhookConfig {
    pub url: String,
    pub min_score: f64,
    pub enabled: bool,
}

#[derive(Serialize)]
struct MutationAlert {
    alert_type: String,
    machine_id: String,
    risk_score: f64,
    dimensions: Vec<String>,
    description: String,
    detected_at: String,
}

#[derive(Serialize)]
struct ChainAlert {
    alert_type: String,
    path: String,
    chain_score: f64,
    links: usize,
    detected_at: String,
}

impl WebhookConfig {
    pub fn new(url: String, min_score: f64) -> Self {
        Self {
            url,
            min_score,
            enabled: true,
        }
    }

    pub fn send_mutation(&self, mutation: &Mutation) {
        if !self.enabled || mutation.risk_score < self.min_score {
            return;
        }

        let alert = MutationAlert {
            alert_type: "mutation".into(),
            machine_id: mutation.machine_id.clone(),
            risk_score: mutation.risk_score,
            dimensions: mutation
                .dimensions
                .iter()
                .map(|d| format!("{d:?}"))
                .collect(),
            description: mutation
                .details
                .first()
                .map_or(String::new(), |d| d.description.clone()),
            detected_at: mutation.detected_at.to_rfc3339(),
        };

        let url = self.url.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            if let Err(e) = client.post(&url).json(&alert).send().await {
                tracing::warn!(error = %e, "webhook failed");
            }
        });
    }

    pub fn send_chain(&self, chain: &LateralChain) {
        if !self.enabled || chain.chain_score < self.min_score {
            return;
        }

        let alert = ChainAlert {
            alert_type: "lateral_movement".into(),
            path: chain.path_str(),
            chain_score: chain.chain_score,
            links: chain.path.len(),
            detected_at: chain.detected_at.to_rfc3339(),
        };

        let url = self.url.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            if let Err(e) = client.post(&url).json(&alert).send().await {
                tracing::warn!(error = %e, "webhook failed");
            }
        });
    }
}
