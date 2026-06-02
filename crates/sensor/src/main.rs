mod buffer;
mod collector;
mod sender;

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
    interval: u64,

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

    info!(id = %sensor_id, core = %args.core_addr, "starting");

    let mut buffer = buffer::EventBuffer::new(sensor_id, 10_000);
    let collector = collector::NetworkCollector::new();
    let sender = sender::CoreSender::new(&args.core_addr);
    let interval = Duration::from_secs(args.interval);

    loop {
        let events = collector.collect_connections();
        let count = events.len();
        for event in events {
            buffer.push_network(event);
        }

        if let Some(batch) = buffer.take_batch()
            && let Err(e) = sender.send_batch(&batch).await
        {
            warn!(error = %e, events = count, "core unreachable, buffering");
            for event in batch.network_events {
                buffer.push_network(event);
            }
        }

        tokio::time::sleep(interval).await;
    }
}
