//! Prometheus text-format metrics rendering.

use crate::state::{AppState, Phase};

/// A point-in-time snapshot of the numbers we expose. Kept separate from the
/// rendering so it can be unit-tested without an `AppState` or HTTP layer.
pub struct Snapshot {
    pub phase_active: bool,
    pub machines: usize,
    pub events_total: u64,
    pub mutations_active: usize,
    pub mutations_total: usize,
    pub chains_active: usize,
}

impl Snapshot {
    pub fn from_state(s: &AppState) -> Self {
        Self {
            phase_active: s.phase == Phase::Active,
            machines: s.profiles.len(),
            events_total: s.event_count,
            mutations_active: s.active_mutations().len(),
            mutations_total: s.mutations.len(),
            chains_active: s.active_chains().len(),
        }
    }
}

/// Render the snapshot in the Prometheus text exposition format.
pub fn render(snap: &Snapshot) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let mut metric = |name: &str, help: &str, kind: &str, value: String| {
        let _ = writeln!(out, "# HELP {name} {help}");
        let _ = writeln!(out, "# TYPE {name} {kind}");
        let _ = writeln!(out, "{name} {value}");
    };

    metric(
        "thymus_events_total",
        "Total network events ingested",
        "counter",
        snap.events_total.to_string(),
    );
    metric(
        "thymus_machines",
        "Number of profiled machines",
        "gauge",
        snap.machines.to_string(),
    );
    metric(
        "thymus_mutations_active",
        "Currently active mutations",
        "gauge",
        snap.mutations_active.to_string(),
    );
    metric(
        "thymus_mutations_total",
        "All mutations ever recorded",
        "counter",
        snap.mutations_total.to_string(),
    );
    metric(
        "thymus_chains_active",
        "Active lateral movement chains",
        "gauge",
        snap.chains_active.to_string(),
    );
    metric(
        "thymus_phase",
        "Detection phase (0=thymus, 1=active)",
        "gauge",
        u8::from(snap.phase_active).to_string(),
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_prometheus_format() {
        let snap = Snapshot {
            phase_active: true,
            machines: 32,
            events_total: 12345,
            mutations_active: 3,
            mutations_total: 47,
            chains_active: 1,
        };
        let out = render(&snap);

        // Each metric carries HELP, TYPE and a value line.
        assert!(out.contains("# TYPE thymus_events_total counter"));
        assert!(out.contains("thymus_events_total 12345"));
        assert!(out.contains("thymus_machines 32"));
        assert!(out.contains("thymus_mutations_active 3"));
        assert!(out.contains("thymus_mutations_total 47"));
        assert!(out.contains("thymus_chains_active 1"));
        // phase active renders as 1
        assert!(out.contains("thymus_phase 1"));
    }

    #[test]
    fn thymus_phase_is_zero_when_learning() {
        let snap = Snapshot {
            phase_active: false,
            machines: 0,
            events_total: 0,
            mutations_active: 0,
            mutations_total: 0,
            chains_active: 0,
        };
        assert!(render(&snap).contains("thymus_phase 0"));
    }
}
