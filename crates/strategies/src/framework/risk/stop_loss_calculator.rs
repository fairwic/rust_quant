#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopLossSide {
    Long,
    Short,
}

pub struct StopLossCalculator;

impl StopLossCalculator {
    /// Select the tightest valid stop-loss from candidates.
    ///
    /// - Long: highest candidate strictly below entry
    /// - Short: lowest candidate strictly above entry
    pub fn select(side: StopLossSide, entry_price: f64, candidates: &[f64]) -> Option<f64> {
        let mut selected: Option<f64> = None;

        for &candidate in candidates {
            if candidate.is_nan() {
                continue;
            }

            match side {
                StopLossSide::Long => {
                    if candidate < entry_price {
                        selected = Some(match selected {
                            Some(prev) => prev.max(candidate),
                            None => candidate,
                        });
                    }
                }
                StopLossSide::Short => {
                    if candidate > entry_price {
                        selected = Some(match selected {
                            Some(prev) => prev.min(candidate),
                            None => candidate,
                        });
                    }
                }
            }
        }

        selected
    }
}

#[cfg(test)]
mod tests {
    use super::{StopLossCalculator, StopLossSide};

    #[test]
    fn select_tightest_long_stop_from_candidates() {
        let entry = 100.0;
        let candidates = vec![95.0, 97.0, 90.0];
        let selected = StopLossCalculator::select(StopLossSide::Long, entry, &candidates);
        assert_eq!(selected, Some(97.0));
    }

    #[test]
    fn select_tightest_short_stop_from_candidates() {
        let entry = 100.0;
        let candidates = vec![105.0, 103.0, 110.0];
        let selected = StopLossCalculator::select(StopLossSide::Short, entry, &candidates);
        assert_eq!(selected, Some(103.0));
    }

    #[test]
    fn ignores_invalid_candidates() {
        let entry = 100.0;
        let candidates = vec![100.0, 101.0];
        let selected = StopLossCalculator::select(StopLossSide::Long, entry, &candidates);
        assert_eq!(selected, None);
    }
}
