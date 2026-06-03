use thymos_common::{MemoryCell, Mutation, ResponseAction};

const MATCH_THRESHOLD: f64 = 0.6;

pub struct ImmuneMemory {
    cells: Vec<MemoryCell>,
}

pub struct MemoryMatch {
    pub cell_id: uuid::Uuid,
    pub similarity: f64,
    pub suggested_response: Option<ResponseAction>,
}

impl ImmuneMemory {
    pub fn new() -> Self {
        Self { cells: Vec::new() }
    }

    pub fn load(cells: Vec<MemoryCell>) -> Self {
        Self { cells }
    }

    pub fn cells(&self) -> &[MemoryCell] {
        &self.cells
    }

    pub fn take_cells(&mut self) -> Vec<MemoryCell> {
        std::mem::take(&mut self.cells)
    }

    pub fn replace_cells(&mut self, cells: Vec<MemoryCell>) {
        self.cells = cells;
    }

    pub fn consult(&self, mutation: &Mutation) -> Option<MemoryMatch> {
        let mut best_match: Option<(usize, f64)> = None;

        for (i, cell) in self.cells.iter().enumerate() {
            let similarity = cell.matches(
                &mutation.dimensions,
                mutation.risk_score,
                mutation.innate_score > 0.3,
            );

            if similarity >= MATCH_THRESHOLD {
                if let Some((_, best_sim)) = best_match {
                    if similarity > best_sim {
                        best_match = Some((i, similarity));
                    }
                } else {
                    best_match = Some((i, similarity));
                }
            }
        }

        best_match.map(|(i, similarity)| MemoryMatch {
            cell_id: self.cells[i].id,
            similarity,
            suggested_response: self.cells[i].effective_response,
        })
    }

    pub fn learn_from_resolved(&mut self, mutation: &Mutation) {
        let cell = MemoryCell::from_resolved_mutation(
            mutation.dimensions.clone(),
            mutation.risk_score,
            mutation.innate_score > 0.3,
            mutation.response,
        );

        tracing::info!(
            cell_id = %cell.id,
            dimensions = ?cell.mutation_dimensions,
            "new memory cell created from resolved mutation"
        );

        self.cells.push(cell);
    }

    pub fn record_match(&mut self, cell_id: uuid::Uuid, was_true_positive: bool) {
        if let Some(cell) = self.cells.iter_mut().find(|c| c.id == cell_id) {
            cell.times_matched += 1;
            if was_true_positive {
                cell.true_matches += 1;
            } else {
                cell.false_matches += 1;
            }
        }
    }

    pub fn prune_ineffective(&mut self) {
        self.cells
            .retain(|c| c.times_matched < 5 || c.effectiveness() > 0.2);
    }
}

impl Default for ImmuneMemory {
    fn default() -> Self {
        Self::new()
    }
}
