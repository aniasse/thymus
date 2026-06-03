use askama::Template;
use axum::{Router, extract::State, response::Html, routing::get};

use crate::api::CoreState;

pub fn router() -> Router<CoreState> {
    Router::new()
        .route("/", get(status_page))
        .route("/mutations", get(mutations_page))
        .route("/machines", get(machines_page))
        .route("/partials/status-cards", get(partial_status_cards))
        .route("/partials/profiles-summary", get(partial_profiles_summary))
        .route("/partials/recent-mutations", get(partial_recent_mutations))
        .route("/partials/mutations-full", get(partial_mutations_full))
        .route("/partials/machines-list", get(partial_machines_list))
}

// --- Full pages ---

#[derive(Template)]
#[template(path = "status.html")]
struct StatusPage;

async fn status_page() -> Html<String> {
    Html(StatusPage.render().unwrap_or_default())
}

#[derive(Template)]
#[template(path = "mutations.html")]
struct MutationsPage;

async fn mutations_page() -> Html<String> {
    Html(MutationsPage.render().unwrap_or_default())
}

#[derive(Template)]
#[template(path = "machines.html")]
struct MachinesPage;

async fn machines_page() -> Html<String> {
    Html(MachinesPage.render().unwrap_or_default())
}

// --- Partials (HTMX) ---

#[derive(Template)]
#[template(path = "partials/status_cards.html")]
struct StatusCardsPartial {
    phase: String,
    machines: usize,
    total_events: u64,
    active_mutations: usize,
}

async fn partial_status_cards(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let tmpl = StatusCardsPartial {
        phase: match s.phase {
            crate::state::Phase::Thymus => "thymus".into(),
            crate::state::Phase::Active => "active".into(),
        },
        machines: s.profiles.len(),
        total_events: s.event_count,
        active_mutations: s.active_mutations().len(),
    };
    Html(tmpl.render().unwrap_or_default())
}

struct ProfileRow {
    hostname: String,
    peer_count: usize,
    maturity_pct: u8,
}

#[derive(Template)]
#[template(path = "partials/profiles_summary.html")]
struct ProfilesSummaryPartial {
    profiles: Vec<ProfileRow>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
async fn partial_profiles_summary(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let profiles: Vec<ProfileRow> = s
        .profiles
        .values()
        .map(|p| ProfileRow {
            hostname: p.hostname.clone(),
            peer_count: p.relational.known_peers.len(),
            maturity_pct: (p.profile_maturity * 100.0) as u8,
        })
        .collect();
    Html(
        ProfilesSummaryPartial { profiles }
            .render()
            .unwrap_or_default(),
    )
}

struct MutationRow {
    id: String,
    machine_id: String,
    risk_score: f64,
    risk_pct: u8,
    dimensions: String,
    description: String,
    detected_at: String,
    details: Vec<MutationDetailRow>,
}

struct MutationDetailRow {
    dimension: String,
    description: String,
    expected: String,
    observed: String,
}

#[derive(Template)]
#[template(path = "partials/recent_mutations.html")]
struct RecentMutationsPartial {
    mutations: Vec<MutationRow>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn mutation_to_row(m: &thymos_common::Mutation) -> MutationRow {
    MutationRow {
        id: m.id.to_string(),
        machine_id: m.machine_id.clone(),
        risk_score: m.risk_score,
        risk_pct: (m.risk_score * 100.0) as u8,
        dimensions: m
            .dimensions
            .iter()
            .map(|d| format!("{d:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        description: m
            .details
            .first()
            .map_or(String::new(), |d| d.description.clone()),
        detected_at: m.detected_at.format("%H:%M:%S").to_string(),
        details: m
            .details
            .iter()
            .map(|d| MutationDetailRow {
                dimension: format!("{:?}", d.dimension),
                description: d.description.clone(),
                expected: d.expected_value.clone(),
                observed: d.observed_value.clone(),
            })
            .collect(),
    }
}

async fn partial_recent_mutations(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let mutations: Vec<MutationRow> = s
        .active_mutations()
        .iter()
        .take(5)
        .map(|m| mutation_to_row(m))
        .collect();
    Html(
        RecentMutationsPartial { mutations }
            .render()
            .unwrap_or_default(),
    )
}

#[derive(Template)]
#[template(path = "partials/mutations_full.html")]
struct MutationsFullPartial {
    mutations: Vec<MutationRow>,
}

async fn partial_mutations_full(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let mutations: Vec<MutationRow> = s
        .active_mutations()
        .iter()
        .map(|m| mutation_to_row(m))
        .collect();
    Html(
        MutationsFullPartial { mutations }
            .render()
            .unwrap_or_default(),
    )
}

struct MachineRow {
    hostname: String,
    peer_count: usize,
    maturity_pct: u8,
    active_hours: String,
    daily_volume: String,
    observation_days: u32,
    peers: Vec<PeerRow>,
}

struct PeerRow {
    ip: String,
    ports: String,
    connections: String,
    confidence_pct: u8,
}

#[derive(Template)]
#[template(path = "partials/machines_list.html")]
struct MachinesListPartial {
    machines: Vec<MachineRow>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
async fn partial_machines_list(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let machines: Vec<MachineRow> = s
        .profiles
        .values()
        .map(|p| {
            let peers: Vec<PeerRow> = p
                .relational
                .known_peers
                .iter()
                .take(20)
                .map(|peer| PeerRow {
                    ip: peer.peer_ip.to_string(),
                    ports: peer
                        .ports
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    connections: format!("{:.0}", peer.avg_daily_connections),
                    confidence_pct: (peer.confidence * 100.0) as u8,
                })
                .collect();

            let vol = p.temporal.avg_daily_volume;
            let daily_volume = if vol > 1_000_000_000 {
                format!("{:.1} Go", vol as f64 / 1_000_000_000.0)
            } else if vol > 1_000_000 {
                format!("{:.1} Mo", vol as f64 / 1_000_000.0)
            } else if vol > 1_000 {
                format!("{:.1} Ko", vol as f64 / 1_000.0)
            } else {
                format!("{vol} o")
            };

            MachineRow {
                hostname: p.hostname.clone(),
                peer_count: p.relational.known_peers.len(),
                maturity_pct: (p.profile_maturity * 100.0) as u8,
                active_hours: format!(
                    "{}h-{}h",
                    p.temporal.active_hour_start, p.temporal.active_hour_end
                ),
                daily_volume,
                observation_days: p.observation_days,
                peers,
            }
        })
        .collect();
    Html(
        MachinesListPartial { machines }
            .render()
            .unwrap_or_default(),
    )
}
