//! 回测性能指标

/// 夏普比率计算
pub fn calculate_sharpe_ratio(returns: &[f64], risk_free_rate: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|r| (r - mean_return).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        return 0.0;
    }

    (mean_return - risk_free_rate) / std_dev
}

/// 最大回撤计算
pub fn calculate_max_drawdown(equity_curve: &[f64]) -> f64 {
    if equity_curve.is_empty() {
        return 0.0;
    }

    let mut max_drawdown = 0.0;
    let mut peak = equity_curve[0];

    for &equity in equity_curve.iter() {
        if equity > peak {
            peak = equity;
        }
        let drawdown = (peak - equity) / peak;
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
        }
    }

    max_drawdown
}

/// 胜率计算
pub fn calculate_win_rate(trades: &[(f64, bool)]) -> f64 {
    if trades.is_empty() {
        return 0.0;
    }

    let wins = trades.iter().filter(|(_, is_win)| *is_win).count();
    wins as f64 / trades.len() as f64
}
