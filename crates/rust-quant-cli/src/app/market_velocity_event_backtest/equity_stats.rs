#[derive(Debug, Clone, PartialEq)]
pub(super) struct ClosedTradeStats {
    pub(super) wins: usize,
    pub(super) losses: usize,
    pub(super) returns: Vec<f64>,
    pub(super) max_drawdown_pct: f64,
}

pub(super) fn analyze_profit_losses(
    profit_losses: impl IntoIterator<Item = f64>,
    initial_equity: f64,
) -> ClosedTradeStats {
    let mut wins = 0;
    let mut losses = 0;
    let mut equity = initial_equity;
    let mut peak = initial_equity;
    let mut max_drawdown_pct = 0.0;
    let mut returns = Vec::new();

    for profit_loss in profit_losses {
        if profit_loss > 0.0 {
            wins += 1;
        } else if profit_loss < 0.0 {
            losses += 1;
        }
        if equity > 0.0 {
            returns.push(profit_loss / equity);
        }
        equity += profit_loss;
        peak = peak.max(equity);
        if peak > 0.0 {
            let drawdown_pct = (peak - equity) / peak * 100.0;
            if drawdown_pct > max_drawdown_pct {
                max_drawdown_pct = drawdown_pct;
            }
        }
    }

    ClosedTradeStats {
        wins,
        losses,
        returns,
        max_drawdown_pct,
    }
}

pub(super) fn trade_sharpe(returns: &[f64]) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let stddev = sample_stddev(returns, mean);
    (stddev > 0.0).then_some(mean / stddev * (returns.len() as f64).sqrt())
}

fn sample_stddev(values: &[f64], mean: f64) -> f64 {
    let variance = values
        .iter()
        .map(|value| {
            let diff = value - mean;
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}

pub(super) fn format_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| {
            if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                format!("{value}")
            }
        })
        .unwrap_or_else(|| "NA".to_string())
}
