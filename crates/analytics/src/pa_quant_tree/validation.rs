use super::ResearchDataset;
use serde::{Deserialize, Serialize};

/// 一个时间顺序的训练/验证窗口，purge 区间不允许进入任何一侧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalkForwardWindow {
    /// 训练样本的半开区间起点。
    pub train_start: usize,
    /// 训练样本的半开区间终点。
    pub train_end: usize,
    /// purge 后验证样本的半开区间起点。
    pub validation_start: usize,
    /// 验证样本的半开区间终点。
    pub validation_end: usize,
}

/// Purged nested walk-forward 的冻结切分配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalkForwardPlan {
    /// 每个训练段的最少样本数。
    pub min_train_size: usize,
    /// 每个验证段长度。
    pub validation_size: usize,
    /// 训练结束与验证开始之间剔除的样本数。
    pub purge_size: usize,
    /// 最多生成的窗口数。
    pub max_windows: usize,
}

impl WalkForwardPlan {
    /// 生成严格向前移动的窗口；没有足够数据时返回空，而不是偷用未来样本。
    pub fn build(&self, dataset: &ResearchDataset) -> Result<Vec<WalkForwardWindow>, String> {
        if self.min_train_size == 0 || self.validation_size == 0 || self.max_windows == 0 {
            return Err("walk-forward sizes must be positive".to_owned());
        }
        let mut windows = Vec::new();
        let mut train_end = self.min_train_size;
        while train_end + self.purge_size + self.validation_size <= dataset.observations.len()
            && windows.len() < self.max_windows
        {
            let validation_start = train_end + self.purge_size;
            windows.push(WalkForwardWindow {
                train_start: 0,
                train_end,
                validation_start,
                validation_end: validation_start + self.validation_size,
            });
            train_end += self.validation_size;
        }
        Ok(windows)
    }
}

/// 检查一个切分不会让尚未结算的训练候选泄漏进验证起点。
pub fn validate_temporal_purge(
    dataset: &ResearchDataset,
    window: &WalkForwardWindow,
) -> Result<(), String> {
    let validation = dataset
        .observations
        .get(window.validation_start)
        .ok_or_else(|| "validation start is outside dataset".to_owned())?;
    if dataset.observations[window.train_start..window.train_end]
        .iter()
        .any(|item| item.settled_ts >= validation.signal_ts)
    {
        return Err("purge window does not remove overlapping outcome horizon".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pa_quant_tree::{ResearchDataset, ResearchObservation};
    use rust_quant_strategies::implementations::pa_quant_tree::{
        PaFeatureSnapshot, PaMarketRegime,
    };

    fn observation(signal_ts: i64, settled_ts: i64) -> ResearchObservation {
        ResearchObservation {
            signal_ts,
            settled_ts,
            symbol: "BTC-USDT-SWAP".to_owned(),
            strategy_key: "pa_trend_15m".to_owned(),
            strategy_version: "1.0.0".to_owned(),
            manifest_hash: "sha256:test".to_owned(),
            candidate_id: format!("candidate-{signal_ts}"),
            features: PaFeatureSnapshot {
                signal_ts,
                atr14: 1.0,
                ema20: 100.0,
                ema_slope_atr_20_5: 0.1,
                range_efficiency_20: 0.5,
                range_high_20: 101.0,
                range_low_20: 99.0,
                range_position_20: 0.5,
                mean_overlap_ratio_8: 0.2,
                always_in_score: 0.8,
                signal_body_ratio: 0.5,
                close_position: 0.7,
                pullback_depth_atr_3: 0.2,
                directional_reclaim_atr: 0.3,
                directional_close_strength: 0.7,
                signal_range_atr: 0.9,
                pullback_close_fraction_3: 1.0 / 3.0,
                recent_ema_touch: true,
                regime: PaMarketRegime::Trend,
                trend_direction: None,
            },
            net_r: 0.2,
            vegas_baseline_net_r: None,
        }
    }

    #[test]
    fn plan_never_uses_future_as_training_data() {
        let dataset = ResearchDataset::new(vec![]).unwrap();
        let plan = WalkForwardPlan {
            min_train_size: 10,
            validation_size: 5,
            purge_size: 2,
            max_windows: 3,
        };
        assert!(plan.build(&dataset).unwrap().is_empty());
    }

    #[test]
    fn purge_rejects_training_outcome_that_reaches_validation_time() {
        let dataset = ResearchDataset::new(vec![
            observation(1, 2),
            observation(2, 9),
            observation(3, 4),
        ])
        .unwrap();
        let window = WalkForwardWindow {
            train_start: 0,
            train_end: 2,
            validation_start: 2,
            validation_end: 3,
        };
        assert!(validate_temporal_purge(&dataset, &window).is_err());
    }
}
