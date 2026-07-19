use super::mark_to_market::EquityPoint;
use std::collections::BTreeMap;

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

/// 以 UTC 自然日收盘权益计算的风险调整指标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DailyEquityMetrics {
    pub(super) observations: usize,
    pub(super) annualized_sharpe_sqrt_365: Option<f64>,
}

/// 将不连续的 4H 权益点补齐为日历日序列，并按 `sqrt(365)` 年化 Sharpe。
pub(super) fn daily_equity_metrics(
    initial_equity: f64,
    points: &[EquityPoint],
) -> DailyEquityMetrics {
    if points.is_empty() || !initial_equity.is_finite() || initial_equity <= 0.0 {
        return DailyEquityMetrics {
            observations: 0,
            annualized_sharpe_sqrt_365: None,
        };
    }

    let mut day_closes = BTreeMap::<i64, (i64, f64)>::new();
    for point in points
        .iter()
        .filter(|point| point.equity.is_finite() && point.equity > 0.0)
    {
        let day = point.ts.div_euclid(DAY_MS);
        let entry = day_closes.entry(day).or_insert((point.ts, point.equity));
        if point.ts >= entry.0 {
            *entry = (point.ts, point.equity);
        }
    }
    let Some((&first_day, _)) = day_closes.first_key_value() else {
        return DailyEquityMetrics {
            observations: 0,
            annualized_sharpe_sqrt_365: None,
        };
    };
    let last_day = *day_closes.last_key_value().expect("non-empty day map").0;
    let mut previous_equity = initial_equity;
    let mut last_equity = initial_equity;
    let mut returns = Vec::with_capacity((last_day - first_day + 1) as usize);
    for day in first_day..=last_day {
        if let Some((_, equity)) = day_closes.get(&day) {
            last_equity = *equity;
        }
        returns.push(last_equity / previous_equity - 1.0);
        previous_equity = last_equity;
    }

    DailyEquityMetrics {
        observations: returns.len(),
        annualized_sharpe_sqrt_365: sample_sharpe(&returns).map(|value| value * 365.0_f64.sqrt()),
    }
}

fn sample_sharpe(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (returns.len() - 1) as f64;
    (variance > 0.0).then_some(mean / variance.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_missing_calendar_days_with_flat_equity() {
        let points = vec![
            EquityPoint {
                ts: 0,
                equity: 101.0,
            },
            EquityPoint {
                ts: DAY_MS * 2,
                equity: 102.01,
            },
        ];

        let metrics = daily_equity_metrics(100.0, &points);

        assert_eq!(metrics.observations, 3);
        assert!(metrics.annualized_sharpe_sqrt_365.is_some());
    }

    #[test]
    fn flat_equity_has_no_finite_sharpe() {
        let points = vec![
            EquityPoint {
                ts: 0,
                equity: 100.0,
            },
            EquityPoint {
                ts: DAY_MS,
                equity: 100.0,
            },
        ];

        let metrics = daily_equity_metrics(100.0, &points);

        assert_eq!(metrics.observations, 2);
        assert_eq!(metrics.annualized_sharpe_sqrt_365, None);
    }
}
