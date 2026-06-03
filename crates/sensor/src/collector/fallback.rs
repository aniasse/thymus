use thymus_common::NetworkEvent;

/// Host mode is not implemented for this platform. Passive mode (`--interface`)
/// remains available where packet capture is supported.
pub struct NetworkCollector;

impl NetworkCollector {
    pub fn new() -> Self {
        tracing::warn!(
            "host mode is not supported on this platform; use passive mode (--interface)"
        );
        Self
    }

    pub fn collect_connections(&self) -> Vec<NetworkEvent> {
        Vec::new()
    }
}
