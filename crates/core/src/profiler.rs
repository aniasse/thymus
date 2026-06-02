use chrono::{Datelike, Timelike, Utc};
use thymos_common::{MachineIdentity, NetworkEvent};

const MATURITY_THRESHOLD: f64 = 0.7;

#[allow(clippy::cast_possible_truncation)]
pub fn update_temporal_stats(profile: &mut MachineIdentity, event: &NetworkEvent) {
    let hour = event.timestamp.hour() as usize;
    profile.temporal.avg_hourly_volume[hour] = profile.temporal.avg_hourly_volume[hour]
        .saturating_add(event.bytes_sent + event.bytes_recv);

    let weekday = event.timestamp.weekday();
    if !profile.temporal.active_days.contains(&weekday) {
        profile.temporal.active_days.push(weekday);
    }

    profile.temporal.avg_daily_volume = profile.temporal.avg_hourly_volume.iter().sum::<u64>()
        / u64::from(profile.observation_days.max(1));

    let total_conns: f64 = profile
        .relational
        .known_peers
        .iter()
        .map(|p| p.avg_daily_connections)
        .sum();
    profile.temporal.avg_daily_connections =
        total_conns / f64::from(profile.observation_days.max(1));
}

#[allow(clippy::cast_possible_truncation)]
pub fn update_active_hours(profile: &mut MachineIdentity) {
    let volumes = &profile.temporal.avg_hourly_volume;
    let total: u64 = volumes.iter().sum();
    if total == 0 {
        return;
    }

    let threshold = total / 20;

    let mut start = 0u8;
    let mut end = 23u8;

    for (h, &v) in volumes.iter().enumerate() {
        if v > threshold {
            start = h as u8;
            break;
        }
    }

    for (h, &v) in volumes.iter().enumerate().rev() {
        if v > threshold {
            end = h as u8;
            break;
        }
    }

    profile.temporal.active_hour_start = start;
    profile.temporal.active_hour_end = end;
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn update_observation_days(profile: &mut MachineIdentity) {
    let days = (Utc::now() - profile.first_seen).num_days();
    profile.observation_days = days.clamp(1, i64::from(u32::MAX)) as u32;
}

#[allow(clippy::cast_precision_loss)]
pub fn compute_maturity(profile: &mut MachineIdentity) {
    let days_factor = (f64::from(profile.observation_days) / 21.0).min(1.0);

    let peer_count = profile.relational.known_peers.len() as f64;
    let peers_factor = (peer_count / 5.0).min(1.0);

    let avg_confidence = if profile.relational.known_peers.is_empty() {
        0.0
    } else {
        profile
            .relational
            .known_peers
            .iter()
            .map(|p| p.confidence)
            .sum::<f64>()
            / peer_count
    };

    profile.profile_maturity =
        (days_factor * 0.4 + peers_factor * 0.3 + avg_confidence * 0.3).min(1.0);
}

pub fn should_auto_activate(profiles: &std::collections::HashMap<String, MachineIdentity>) -> bool {
    if profiles.is_empty() {
        return false;
    }

    profiles
        .values()
        .all(|p| p.profile_maturity >= MATURITY_THRESHOLD)
}
