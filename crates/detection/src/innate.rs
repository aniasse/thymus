use thymos_common::{MachineIdentity, NetworkEvent};

const PORT_SCAN_THRESHOLD: u16 = 10;
const KNOWN_MALICIOUS_PORTS: &[u16] = &[4444, 5555, 6666, 1234, 31337];

pub struct InnateLayer;

impl InnateLayer {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_network(&self, event: &NetworkEvent, _profile: &MachineIdentity) -> f64 {
        let mut score = 0.0_f64;

        if KNOWN_MALICIOUS_PORTS.contains(&event.dest_port) {
            score = score.max(0.8);
        }

        if event.dest_port == 53 && event.bytes_sent > 512 {
            score = score.max(0.6);
        }

        if event.process_name.is_empty() {
            score = score.max(0.5);
        }

        if event.dest_port == 445 || event.dest_port == 135 {
            score = score.max(0.3);
        }

        score.min(1.0)
    }
}

impl Default for InnateLayer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PortScanDetector {
    connection_counts: std::collections::HashMap<String, Vec<(u16, chrono::DateTime<chrono::Utc>)>>,
    window_seconds: i64,
}

impl PortScanDetector {
    pub fn new(window_seconds: i64) -> Self {
        Self {
            connection_counts: std::collections::HashMap::new(),
            window_seconds,
        }
    }

    pub fn record_connection(
        &mut self,
        source: &str,
        dest_port: u16,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        let entry = self
            .connection_counts
            .entry(source.to_string())
            .or_default();

        entry.push((dest_port, timestamp));

        let cutoff = timestamp - chrono::Duration::seconds(self.window_seconds);
        entry.retain(|(_, ts)| *ts > cutoff);

        let unique_ports: std::collections::HashSet<u16> = entry.iter().map(|(p, _)| *p).collect();
        unique_ports.len() >= PORT_SCAN_THRESHOLD as usize
    }
}
