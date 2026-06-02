use anyhow::Result;
use thymos_common::EventBatch;
use tracing::{info, warn};

pub struct CoreSender {
    client: reqwest::Client,
    base_url: String,
}

impl CoreSender {
    pub fn new(core_addr: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: core_addr.trim_end_matches('/').to_string(),
        }
    }

    pub async fn send_batch(&self, batch: &EventBatch) -> Result<()> {
        let url = format!("{}/api/events", self.base_url);
        let resp = self.client.post(&url).json(batch).send().await?;

        if resp.status().is_success() {
            info!(events = batch.event_count(), "batch sent to core");
        } else {
            warn!(status = %resp.status(), "core rejected batch");
        }

        Ok(())
    }
}
