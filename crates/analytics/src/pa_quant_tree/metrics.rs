use super::ResearchObservation;
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// 成本后 R 序列的基础统计结果；组合回撤必须由共享组合权益序列单独传入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// 已结算样本数量。
    pub trade_count: usize,
    /// 成本后平均 R。
    pub mean_net_r: f64,
    /// 获胜交易占比。
    pub win_rate: f64,
    /// 总正 R 与总负 R 绝对值之比。
    pub profit_factor: f64,
    /// 以 R 表示的路径最大回撤。
    pub max_drawdown_r: f64,
    /// 总成本后 R。
    pub total_net_r: f64,
}

/// 同一 Vegas 原始候选两条路径的配对增量统计。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedPathDelta {
    /// 参与配对的 Vegas 候选数量。
    pub pair_count: usize,
    /// 过滤路径相对基线路径的平均 R 增量。
    pub mean_delta_r: f64,
    /// 正态近似 95% 下界；正式升级仍需 block bootstrap 复核。
    pub confidence_lower_95: f64,
}

/// 同一市场时间块内所有候选的成本后 R；重采样时必须作为整体保留横截面相关性。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedMarketTimeBlock {
    /// 时间块起点的 Unix 毫秒时间戳。
    pub start_ts: i64,
    /// 该时间块内多个币种/策略候选的成本后 R。
    pub net_r: Vec<f64>,
}

/// 共享市场时间块 bootstrap 的固定配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// 每次连续抽取的时间块数量。
    pub block_size: usize,
    /// Bootstrap 重采样次数。
    pub resamples: usize,
    /// 固定随机种子，保证证据报告可复现。
    pub seed: u64,
}

/// Bootstrap 均值分布和单侧下界。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootstrapMeanEstimate {
    /// 原始市场块全部候选的平均成本后 R。
    pub observed_mean_r: f64,
    /// 重采样均值的 5% 分位数，作为 95% 单侧下界。
    pub lower_bound_95: f64,
    /// 实际生成的重采样数量。
    pub resamples: usize,
}

/// 从按时间排序的净 R 序列计算研究指标。
pub fn calculate_metrics(net_r: &[f64]) -> PerformanceMetrics {
    let trade_count = net_r.len();
    let total_net_r: f64 = net_r.iter().sum();
    let mean_net_r = if trade_count == 0 {
        0.0
    } else {
        total_net_r / trade_count as f64
    };
    let wins = net_r.iter().filter(|value| **value > 0.0).count();
    let gross_profit: f64 = net_r.iter().filter(|value| **value > 0.0).sum();
    let gross_loss: f64 = net_r
        .iter()
        .filter(|value| **value < 0.0)
        .map(|value| value.abs())
        .sum();
    let profit_factor = if gross_loss == 0.0 {
        if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    } else {
        gross_profit / gross_loss
    };
    let mut equity: f64 = 0.0;
    let mut peak: f64 = 0.0;
    let mut max_drawdown: f64 = 0.0;
    for value in net_r {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    PerformanceMetrics {
        trade_count,
        mean_net_r,
        win_rate: if trade_count == 0 {
            0.0
        } else {
            wins as f64 / trade_count as f64
        },
        profit_factor,
        max_drawdown_r: max_drawdown,
        total_net_r,
    }
}

/// 计算 Vegas 保留/过滤路径对同一原始候选的配对增量。
pub fn calculate_paired_delta(
    observations: &[ResearchObservation],
    kept: impl Fn(&ResearchObservation) -> bool,
) -> PairedPathDelta {
    let deltas: Vec<f64> = observations
        .iter()
        .filter_map(|item| {
            item.vegas_baseline_net_r
                .map(|baseline| if kept(item) { 0.0 } else { -baseline })
        })
        .collect();
    let count = deltas.len();
    let mean = if count == 0 {
        0.0
    } else {
        deltas.iter().sum::<f64>() / count as f64
    };
    let variance = if count < 2 {
        0.0
    } else {
        deltas
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (count - 1) as f64
    };
    PairedPathDelta {
        pair_count: count,
        mean_delta_r: mean,
        confidence_lower_95: mean - 1.96 * (variance / count.max(1) as f64).sqrt(),
    }
}

/// 按共享市场时间块进行固定种子 bootstrap，避免把同期多币种结果当成独立样本。
pub fn bootstrap_shared_market_mean(
    blocks: &[SharedMarketTimeBlock],
    config: &BootstrapConfig,
) -> Result<BootstrapMeanEstimate, String> {
    if blocks.is_empty()
        || config.block_size == 0
        || config.resamples == 0
        || blocks
            .iter()
            .any(|block| block.net_r.iter().any(|value| !value.is_finite()))
    {
        return Err(
            "bootstrap requires non-empty finite market blocks and positive configuration"
                .to_owned(),
        );
    }
    let original = flattened_mean(blocks.iter().flat_map(|block| block.net_r.iter().copied()));
    let mut rng = StdRng::seed_from_u64(config.seed);
    let target_blocks = blocks.len();
    let mut means = Vec::with_capacity(config.resamples);
    for _ in 0..config.resamples {
        let mut sampled = Vec::new();
        let mut sampled_block_count = 0;
        while sampled_block_count < target_blocks {
            let start = rng.gen_range(0..blocks.len());
            for offset in 0..config.block_size {
                if sampled_block_count == target_blocks {
                    break;
                }
                sampled.extend(
                    blocks[(start + offset) % blocks.len()]
                        .net_r
                        .iter()
                        .copied(),
                );
                sampled_block_count += 1;
            }
        }
        means.push(flattened_mean(sampled.into_iter()));
    }
    means.sort_by(|left, right| left.total_cmp(right));
    let lower_index = ((config.resamples - 1) * 5) / 100;
    Ok(BootstrapMeanEstimate {
        observed_mean_r: original,
        lower_bound_95: means[lower_index],
        resamples: config.resamples,
    })
}

/// 对同一研究批次的多个假设 p 值执行 Holm-Bonferroni 校正，返回原始顺序的调整后值。
pub fn holm_bonferroni_adjust(p_values: &[f64]) -> Result<Vec<f64>, String> {
    if p_values
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err("p-values must be finite values between zero and one".to_owned());
    }
    let mut ranked: Vec<_> = p_values.iter().copied().enumerate().collect();
    ranked.sort_by(|left, right| left.1.total_cmp(&right.1));
    let mut adjusted = vec![0.0; p_values.len()];
    let mut previous = 0.0;
    for (rank, (original_index, p_value)) in ranked.into_iter().enumerate() {
        let corrected = (p_value * (p_values.len() - rank) as f64)
            .min(1.0)
            .max(previous);
        adjusted[original_index] = corrected;
        previous = corrected;
    }
    Ok(adjusted)
}

fn flattened_mean(values: impl Iterator<Item = f64>) -> f64 {
    let (total, count) = values.fold((0.0, 0usize), |(total, count), value| {
        (total + value, count + 1)
    });
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_use_path_drawdown_not_sum_of_single_trade_losses() {
        let metrics = calculate_metrics(&[1.0, -0.5, -0.75, 1.0]);
        assert_eq!(metrics.trade_count, 4);
        assert!((metrics.max_drawdown_r - 1.25).abs() < 1e-12);
    }

    #[test]
    fn market_block_bootstrap_is_deterministic_and_holm_preserves_original_order() {
        let blocks = vec![
            SharedMarketTimeBlock {
                start_ts: 1,
                net_r: vec![0.2, -0.1],
            },
            SharedMarketTimeBlock {
                start_ts: 2,
                net_r: vec![0.3, 0.1],
            },
        ];
        let config = BootstrapConfig {
            block_size: 1,
            resamples: 100,
            seed: 7,
        };
        assert_eq!(
            bootstrap_shared_market_mean(&blocks, &config).unwrap(),
            bootstrap_shared_market_mean(&blocks, &config).unwrap()
        );
        assert_eq!(
            holm_bonferroni_adjust(&[0.04, 0.01, 0.03]).unwrap(),
            vec![0.06, 0.03, 0.06]
        );
    }
}
