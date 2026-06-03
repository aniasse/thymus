mod buffer;
mod collector;
mod sender;

// Passive capture (pnet) links a packet-capture lib unavailable by default on
// Windows; gate it (and its deps) to non-Windows platforms for now.
#[cfg(not(windows))]
mod capture;
#[cfg(not(windows))]
mod flows;

// Host-mode process enrichment reads /proc; Linux only.
#[cfg(target_os = "linux")]
mod procinfo;

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

    #[arg(long)]
    token: Option<String>,

    /// Passive mode: capture flows on this network interface (e.g. a SPAN/mirror
    /// port) instead of reading /proc/net. Sees agentless devices too.
    #[arg(long)]
    interface: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("thymos_sensor=info")
        .init();

    let args = Args::parse();
    let sensor_id = args.sensor_id.clone().unwrap_or_else(|| {
        std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string()
    });

    let sender = sender::CoreSender::new(&args.core_addr, args.token.clone());

    if let Some(iface) = args.interface.clone() {
        #[cfg(not(windows))]
        {
            info!(id = %sensor_id, core = %args.core_addr, interface = %iface, "starting (passive mode)");
            run_passive(iface, sensor_id, sender, args.interval).await
        }
        // On Windows we return an error value (not `bail!`) so this branch does
        // not diverge — keeping the `else` meaningful for the clippy lints that
        // run per-target.
        #[cfg(windows)]
        {
            let _ = iface;
            Err(anyhow::anyhow!(
                "passive mode (--interface) is not yet supported on Windows; use host mode"
            ))
        }
    } else {
        info!(id = %sensor_id, core = %args.core_addr, "starting (host mode)");
        run_host(sensor_id, sender, args.interval).await
    }
}

async fn run_host(sensor_id: String, sender: sender::CoreSender, interval: u64) -> Result<()> {
    let mut buffer = buffer::EventBuffer::new(sensor_id, 10_000);
    let collector = collector::NetworkCollector::new();
    let interval = Duration::from_secs(interval);

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

#[cfg(not(windows))]
async fn run_passive(
    iface: String,
    sensor_id: String,
    sender: sender::CoreSender,
    interval: u64,
) -> Result<()> {
    use std::sync::{Arc, Mutex};

    let aggregator = Arc::new(Mutex::new(flows::FlowAggregator::new(
        sensor_id.clone(),
        15,
        50_000,
    )));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<flows::FlowPacket>();

    // Capture runs on a blocking thread; the non-Send receiver never leaves it.
    tokio::task::spawn_blocking(move || {
        let mut receiver = match capture::open(&iface) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(error = %e, "failed to open capture interface (need root/CAP_NET_RAW?)");
                return;
            }
        };
        info!(interface = %iface, "capture started");
        loop {
            match receiver.next() {
                Ok(frame) => {
                    if let Some(fp) = capture::parse_frame(frame)
                        && tx.send(fp).is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    warn!(error = %e, "capture read error");
                }
            }
        }
    });

    // Recorder: fold packets into the flow aggregator.
    let rec_agg = aggregator.clone();
    tokio::spawn(async move {
        while let Some(fp) = rx.recv().await {
            rec_agg.lock().unwrap().record(&fp);
        }
    });

    // Emitter: periodically drain idle flows and send a passive batch.
    let mut tick = tokio::time::interval(Duration::from_secs(interval));
    loop {
        let events = tokio::select! {
            _ = tick.tick() => {
                let mut agg = aggregator.lock().unwrap();
                let active = agg.active_flows();
                let drained = agg.drain_idle();
                if !drained.is_empty() {
                    info!(flows = drained.len(), active, "draining idle flows");
                }
                drained
            }
            _ = tokio::signal::ctrl_c() => {
                info!("shutting down, flushing remaining flows");
                let flushed = aggregator.lock().unwrap().flush();
                send_passive(&sender, &sensor_id, flushed).await;
                return Ok(());
            }
        };

        send_passive(&sender, &sensor_id, events).await;
    }
}

#[cfg(not(windows))]
async fn send_passive(
    sender: &sender::CoreSender,
    sensor_id: &str,
    events: Vec<thymos_common::NetworkEvent>,
) {
    use thymos_common::EventBatch;

    if events.is_empty() {
        return;
    }
    let count = events.len();
    let mut batch = EventBatch::new_passive(sensor_id.to_string());
    batch.network_events = events;

    if let Err(e) = sender.send_batch(&batch).await {
        warn!(error = %e, flows = count, "core unreachable, dropping flows");
    } else {
        info!(flows = count, "flows sent");
    }
}
