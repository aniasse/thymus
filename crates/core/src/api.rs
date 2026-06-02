use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::Serialize;
use std::sync::Arc;
use thymos_common::EventBatch;
use tokio::sync::RwLock;

use crate::db::Db;
use crate::state::{AppState, Phase};

#[derive(Clone)]
pub struct CoreState {
    pub app: Arc<RwLock<AppState>>,
    pub db: Arc<Db>,
}

pub fn router(app: Arc<RwLock<AppState>>, db: Arc<Db>) -> Router {
    let state = CoreState { app, db };
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
}

async fn status(State(state): State<CoreState>) -> Json<StatusResponse> {
    let s = state.app.read().await;
    Json(StatusResponse {
        phase: match s.phase {
            Phase::Thymus => "thymus".to_string(),
            Phase::Active => "active".to_string(),
        },
        machines: s.profiles.len(),
        total_events: s.event_count,
        active_mutations: s.active_mutations().len(),
    })
}

async fn ingest_events(
    State(state): State<CoreState>,
    Json(batch): Json<EventBatch>,
) -> StatusCode {
    let count = batch.event_count();
    let sensor = batch.sensor_id.clone();

    let mut s = state.app.write().await;
    s.ingest_batch(&batch);

    if s.should_save() {
        s.save_to_db(&state.db);
    }

    drop(s);
    tracing::info!(sensor = %sensor, events = count, "ingested");
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

async fn list_mutations(State(state): State<CoreState>) -> Json<Vec<MutationResponse>> {
    let s = state.app.read().await;
    let mutations = s
        .active_mutations()
        .iter()
        .map(|m| MutationResponse {
            id: m.id.to_string(),
            machine_id: m.machine_id.clone(),
            risk_score: m.risk_score,
            dimensions: m.dimensions.iter().map(|d| format!("{d:?}")).collect(),
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
        .collect();
    Json(mutations)
}

async fn list_profiles(State(state): State<CoreState>) -> Json<serde_json::Value> {
    let s = state.app.read().await;
    Json(serde_json::to_value(&s.profiles).unwrap_or_default())
}

async fn activate(State(state): State<CoreState>) -> &'static str {
    let mut s = state.app.write().await;
    s.activate();
    s.save_to_db(&state.db);
    "activated"
}
