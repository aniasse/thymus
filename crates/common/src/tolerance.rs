use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::MutationDimension;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToleranceEntry {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub machine_id: Option<String>,
    pub dimensions: Vec<MutationDimension>,
    pub dest_ip: Option<String>,
    pub dest_port: Option<u16>,
    pub min_score: f64,
    pub max_score: f64,
    pub source: ToleranceSource,
    pub expires_at: Option<DateTime<Utc>>,
    pub hits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToleranceSource {
    FalsePositive,
    Manual,
    Context,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToleranceContext {
    pub id: Uuid,
    pub context_type: String,
    pub affected_machines: Vec<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub description: String,
}

impl ToleranceEntry {
    pub fn from_false_positive(
        machine_id: &str,
        dimensions: Vec<MutationDimension>,
        risk_score: f64,
        dest_ip: Option<String>,
        dest_port: Option<u16>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            machine_id: Some(machine_id.to_string()),
            dimensions,
            dest_ip,
            dest_port,
            min_score: (risk_score - 0.2).max(0.0),
            max_score: (risk_score + 0.2).min(1.0),
            source: ToleranceSource::FalsePositive,
            expires_at: None,
            hits: 0,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Utc::now() > exp)
    }

    pub fn matches(
        &self,
        machine_id: &str,
        dimensions: &[MutationDimension],
        risk_score: f64,
        dest_ip: Option<&str>,
    ) -> bool {
        if self.is_expired() {
            return false;
        }

        if let Some(ref mid) = self.machine_id
            && mid != machine_id
        {
            return false;
        }

        if risk_score < self.min_score || risk_score > self.max_score {
            return false;
        }

        let dim_match = self.dimensions.iter().any(|d| dimensions.contains(d));
        if !self.dimensions.is_empty() && !dim_match {
            return false;
        }

        if let Some(ref tolerated_ip) = self.dest_ip
            && let Some(event_ip) = dest_ip
            && tolerated_ip != event_ip
        {
            return false;
        }

        true
    }
}

impl ToleranceContext {
    pub fn is_active(&self) -> bool {
        let now = Utc::now();
        now >= self.start && now <= self.end
    }

    pub fn affects_machine(&self, machine_id: &str) -> bool {
        self.affected_machines.is_empty() || self.affected_machines.iter().any(|m| m == machine_id)
    }
}
