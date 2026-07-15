use super::{PaBlocker, PaFeatureSnapshot, RuntimeManifest};
use crate::framework::backtest::types::SignalResult;
use serde::{Deserialize, Serialize};

/// Vegas 候选的只读 shadow 过滤结果；原始 Vegas 信号始终原样保留。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VegasMetaFilterOutcome {
    /// 未修改的原始 Vegas 信号副本。
    pub original_signal: SignalResult,
    /// 过滤器是否保留该信号；false 仅用于 shadow 路径分析。
    pub keep: bool,
    /// 冻结 manifest 的评分。
    pub score: f64,
    /// 拒绝原因；保留时为空。
    pub blocker: Option<PaBlocker>,
    /// 用于审计的 manifest 哈希。
    pub manifest_hash: String,
}

/// 只读评估 Vegas 原始候选，不会修改方向、入场、止损、目标或执行路径。
pub fn evaluate_vegas_meta_filter(
    vegas_signal: &SignalResult,
    features: &PaFeatureSnapshot,
    manifest: &RuntimeManifest,
) -> Result<VegasMetaFilterOutcome, String> {
    let manifest_hash = manifest.manifest_hash()?;
    let decision = manifest.model.evaluate(features);
    Ok(VegasMetaFilterOutcome {
        original_signal: vegas_signal.clone(),
        keep: decision.keep,
        score: decision.score,
        blocker: (!decision.keep).then_some(PaBlocker::QualityRejected),
        manifest_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementations::pa_quant_tree::{
        calculate_pa_features, features::tests::trend_candles, FeatureThreshold, PaFeatureId,
        RuntimeModel,
    };

    #[test]
    fn shadow_filter_never_mutates_vegas_signal() {
        let original = SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: 101.0,
            signal_kline_stop_loss_price: Some(99.0),
            atr_take_profit_ratio_price: Some(104.0),
            ..SignalResult::default()
        };
        let manifest = RuntimeManifest {
            strategy_key: "vegas_pa_meta_filter".to_owned(),
            version: "1.0.0".to_owned(),
            feature_registry_version: "pa-feature-registry-v1".to_owned(),
            dataset_fingerprint: "dataset".to_owned(),
            code_revision: "revision".to_owned(),
            model: RuntimeModel::FixedRules {
                rules: vec![FeatureThreshold {
                    feature: PaFeatureId::SignalBodyRatio,
                    threshold: 2.0,
                    greater_or_equal: true,
                }],
            },
        };
        let features = calculate_pa_features(&trend_candles(100)).unwrap();
        let outcome = evaluate_vegas_meta_filter(&original, &features, &manifest).unwrap();
        assert!(!outcome.keep);
        assert_eq!(outcome.original_signal.should_buy, original.should_buy);
        assert_eq!(outcome.original_signal.open_price, original.open_price);
        assert_eq!(
            outcome.original_signal.signal_kline_stop_loss_price,
            original.signal_kline_stop_loss_price
        );
        assert_eq!(
            outcome.original_signal.atr_take_profit_ratio_price,
            original.atr_take_profit_ratio_price
        );
    }
}
