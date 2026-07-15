use rust_quant_common::utils::function::sha256;
use rust_quant_strategies::implementations::pa_quant_tree::{PaDecisionTrace, PaFeatureSnapshot};
use serde::{Deserialize, Serialize};

/// 一个候选在信号时点冻结、在结算后补齐标签的研究样本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchObservation {
    /// 信号产生时间戳，必须早于或等于后续结算时间。
    pub signal_ts: i64,
    /// 结算时间戳，供 purge 和时间分割使用。
    pub settled_ts: i64,
    /// 交易对，用于多资产时间块验证。
    pub symbol: String,
    /// 候选所属策略与版本。
    pub strategy_key: String,
    /// 策略不可变版本；不得使用可变的 default 标识。
    pub strategy_version: String,
    /// 生成候选时使用的 RuntimeManifest 哈希。
    pub manifest_hash: String,
    /// 候选唯一审计 ID。
    pub candidate_id: String,
    /// 信号时点冻结的特征快照。
    pub features: PaFeatureSnapshot,
    /// 计入手续费、滑点和资金费率后的已结算 R。
    pub net_r: f64,
    /// 同一个 Vegas 原始候选的基线路径 R；独立 PA 策略为空。
    pub vegas_baseline_net_r: Option<f64>,
}

/// 不可变的时间排序研究数据集，不携带未打开 OOS 数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchDataset {
    /// 仅包含已经授权进入本轮训练/验证的数据。
    pub observations: Vec<ResearchObservation>,
    /// OOS 是否已经被打开；打开后不能再次称为 OOS。
    pub sealed_oos_opened: bool,
}

impl ResearchDataset {
    /// 创建并验证时间顺序、结算顺序和有限收益，拒绝潜在未来泄漏数据。
    pub fn new(mut observations: Vec<ResearchObservation>) -> Result<Self, String> {
        observations.sort_by_key(|item| {
            (
                item.signal_ts,
                item.symbol.clone(),
                item.candidate_id.clone(),
            )
        });
        for observation in &observations {
            if observation.settled_ts < observation.signal_ts
                || observation.symbol.is_empty()
                || observation.strategy_key.is_empty()
                || observation.strategy_version.is_empty()
                || observation.manifest_hash.is_empty()
                || observation.candidate_id.is_empty()
                || !observation.net_r.is_finite()
                || observation
                    .vegas_baseline_net_r
                    .is_some_and(|value| !value.is_finite())
            {
                return Err("research observation violates temporal or numeric contract".to_owned());
            }
        }
        Ok(Self {
            observations,
            sealed_oos_opened: false,
        })
    }

    /// 返回稳定数据指纹，供 manifest 与 PromotionRecord 回溯使用。
    pub fn fingerprint(&self) -> Result<String, String> {
        let canonical = serde_json::to_string(self).map_err(|error| error.to_string())?;
        Ok(format!("sha256:{}", sha256(&canonical)))
    }

    /// 将密封 OOS 标记为已打开；调用者必须先完成预注册验证计划。
    pub fn mark_sealed_oos_opened(&mut self) -> Result<(), String> {
        if self.sealed_oos_opened {
            return Err("sealed OOS was already opened and cannot be reused".to_owned());
        }
        self.sealed_oos_opened = true;
        Ok(())
    }
}

impl ResearchObservation {
    /// 将一个已结算的 PA 决策转成训练样本，拒绝没有冻结特征或候选的审计记录。
    pub fn from_pa_settlement(
        symbol: String,
        strategy_key: String,
        strategy_version: String,
        candidate_id: String,
        trace: &PaDecisionTrace,
        settled_ts: i64,
        net_r: f64,
        vegas_baseline_net_r: Option<f64>,
    ) -> Result<Self, String> {
        let features = trace
            .features
            .clone()
            .ok_or_else(|| "PA trace has no frozen features".to_owned())?;
        if trace.candidate.is_none()
            || trace.manifest_hash.is_empty()
            || settled_ts < trace.signal_ts
            || !net_r.is_finite()
            || vegas_baseline_net_r.is_some_and(|value| !value.is_finite())
        {
            return Err("PA settlement violates research audit contract".to_owned());
        }
        Ok(Self {
            signal_ts: trace.signal_ts,
            settled_ts,
            symbol,
            strategy_key,
            strategy_version,
            manifest_hash: trace.manifest_hash.clone(),
            candidate_id,
            features,
            net_r,
            vegas_baseline_net_r,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_strategies::implementations::pa_quant_tree::{
        PaCandidate, PaCandidateKind, PaDecisionTrace, PaDirection, PaFeatureSnapshot,
        PaMarketRegime,
    };

    fn trace(features: Option<PaFeatureSnapshot>) -> PaDecisionTrace {
        PaDecisionTrace {
            signal_ts: 1,
            manifest_hash: "sha256:manifest".to_owned(),
            model_score: Some(0.7),
            features,
            candidate: Some(PaCandidate {
                signal_ts: 1,
                setup_ts: None,
                direction: PaDirection::Long,
                kind: PaCandidateKind::TrendPullback,
                stop_price: 99.0,
                range_target: None,
            }),
            execution: None,
            blocker: None,
        }
    }

    fn features() -> PaFeatureSnapshot {
        PaFeatureSnapshot {
            signal_ts: 1,
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
            trend_direction: Some(PaDirection::Long),
        }
    }

    #[test]
    fn sealed_oos_can_only_be_opened_once() {
        let mut dataset = ResearchDataset::new(vec![]).unwrap();
        assert!(dataset.mark_sealed_oos_opened().is_ok());
        assert!(dataset.mark_sealed_oos_opened().is_err());
    }

    #[test]
    fn settlement_conversion_preserves_versioned_manifest_and_rejects_missing_features() {
        let observation = ResearchObservation::from_pa_settlement(
            "BTC-USDT-SWAP".to_owned(),
            "pa_trend_15m".to_owned(),
            "1.0.0".to_owned(),
            "candidate-1".to_owned(),
            &trace(Some(features())),
            2,
            0.4,
            None,
        )
        .unwrap();
        assert_eq!(observation.strategy_version, "1.0.0");
        assert_eq!(observation.manifest_hash, "sha256:manifest");
        assert!(ResearchObservation::from_pa_settlement(
            "BTC-USDT-SWAP".to_owned(),
            "pa_trend_15m".to_owned(),
            "1.0.0".to_owned(),
            "candidate-2".to_owned(),
            &trace(None),
            2,
            0.4,
            None,
        )
        .is_err());
    }
}
