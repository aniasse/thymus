use thymos_common::MemoryCell;

pub struct ClonalSelection;

impl ClonalSelection {
    pub fn optimize(cells: &mut Vec<MemoryCell>) {
        let mut amplified = 0u32;
        let mut attenuated = 0u32;
        let mut eliminated = 0u32;

        cells.retain_mut(|cell| {
            if cell.times_matched < 5 {
                return true;
            }

            let eff = cell.effectiveness();

            if eff > 0.8 {
                // Amplify: widen the risk range to catch more variants
                cell.risk_range.0 = (cell.risk_range.0 - 0.05).max(0.0);
                cell.risk_range.1 = (cell.risk_range.1 + 0.05).min(1.0);
                amplified += 1;
                true
            } else if eff > 0.4 {
                // Keep as-is
                true
            } else if eff > 0.2 {
                // Attenuate: narrow the risk range
                cell.risk_range.0 = (cell.risk_range.0 + 0.05).min(1.0);
                cell.risk_range.1 = (cell.risk_range.1 - 0.05).max(0.0);
                attenuated += 1;
                true
            } else {
                // Eliminate
                eliminated += 1;
                false
            }
        });

        if amplified > 0 || attenuated > 0 || eliminated > 0 {
            tracing::info!(
                amplified,
                attenuated,
                eliminated,
                remaining = cells.len(),
                "clonal selection completed"
            );
        }
    }
}
