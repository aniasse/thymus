//! Windows host-mode collector using ETW (Event Tracing for Windows).
//!
//! Subscribes to the kernel TCP/IP provider and turns connection events into
//! `NetworkEvent`s. ETW is push-based, so a background trace fills a shared
//! buffer that `collect_connections()` drains on each poll — preserving the same
//! pull API as the Linux collector.
//!
//! Note: requires Administrator privileges to start a kernel trace.

use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use thymos_common::{NetworkEvent, Protocol};
use uuid::Uuid;

use ferrisetw::EventRecord;
use ferrisetw::parser::Parser;
use ferrisetw::provider::{Provider, kernel_providers};
use ferrisetw::schema_locator::SchemaLocator;
use ferrisetw::trace::KernelTrace;

// TcpIp MOF event opcodes (IPv4 / IPv6) from the kernel TCP/IP provider.
const OP_SEND_V4: u8 = 10;
const OP_RECV_V4: u8 = 11;
const OP_CONNECT_V4: u8 = 12;
const OP_SEND_V6: u8 = 26;
const OP_RECV_V6: u8 = 27;
const OP_CONNECT_V6: u8 = 28;

pub struct NetworkCollector {
    buffer: Arc<Mutex<Vec<NetworkEvent>>>,
    // Kept alive for the lifetime of the collector; dropping it stops the trace.
    _trace: Option<KernelTrace>,
}

impl NetworkCollector {
    pub fn new() -> Self {
        let buffer: Arc<Mutex<Vec<NetworkEvent>>> = Arc::new(Mutex::new(Vec::new()));

        let cb_buffer = buffer.clone();
        let cb_sensor = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into());
        let provider = Provider::kernel(&kernel_providers::TCP_IP_PROVIDER)
            .add_callback(move |record: &EventRecord, locator: &SchemaLocator| {
                handle_event(record, locator, &cb_sensor, &cb_buffer);
            })
            .build();

        let trace = KernelTrace::new()
            .named("Thymos-Kernel-Network".to_string())
            .enable(provider)
            .start_and_process();

        let trace = match trace {
            Ok(t) => {
                tracing::info!("ETW kernel network trace started");
                Some(t)
            }
            Err(e) => {
                tracing::error!(error = ?e, "failed to start ETW trace (run as Administrator?)");
                None
            }
        };

        Self {
            buffer,
            _trace: trace,
        }
    }

    pub fn collect_connections(&self) -> Vec<NetworkEvent> {
        let mut buf = self.buffer.lock().unwrap();
        std::mem::take(&mut buf)
    }
}

fn handle_event(
    record: &EventRecord,
    locator: &SchemaLocator,
    sensor_id: &str,
    buffer: &Arc<Mutex<Vec<NetworkEvent>>>,
) {
    let opcode = record.opcode();
    let (is_connection, to_local) = match opcode {
        OP_CONNECT_V4 | OP_CONNECT_V6 | OP_SEND_V4 | OP_SEND_V6 => (true, false),
        OP_RECV_V4 | OP_RECV_V6 => (true, true),
        _ => (false, false),
    };
    if !is_connection {
        return;
    }

    let Ok(schema) = locator.event_schema(record) else {
        return;
    };
    let parser = Parser::create(record, &schema);

    // Property names come from the kernel TcpIp MOF class.
    let saddr = parser.try_parse::<IpAddr>("saddr").ok();
    let daddr = parser.try_parse::<IpAddr>("daddr").ok();
    let sport = parser.try_parse::<u16>("sport").ok();
    let dport = parser.try_parse::<u16>("dport").ok();
    let pid = parser.try_parse::<u32>("PID").unwrap_or(0);
    let size = parser.try_parse::<u32>("size").unwrap_or(0);

    let (Some(saddr), Some(daddr), Some(sport), Some(dport)) = (saddr, daddr, sport, dport) else {
        return;
    };

    if saddr.is_loopback() || daddr.is_loopback() {
        return;
    }

    let (bytes_sent, bytes_recv) = if to_local {
        (0, u64::from(size))
    } else {
        (u64::from(size), 0)
    };

    let event = NetworkEvent {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        sensor_id: sensor_id.to_string(),
        source_ip: saddr,
        source_port: sport,
        dest_ip: daddr,
        dest_port: dport,
        protocol: Protocol::Tcp,
        bytes_sent,
        bytes_recv,
        process_pid: pid,
        process_name: String::new(),
        process_user: String::new(),
    };

    if let Ok(mut buf) = buffer.lock() {
        // Bound memory between polls.
        if buf.len() < 50_000 {
            buf.push(event);
        }
    }
}
