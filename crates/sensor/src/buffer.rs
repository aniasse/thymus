use thymos_common::{EventBatch, NetworkEvent};

pub struct EventBuffer {
    sensor_id: String,
    network_events: Vec<NetworkEvent>,
    max_size: usize,
}

impl EventBuffer {
    pub fn new(sensor_id: String, max_size: usize) -> Self {
        Self {
            sensor_id,
            network_events: Vec::new(),
            max_size,
        }
    }

    pub fn push_network(&mut self, event: NetworkEvent) {
        if self.network_events.len() < self.max_size {
            self.network_events.push(event);
        }
    }

    pub fn take_batch(&mut self) -> Option<EventBatch> {
        if self.network_events.is_empty() {
            return None;
        }

        let mut batch = EventBatch::new(self.sensor_id.clone());
        batch.network_events = std::mem::take(&mut self.network_events);
        Some(batch)
    }
}
