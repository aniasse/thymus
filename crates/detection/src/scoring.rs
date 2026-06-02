use thymos_common::{MachineIdentity, MutationDetail, MutationDimension, NetworkEvent};

pub fn combine_scores(innate: f64, adaptive: f64) -> f64 {
    let base = innate.max(adaptive);

    if innate > 0.3 && adaptive > 0.3 {
        (base * 1.3).min(1.0)
    } else {
        base
    }
}

#[allow(clippy::cast_precision_loss)]
pub fn build_details(
    event: &NetworkEvent,
    profile: &MachineIdentity,
    innate_score: f64,
    _adaptive_score: f64,
) -> Vec<MutationDetail> {
    let mut details = Vec::new();

    if !profile.is_known_peer(&event.dest_ip) {
        details.push(MutationDetail {
            dimension: MutationDimension::Relational,
            description: format!(
                "{} a contacté {} pour la première fois",
                profile.hostname, event.dest_ip
            ),
            expected_value: format!("{} pairs connus", profile.relational.known_peers.len()),
            observed_value: format!("nouvelle destination {}", event.dest_ip),
            deviation_sigma: innate_score * 5.0,
        });
    }

    let hour = event
        .timestamp
        .format("%H")
        .to_string()
        .parse::<u8>()
        .unwrap_or(0);
    if !profile.is_within_active_hours(hour) {
        details.push(MutationDetail {
            dimension: MutationDimension::Temporal,
            description: format!(
                "Activité à {hour}h, hors plage habituelle ({}-{}h)",
                profile.temporal.active_hour_start, profile.temporal.active_hour_end
            ),
            expected_value: format!(
                "{}h-{}h",
                profile.temporal.active_hour_start, profile.temporal.active_hour_end
            ),
            observed_value: format!("{hour}h"),
            deviation_sigma: 3.0,
        });
    }

    let total = event.bytes_sent + event.bytes_recv;
    let avg = profile.temporal.avg_daily_volume;
    if avg > 0 && total > avg * 3 {
        let ratio = total as f64 / avg as f64;
        details.push(MutationDetail {
            dimension: MutationDimension::Volumetric,
            description: format!("Volume transféré {ratio:.1}x supérieur à la moyenne"),
            expected_value: format!("{avg} octets/jour"),
            observed_value: format!("{total} octets"),
            deviation_sigma: ratio,
        });
    }

    if innate_score > 0.3 {
        details.push(MutationDetail {
            dimension: MutationDimension::Technical,
            description: format!(
                "Règle innée déclenchée (port {}, processus {})",
                event.dest_port, event.process_name
            ),
            expected_value: "aucun indicateur inné".to_string(),
            observed_value: format!("score inné {innate_score:.2}"),
            deviation_sigma: innate_score * 5.0,
        });
    }

    details
}

pub fn affected_dimensions(
    event: &NetworkEvent,
    profile: &MachineIdentity,
) -> Vec<MutationDimension> {
    let mut dims = Vec::new();

    if !profile.is_known_peer(&event.dest_ip) {
        dims.push(MutationDimension::Relational);
    }

    let hour = event
        .timestamp
        .format("%H")
        .to_string()
        .parse::<u8>()
        .unwrap_or(0);
    if !profile.is_within_active_hours(hour) {
        dims.push(MutationDimension::Temporal);
    }

    let total = event.bytes_sent + event.bytes_recv;
    if profile.temporal.avg_daily_volume > 0 && total > profile.temporal.avg_daily_volume * 3 {
        dims.push(MutationDimension::Volumetric);
    }

    dims
}
