use anyhow::Result;
use chrono::Utc;
use std::net::IpAddr;
use thymos_common::{NetworkEvent, Protocol};
use uuid::Uuid;

pub struct NetworkCollector {
    sensor_id: String,
}

impl NetworkCollector {
    pub fn new() -> Result<Self> {
        let sensor_id = std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string();

        Ok(Self { sensor_id })
    }

    pub fn collect_connections(&self) -> Result<Vec<NetworkEvent>> {
        let mut events = Vec::new();

        if let Ok(tcp_entries) = self.read_proc_net("tcp") {
            events.extend(tcp_entries);
        }
        if let Ok(tcp6_entries) = self.read_proc_net("tcp6") {
            events.extend(tcp6_entries);
        }
        if let Ok(udp_entries) = self.read_proc_net_udp("udp") {
            events.extend(udp_entries);
        }

        Ok(events)
    }

    fn read_proc_net(&self, proto: &str) -> Result<Vec<NetworkEvent>> {
        let path = format!("/proc/net/{}", proto);
        let content = std::fs::read_to_string(&path)?;
        let mut events = Vec::new();

        for line in content.lines().skip(1) {
            if let Some(event) = self.parse_tcp_line(line) {
                events.push(event);
            }
        }

        Ok(events)
    }

    fn read_proc_net_udp(&self, proto: &str) -> Result<Vec<NetworkEvent>> {
        let path = format!("/proc/net/{}", proto);
        let content = std::fs::read_to_string(&path)?;
        let mut events = Vec::new();

        for line in content.lines().skip(1) {
            if let Some(mut event) = self.parse_tcp_line(line) {
                event.protocol = Protocol::Udp;
                events.push(event);
            }
        }

        Ok(events)
    }

    fn parse_tcp_line(&self, line: &str) -> Option<NetworkEvent> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            return None;
        }

        let local = parse_addr_port(fields[1])?;
        let remote = parse_addr_port(fields[2])?;
        let state = u8::from_str_radix(fields[3], 16).ok()?;

        // Only ESTABLISHED connections (state 01)
        if state != 0x01 {
            return None;
        }

        // Skip loopback
        if local.0.is_loopback() || remote.0.is_loopback() {
            return None;
        }

        let uid = fields.get(7).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);

        Some(NetworkEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sensor_id: self.sensor_id.clone(),
            source_ip: local.0,
            source_port: local.1,
            dest_ip: remote.0,
            dest_port: remote.1,
            protocol: Protocol::Tcp,
            bytes_sent: 0,
            bytes_recv: 0,
            process_pid: 0,
            process_name: String::new(),
            process_user: uid.to_string(),
        })
    }
}

fn parse_addr_port(s: &str) -> Option<(IpAddr, u16)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let port = u16::from_str_radix(parts[1], 16).ok()?;

    let ip = if parts[0].len() == 8 {
        // IPv4 in hex, little-endian
        let ip_num = u32::from_str_radix(parts[0], 16).ok()?;
        IpAddr::V4(std::net::Ipv4Addr::from(ip_num.to_be()))
    } else {
        return None;
    };

    Some((ip, port))
}
