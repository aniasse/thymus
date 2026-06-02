use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub id: Uuid,
    pub detected_at: DateTime<Utc>,
    pub machine_id: String,
    pub dimensions: Vec<MutationDimension>,
    pub risk_score: f64,
    pub innate_score: f64,
    pub adaptive_score: f64,
    pub details: Vec<MutationDetail>,
    pub status: MutationStatus,
    pub response: Option<ResponseAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationDimension {
    Technical,
    Relational,
    Temporal,
    Volumetric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationDetail {
    pub dimension: MutationDimension,
    pub description: String,
    pub expected_value: String,
    pub observed_value: String,
    pub deviation_sigma: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationStatus {
    Active,
    Investigating,
    Resolved,
    FalsePositive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseAction {
    Monitor,
    ThrottleBandwidth,
    BlockNewConnections,
    Isolate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResponseLevel {
    None = 0,
    Monitor = 1,
    Throttle = 2,
    Block = 3,
    Isolate = 4,
}

impl Mutation {
    pub fn new(machine_id: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            detected_at: Utc::now(),
            machine_id,
            dimensions: Vec::new(),
            risk_score: 0.0,
            innate_score: 0.0,
            adaptive_score: 0.0,
            details: Vec::new(),
            status: MutationStatus::Active,
            response: None,
        }
    }

    pub fn response_level(&self) -> ResponseLevel {
        match self.risk_score {
            s if s > 0.95 => ResponseLevel::Isolate,
            s if s > 0.8 => ResponseLevel::Block,
            s if s > 0.6 => ResponseLevel::Throttle,
            s if s > 0.4 => ResponseLevel::Monitor,
            _ => ResponseLevel::None,
        }
    }
}
