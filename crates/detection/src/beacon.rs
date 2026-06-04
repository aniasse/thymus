//! C2 beaconing detection.
//!
//! Malware that "calls home" to a command-and-control server tends to do so at
//! very regular intervals (a heartbeat). Legitimate human-driven traffic is
//! irregular. We track per-destination connection timestamps and flag a beacon
//! when the inter-arrival times are tight and regular over enough samples.

use std::collections::HashMap;
use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};

const MIN_SAMPLES: usize = 6;
const MAX_SAMPLES: usize = 32;
/// Coefficient of variation (stddev / mean) below this = "very regular".
const REGULARITY_THRESHOLD: f64 = 0.15;
const MIN_INTERVAL_SECS: f64 = 5.0;
const MAX_INTERVAL_SECS: f64 = 3600.0;
const HISTORY_WINDOW_MINUTES: i64 = 180;
const REFLAG_COOLDOWN_MINUTES: i64 = 60;

#[derive(Clone)]
struct DestKey {
    machine_id: String,
    dest_ip: IpAddr,
    dest_port: u16,
}

impl PartialEq for DestKey {
    fn eq(&self, other: &Self) -> bool {
        self.machine_id == other.machine_id
            && self.dest_ip == other.dest_ip
            && self.dest_port == other.dest_port
    }
}
impl Eq for DestKey {}
impl std::hash::Hash for DestKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.machine_id.hash(state);
        self.dest_ip.hash(state);
        self.dest_port.hash(state);
    }
}

struct DestState {
    timestamps: Vec<DateTime<Utc>>,
    last_flagged: Option<DateTime<Utc>>,
}

pub struct BeaconHit {
    pub dest_ip: IpAddr,
    pub dest_port: u16,
    pub interval_secs: f64,
    pub samples: usize,
    /// 0.0 = perfectly regular, higher = more jitter.
    pub regularity: f64,
}

pub struct BeaconDetector {
    dests: HashMap<DestKey, DestState>,
}

impl BeaconDetector {
    pub fn new() -> Self {
        Self {
            dests: HashMap::new(),
        }
    }

    pub fn record(
        &mut self,
        machine_id: &str,
        dest_ip: IpAddr,
        dest_port: u16,
        ts: DateTime<Utc>,
    ) -> Option<BeaconHit> {
        let key = DestKey {
            machine_id: machine_id.to_string(),
            dest_ip,
            dest_port,
        };

        let entry = self.dests.entry(key).or_insert_with(|| DestState {
            timestamps: Vec::new(),
            last_flagged: None,
        });

        entry.timestamps.push(ts);

        // Prune old samples and cap the buffer.
        let cutoff = ts - Duration::minutes(HISTORY_WINDOW_MINUTES);
        entry.timestamps.retain(|t| *t > cutoff);
        if entry.timestamps.len() > MAX_SAMPLES {
            let excess = entry.timestamps.len() - MAX_SAMPLES;
            entry.timestamps.drain(0..excess);
        }

        if entry.timestamps.len() < MIN_SAMPLES {
            return None;
        }

        // Respect cooldown so one beacon doesn't alert on every packet.
        if let Some(last) = entry.last_flagged
            && ts - last < Duration::minutes(REFLAG_COOLDOWN_MINUTES)
        {
            return None;
        }

        let (mean, cov) = interval_stats(&entry.timestamps)?;

        if (MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&mean) && cov < REGULARITY_THRESHOLD {
            entry.last_flagged = Some(ts);
            Some(BeaconHit {
                dest_ip,
                dest_port,
                interval_secs: mean,
                samples: entry.timestamps.len(),
                regularity: cov,
            })
        } else {
            None
        }
    }
}

impl Default for BeaconDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Mean inter-arrival interval (seconds) and its coefficient of variation.
#[allow(clippy::cast_precision_loss)]
fn interval_stats(timestamps: &[DateTime<Utc>]) -> Option<(f64, f64)> {
    if timestamps.len() < 2 {
        return None;
    }

    let mut sorted = timestamps.to_vec();
    sorted.sort_unstable();

    let intervals: Vec<f64> = sorted
        .windows(2)
        .map(|w| (w[1] - w[0]).num_milliseconds() as f64 / 1000.0)
        .filter(|d| *d > 0.0)
        .collect();

    if intervals.len() < 2 {
        return None;
    }

    let n = intervals.len() as f64;
    let mean = intervals.iter().sum::<f64>() / n;
    if mean <= 0.0 {
        return None;
    }

    let variance = intervals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    let cov = variance.sqrt() / mean;

    Some((mean, cov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))
    }

    fn base() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-04T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn regular_beacon_is_detected() {
        let mut d = BeaconDetector::new();
        let t0 = base();
        let mut hit = None;
        // 8 connections exactly every 60 seconds. The first flag fires once enough
        // samples accumulate; later calls are suppressed by the cooldown, so keep
        // the first Some.
        for i in 0..8 {
            if let Some(h) = d.record("host-a", ip(), 443, t0 + Duration::seconds(60 * i)) {
                hit = Some(h);
                break;
            }
        }
        let hit = hit.expect("regular beacon should be flagged");
        assert!((hit.interval_secs - 60.0).abs() < 1.0);
        assert!(hit.regularity < REGULARITY_THRESHOLD);
        assert!(hit.samples >= MIN_SAMPLES);
    }

    #[test]
    fn irregular_traffic_is_not_a_beacon() {
        let mut d = BeaconDetector::new();
        let t0 = base();
        // Human-like irregular gaps.
        let offsets = [0, 7, 50, 51, 200, 203, 800, 802];
        let mut hit = None;
        for o in offsets {
            hit = d.record("host-a", ip(), 443, t0 + Duration::seconds(o));
        }
        assert!(hit.is_none(), "irregular traffic must not be flagged");
    }

    #[test]
    fn too_few_samples_no_alert() {
        let mut d = BeaconDetector::new();
        let t0 = base();
        let mut hit = None;
        for i in 0..4 {
            hit = d.record("host-a", ip(), 443, t0 + Duration::seconds(30 * i));
        }
        assert!(hit.is_none(), "fewer than MIN_SAMPLES must not flag");
    }

    #[test]
    fn cooldown_prevents_repeated_alerts() {
        let mut d = BeaconDetector::new();
        let t0 = base();
        let mut flags = 0;
        // 16 regular beacons every 60s; should flag once, then cool down.
        for i in 0..16 {
            if d.record("host-a", ip(), 443, t0 + Duration::seconds(60 * i))
                .is_some()
            {
                flags += 1;
            }
        }
        assert_eq!(flags, 1, "cooldown should limit to a single alert");
    }

    #[test]
    fn separate_destinations_are_independent() {
        let mut d = BeaconDetector::new();
        let t0 = base();
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 9));
        // Two independent regular beacons; each key reaches detection on its own.
        let mut flagged_main = false;
        let mut flagged_other = false;
        for i in 0..8 {
            if d.record("host-a", ip(), 443, t0 + Duration::seconds(60 * i))
                .is_some()
            {
                flagged_main = true;
            }
            if d.record("host-a", other, 443, t0 + Duration::seconds(60 * i))
                .is_some()
            {
                flagged_other = true;
            }
        }
        assert!(flagged_main && flagged_other, "keys tracked independently");
    }
}
