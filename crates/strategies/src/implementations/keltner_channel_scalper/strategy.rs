use super::indicators::{adx_at, basis_slope_atr, keltner_at};
use super::types::{
    round_price, KeltnerChannelScalperAction, KeltnerChannelScalperBacktestTuning,
    KeltnerChannelScalperDecision, KeltnerChannelScalperEntryMode,
    KeltnerChannelScalperSignalSnapshot, KeltnerChannelScalperThresholds,
};
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde_json::json;
use std::sync::Arc;

/// Keltner Channel 1m scalp research strategy；只输出可回测/可审计信号。
pub struct KeltnerChannelScalperStrategy;

impl KeltnerChannelScalperStrategy {
    /// 基于 Keltner re-entry 快照评估方向，严格执行 ADX 上下阈值。
    pub fn evaluate(
        thresholds: &KeltnerChannelScalperThresholds,
        snapshot: &KeltnerChannelScalperSignalSnapshot,
    ) -> KeltnerChannelScalperDecision {
        Self::evaluate_with_entry_mode(
            thresholds,
            snapshot,
            KeltnerChannelScalperEntryMode::Reversal,
        )
    }

    /// 基于指定 re-entry 解释评估方向；Continuation 只用于 research 对照扫描。
    pub fn evaluate_with_entry_mode(
        thresholds: &KeltnerChannelScalperThresholds,
        snapshot: &KeltnerChannelScalperSignalSnapshot,
        entry_mode: KeltnerChannelScalperEntryMode,
    ) -> KeltnerChannelScalperDecision {
        let blockers = Self::blockers(snapshot, thresholds);
        if !blockers.is_empty() {
            return Self::decision(KeltnerChannelScalperAction::Flat, blockers);
        }

        let (short_setup, long_setup) = setup_sides(snapshot, entry_mode);
        if short_setup && long_setup {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["AMBIGUOUS_BOTH_SIDE_REENTRY".to_string()],
            );
        }
        if (short_setup || long_setup)
            && snapshot.reentry_body_ratio < thresholds.min_reentry_body_ratio
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["REENTRY_BODY_TOO_SMALL".to_string()],
            );
        }
        if (short_setup || long_setup)
            && thresholds.max_reentry_body_ratio > 0.0
            && snapshot.reentry_body_ratio > thresholds.max_reentry_body_ratio
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["REENTRY_BODY_TOO_LARGE".to_string()],
            );
        }
        if (short_setup || long_setup) && atr_percent(snapshot) < thresholds.min_atr_pct {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["ATR_PCT_TOO_LOW".to_string()],
            );
        }
        if (short_setup || long_setup)
            && snapshot.rejection_wick_ratio < thresholds.min_rejection_wick_ratio
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["REJECTION_WICK_TOO_SMALL".to_string()],
            );
        }
        if (short_setup || long_setup)
            && snapshot.reentry_close_progress_ratio < thresholds.min_reentry_close_progress_ratio
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["REENTRY_CLOSE_PROGRESS_TOO_WEAK".to_string()],
            );
        }
        if (short_setup || long_setup)
            && thresholds.max_breakout_reentry_candles > 0
            && snapshot.breakout_reentry_candles.saturating_add(1)
                > thresholds.max_breakout_reentry_candles
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["BREAKOUT_REENTRY_TOO_SLOW".to_string()],
            );
        }
        if short_setup
            && inner_reclaim_atr_distance(snapshot, KeltnerChannelScalperAction::Short)
                < thresholds.min_inner_reclaim_atr
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["INNER_RECLAIM_DISTANCE_TOO_SMALL".to_string()],
            );
        }
        if short_setup
            && thresholds.max_inner_reclaim_atr > 0.0
            && inner_reclaim_atr_distance(snapshot, KeltnerChannelScalperAction::Short)
                > thresholds.max_inner_reclaim_atr
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["INNER_RECLAIM_DISTANCE_TOO_LARGE".to_string()],
            );
        }
        if long_setup
            && inner_reclaim_atr_distance(snapshot, KeltnerChannelScalperAction::Long)
                < thresholds.min_inner_reclaim_atr
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["INNER_RECLAIM_DISTANCE_TOO_SMALL".to_string()],
            );
        }
        if long_setup
            && thresholds.max_inner_reclaim_atr > 0.0
            && inner_reclaim_atr_distance(snapshot, KeltnerChannelScalperAction::Long)
                > thresholds.max_inner_reclaim_atr
        {
            return Self::decision(
                KeltnerChannelScalperAction::Flat,
                vec!["INNER_RECLAIM_DISTANCE_TOO_LARGE".to_string()],
            );
        }
        if short_setup {
            if snapshot.adx <= thresholds.adx_level {
                return Self::decision(
                    KeltnerChannelScalperAction::Flat,
                    vec!["ADX_NOT_ABOVE_SHORT_LEVEL".to_string()],
                );
            }
            let (action, reason) = match entry_mode {
                KeltnerChannelScalperEntryMode::Reversal => (
                    KeltnerChannelScalperAction::Short,
                    "KELTNER_UPPER_REENTRY_SHORT",
                ),
                KeltnerChannelScalperEntryMode::Continuation => (
                    KeltnerChannelScalperAction::Long,
                    "KELTNER_UPPER_REENTRY_CONTINUATION_LONG",
                ),
                KeltnerChannelScalperEntryMode::ExtremeMomentumReversal => (
                    KeltnerChannelScalperAction::Short,
                    "KELTNER_UPPER_EXTREME_MOMENTUM_SHORT",
                ),
            };
            if let Some(reason) = basis_slope_filter_reason(snapshot, thresholds, action) {
                return Self::decision(KeltnerChannelScalperAction::Flat, vec![reason.to_string()]);
            }
            if let Some(reason) = basis_cross_filter_reason(snapshot, thresholds, action) {
                return Self::decision(KeltnerChannelScalperAction::Flat, vec![reason.to_string()]);
            }
            return Self::confirmed_decision(thresholds, snapshot, action, reason);
        }
        if long_setup {
            if snapshot.adx >= thresholds.adx_level {
                return Self::decision(
                    KeltnerChannelScalperAction::Flat,
                    vec!["ADX_NOT_BELOW_LONG_LEVEL".to_string()],
                );
            }
            if snapshot.adx <= thresholds.min_long_adx {
                return Self::decision(
                    KeltnerChannelScalperAction::Flat,
                    vec!["ADX_BELOW_LONG_MIN_LEVEL".to_string()],
                );
            }
            let (action, reason) = match entry_mode {
                KeltnerChannelScalperEntryMode::Reversal => (
                    KeltnerChannelScalperAction::Long,
                    "KELTNER_LOWER_REENTRY_LONG",
                ),
                KeltnerChannelScalperEntryMode::Continuation => (
                    KeltnerChannelScalperAction::Short,
                    "KELTNER_LOWER_REENTRY_CONTINUATION_SHORT",
                ),
                KeltnerChannelScalperEntryMode::ExtremeMomentumReversal => (
                    KeltnerChannelScalperAction::Long,
                    "KELTNER_LOWER_EXTREME_MOMENTUM_LONG",
                ),
            };
            if let Some(reason) = basis_slope_filter_reason(snapshot, thresholds, action) {
                return Self::decision(KeltnerChannelScalperAction::Flat, vec![reason.to_string()]);
            }
            if let Some(reason) = basis_cross_filter_reason(snapshot, thresholds, action) {
                return Self::decision(KeltnerChannelScalperAction::Flat, vec![reason.to_string()]);
            }
            return Self::confirmed_decision(thresholds, snapshot, action, reason);
        }

        let no_setup_reason = match entry_mode {
            KeltnerChannelScalperEntryMode::ExtremeMomentumReversal => "NO_KELTNER_MOMENTUM_SETUP",
            _ => "NO_KELTNER_REENTRY_SETUP",
        };
        Self::decision(
            KeltnerChannelScalperAction::Flat,
            vec![no_setup_reason.to_string()],
        )
    }

    /// 缺少有效快照时返回 flat，避免 live/paper 调用用单根 K 线猜测通道状态。
    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            KeltnerChannelScalperAction::Flat,
            vec!["MISSING_MARKET_SNAPSHOT".to_string()],
        )
        .to_signal(price, ts)
    }

    /// 默认参数回测入口。
    pub fn run_test(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        self.run_test_with_tuning(
            inst_id,
            candles,
            risk,
            KeltnerChannelScalperBacktestTuning::default(),
        )
    }

    /// 指定调参回测入口；复用统一 pipeline 的成交、止损和审计口径。
    pub fn run_test_with_tuning(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: KeltnerChannelScalperBacktestTuning,
    ) -> BackTestResult {
        self.run_test_with_tuning_for_timeframe(inst_id, "1m", candles, risk, tuning)
    }

    /// 指定周期的研究回测入口；用于 5m/15m 对照，不改变默认 1m 行为。
    pub fn run_test_with_tuning_for_timeframe(
        self,
        inst_id: &str,
        timeframe: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: KeltnerChannelScalperBacktestTuning,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(
            inst_id,
            KeltnerChannelScalperBacktestAdapter::new_for_timeframe(inst_id, timeframe, tuning),
            candles,
            risk,
        )
    }

    /// 预先生成与统一回测 pipeline 同窗口口径一致的 Keltner/ADX 快照。
    pub fn precompute_backtest_snapshots(
        inst_id: &str,
        candles: &[CandleItem],
        tuning: KeltnerChannelScalperBacktestTuning,
    ) -> Vec<Option<KeltnerChannelScalperSignalSnapshot>> {
        Self::precompute_backtest_snapshots_for_timeframe(inst_id, "1m", candles, tuning)
    }

    /// 预先生成指定周期的 Keltner/ADX 快照，避免 5m/15m 研究被误记为 1m。
    pub fn precompute_backtest_snapshots_for_timeframe(
        inst_id: &str,
        timeframe: &str,
        candles: &[CandleItem],
        tuning: KeltnerChannelScalperBacktestTuning,
    ) -> Vec<Option<KeltnerChannelScalperSignalSnapshot>> {
        let adapter =
            KeltnerChannelScalperBacktestAdapter::new_for_timeframe(inst_id, timeframe, tuning);
        let min_data_length = adapter.min_data_length();
        let mut snapshots = vec![None; candles.len()];
        if min_data_length == 0 || candles.len() < min_data_length {
            return snapshots;
        }
        for end_index in (min_data_length - 1)..candles.len() {
            let start_index = end_index + 1 - min_data_length;
            snapshots[end_index] = adapter.snapshot(&candles[start_index..=end_index], end_index);
        }
        snapshots
    }

    /// 使用预计算快照回测；成交、风控与持仓推进仍走统一 pipeline。
    pub fn run_test_with_precomputed_snapshots(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: KeltnerChannelScalperBacktestTuning,
        snapshots: Arc<Vec<Option<KeltnerChannelScalperSignalSnapshot>>>,
    ) -> BackTestResult {
        self.run_test_with_precomputed_snapshots_for_timeframe(
            inst_id, "1m", candles, risk, tuning, snapshots,
        )
    }

    /// 使用指定周期的预计算快照回测；成交、风控与持仓推进仍走统一 pipeline。
    pub fn run_test_with_precomputed_snapshots_for_timeframe(
        self,
        inst_id: &str,
        timeframe: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: KeltnerChannelScalperBacktestTuning,
        snapshots: Arc<Vec<Option<KeltnerChannelScalperSignalSnapshot>>>,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(
            inst_id,
            KeltnerChannelScalperBacktestAdapter::new_with_snapshots_for_timeframe(
                inst_id, timeframe, tuning, snapshots,
            ),
            candles,
            risk,
        )
    }

    fn blockers(
        snapshot: &KeltnerChannelScalperSignalSnapshot,
        thresholds: &KeltnerChannelScalperThresholds,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        Self::push_if(
            !is_supported_short_timeframe(&snapshot.timeframe),
            "TIMEFRAME_NOT_KELTNER_SHORT",
            &mut reasons,
        );
        Self::push_if(snapshot.price <= 0.0, "PRICE_NOT_READY", &mut reasons);
        Self::push_if(snapshot.atr <= 0.0, "ATR_NOT_READY", &mut reasons);
        Self::push_if(!snapshot.adx.is_finite(), "ADX_NOT_READY", &mut reasons);
        Self::push_if(
            thresholds.stop_atr_mult <= 0.0,
            "STOP_ATR_MULT_INVALID",
            &mut reasons,
        );
        Self::push_if(
            !bands_are_ordered(snapshot),
            "KELTNER_BANDS_INVALID",
            &mut reasons,
        );
        reasons
    }

    fn confirmed_decision(
        thresholds: &KeltnerChannelScalperThresholds,
        snapshot: &KeltnerChannelScalperSignalSnapshot,
        action: KeltnerChannelScalperAction,
        reason: &str,
    ) -> KeltnerChannelScalperDecision {
        let (stop, target_1, target_2, target_3) = stop_and_targets(thresholds, snapshot, action);
        Self::decision(
            action,
            vec![
                reason.to_string(),
                format!("STOP_PRICE:{}", round_price(stop)),
                format!("TARGET_1:{}", round_price(target_1)),
                format!("TARGET_2:{}", round_price(target_2)),
                format!("TARGET_3:{}", round_price(target_3)),
            ],
        )
    }

    fn decision(
        action: KeltnerChannelScalperAction,
        reasons: Vec<String>,
    ) -> KeltnerChannelScalperDecision {
        KeltnerChannelScalperDecision { action, reasons }
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}

/// 回测适配器只从已完成 1m OHLCV 构造 Keltner/ADX 快照。
#[derive(Debug, Clone)]
struct KeltnerChannelScalperBacktestAdapter {
    symbol: String,
    timeframe: String,
    tuning: KeltnerChannelScalperBacktestTuning,
    cooldown_remaining: usize,
    precomputed_snapshots: Option<Arc<Vec<Option<KeltnerChannelScalperSignalSnapshot>>>>,
    next_snapshot_index: usize,
}

impl KeltnerChannelScalperBacktestAdapter {
    fn new_for_timeframe(
        inst_id: &str,
        timeframe: &str,
        tuning: KeltnerChannelScalperBacktestTuning,
    ) -> Self {
        let next_snapshot_index = Self::min_data_length_for(tuning).saturating_sub(1);
        Self {
            symbol: inst_id.to_string(),
            timeframe: timeframe.to_string(),
            tuning,
            cooldown_remaining: 0,
            precomputed_snapshots: None,
            next_snapshot_index,
        }
    }

    fn new_with_snapshots_for_timeframe(
        inst_id: &str,
        timeframe: &str,
        tuning: KeltnerChannelScalperBacktestTuning,
        snapshots: Arc<Vec<Option<KeltnerChannelScalperSignalSnapshot>>>,
    ) -> Self {
        let next_snapshot_index = Self::min_data_length_for(tuning).saturating_sub(1);
        Self {
            symbol: inst_id.to_string(),
            timeframe: timeframe.to_string(),
            tuning,
            cooldown_remaining: 0,
            precomputed_snapshots: Some(snapshots),
            next_snapshot_index,
        }
    }

    fn snapshot(
        &self,
        candles: &[CandleItem],
        snapshot_index: usize,
    ) -> Option<KeltnerChannelScalperSignalSnapshot> {
        if let Some(snapshots) = self.precomputed_snapshots.as_ref() {
            return snapshots.get(snapshot_index).cloned().flatten();
        }

        if self.tuning.confirm_next_candle {
            return self.next_candle_confirmation_snapshot(candles);
        }

        self.same_candle_snapshot(candles)
    }

    fn same_candle_snapshot(
        &self,
        candles: &[CandleItem],
    ) -> Option<KeltnerChannelScalperSignalSnapshot> {
        let thresholds = self.tuning.thresholds;
        let last = candles.last()?;
        let bands = keltner_at(candles, candles.len(), &thresholds)?;
        let previous = candles.get(candles.len().checked_sub(2)?)?;
        let previous_bands = keltner_at(candles, candles.len() - 1, &thresholds)?;
        let adx = adx_at(
            candles,
            candles.len(),
            thresholds.adx_trend_length,
            thresholds.adx_smoothing,
        )?;
        let basis_slope_atr = basis_slope_atr(candles, &bands, &thresholds)?;
        let lookback = self
            .tuning
            .reentry_lookback_candles
            .max(1)
            .min(candles.len());
        let start = candles.len().saturating_sub(lookback);
        let mut latest_upper_breach_index = None;
        let mut latest_lower_breach_index = None;
        for index in start..candles.len() {
            let Some(prior_bands) = keltner_at(candles, index + 1, &thresholds) else {
                continue;
            };
            let candle = &candles[index];
            if candle.h > prior_bands.outer_upper {
                latest_upper_breach_index = Some(index);
            }
            if candle.l < prior_bands.outer_lower {
                latest_lower_breach_index = Some(index);
            }
        }
        let outer_upper_breached = latest_upper_breach_index.is_some();
        let outer_lower_breached = latest_lower_breach_index.is_some();
        let returned_inside_inner_upper = last.c < last.o
            && last.c <= bands.inner_upper
            && (previous.c > previous_bands.inner_upper || last.h > bands.outer_upper);
        let returned_inside_inner_lower = last.c > last.o
            && last.c >= bands.inner_lower
            && (previous.c < previous_bands.inner_lower || last.l < bands.outer_lower);
        let breakout_reentry_candles = breakout_reentry_candles(
            candles.len(),
            latest_upper_breach_index,
            latest_lower_breach_index,
            returned_inside_inner_upper,
            returned_inside_inner_lower,
        );
        Some(KeltnerChannelScalperSignalSnapshot {
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.clone(),
            price: last.c,
            basis: bands.basis,
            inner_upper: bands.inner_upper,
            inner_lower: bands.inner_lower,
            outer_upper: bands.outer_upper,
            outer_lower: bands.outer_lower,
            atr: bands.atr,
            adx,
            basis_slope_atr,
            outer_upper_breached,
            outer_lower_breached,
            returned_inside_inner_upper,
            returned_inside_inner_lower,
            reentry_body_ratio: candle_body_ratio(last),
            rejection_wick_ratio: rejection_wick_ratio(
                last,
                returned_inside_inner_upper,
                returned_inside_inner_lower,
            ),
            reentry_close_progress_ratio: reentry_close_progress_ratio(
                last,
                returned_inside_inner_upper,
                returned_inside_inner_lower,
            ),
            breakout_reentry_candles,
            bullish_momentum_break: last.c > last.o && last.c > previous.h,
            bearish_momentum_break: last.c < last.o && last.c < previous.l,
        })
    }

    fn next_candle_confirmation_snapshot(
        &self,
        candles: &[CandleItem],
    ) -> Option<KeltnerChannelScalperSignalSnapshot> {
        let setup_end = candles.len().checked_sub(1)?;
        let setup_candles = candles.get(..setup_end)?;
        let setup_snapshot = self.same_candle_snapshot(setup_candles)?;
        let action = setup_action_for_confirmation(
            &self.tuning.thresholds,
            &setup_snapshot,
            self.tuning.entry_mode,
        )?;
        let setup_candle = setup_candles.last()?;
        let confirmation_candle = candles.last()?;
        if !next_candle_confirms_action(setup_candle, confirmation_candle, action) {
            return None;
        }

        let thresholds = self.tuning.thresholds;
        let bands = keltner_at(candles, candles.len(), &thresholds)?;
        let adx = adx_at(
            candles,
            candles.len(),
            thresholds.adx_trend_length,
            thresholds.adx_smoothing,
        )?;
        let basis_slope_atr = basis_slope_atr(candles, &bands, &thresholds)?;
        let mut snapshot = setup_snapshot;
        snapshot.price = confirmation_candle.c;
        snapshot.basis = bands.basis;
        snapshot.inner_upper = bands.inner_upper;
        snapshot.inner_lower = bands.inner_lower;
        snapshot.outer_upper = bands.outer_upper;
        snapshot.outer_lower = bands.outer_lower;
        snapshot.atr = bands.atr;
        snapshot.adx = adx;
        snapshot.basis_slope_atr = basis_slope_atr;
        // Confirmation may flip the trade direction in Continuation mode, but the
        // setup side still belongs to the original re-entry candle for ADX gating.
        Some(snapshot)
    }

    fn min_data_length_for(tuning: KeltnerChannelScalperBacktestTuning) -> usize {
        let thresholds = tuning.thresholds;
        (thresholds.keltner_length + thresholds.adx_trend_length + thresholds.adx_smoothing + 2)
            .max(thresholds.keltner_length + tuning.reentry_lookback_candles + 2)
            .max(80)
    }
}

impl IndicatorStrategyBacktest for KeltnerChannelScalperBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        Self::min_data_length_for(self.tuning)
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        _candle: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _values: &mut Self::IndicatorValues,
        _risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let snapshot_index = self.next_snapshot_index;
        self.next_snapshot_index = self.next_snapshot_index.saturating_add(1);
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return SignalResult::default();
        }
        let Some(last) = candles.last() else {
            return SignalResult::default();
        };
        let Some(snapshot) = self.snapshot(candles, snapshot_index) else {
            return SignalResult::default();
        };
        let mut decision = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
            &self.tuning.thresholds,
            &snapshot,
            self.tuning.entry_mode,
        );
        if !self.tuning.allow_short && decision.action == KeltnerChannelScalperAction::Short {
            decision = KeltnerChannelScalperDecision {
                action: KeltnerChannelScalperAction::Flat,
                reasons: vec!["SHORT_DISABLED".to_string()],
            };
        }
        if !self.tuning.allow_long && decision.action == KeltnerChannelScalperAction::Long {
            decision = KeltnerChannelScalperDecision {
                action: KeltnerChannelScalperAction::Flat,
                reasons: vec!["LONG_DISABLED".to_string()],
            };
        }
        if is_routine_no_setup(&decision) {
            return SignalResult::default();
        }
        let mut signal = decision.to_signal(snapshot.price, last.ts);
        if signal.should_buy || signal.should_sell {
            self.cooldown_remaining = self.tuning.cooldown_candles;
        }
        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

fn stop_and_targets(
    thresholds: &KeltnerChannelScalperThresholds,
    snapshot: &KeltnerChannelScalperSignalSnapshot,
    action: KeltnerChannelScalperAction,
) -> (f64, f64, f64, f64) {
    let risk = snapshot.atr * thresholds.stop_atr_mult;
    match action {
        KeltnerChannelScalperAction::Long => (
            snapshot.price - risk,
            snapshot.price + risk * thresholds.target_r_1,
            snapshot.price + risk * thresholds.target_r_2,
            snapshot.price + risk * thresholds.target_r_3,
        ),
        KeltnerChannelScalperAction::Short => (
            snapshot.price + risk,
            snapshot.price - risk * thresholds.target_r_1,
            snapshot.price - risk * thresholds.target_r_2,
            snapshot.price - risk * thresholds.target_r_3,
        ),
        KeltnerChannelScalperAction::Flat => (
            snapshot.price,
            snapshot.price,
            snapshot.price,
            snapshot.price,
        ),
    }
}

fn candle_body_ratio(candle: &CandleItem) -> f64 {
    let range = (candle.h - candle.l).abs();
    if range <= f64::EPSILON {
        return 0.0;
    }
    (candle.c - candle.o).abs() / range
}

fn rejection_wick_ratio(candle: &CandleItem, short_reentry: bool, long_reentry: bool) -> f64 {
    let range = (candle.h - candle.l).abs();
    if range <= f64::EPSILON {
        return 0.0;
    }
    if short_reentry && !long_reentry {
        return (candle.h - candle.o.max(candle.c)).max(0.0) / range;
    }
    if long_reentry && !short_reentry {
        return (candle.o.min(candle.c) - candle.l).max(0.0) / range;
    }
    0.0
}

fn reentry_close_progress_ratio(
    candle: &CandleItem,
    short_reentry: bool,
    long_reentry: bool,
) -> f64 {
    let range = (candle.h - candle.l).abs();
    if range <= f64::EPSILON {
        return 0.0;
    }
    if long_reentry && !short_reentry {
        return ((candle.c - candle.l) / range).clamp(0.0, 1.0);
    }
    if short_reentry && !long_reentry {
        return ((candle.h - candle.c) / range).clamp(0.0, 1.0);
    }
    0.0
}

fn setup_sides(
    snapshot: &KeltnerChannelScalperSignalSnapshot,
    entry_mode: KeltnerChannelScalperEntryMode,
) -> (bool, bool) {
    match entry_mode {
        KeltnerChannelScalperEntryMode::ExtremeMomentumReversal => (
            snapshot.outer_upper_breached && snapshot.bearish_momentum_break,
            snapshot.outer_lower_breached && snapshot.bullish_momentum_break,
        ),
        _ => (
            snapshot.outer_upper_breached && snapshot.returned_inside_inner_upper,
            snapshot.outer_lower_breached && snapshot.returned_inside_inner_lower,
        ),
    }
}

/// 返回最近一次外层突破到当前 re-entry K 线之间相隔的 K 线数。
fn breakout_reentry_candles(
    candle_count: usize,
    latest_upper_breach_index: Option<usize>,
    latest_lower_breach_index: Option<usize>,
    short_reentry: bool,
    long_reentry: bool,
) -> usize {
    let last_index = candle_count.saturating_sub(1);
    if short_reentry && !long_reentry {
        return latest_upper_breach_index
            .map(|index| last_index.saturating_sub(index))
            .unwrap_or(0);
    }
    if long_reentry && !short_reentry {
        return latest_lower_breach_index
            .map(|index| last_index.saturating_sub(index))
            .unwrap_or(0);
    }
    0
}

fn inner_reclaim_atr_distance(
    snapshot: &KeltnerChannelScalperSignalSnapshot,
    action: KeltnerChannelScalperAction,
) -> f64 {
    if snapshot.atr <= f64::EPSILON {
        return 0.0;
    }
    match action {
        KeltnerChannelScalperAction::Long => (snapshot.price - snapshot.inner_lower) / snapshot.atr,
        KeltnerChannelScalperAction::Short => {
            (snapshot.inner_upper - snapshot.price) / snapshot.atr
        }
        KeltnerChannelScalperAction::Flat => 0.0,
    }
}

fn atr_percent(snapshot: &KeltnerChannelScalperSignalSnapshot) -> f64 {
    if snapshot.price <= f64::EPSILON {
        return 0.0;
    }
    snapshot.atr / snapshot.price * 100.0
}

fn basis_slope_filter_reason(
    snapshot: &KeltnerChannelScalperSignalSnapshot,
    thresholds: &KeltnerChannelScalperThresholds,
    action: KeltnerChannelScalperAction,
) -> Option<&'static str> {
    let min_slope = thresholds.min_basis_slope_atr;
    let min_slope_reason = match action {
        KeltnerChannelScalperAction::Long if snapshot.basis_slope_atr < min_slope => {
            Some("BASIS_SLOPE_NOT_UP_FOR_LONG")
        }
        KeltnerChannelScalperAction::Short if snapshot.basis_slope_atr > -min_slope => {
            Some("BASIS_SLOPE_NOT_DOWN_FOR_SHORT")
        }
        _ => None,
    };
    if min_slope > 0.0 && min_slope_reason.is_some() {
        return min_slope_reason;
    }

    let max_adverse_slope = thresholds.max_adverse_basis_slope_atr;
    if max_adverse_slope <= 0.0 {
        return None;
    }
    match action {
        KeltnerChannelScalperAction::Long if snapshot.basis_slope_atr < -max_adverse_slope => {
            Some("ADVERSE_BASIS_SLOPE_FOR_LONG")
        }
        KeltnerChannelScalperAction::Short if snapshot.basis_slope_atr > max_adverse_slope => {
            Some("ADVERSE_BASIS_SLOPE_FOR_SHORT")
        }
        _ => None,
    }
}

fn basis_cross_filter_reason(
    snapshot: &KeltnerChannelScalperSignalSnapshot,
    thresholds: &KeltnerChannelScalperThresholds,
    action: KeltnerChannelScalperAction,
) -> Option<&'static str> {
    if !thresholds.require_basis_cross {
        return None;
    }
    match action {
        KeltnerChannelScalperAction::Long if snapshot.price < snapshot.basis => {
            Some("BASIS_NOT_CROSSED_FOR_LONG")
        }
        KeltnerChannelScalperAction::Short if snapshot.price > snapshot.basis => {
            Some("BASIS_NOT_CROSSED_FOR_SHORT")
        }
        _ => None,
    }
}

fn setup_action_for_confirmation(
    thresholds: &KeltnerChannelScalperThresholds,
    snapshot: &KeltnerChannelScalperSignalSnapshot,
    entry_mode: KeltnerChannelScalperEntryMode,
) -> Option<KeltnerChannelScalperAction> {
    if !KeltnerChannelScalperStrategy::blockers(snapshot, thresholds).is_empty() {
        return None;
    }
    let (short_setup, long_setup) = setup_sides(snapshot, entry_mode);
    if short_setup == long_setup {
        return None;
    }
    if snapshot.reentry_body_ratio < thresholds.min_reentry_body_ratio
        || snapshot.rejection_wick_ratio < thresholds.min_rejection_wick_ratio
    {
        return None;
    }
    if short_setup && snapshot.adx <= thresholds.adx_level {
        return None;
    }
    if long_setup && snapshot.adx >= thresholds.adx_level {
        return None;
    }
    if long_setup && snapshot.adx <= thresholds.min_long_adx {
        return None;
    }
    let setup_reversal_action = if short_setup {
        KeltnerChannelScalperAction::Short
    } else {
        KeltnerChannelScalperAction::Long
    };
    if inner_reclaim_atr_distance(snapshot, setup_reversal_action)
        < thresholds.min_inner_reclaim_atr
    {
        return None;
    }
    Some(match (setup_reversal_action, entry_mode) {
        (KeltnerChannelScalperAction::Short, KeltnerChannelScalperEntryMode::Reversal) => {
            KeltnerChannelScalperAction::Short
        }
        (KeltnerChannelScalperAction::Short, KeltnerChannelScalperEntryMode::Continuation) => {
            KeltnerChannelScalperAction::Long
        }
        (
            KeltnerChannelScalperAction::Short,
            KeltnerChannelScalperEntryMode::ExtremeMomentumReversal,
        ) => KeltnerChannelScalperAction::Short,
        (KeltnerChannelScalperAction::Long, KeltnerChannelScalperEntryMode::Reversal) => {
            KeltnerChannelScalperAction::Long
        }
        (KeltnerChannelScalperAction::Long, KeltnerChannelScalperEntryMode::Continuation) => {
            KeltnerChannelScalperAction::Short
        }
        (
            KeltnerChannelScalperAction::Long,
            KeltnerChannelScalperEntryMode::ExtremeMomentumReversal,
        ) => KeltnerChannelScalperAction::Long,
        (KeltnerChannelScalperAction::Flat, _) => KeltnerChannelScalperAction::Flat,
    })
}

fn next_candle_confirms_action(
    setup: &CandleItem,
    current: &CandleItem,
    action: KeltnerChannelScalperAction,
) -> bool {
    match action {
        KeltnerChannelScalperAction::Long => current.c > current.o && current.c > setup.c,
        KeltnerChannelScalperAction::Short => current.c < current.o && current.c < setup.c,
        KeltnerChannelScalperAction::Flat => false,
    }
}

fn is_supported_short_timeframe(timeframe: &str) -> bool {
    matches!(
        timeframe.trim().to_ascii_lowercase().as_str(),
        "1m" | "1min" | "5m" | "5min" | "15m" | "15min"
    )
}

fn bands_are_ordered(snapshot: &KeltnerChannelScalperSignalSnapshot) -> bool {
    snapshot.outer_upper >= snapshot.inner_upper
        && snapshot.inner_upper >= snapshot.basis
        && snapshot.basis >= snapshot.inner_lower
        && snapshot.inner_lower >= snapshot.outer_lower
}

fn is_routine_no_setup(decision: &KeltnerChannelScalperDecision) -> bool {
    decision.action == KeltnerChannelScalperAction::Flat
        && decision.reasons.len() == 1
        && decision.reasons[0] == "NO_KELTNER_REENTRY_SETUP"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(open: f64, high: f64, low: f64, close: f64) -> CandleItem {
        CandleItem {
            o: open,
            h: high,
            l: low,
            c: close,
            v: 1_000.0,
            ts: 1_783_000_000_000,
            confirm: 1,
        }
    }

    #[test]
    fn next_candle_confirmation_requires_directional_close() {
        let setup = candle(90.0, 100.0, 80.0, 95.0);
        let short_setup = candle(110.0, 112.0, 104.0, 105.0);
        let long_confirm = candle(95.0, 99.0, 94.0, 98.0);
        let weak_long = candle(95.0, 99.0, 94.0, 94.5);
        let short_confirm = candle(105.0, 106.0, 95.0, 100.0);
        let weak_short = candle(105.0, 106.0, 100.0, 106.0);

        assert!(next_candle_confirms_action(
            &setup,
            &long_confirm,
            KeltnerChannelScalperAction::Long,
        ));
        assert!(!next_candle_confirms_action(
            &setup,
            &weak_long,
            KeltnerChannelScalperAction::Long,
        ));
        assert!(next_candle_confirms_action(
            &short_setup,
            &short_confirm,
            KeltnerChannelScalperAction::Short,
        ));
        assert!(!next_candle_confirms_action(
            &short_setup,
            &weak_short,
            KeltnerChannelScalperAction::Short,
        ));
    }

    #[test]
    fn next_candle_confirmation_preserves_reentry_side_for_continuation() {
        let mut candles = (0..88)
            .map(|index| {
                let base = 100.0 + index as f64 * 0.08;
                candle(base, base + 0.8, base - 0.8, base + 0.25)
            })
            .collect::<Vec<_>>();
        candles.push(candle(112.0, 120.0, 105.0, 106.0));
        candles.push(candle(106.4, 108.0, 105.8, 107.4));

        let tuning = KeltnerChannelScalperBacktestTuning {
            confirm_next_candle: true,
            entry_mode: KeltnerChannelScalperEntryMode::Continuation,
            thresholds: KeltnerChannelScalperThresholds {
                keltner_length: 12,
                outer_multiplier: 0.60,
                inner_multiplier: 0.50,
                adx_trend_length: 3,
                adx_smoothing: 3,
                adx_level: 0.0,
                ..KeltnerChannelScalperThresholds::default()
            },
            ..KeltnerChannelScalperBacktestTuning::default()
        };
        let adapter =
            KeltnerChannelScalperBacktestAdapter::new_for_timeframe("BTC-USDT-SWAP", "5m", tuning);

        let snapshot = adapter
            .next_candle_confirmation_snapshot(&candles)
            .expect("upper re-entry should be confirmed by the next candle");

        assert!(snapshot.outer_upper_breached);
        assert!(snapshot.returned_inside_inner_upper);
        assert!(!snapshot.returned_inside_inner_lower);
        let action = KeltnerChannelScalperStrategy::evaluate_with_entry_mode(
            &tuning.thresholds,
            &snapshot,
            KeltnerChannelScalperEntryMode::Continuation,
        );
        assert_eq!(action.action, KeltnerChannelScalperAction::Long);
    }
}
