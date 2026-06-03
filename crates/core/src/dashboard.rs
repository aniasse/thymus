use askama::Template;
use axum::{Router, extract::State, response::Html, routing::get};

use crate::api::CoreState;

pub fn router() -> Router<CoreState> {
    Router::new()
        .route("/", get(status_page))
        .route("/login", get(login_page))
        .route("/mutations", get(mutations_page))
        .route("/machines", get(machines_page))
        .route("/network", get(network_page))
        .route("/partials/status-cards", get(partial_status_cards))
        .route("/partials/profiles-summary", get(partial_profiles_summary))
        .route("/partials/recent-mutations", get(partial_recent_mutations))
        .route("/partials/mutations-full", get(partial_mutations_full))
        .route("/partials/machines-list", get(partial_machines_list))
        .route("/partials/network-graph", get(partial_network_graph))
        .route("/partials/chains", get(partial_chains))
}

// --- Full pages ---

#[derive(Template)]
#[template(path = "status.html")]
struct StatusPage;

async fn status_page() -> Html<String> {
    Html(StatusPage.render().unwrap_or_default())
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage;

async fn login_page() -> Html<String> {
    Html(LoginPage.render().unwrap_or_default())
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

#[derive(Template)]
#[template(path = "network.html")]
struct NetworkPage;

async fn network_page() -> Html<String> {
    Html(NetworkPage.render().unwrap_or_default())
}

// --- Partials (HTMX) ---

#[derive(Template)]
#[template(path = "partials/status_cards.html")]
struct StatusCardsPartial {
    phase: String,
    machines: usize,
    agent_count: usize,
    passive_count: usize,
    total_events: u64,
    active_mutations: usize,
}

async fn partial_status_cards(State(state): State<CoreState>) -> Html<String> {
    use thymos_common::Discovery;
    let s = state.app.read().await;
    let passive_count = s
        .profiles
        .values()
        .filter(|p| p.discovery == Discovery::Passive)
        .count();
    let tmpl = StatusCardsPartial {
        phase: match s.phase {
            crate::state::Phase::Thymus => "thymus".into(),
            crate::state::Phase::Active => "active".into(),
        },
        machines: s.profiles.len(),
        agent_count: s.profiles.len() - passive_count,
        passive_count,
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
    is_passive: bool,
    kind: String,
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

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
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
                is_passive: p.discovery == thymos_common::Discovery::Passive,
                kind: p.device_kind().to_string(),
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

// --- Network Graph ---

struct GraphNode {
    label: String,
    x: u32,
    y: u32,
    y_label: u32,
    y_sub: u32,
    kind: String,
    has_mutation: bool,
    is_passive: bool,
}

struct GraphEdge {
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    is_anomalous: bool,
}

struct ChainPath {
    points: String,
}

#[derive(Template)]
#[template(path = "partials/network_graph.html")]
struct NetworkGraphPartial {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    chain_paths: Vec<ChainPath>,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]
async fn partial_network_graph(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let machine_ids: Vec<String> = s.profiles.keys().cloned().collect();
    let machine_count = machine_ids.len();

    if machine_count == 0 {
        return Html(
            NetworkGraphPartial {
                nodes: vec![],
                edges: vec![],
                chain_paths: vec![],
            }
            .render()
            .unwrap_or_default(),
        );
    }

    let active_mutation_machines: std::collections::HashSet<String> = s
        .active_mutations()
        .iter()
        .map(|m| m.machine_id.clone())
        .collect();

    // Place nodes in a circle
    let cx = 400.0_f64;
    let cy = 250.0;
    let radius = 150.0;

    let mut node_positions: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();

    let nodes: Vec<GraphNode> = machine_ids
        .iter()
        .enumerate()
        .map(|(i, mid)| {
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / (machine_count as f64)
                - std::f64::consts::FRAC_PI_2;
            let x = (cx + radius * angle.cos()) as u32;
            let y = (cy + radius * angle.sin()) as u32;
            node_positions.insert(mid.clone(), (x, y));

            let profile = &s.profiles[mid];
            let label = if mid.len() > 16 {
                format!("{}…", &mid[..14])
            } else {
                mid.clone()
            };

            GraphNode {
                label,
                x,
                y,
                y_label: y + 4,
                y_sub: y + 36,
                kind: profile.device_kind().to_string(),
                has_mutation: active_mutation_machines.contains(mid),
                is_passive: profile.discovery == thymos_common::Discovery::Passive,
            }
        })
        .collect();

    // Build edges from shared peers
    let mut edges = Vec::new();
    let ids: Vec<&String> = machine_ids.iter().collect();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let p1 = &s.profiles[ids[i]];
            let p2 = &s.profiles[ids[j]];

            let shared = p1.relational.known_peers.iter().any(|peer| {
                p2.relational
                    .known_peers
                    .iter()
                    .any(|p| p.peer_ip == peer.peer_ip)
            });

            if shared {
                let (x1, y1) = node_positions[ids[i]];
                let (x2, y2) = node_positions[ids[j]];
                let is_anomalous = active_mutation_machines.contains(ids[i])
                    && active_mutation_machines.contains(ids[j]);
                edges.push(GraphEdge {
                    x1,
                    y1,
                    x2,
                    y2,
                    is_anomalous,
                });
            }
        }
    }

    // Chain paths
    let chain_paths: Vec<ChainPath> = s
        .active_chains()
        .iter()
        .filter_map(|chain| {
            let points: Vec<String> = chain
                .path
                .iter()
                .filter_map(|link| {
                    node_positions
                        .get(&link.machine_id)
                        .map(|(x, y)| format!("{x},{y}"))
                })
                .collect();

            if points.len() >= 2 {
                Some(ChainPath {
                    points: points.join(" "),
                })
            } else {
                None
            }
        })
        .collect();

    Html(
        NetworkGraphPartial {
            nodes,
            edges,
            chain_paths,
        }
        .render()
        .unwrap_or_default(),
    )
}

// --- Chains partial ---

struct ChainRow {
    path_str: String,
    score_pct: u8,
    links: usize,
    detected_at: String,
}

#[derive(Template)]
#[template(path = "partials/chains.html")]
struct ChainsPartial {
    chains: Vec<ChainRow>,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
async fn partial_chains(State(state): State<CoreState>) -> Html<String> {
    let s = state.app.read().await;
    let chains: Vec<ChainRow> = s
        .active_chains()
        .iter()
        .map(|c| ChainRow {
            path_str: c.path_str(),
            score_pct: (c.chain_score * 100.0) as u8,
            links: c.path.len(),
            detected_at: c.detected_at.format("%H:%M:%S").to_string(),
        })
        .collect();
    Html(ChainsPartial { chains }.render().unwrap_or_default())
}
