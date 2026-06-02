pub mod innate;
pub mod adaptive;
pub mod scoring;

use thymos_common::{MachineIdentity, Mutation, NetworkEvent};

pub struct ImmuneEngine {
    innate: innate::InnateLayer,
    adaptive: adaptive::AdaptiveLayer,
}

impl ImmuneEngine {
    pub fn new() -> Self {
        Self {
            innate: innate::InnateLayer::new(),
            adaptive: adaptive::AdaptiveLayer::new(),
        }
    }

    pub fn analyze_network_event(
        &self,
        event: &NetworkEvent,
        profile: &MachineIdentity,
    ) -> Option<Mutation> {
        let innate_score = self.innate.evaluate_network(event, profile);
        let adaptive_score = self.adaptive.evaluate_network(event, profile);

        let combined = scoring::combine_scores(innate_score, adaptive_score);

        if combined > 0.4 {
            let mut mutation = Mutation::new(profile.machine_id.clone());
            mutation.innate_score = innate_score;
            mutation.adaptive_score = adaptive_score;
            mutation.risk_score = combined;
            mutation.details = scoring::build_details(event, profile, innate_score, adaptive_score);
            mutation.dimensions = scoring::affected_dimensions(event, profile);
            Some(mutation)
        } else {
            None
        }
    }
}

impl Default for ImmuneEngine {
    fn default() -> Self {
        Self::new()
    }
}
