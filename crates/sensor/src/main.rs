mod buffer;
mod collector;

use anyhow::Result;
use clap::Parser;
use std::time::Duration;
use tracing::info;

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
    let sensor_id = args.sensor_id.unwrap_or_else(|| {
        std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string()
    });

    info!(sensor_id = %sensor_id, core = %args.core_addr, "starting");

    let mut buffer = buffer::EventBuffer::new(sensor_id, 10_000);
    let collector = collector::NetworkCollector::new();
    let interval = Duration::from_secs(args.collect_interval_secs);

    loop {
        let events = collector.collect_connections();
        let count = events.len();
        for event in events {
            buffer.push_network(event);
        }
        if count > 0 {
            info!(count, "collected network events");
        }

        if let Some(batch) = buffer.take_batch() {
            info!(events = batch.event_count(), "batch ready");
            // TODO: POST to core_addr/api/events
        }

        tokio::time::sleep(interval).await;
    }
}
