use chrono::Utc;
use std::net::IpAddr;
use thymus_common::{NetworkEvent, Protocol};
use uuid::Uuid;

use crate::procinfo;

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
        let socket_map = procinfo::build_socket_to_pid_map();
        let mut events = Vec::new();

        for proto in &["tcp", "tcp6"] {
            if let Some(entries) = self.read_proc_net_tcp(proto, &socket_map) {
                events.extend(entries);
            }
        }

        if let Some(entries) = self.read_proc_net_udp(&socket_map) {
            events.extend(entries);
        }

        events
    }

    fn read_proc_net_tcp(
        &self,
        proto: &str,
        socket_map: &std::collections::HashMap<(IpAddr, u16), u32>,
    ) -> Option<Vec<NetworkEvent>> {
        let path = format!("/proc/net/{proto}");
        let content = std::fs::read_to_string(path).ok()?;
        let events = content
            .lines()
            .skip(1)
            .filter_map(|line| self.parse_proc_line(line, Protocol::Tcp, socket_map))
            .collect();
        Some(events)
    }

    fn read_proc_net_udp(
        &self,
        socket_map: &std::collections::HashMap<(IpAddr, u16), u32>,
    ) -> Option<Vec<NetworkEvent>> {
        let content = std::fs::read_to_string("/proc/net/udp").ok()?;
        let events = content
            .lines()
            .skip(1)
            .filter_map(|line| self.parse_proc_line(line, Protocol::Udp, socket_map))
            .collect();
        Some(events)
    }

    fn parse_proc_line(
        &self,
        line: &str,
        protocol: Protocol,
        socket_map: &std::collections::HashMap<(IpAddr, u16), u32>,
    ) -> Option<NetworkEvent> {
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

        let (pid, process_name, process_user) =
            if let Some(&pid) = socket_map.get(&(local.0, local.1)) {
                if let Some(info) = procinfo::get_process_info(pid) {
                    (pid, info.name, info.user)
                } else {
                    (pid, String::new(), String::new())
                }
            } else {
                (0, String::new(), String::new())
            };

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
            process_pid: pid,
            process_name,
            process_user,
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
