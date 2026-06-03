use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{MutationDimension, ResponseAction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCell {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub mutation_dimensions: Vec<MutationDimension>,
    pub risk_range: (f64, f64),
    pub innate_triggered: bool,
    pub effective_response: Option<ResponseAction>,
    pub times_matched: u32,
    pub true_matches: u32,
    pub false_matches: u32,
    pub source: MemorySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySource {
    Local,
    Collective,
    Vaccination,
}

impl MemoryCell {
    pub fn from_resolved_mutation(
        dimensions: Vec<MutationDimension>,
        risk_score: f64,
        innate_triggered: bool,
        response: Option<ResponseAction>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            mutation_dimensions: dimensions,
            risk_range: ((risk_score - 0.15).max(0.0), (risk_score + 0.15).min(1.0)),
            innate_triggered,
            effective_response: response,
            times_matched: 0,
            true_matches: 0,
            false_matches: 0,
            source: MemorySource::Local,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn matches(&self, dimensions: &[MutationDimension], risk_score: f64, innate: bool) -> f64 {
        let dim_overlap = self
            .mutation_dimensions
            .iter()
            .filter(|d| dimensions.contains(d))
            .count();

        if dim_overlap == 0 {
            return 0.0;
        }

        let dim_score = dim_overlap as f64 / self.mutation_dimensions.len().max(1) as f64;

        let risk_in_range = risk_score >= self.risk_range.0 && risk_score <= self.risk_range.1;
        let risk_score_factor = if risk_in_range { 1.0 } else { 0.5 };

        let innate_factor = if self.innate_triggered == innate {
            1.0
        } else {
            0.7
        };

        dim_score * risk_score_factor * innate_factor
    }

    pub fn effectiveness(&self) -> f64 {
        let total = self.true_matches + self.false_matches;
        if total == 0 {
            return 0.5;
        }
        f64::from(self.true_matches) / f64::from(total)
    }
}
