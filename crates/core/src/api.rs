use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thymus_common::{EventBatch, MutationStatus, ToleranceContext, ToleranceEntry};
use tokio::sync::RwLock;

use crate::db::Db;
use crate::state::{AppState, Phase};

#[derive(Clone)]
pub struct CoreState {
    pub app: Arc<RwLock<AppState>>,
    pub db: Arc<Db>,
    pub token: Option<String>,
}

pub fn router(state: CoreState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/events", post(ingest_events))
        .route("/api/mutations", get(list_mutations))
        .route("/api/mutations/{id}/resolve", post(resolve_mutation))
        .route(
            "/api/mutations/{id}/false-positive",
            post(false_positive_mutation),
        )
        .route("/api/profiles", get(list_profiles))
        .route("/api/chains", get(list_chains))
        .route("/api/tolerances", get(list_tolerances))
        .route("/api/context", post(add_context))
        .route("/api/activate", post(activate))
        .route("/api/login", post(login))
        .with_state(state)
}

#[derive(Deserialize)]
struct LoginRequest {
    token: String,
}

async fn login(State(state): State<CoreState>, Json(req): Json<LoginRequest>) -> Response {
    use axum::http::header;
    use axum::response::IntoResponse;

    match state.token {
        Some(ref expected) if &req.token == expected => (
            StatusCode::OK,
            [(header::SET_COOKIE, crate::auth::session_cookie(&req.token))],
            "ok",
        )
            .into_response(),
        _ => (StatusCode::UNAUTHORIZED, "invalid token").into_response(),
    }
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

async fn resolve_mutation(State(state): State<CoreState>, Path(id): Path<String>) -> StatusCode {
    let mut s = state.app.write().await;
    let idx = s.mutations.iter().position(|m| m.id.to_string() == id);
    if let Some(idx) = idx {
        s.mutations[idx].status = MutationStatus::Resolved;
        let resolved = s.mutations[idx].clone();
        s.memory.learn_from_resolved(&resolved);
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn false_positive_mutation(
    State(state): State<CoreState>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut s = state.app.write().await;
    let idx = s.mutations.iter().position(|m| m.id.to_string() == id);
    if let Some(idx) = idx {
        s.mutations[idx].status = MutationStatus::FalsePositive;
        let m = &s.mutations[idx];
        let dest_ip = m
            .details
            .first()
            .and_then(|d| d.observed_value.split_whitespace().last())
            .map(String::from);
        let tolerance = ToleranceEntry::from_false_positive(
            &m.machine_id,
            m.dimensions.clone(),
            m.risk_score,
            dest_ip,
            None,
        );
        tracing::info!(tolerance_id = %tolerance.id, "tolerance created from false positive");
        s.tolerances.push(tolerance);
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn list_tolerances(State(state): State<CoreState>) -> Json<serde_json::Value> {
    let s = state.app.read().await;
    Json(serde_json::to_value(&s.tolerances).unwrap_or_default())
}

#[derive(Deserialize)]
struct ContextRequest {
    context_type: String,
    affected_machines: Vec<String>,
    duration_hours: i64,
    description: String,
}

async fn add_context(
    State(state): State<CoreState>,
    Json(req): Json<ContextRequest>,
) -> StatusCode {
    let mut s = state.app.write().await;
    let ctx = ToleranceContext {
        id: uuid::Uuid::new_v4(),
        context_type: req.context_type,
        affected_machines: req.affected_machines,
        start: chrono::Utc::now(),
        end: chrono::Utc::now() + chrono::Duration::hours(req.duration_hours),
        description: req.description,
    };
    tracing::info!(context = %ctx.context_type, hours = req.duration_hours, "context declared");
    s.contexts.push(ctx);
    StatusCode::CREATED
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

#[derive(Serialize)]
struct ChainResponse {
    id: String,
    path: Vec<String>,
    path_str: String,
    chain_score: f64,
    detected_at: String,
    links: usize,
}

async fn list_chains(State(state): State<CoreState>) -> Json<Vec<ChainResponse>> {
    let s = state.app.read().await;
    Json(
        s.active_chains()
            .iter()
            .map(|c| ChainResponse {
                id: c.id.to_string(),
                path: c.path.iter().map(|l| l.machine_id.clone()).collect(),
                path_str: c.path_str(),
                chain_score: c.chain_score,
                detected_at: c.detected_at.to_rfc3339(),
                links: c.path.len(),
            })
            .collect(),
    )
}
