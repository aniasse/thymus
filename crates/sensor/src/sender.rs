use anyhow::Result;
use thymos_common::EventBatch;
use tracing::{info, warn};

pub struct CoreSender {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl CoreSender {
    pub fn new(core_addr: &str, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: core_addr.trim_end_matches('/').to_string(),
            token,
        }
    }

    pub async fn send_batch(&self, batch: &EventBatch) -> Result<()> {
        let url = format!("{}/api/events", self.base_url);
        let mut req = self.client.post(&url).json(batch);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?;

        if resp.status().is_success() {
            info!(events = batch.event_count(), "batch sent to core");
        } else {
            warn!(status = %resp.status(), "core rejected batch");
        }

        Ok(())
    }
}
