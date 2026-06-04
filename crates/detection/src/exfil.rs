//! Data-exfiltration detection.
//!
//! Two patterns the spike-based volumetric check misses:
//!
//! - **Low-and-slow exfiltration**: an attacker dribbles data out over a long
//!   period to stay under per-event thresholds. We accumulate sustained,
//!   upload-dominant traffic to a single external destination over a window.
//! - **DNS tunneling**: data smuggled through DNS queries (port 53). DNS should
//!   never carry meaningful payload, so even modest outbound volume on :53 is
//!   suspicious.

use std::collections::HashMap;
use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};

const WINDOW_MINUTES: i64 = 60;
const REFLAG_COOLDOWN_MINUTES: i64 = 60;
const MAX_SAMPLES: usize = 256;

// Low-and-slow exfiltration thresholds.
const SLOW_MIN_SENT: u64 = 100 * 1024 * 1024; // 100 MB out over the window
const SLOW_MIN_SAMPLES: usize = 5; // sustained, not a single big transfer
const SLOW_UPLOAD_RATIO: f64 = 5.0; // sent >> recv

// DNS tunneling threshold (bytes out on port 53).
const DNS_MIN_SENT: u64 = 1024 * 1024; // 1 MB up via DNS is already absurd

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    machine_id: String,
    dest_ip: IpAddr,
    dest_port: u16,
}

struct Window {
    samples: Vec<(DateTime<Utc>, u64, u64)>, // (ts, sent, recv)
    last_flagged: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExfilKind {
    SlowExfiltration,
    DnsTunneling,
}

pub struct ExfilHit {
    pub kind: ExfilKind,
    pub dest_ip: IpAddr,
    pub dest_port: u16,
    pub sent_bytes: u64,
    pub recv_bytes: u64,
    pub window_minutes: i64,
}

pub struct ExfilDetector {
    windows: HashMap<Key, Window>,
}

impl ExfilDetector {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }

    /// Record one flow to an external destination. Call only for non-local dests.
    pub fn record(
        &mut self,
        machine_id: &str,
        dest_ip: IpAddr,
        dest_port: u16,
        sent: u64,
        recv: u64,
        ts: DateTime<Utc>,
    ) -> Option<ExfilHit> {
        let key = Key {
            machine_id: machine_id.to_string(),
            dest_ip,
            dest_port,
        };
        let win = self.windows.entry(key).or_insert_with(|| Window {
            samples: Vec::new(),
            last_flagged: None,
        });

        win.samples.push((ts, sent, recv));

        let cutoff = ts - Duration::minutes(WINDOW_MINUTES);
        win.samples.retain(|(t, _, _)| *t > cutoff);
        if win.samples.len() > MAX_SAMPLES {
            let excess = win.samples.len() - MAX_SAMPLES;
            win.samples.drain(0..excess);
        }

        if let Some(last) = win.last_flagged
            && ts - last < Duration::minutes(REFLAG_COOLDOWN_MINUTES)
        {
            return None;
        }

        let total_sent: u64 = win.samples.iter().map(|(_, s, _)| *s).sum();
        let total_recv: u64 = win.samples.iter().map(|(_, _, r)| *r).sum();

        let hit = classify(
            dest_port,
            total_sent,
            total_recv,
            win.samples.len(),
            dest_ip,
        );

        if hit.is_some() {
            win.last_flagged = Some(ts);
        }
        hit
    }
}

impl Default for ExfilDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::cast_precision_loss)]
fn classify(
    dest_port: u16,
    total_sent: u64,
    total_recv: u64,
    samples: usize,
    dest_ip: IpAddr,
) -> Option<ExfilHit> {
    // DNS tunneling: meaningful upload on port 53.
    if dest_port == 53 && total_sent >= DNS_MIN_SENT {
        return Some(ExfilHit {
            kind: ExfilKind::DnsTunneling,
            dest_ip,
            dest_port,
            sent_bytes: total_sent,
            recv_bytes: total_recv,
            window_minutes: WINDOW_MINUTES,
        });
    }

    // Low-and-slow: sustained, upload-dominant, above the volume floor.
    let upload_dominant = total_sent as f64 > SLOW_UPLOAD_RATIO * (total_recv as f64 + 1.0);
    if samples >= SLOW_MIN_SAMPLES && total_sent >= SLOW_MIN_SENT && upload_dominant {
        return Some(ExfilHit {
            kind: ExfilKind::SlowExfiltration,
            dest_ip,
            dest_port,
            sent_bytes: total_sent,
            recv_bytes: total_recv,
            window_minutes: WINDOW_MINUTES,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ext() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))
    }

    fn base() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-04T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn slow_exfiltration_is_detected() {
        let mut d = ExfilDetector::new();
        let t0 = base();
        // 10 chunks of ~15 MB uploaded, almost nothing downloaded, over an hour.
        let mut hit = None;
        for i in 0..10 {
            if let Some(h) = d.record(
                "host-a",
                ext(),
                443,
                15 * 1024 * 1024,
                1000,
                t0 + Duration::minutes(5 * i),
            ) {
                hit = Some(h);
                break;
            }
        }
        let hit = hit.expect("sustained upload should flag slow exfiltration");
        assert_eq!(hit.kind, ExfilKind::SlowExfiltration);
        assert!(hit.sent_bytes >= SLOW_MIN_SENT);
    }

    #[test]
    fn normal_download_is_not_exfiltration() {
        let mut d = ExfilDetector::new();
        let t0 = base();
        // Download-heavy: recv >> sent (typical browsing/streaming).
        let mut hit = None;
        for i in 0..10 {
            hit = d.record(
                "host-a",
                ext(),
                443,
                50_000,
                20 * 1024 * 1024,
                t0 + Duration::minutes(5 * i),
            );
        }
        assert!(hit.is_none(), "download-dominant traffic must not flag");
    }

    #[test]
    fn dns_tunneling_is_detected() {
        let mut d = ExfilDetector::new();
        let t0 = base();
        // 1.5 MB uploaded to port 53 across many small queries.
        let mut hit = None;
        for i in 0..30 {
            if let Some(h) = d.record(
                "host-a",
                ext(),
                53,
                60_000,
                200,
                t0 + Duration::seconds(20 * i),
            ) {
                hit = Some(h);
                break;
            }
        }
        let hit = hit.expect("high DNS upload should flag tunneling");
        assert_eq!(hit.kind, ExfilKind::DnsTunneling);
        assert_eq!(hit.dest_port, 53);
    }

    #[test]
    fn normal_dns_is_not_tunneling() {
        let mut d = ExfilDetector::new();
        let t0 = base();
        // Ordinary DNS: tiny queries.
        let mut hit = None;
        for i in 0..30 {
            hit = d.record("host-a", ext(), 53, 80, 200, t0 + Duration::seconds(20 * i));
        }
        assert!(hit.is_none(), "normal DNS volume must not flag");
    }

    #[test]
    fn cooldown_rate_limits_alerts() {
        let mut d = ExfilDetector::new();
        let t0 = base();
        let mut flags = 0;
        // 20 qualifying events over ~95 min. Without cooldown this would alert on
        // most of them; with a 60-min cooldown it should fire only a couple times.
        for i in 0..20 {
            if d.record(
                "host-a",
                ext(),
                443,
                15 * 1024 * 1024,
                1000,
                t0 + Duration::minutes(5 * i),
            )
            .is_some()
            {
                flags += 1;
            }
        }
        assert!(flags >= 1, "ongoing exfiltration should alert");
        assert!(flags <= 2, "cooldown must rate-limit (got {flags})");
    }
}
