mod collector;
mod buffer;

use anyhow::Result;
use clap::Parser;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "thymos-sensor", about = "Thymos network immune sensor")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:9443")]
    core_addr: String,

    #[arg(long, default_value = "10")]
    collect_interval_secs: u64,

    #[arg(long)]
    sensor_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("thymos_sensor=info")
        .init();

    let args = Args::parse();
    let sensor_id = args
        .sensor_id
        .unwrap_or_else(|| gethostname().unwrap_or_else(|| "unknown".into()));

    info!(sensor_id = %sensor_id, core = %args.core_addr, "Thymos Sensor starting");

    let mut buffer = buffer::EventBuffer::new(sensor_id.clone(), 10_000);
    let collector = collector::NetworkCollector::new()?;
    let interval = Duration::from_secs(args.collect_interval_secs);

    info!("Entering collection loop (interval: {}s)", args.collect_interval_secs);

    loop {
        match collector.collect_connections() {
            Ok(events) => {
                let count = events.len();
                for event in events {
                    buffer.push_network(event);
                }
                if count > 0 {
                    info!(count, "Collected network events");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to collect network events");
            }
        }

        if let Some(batch) = buffer.take_batch() {
            let event_count = batch.event_count();
            info!(events = event_count, "Batch ready to send to core");
            // TODO: send via gRPC to core
            // For now, log the batch as JSON
            if let Ok(json) = serde_json::to_string_pretty(&batch) {
                tracing::debug!(batch = %json);
            }
        }

        tokio::time::sleep(interval).await;
    }
}

fn gethostname() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
}
