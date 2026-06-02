use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use thymos_common::EventBatch;

use crate::state::{AppState, Phase};

type SharedState = Arc<RwLock<AppState>>;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/events", post(ingest_events))
        .route("/api/mutations", get(list_mutations))
        .route("/api/profiles", get(list_profiles))
        .route("/api/activate", post(activate))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct StatusResponse {
    phase: String,
    machines: usize,
    total_events: u64,
    active_mutations: usize,
    profiles: Vec<ProfileSummary>,
}

#[derive(Serialize)]
struct ProfileSummary {
    machine_id: String,
    hostname: String,
    known_peers: usize,
    maturity: f64,
}

async fn status(State(state): State<SharedState>) -> Json<StatusResponse> {
    let state = state.read().await;
    Json(StatusResponse {
        phase: match state.phase {
            Phase::Thymus => "thymus".to_string(),
            Phase::Active => "active".to_string(),
        },
        machines: state.profiles.len(),
        total_events: state.event_count,
        active_mutations: state.active_mutations().len(),
        profiles: state
            .profiles
            .values()
            .map(|p| ProfileSummary {
                machine_id: p.machine_id.clone(),
                hostname: p.hostname.clone(),
                known_peers: p.relational.known_peers.len(),
                maturity: p.profile_maturity,
            })
            .collect(),
    })
}

async fn ingest_events(
    State(state): State<SharedState>,
    Json(batch): Json<EventBatch>,
) -> StatusCode {
    let event_count = batch.event_count();
    let sensor = batch.sensor_id.clone();

    let mut state = state.write().await;
    state.ingest_batch(batch);

    tracing::info!(sensor = %sensor, events = event_count, "Batch ingested");
    StatusCode::ACCEPTED
}

#[derive(Serialize)]
struct MutationResponse {
    id: String,
    machine_id: String,
    risk_score: f64,
    dimensions: Vec<String>,
    detected_at: String,
    details: Vec<DetailResponse>,
}

#[derive(Serialize)]
struct DetailResponse {
    dimension: String,
    description: String,
    expected: String,
    observed: String,
}

async fn list_mutations(State(state): State<SharedState>) -> Json<Vec<MutationResponse>> {
    let state = state.read().await;
    Json(
        state
            .active_mutations()
            .iter()
            .map(|m| MutationResponse {
                id: m.id.to_string(),
                machine_id: m.machine_id.clone(),
                risk_score: m.risk_score,
                dimensions: m.dimensions.iter().map(|d| format!("{:?}", d)).collect(),
                detected_at: m.detected_at.to_rfc3339(),
                details: m
                    .details
                    .iter()
                    .map(|d| DetailResponse {
                        dimension: format!("{:?}", d.dimension),
                        description: d.description.clone(),
                        expected: d.expected_value.clone(),
                        observed: d.observed_value.clone(),
                    })
                    .collect(),
            })
            .collect(),
    )
}

async fn list_profiles(State(state): State<SharedState>) -> Json<serde_json::Value> {
    let state = state.read().await;
    Json(serde_json::to_value(&state.profiles).unwrap_or_default())
}

async fn activate(State(state): State<SharedState>) -> &'static str {
    let mut state = state.write().await;
    state.activate();
    "Immune detection activated"
}
