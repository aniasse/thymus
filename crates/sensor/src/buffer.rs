use thymos_common::{EventBatch, NetworkEvent, ProcessEvent, SystemEvent};

pub struct EventBuffer {
    sensor_id: String,
    network_events: Vec<NetworkEvent>,
    process_events: Vec<ProcessEvent>,
    system_events: Vec<SystemEvent>,
    max_size: usize,
}

impl EventBuffer {
    pub fn new(sensor_id: String, max_size: usize) -> Self {
        Self {
            sensor_id,
            network_events: Vec::new(),
            process_events: Vec::new(),
            system_events: Vec::new(),
            max_size,
        }
    }

    pub fn push_network(&mut self, event: NetworkEvent) {
        if self.network_events.len() < self.max_size {
            self.network_events.push(event);
        }
    }

    pub fn push_process(&mut self, event: ProcessEvent) {
        if self.process_events.len() < self.max_size {
            self.process_events.push(event);
        }
    }

    pub fn push_system(&mut self, event: SystemEvent) {
        if self.system_events.len() < self.max_size {
            self.system_events.push(event);
        }
    }

    pub fn take_batch(&mut self) -> Option<EventBatch> {
        if self.network_events.is_empty()
            && self.process_events.is_empty()
            && self.system_events.is_empty()
        {
            return None;
        }

        let mut batch = EventBatch::new(self.sensor_id.clone());
        batch.network_events = std::mem::take(&mut self.network_events);
        batch.process_events = std::mem::take(&mut self.process_events);
        batch.system_events = std::mem::take(&mut self.system_events);
        Some(batch)
    }

    pub fn pending_count(&self) -> usize {
        self.network_events.len() + self.process_events.len() + self.system_events.len()
    }
}
