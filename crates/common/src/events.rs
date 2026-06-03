use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sensor_id: String,
    pub source_ip: IpAddr,
    pub source_port: u16,
    pub dest_ip: IpAddr,
    pub dest_port: u16,
    pub protocol: Protocol,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub process_pid: u32,
    pub process_name: String,
    pub process_user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sensor_id: String,
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub exe_path: String,
    pub cmdline: String,
    pub user: String,
    pub event_type: ProcessEventType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sensor_id: String,
    pub event_type: SystemEventType,
    pub source: String,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessEventType {
    Started,
    Stopped,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemEventType {
    FileModified {
        path: String,
    },
    ServiceChanged {
        name: String,
        old_state: String,
        new_state: String,
    },
    UserCreated {
        username: String,
    },
    PrivilegeEscalation {
        user: String,
        method: String,
    },
    CronModified {
        entry: String,
    },
    KernelModuleLoaded {
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CollectionMode {
    /// Agent installed on the host, reads /proc/net (one machine per sensor).
    #[default]
    Host,
    /// Passive sniffing on a mirror/SPAN port (many devices seen by one sensor).
    Passive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatch {
    pub sensor_id: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub mode: CollectionMode,
    pub network_events: Vec<NetworkEvent>,
    pub process_events: Vec<ProcessEvent>,
    pub system_events: Vec<SystemEvent>,
}

impl EventBatch {
    pub fn new(sensor_id: String) -> Self {
        Self {
            sensor_id,
            timestamp: Utc::now(),
            mode: CollectionMode::Host,
            network_events: Vec::new(),
            process_events: Vec::new(),
            system_events: Vec::new(),
        }
    }

    pub fn new_passive(sensor_id: String) -> Self {
        Self {
            mode: CollectionMode::Passive,
            ..Self::new(sensor_id)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.network_events.is_empty()
            && self.process_events.is_empty()
            && self.system_events.is_empty()
    }

    pub fn event_count(&self) -> usize {
        self.network_events.len() + self.process_events.len() + self.system_events.len()
    }
}
