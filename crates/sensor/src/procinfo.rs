use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;

pub struct ProcessInfo {
    pub name: String,
    pub user: String,
}

pub fn build_socket_to_pid_map() -> HashMap<(IpAddr, u16), u32> {
    let mut map = HashMap::new();

    let Ok(entries) = fs::read_dir("/proc") else {
        return map;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };

        let fd_path = format!("/proc/{pid}/fd");
        let Ok(fds) = fs::read_dir(&fd_path) else {
            continue;
        };

        for fd in fds.flatten() {
            let Ok(link) = fs::read_link(fd.path()) else {
                continue;
            };
            let link_str = link.to_string_lossy();
            if !link_str.starts_with("socket:[") {
                continue;
            }

            let inode = link_str
                .trim_start_matches("socket:[")
                .trim_end_matches(']');

            if let Some((ip, port)) = find_socket_by_inode(inode) {
                map.insert((ip, port), pid);
            }
        }
    }

    map
}

pub fn get_process_info(pid: u32) -> Option<ProcessInfo> {
    let proc_path = format!("/proc/{pid}");
    if !Path::new(&proc_path).exists() {
        return None;
    }

    let name = fs::read_to_string(format!("{proc_path}/comm"))
        .unwrap_or_default()
        .trim()
        .to_string();

    let uid = fs::read_to_string(format!("{proc_path}/status"))
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("0")
        .to_string();

    Some(ProcessInfo { name, user: uid })
}

fn find_socket_by_inode(target_inode: &str) -> Option<(IpAddr, u16)> {
    for proto in &["/proc/net/tcp", "/proc/net/tcp6", "/proc/net/udp"] {
        let Ok(content) = fs::read_to_string(proto) else {
            continue;
        };

        for line in content.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                continue;
            }

            if fields[9] == target_inode {
                return parse_hex_addr(fields[1]);
            }
        }
    }
    None
}

fn parse_hex_addr(s: &str) -> Option<(IpAddr, u16)> {
    let (addr_hex, port_hex) = s.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;

    if addr_hex.len() == 8 {
        let ip_num = u32::from_str_radix(addr_hex, 16).ok()?;
        Some((IpAddr::V4(Ipv4Addr::from(ip_num.to_be())), port))
    } else {
        None
    }
}
