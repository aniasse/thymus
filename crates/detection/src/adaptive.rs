use thymus_common::{MachineIdentity, NetworkEvent};

pub struct AdaptiveLayer;

impl AdaptiveLayer {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate_network(&self, event: &NetworkEvent, profile: &MachineIdentity) -> f64 {
        let relational = Self::score_relational(event, profile);
        let temporal = Self::score_temporal(event, profile);
        let volumetric = Self::score_volumetric(event, profile);

        let weighted = relational * 0.4 + volumetric * 0.35 + temporal * 0.25;
        let max_single = relational.max(volumetric).max(temporal);

        weighted.max(max_single * 0.9)
    }

    fn score_relational(event: &NetworkEvent, profile: &MachineIdentity) -> f64 {
        if !profile.is_known_peer(&event.dest_ip) {
            return 0.8;
        }

        let peer = profile
            .relational
            .known_peers
            .iter()
            .find(|p| p.peer_ip == event.dest_ip);

        if let Some(peer) = peer
            && !peer.ports.contains(&event.dest_port)
        {
            return 0.4;
        }

        0.0
    }

    fn score_temporal(event: &NetworkEvent, profile: &MachineIdentity) -> f64 {
        let hour = event
            .timestamp
            .format("%H")
            .to_string()
            .parse::<u8>()
            .unwrap_or(0);

        if !profile.is_within_active_hours(hour) {
            return 0.6;
        }

        0.0
    }

    #[allow(clippy::cast_precision_loss)]
    fn score_volumetric(event: &NetworkEvent, profile: &MachineIdentity) -> f64 {
        let avg = profile.temporal.avg_daily_volume;
        if avg == 0 {
            return 0.0;
        }

        let total = event.bytes_sent + event.bytes_recv;
        let ratio = total as f64 / avg as f64;

        match ratio {
            r if r > 50.0 => 0.9,
            r if r > 10.0 => 0.7,
            r if r > 3.0 => 0.4,
            _ => 0.0,
        }
    }
}

impl Default for AdaptiveLayer {
    fn default() -> Self {
        Self::new()
    }
}
