use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::Protocol;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineIdentity {
    pub machine_id: String,
    pub hostname: String,
    pub first_seen: DateTime<Utc>,

    pub technical: TechnicalDna,
    pub relational: RelationalDna,
    pub temporal: TemporalDna,

    pub profile_maturity: f64,
    pub last_updated: DateTime<Utc>,
    pub observation_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalDna {
    pub os: String,
    pub os_version: String,
    pub listening_ports: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationalDna {
    pub known_peers: Vec<PeerProfile>,
    pub organ: Option<String>,
    pub role: MachineRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerProfile {
    pub peer_ip: IpAddr,
    pub peer_hostname: Option<String>,
    pub ports: Vec<u16>,
    pub protocols: Vec<Protocol>,
    pub direction: ConnectionDirection,
    pub avg_daily_volume: u64,
    pub avg_daily_connections: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDna {
    pub active_hour_start: u8,
    pub active_hour_end: u8,
    pub active_days: Vec<chrono::Weekday>,
    pub avg_hourly_volume: [u64; 24],
    pub avg_daily_connections: f64,
    pub avg_daily_volume: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MachineRole {
    Workstation,
    Server,
    Infrastructure,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionDirection {
    Outgoing,
    Incoming,
    Both,
}

impl MachineIdentity {
    pub fn new(machine_id: String, hostname: String) -> Self {
        let now = Utc::now();
        Self {
            machine_id,
            hostname,
            first_seen: now,
            technical: TechnicalDna {
                os: String::new(),
                os_version: String::new(),
                listening_ports: Vec::new(),
            },
            relational: RelationalDna {
                known_peers: Vec::new(),
                organ: None,
                role: MachineRole::Unknown,
            },
            temporal: TemporalDna {
                active_hour_start: 0,
                active_hour_end: 23,
                active_days: Vec::new(),
                avg_hourly_volume: [0; 24],
                avg_daily_connections: 0.0,
                avg_daily_volume: 0,
            },
            profile_maturity: 0.0,
            last_updated: now,
            observation_days: 0,
        }
    }

    pub fn is_known_peer(&self, ip: &IpAddr) -> bool {
        self.relational.known_peers.iter().any(|p| &p.peer_ip == ip)
    }

    pub fn is_within_active_hours(&self, hour: u8) -> bool {
        if self.temporal.active_hour_start <= self.temporal.active_hour_end {
            hour >= self.temporal.active_hour_start && hour <= self.temporal.active_hour_end
        } else {
            hour >= self.temporal.active_hour_start || hour <= self.temporal.active_hour_end
        }
    }
}
