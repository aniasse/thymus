use chrono::Utc;
use std::net::IpAddr;
use thymos_common::{NetworkEvent, Protocol};
use uuid::Uuid;

pub struct NetworkCollector {
    sensor_id: String,
}

impl NetworkCollector {
    pub fn new() -> Self {
        let sensor_id = std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string();

        Self { sensor_id }
    }

    pub fn collect_connections(&self) -> Vec<NetworkEvent> {
        let mut events = Vec::new();

        for proto in &["tcp", "tcp6"] {
            if let Some(entries) = self.read_proc_net_tcp(proto) {
                events.extend(entries);
            }
        }

        if let Some(entries) = self.read_proc_net_udp() {
            events.extend(entries);
        }

        events
    }

    fn read_proc_net_tcp(&self, proto: &str) -> Option<Vec<NetworkEvent>> {
        let path = format!("/proc/net/{proto}");
        let content = std::fs::read_to_string(path).ok()?;
        let events = content
            .lines()
            .skip(1)
            .filter_map(|line| self.parse_proc_line(line, Protocol::Tcp))
            .collect();
        Some(events)
    }

    fn read_proc_net_udp(&self) -> Option<Vec<NetworkEvent>> {
        let content = std::fs::read_to_string("/proc/net/udp").ok()?;
        let events = content
            .lines()
            .skip(1)
            .filter_map(|line| self.parse_proc_line(line, Protocol::Udp))
            .collect();
        Some(events)
    }

    fn parse_proc_line(&self, line: &str, protocol: Protocol) -> Option<NetworkEvent> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            return None;
        }

        let local = parse_addr_port(fields[1])?;
        let remote = parse_addr_port(fields[2])?;

        if protocol == Protocol::Tcp {
            let state = u8::from_str_radix(fields[3], 16).ok()?;
            if state != 0x01 {
                return None;
            }
        }

        if local.0.is_loopback() || remote.0.is_loopback() {
            return None;
        }

        Some(NetworkEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sensor_id: self.sensor_id.clone(),
            source_ip: local.0,
            source_port: local.1,
            dest_ip: remote.0,
            dest_port: remote.1,
            protocol,
            bytes_sent: 0,
            bytes_recv: 0,
            process_pid: 0,
            process_name: String::new(),
            process_user: fields.get(7).unwrap_or(&"0").to_string(),
        })
    }
}

fn parse_addr_port(s: &str) -> Option<(IpAddr, u16)> {
    let (addr_hex, port_hex) = s.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;

    if addr_hex.len() == 8 {
        let ip_num = u32::from_str_radix(addr_hex, 16).ok()?;
        Some((IpAddr::V4(std::net::Ipv4Addr::from(ip_num.to_be())), port))
    } else {
        None
    }
}
