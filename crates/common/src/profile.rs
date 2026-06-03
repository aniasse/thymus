use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::Protocol;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineIdentity {
    pub machine_id: String,
    pub hostname: String,
    pub first_seen: DateTime<Utc>,

    #[serde(default)]
    pub discovery: Discovery,

    pub technical: TechnicalDna,
    pub relational: RelationalDna,
    pub temporal: TemporalDna,

    pub profile_maturity: f64,
    pub last_updated: DateTime<Utc>,
    pub observation_days: u32,
}

/// How a device became known to Thymus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Discovery {
    /// An agent is installed on the device (host mode).
    #[default]
    Agent,
    /// Observed passively on a mirror/SPAN port (no agent).
    Passive,
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
            discovery: Discovery::Agent,
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

    /// Ports on which this device receives connections (i.e. the services it
    /// exposes), gathered from incoming peers plus any declared listening ports.
    pub fn served_ports(&self) -> Vec<u16> {
        let mut ports: Vec<u16> = self.technical.listening_ports.clone();
        for peer in &self.relational.known_peers {
            if peer.direction == ConnectionDirection::Incoming
                || peer.direction == ConnectionDirection::Both
            {
                ports.extend(&peer.ports);
            }
        }
        ports.sort_unstable();
        ports.dedup();
        ports
    }

    /// Infer a human-readable device type from the services it exposes. Specific
    /// device classes (printer, camera, PLC...) take priority over generic
    /// services (SSH, web) so a printer that also runs SSH stays a printer.
    pub fn device_kind(&self) -> &'static str {
        let served = self.served_ports();

        for (ports, label) in DEVICE_PORT_PRIORITY {
            if ports.iter().any(|p| served.contains(p)) {
                return label;
            }
        }

        if served.is_empty() {
            "Poste / Client"
        } else {
            "Serveur"
        }
    }

    pub fn is_within_active_hours(&self, hour: u8) -> bool {
        if self.temporal.active_hour_start <= self.temporal.active_hour_end {
            hour >= self.temporal.active_hour_start && hour <= self.temporal.active_hour_end
        } else {
            hour >= self.temporal.active_hour_start || hour <= self.temporal.active_hour_end
        }
    }
}

/// Device-type heuristics, ordered by specificity (first match wins). Specific
/// hardware classes are listed before generic services.
const DEVICE_PORT_PRIORITY: &[(&[u16], &str)] = &[
    (&[9100, 631, 515], "Imprimante"),
    (&[554, 8554, 37777], "Caméra IP"),
    (&[502, 20000, 44818], "Automate industriel"),
    (&[161, 162], "Équipement réseau"),
    (&[5432, 3306, 1433, 1521, 27017, 6379], "Base de données"),
    (&[389, 636, 88], "Annuaire / AD"),
    (&[25, 587, 465, 143, 993, 110, 995], "Serveur mail"),
    (&[53], "Serveur DNS"),
    (&[445, 139], "Partage de fichiers"),
    (&[3389], "Bureau à distance (RDP)"),
    (&[80, 443, 8080, 8443], "Serveur web"),
    (&[22], "Serveur SSH"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn with_incoming_port(port: u16) -> MachineIdentity {
        let mut m = MachineIdentity::new("dev".into(), "dev".into());
        m.relational.known_peers.push(PeerProfile {
            peer_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            peer_hostname: None,
            ports: vec![port],
            protocols: vec![Protocol::Tcp],
            direction: ConnectionDirection::Incoming,
            avg_daily_volume: 0,
            avg_daily_connections: 1.0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            confidence: 0.5,
        });
        m
    }

    #[test]
    fn classifies_printer() {
        assert_eq!(with_incoming_port(9100).device_kind(), "Imprimante");
    }

    #[test]
    fn classifies_camera() {
        assert_eq!(with_incoming_port(554).device_kind(), "Caméra IP");
    }

    #[test]
    fn classifies_database() {
        assert_eq!(with_incoming_port(5432).device_kind(), "Base de données");
    }

    #[test]
    fn specific_device_beats_generic_service() {
        // A printer that also exposes SSH must remain a printer.
        let mut m = with_incoming_port(9100);
        m.relational.known_peers[0].ports.push(22);
        assert_eq!(m.device_kind(), "Imprimante");
    }

    #[test]
    fn client_with_no_served_ports() {
        // Only outgoing connections → an endpoint/client.
        let mut m = MachineIdentity::new("ws".into(), "ws".into());
        m.relational.known_peers.push(PeerProfile {
            peer_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            peer_hostname: None,
            ports: vec![443],
            protocols: vec![Protocol::Tcp],
            direction: ConnectionDirection::Outgoing,
            avg_daily_volume: 0,
            avg_daily_connections: 1.0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            confidence: 0.5,
        });
        assert_eq!(m.device_kind(), "Poste / Client");
    }
}
