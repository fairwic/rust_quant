use super::{
    build_execution_plan, calculate_pa_features, generate_pa_candidate,
    generate_pa_followthrough_candidate, PaBlocker, PaDecisionTrace, PaDirection, PaExecutionPlan,
    PaStrategyKey, RuntimeManifest,
};
use crate::framework::backtest::adapter::IndicatorStrategyBacktest;
use crate::framework::backtest::types::{BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use rust_quant_domain::SignalDirection;

/// 等待下一棒开盘执行的冻结候选及其信号时点审计上下文。
#[derive(Debug, Clone)]
struct PendingCandidate {
    /// 信号时点冻结的候选定义。
    candidate: super::PaCandidate,
    /// 候选生成时可见的确定性特征。
    features: super::PaFeatureSnapshot,
    /// 冻结模型对候选给出的分数。
    model_score: f64,
}

/// PA 独立策略的无状态指标组合；特征由固定窗口在信号时点重建。
#[derive(Debug, Default)]
pub struct PaIndicatorCombine;

/// PA 独立策略每根 K 线没有额外增量指标值。
#[derive(Debug, Default)]
pub struct PaIndicatorValues;

/// 在下一棒开盘执行已冻结候选的 PA 量化树回测适配器。
#[derive(Debug, Clone)]
pub struct PaQuantTreeStrategy {
    /// 运行时只读的不可变模型与审计身份。
    manifest: RuntimeManifest,
    /// 冻结策略 key，用于阻止趋势与区间候选跨策略混用。
    strategy_key: PaStrategyKey,
    /// 上一根已确认 K 线生成、等待下一根开盘的候选。
    pending_candidate: Option<PendingCandidate>,
    /// 最近一次决策的完整审计轨迹。
    last_trace: Option<PaDecisionTrace>,
}

impl PaQuantTreeStrategy {
    /// 用已经验证的不可变 manifest 创建 PA 策略。
    pub fn new(manifest: RuntimeManifest) -> Result<Self, String> {
        manifest.validate()?;
        let strategy_key = PaStrategyKey::parse(&manifest.strategy_key)?;
        if strategy_key.is_meta_filter() {
            return Err("vegas_pa_meta_filter must use the Vegas shadow path".to_owned());
        }
        Ok(Self {
            manifest,
            strategy_key,
            pending_candidate: None,
            last_trace: None,
        })
    }

    /// 返回最近一次决策，供回测审计或研究数据集读取。
    pub fn last_trace(&self) -> Option<&PaDecisionTrace> {
        self.last_trace.as_ref()
    }

    fn to_signal(execution: &PaExecutionPlan, trace: &PaDecisionTrace) -> SignalResult {
        let mut signal = SignalResult {
            ts: execution.entry_ts,
            open_price: execution.entry_price,
            signal_kline_stop_loss_price: Some(execution.stop_price),
            stop_loss_source: Some("PA_QUANT_TREE".to_owned()),
            dynamic_config_snapshot: serde_json::to_string(trace).ok(),
            ..SignalResult::default()
        };
        match execution.direction {
            PaDirection::Long => {
                signal.should_buy = true;
                signal.direction = SignalDirection::Long;
                signal.long_signal_take_profit_price = Some(execution.target_price);
            }
            PaDirection::Short => {
                signal.should_sell = true;
                signal.direction = SignalDirection::Short;
                signal.short_signal_take_profit_price = Some(execution.target_price);
            }
        }
        signal
    }

    fn no_trade(&mut self, signal_ts: i64, blocker: PaBlocker) -> SignalResult {
        let manifest_hash = self
            .manifest
            .manifest_hash()
            .unwrap_or_else(|_| "invalid_manifest".to_owned());
        self.last_trace = Some(PaDecisionTrace {
            signal_ts,
            manifest_hash,
            model_score: None,
            features: None,
            candidate: None,
            execution: None,
            blocker: Some(blocker.clone()),
        });
        SignalResult {
            ts: signal_ts,
            filter_reasons: vec![format!("PA_{}", blocker.code().to_ascii_uppercase())],
            ..SignalResult::default()
        }
    }
}

impl IndicatorStrategyBacktest for PaQuantTreeStrategy {
    type IndicatorCombine = PaIndicatorCombine;
    type IndicatorValues = PaIndicatorValues;

    /// 保持足够窗口以计算 EMA20、ATR14、效率与结构特征。
    fn min_data_length(&self) -> usize {
        super::features::PA_MIN_CANDLES
    }

    /// PA 特征通过确定性固定窗口重建，因此没有跨请求可变指标状态。
    fn init_indicator_combine(&self) -> Self::IndicatorCombine {
        PaIndicatorCombine
    }

    /// 此适配器无需由 pipeline 预先计算指标。
    fn build_indicator_values(
        _: &mut Self::IndicatorCombine,
        _: &CandleItem,
    ) -> Self::IndicatorValues {
        PaIndicatorValues
    }

    /// 先用当前棒开盘执行上一候选，再只根据当前已确认棒生成下一候选。
    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _: &mut Self::IndicatorValues,
        _: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let current = match candles.last() {
            Some(candle) => candle,
            None => return self.no_trade(0, PaBlocker::DataNotReady),
        };
        if let Some(pending) = self.pending_candidate.take() {
            match build_execution_plan(&pending.candidate, current) {
                Ok(execution) => {
                    let manifest_hash = self
                        .manifest
                        .manifest_hash()
                        .unwrap_or_else(|_| "invalid_manifest".to_owned());
                    let trace = PaDecisionTrace {
                        signal_ts: pending.candidate.signal_ts,
                        manifest_hash,
                        model_score: Some(pending.model_score),
                        features: Some(pending.features),
                        candidate: Some(pending.candidate),
                        execution: Some(execution.clone()),
                        blocker: None,
                    };
                    let signal = Self::to_signal(&execution, &trace);
                    self.last_trace = Some(trace);
                    // 当前调用的真实执行信号优先返回，下一候选仍由下一次完整已确认 K 线计算。
                    return signal;
                }
                Err(blocker) => {
                    let manifest_hash = self
                        .manifest
                        .manifest_hash()
                        .unwrap_or_else(|_| "invalid_manifest".to_owned());
                    self.last_trace = Some(PaDecisionTrace {
                        signal_ts: pending.candidate.signal_ts,
                        manifest_hash,
                        model_score: Some(pending.model_score),
                        features: Some(pending.features),
                        candidate: Some(pending.candidate),
                        execution: None,
                        blocker: Some(blocker.clone()),
                    });
                    return SignalResult {
                        ts: current.ts,
                        filter_reasons: vec![format!("PA_{}", blocker.code().to_ascii_uppercase())],
                        ..SignalResult::default()
                    };
                }
            }
        }
        let features = match calculate_pa_features(candles) {
            Ok(features) => features,
            Err(blocker) => return self.no_trade(current.ts, blocker),
        };
        let candidate_result = if self.strategy_key.uses_followthrough_confirmation() {
            generate_pa_followthrough_candidate(candles)
        } else {
            generate_pa_candidate(candles, &features)
        };
        let candidate = match candidate_result {
            Ok(candidate) => candidate,
            Err(blocker) => return self.no_trade(current.ts, blocker),
        };
        if !self.strategy_key.supports_candidate(candidate.kind) {
            return self.no_trade(current.ts, PaBlocker::NoCandidate);
        }
        let decision = self.manifest.model.evaluate(&features);
        let manifest_hash = self
            .manifest
            .manifest_hash()
            .unwrap_or_else(|_| "invalid_manifest".to_owned());
        if !decision.keep {
            self.last_trace = Some(PaDecisionTrace {
                signal_ts: current.ts,
                manifest_hash,
                model_score: Some(decision.score),
                features: Some(features),
                candidate: Some(candidate),
                execution: None,
                blocker: Some(PaBlocker::QualityRejected),
            });
            return SignalResult {
                ts: current.ts,
                filter_reasons: vec!["PA_QUALITY_REJECTED".to_owned()],
                ..SignalResult::default()
            };
        }
        self.pending_candidate = Some(PendingCandidate {
            candidate: candidate.clone(),
            features: features.clone(),
            model_score: decision.score,
        });
        self.last_trace = Some(PaDecisionTrace {
            signal_ts: current.ts,
            manifest_hash,
            model_score: Some(decision.score),
            features: Some(features),
            candidate: Some(candidate),
            execution: None,
            blocker: None,
        });
        SignalResult {
            ts: current.ts,
            ..SignalResult::default()
        }
    }
}
