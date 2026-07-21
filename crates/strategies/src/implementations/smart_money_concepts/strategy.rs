use super::types::{
    round_price, CausalMarketStructureFeatures, SmartMoneyConceptsAction,
    SmartMoneyConceptsBacktestTuning, SmartMoneyConceptsDecision, SmartMoneyConceptsEvent,
    SmartMoneyConceptsSignalSnapshot, SmartMoneyConceptsThresholds,
};
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde_json::json;

mod causal_features;

use causal_features::{causal_market_structure_feature_series, causal_market_structure_features};

/// Smart Money Concepts v1 research strategy，复刻结构突破思想但禁止使用未来函数。
pub struct SmartMoneyConceptsStrategy;

impl SmartMoneyConceptsStrategy {
    /// 提取可供其他策略分层的因果 BOS/CHoCH/FVG 特征，不生成交易信号。
    pub fn causal_market_structure_features(
        candles: &[CandleItem],
        pivot_confirmation_bars: usize,
    ) -> CausalMarketStructureFeatures {
        causal_market_structure_features(candles, pivot_confirmation_bars)
    }

    /// 一次前向扫描返回每根已完成 K 线的因果结构状态，供长窗口研究复用。
    pub fn causal_market_structure_feature_series(
        candles: &[CandleItem],
        pivot_confirmation_bars: usize,
    ) -> Vec<CausalMarketStructureFeatures> {
        causal_market_structure_feature_series(candles, pivot_confirmation_bars)
    }

    /// 基于已确认结构快照评估交易方向；缺少止损保护时必须返回 Flat。
    pub fn evaluate(
        thresholds: &SmartMoneyConceptsThresholds,
        snapshot: &SmartMoneyConceptsSignalSnapshot,
    ) -> SmartMoneyConceptsDecision {
        let blockers = Self::blockers(thresholds, snapshot);
        if !blockers.is_empty() {
            return Self::decision(SmartMoneyConceptsAction::Flat, blockers);
        }
        let action = if snapshot.event.is_bullish() {
            SmartMoneyConceptsAction::Long
        } else if snapshot.event.is_bearish() {
            SmartMoneyConceptsAction::Short
        } else {
            SmartMoneyConceptsAction::Flat
        };
        let Some((stop, target_1, target_2, target_3)) =
            Self::stop_and_targets(thresholds, snapshot, action)
        else {
            return Self::decision(
                SmartMoneyConceptsAction::Flat,
                vec!["STRUCTURE_STOP_NOT_PROTECTED".to_string()],
            );
        };
        Self::decision(
            action,
            vec![
                snapshot.event.reason().to_string(),
                format!("STOP_PRICE:{}", round_price(stop)),
                format!("TARGET_1:{}", round_price(target_1)),
                format!("TARGET_2:{}", round_price(target_2)),
                format!("TARGET_3:{}", round_price(target_3)),
            ],
        )
    }

    /// 缺少有效结构快照时返回 flat，避免用单根 K 线伪造 live 信号。
    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            SmartMoneyConceptsAction::Flat,
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
            SmartMoneyConceptsBacktestTuning::default(),
        )
    }

    /// 指定调参回测入口；复用统一 pipeline 的成交、止损和审计口径。
    pub fn run_test_with_tuning(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: SmartMoneyConceptsBacktestTuning,
    ) -> BackTestResult {
        run_indicator_strategy_backtest(
            inst_id,
            SmartMoneyConceptsBacktestAdapter::new(inst_id, tuning),
            candles,
            risk,
        )
    }

    fn blockers(
        thresholds: &SmartMoneyConceptsThresholds,
        snapshot: &SmartMoneyConceptsSignalSnapshot,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        Self::push_if(
            snapshot.event == SmartMoneyConceptsEvent::None,
            "NO_STRUCTURE_BREAK",
            &mut reasons,
        );
        Self::push_if(snapshot.atr <= 0.0, "ATR_NOT_READY", &mut reasons);
        Self::push_if(
            snapshot.entry_extension_atr > thresholds.max_entry_extension_atr,
            "ENTRY_TOO_EXTENDED_FROM_BREAK",
            &mut reasons,
        );
        Self::push_if(
            thresholds.require_retest
                && snapshot.retest_distance_atr > thresholds.max_retest_distance_atr,
            "RETEST_TOO_FAR_FROM_ORDER_BLOCK",
            &mut reasons,
        );
        let has_structure_event = snapshot.event != SmartMoneyConceptsEvent::None;
        Self::push_if(
            thresholds.require_trend_alignment && has_structure_event && !trend_aligned(snapshot),
            "TREND_NOT_ALIGNED",
            &mut reasons,
        );
        if thresholds.require_premium_discount_zone && has_structure_event {
            match snapshot.range_position_pct {
                Some(position) if snapshot.event.is_bullish() && position > 50.0 => {
                    reasons.push("NOT_IN_DISCOUNT_ZONE".to_string());
                }
                Some(position) if snapshot.event.is_bearish() && position < 50.0 => {
                    reasons.push("NOT_IN_PREMIUM_ZONE".to_string());
                }
                Some(_) => {}
                None => reasons.push("SWING_RANGE_NOT_READY".to_string()),
            }
        }
        Self::push_if(
            thresholds.require_trend_alignment
                && has_structure_event
                && snapshot.trend_strength_pct < thresholds.min_trend_strength_pct,
            "TREND_TOO_WEAK",
            &mut reasons,
        );
        Self::push_if(
            has_structure_event
                && snapshot.displacement_body_atr < thresholds.min_displacement_body_atr,
            "DISPLACEMENT_BODY_TOO_WEAK",
            &mut reasons,
        );
        let atr_pct = atr_pct(snapshot);
        Self::push_if(
            has_structure_event && atr_pct < thresholds.min_atr_pct,
            "VOLATILITY_TOO_LOW",
            &mut reasons,
        );
        Self::push_if(
            has_structure_event && atr_pct > thresholds.max_atr_pct,
            "VOLATILITY_TOO_HIGH",
            &mut reasons,
        );
        reasons
    }

    fn stop_and_targets(
        thresholds: &SmartMoneyConceptsThresholds,
        snapshot: &SmartMoneyConceptsSignalSnapshot,
        action: SmartMoneyConceptsAction,
    ) -> Option<(f64, f64, f64, f64)> {
        match action {
            SmartMoneyConceptsAction::Long => {
                let anchor = min_optional(snapshot.protected_low, snapshot.order_block_low)?;
                let stop = anchor - snapshot.atr * thresholds.stop_atr_buffer;
                if stop >= snapshot.price {
                    return None;
                }
                Some(targets_from_r(snapshot.price, stop, thresholds, true))
            }
            SmartMoneyConceptsAction::Short => {
                let anchor = max_optional(snapshot.protected_high, snapshot.order_block_high)?;
                let stop = anchor + snapshot.atr * thresholds.stop_atr_buffer;
                if stop <= snapshot.price {
                    return None;
                }
                Some(targets_from_r(snapshot.price, stop, thresholds, false))
            }
            SmartMoneyConceptsAction::Flat => None,
        }
    }

    fn decision(
        action: SmartMoneyConceptsAction,
        reasons: Vec<String>,
    ) -> SmartMoneyConceptsDecision {
        SmartMoneyConceptsDecision { action, reasons }
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}

/// 回测适配器只维护 OHLCV 可复现状态；FVG/OB 只使用当前 bar 之前已完成的 K 线。
#[derive(Debug, Clone)]
struct SmartMoneyConceptsBacktestAdapter {
    symbol: String,
    tuning: SmartMoneyConceptsBacktestTuning,
    cooldown_remaining: usize,
    pending_setup: Option<PendingStructureSetup>,
}

impl SmartMoneyConceptsBacktestAdapter {
    fn new(inst_id: &str, tuning: SmartMoneyConceptsBacktestTuning) -> Self {
        Self {
            symbol: inst_id.to_string(),
            tuning,
            cooldown_remaining: 0,
            pending_setup: None,
        }
    }

    fn snapshot(&mut self, candles: &[CandleItem]) -> Option<SmartMoneyConceptsSignalSnapshot> {
        let last = candles.last()?;
        let previous = candles.get(candles.len().saturating_sub(2))?;
        let state = confirmed_structure(candles, self.tuning.pivot_confirmation_bars)?;
        let atr = average_true_range(candles, 14)?;
        let trend = trend_context(
            candles,
            self.tuning.trend_fast_window,
            self.tuning.trend_slow_window,
        )?;
        if let Some(mut setup) = self.pending_setup.take() {
            setup.waited_candles += 1;
            if setup.waited_candles <= self.tuning.retest_max_wait_candles {
                let retest_distance = distance_from_candle_to_zone(
                    last,
                    setup.order_block.low,
                    setup.order_block.high,
                );
                if retest_distance <= 0.0 {
                    return Some(snapshot_from_setup(
                        &self.symbol,
                        last,
                        atr,
                        trend,
                        setup,
                        0.0,
                    ));
                }
                self.pending_setup = Some(setup);
            }
        }
        let event = structure_event(last, previous, &state)
            .or_else(|| {
                self.tuning
                    .enable_liquidity_sweep
                    .then(|| liquidity_sweep_event(last, &state))
                    .flatten()
            })
            .or_else(|| {
                self.tuning
                    .enable_fair_value_gap
                    .then(|| fair_value_gap_event(candles))
                    .flatten()
            });
        let Some(event) = event else {
            return None;
        };
        let setup = setup_for_event(candles, &state, event)?;
        let retest_distance =
            distance_to_zone(last.c, setup.order_block.low, setup.order_block.high) / atr.max(1e-9);
        if self.tuning.thresholds.require_retest {
            if retest_distance <= self.tuning.thresholds.max_retest_distance_atr {
                return Some(snapshot_from_setup(
                    &self.symbol,
                    last,
                    atr,
                    trend,
                    setup,
                    retest_distance,
                ));
            }
            self.pending_setup = Some(setup);
            return None;
        }
        Some(snapshot_from_setup(
            &self.symbol,
            last,
            atr,
            trend,
            setup,
            retest_distance,
        ))
    }
}

impl IndicatorStrategyBacktest for SmartMoneyConceptsBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        (self.tuning.pivot_confirmation_bars * 2 + 5)
            .max(self.tuning.trend_slow_window)
            .max(11)
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
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return SignalResult::default();
        }
        let Some(last) = candles.last() else {
            return SignalResult::default();
        };
        let Some(snapshot) = self.snapshot(candles) else {
            return SignalResult::default();
        };
        let Some(snapshot) = (if self.tuning.fade_signal {
            fade_snapshot(snapshot)
        } else {
            Some(snapshot)
        }) else {
            return SignalResult::default();
        };
        if !self.tuning.allow_short && snapshot.event.is_bearish() {
            return SmartMoneyConceptsDecision {
                action: SmartMoneyConceptsAction::Flat,
                reasons: vec!["SHORT_DISABLED".to_string()],
            }
            .to_signal(last.c, last.ts);
        }
        let decision = SmartMoneyConceptsStrategy::evaluate(&self.tuning.thresholds, &snapshot);
        let mut signal = decision.to_signal(snapshot.price, last.ts);
        if signal.should_buy || signal.should_sell {
            self.cooldown_remaining = self.tuning.cooldown_candles;
        }
        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

#[derive(Debug, Clone, Copy)]
struct ConfirmedPivot {
    index: usize,
    level: f64,
}

#[derive(Debug, Clone, Copy)]
struct StructureState {
    latest_high: Option<ConfirmedPivot>,
    latest_low: Option<ConfirmedPivot>,
}

#[derive(Debug, Clone, Copy)]
struct OrderBlockZone {
    low: f64,
    high: f64,
}

#[derive(Debug, Clone, Copy)]
struct PendingStructureSetup {
    waited_candles: usize,
    event: SmartMoneyConceptsEvent,
    break_level: f64,
    protected_low: Option<f64>,
    protected_high: Option<f64>,
    order_block: OrderBlockZone,
    displacement_body: f64,
    range_low: Option<f64>,
    range_high: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct TrendContext {
    bias: &'static str,
    strength_pct: f64,
}

fn confirmed_structure(candles: &[CandleItem], confirmation: usize) -> Option<StructureState> {
    if confirmation == 0 || candles.len() <= confirmation + 1 {
        return None;
    }
    let mut latest_high = None;
    let mut latest_low = None;
    for center in 0..candles.len() - confirmation {
        let after = &candles[center + 1..=center + confirmation];
        let candle = &candles[center];
        if after.iter().all(|item| candle.h > item.h) {
            latest_high = Some(ConfirmedPivot {
                index: center,
                level: candle.h,
            });
        }
        if after.iter().all(|item| candle.l < item.l) {
            latest_low = Some(ConfirmedPivot {
                index: center,
                level: candle.l,
            });
        }
    }
    Some(StructureState {
        latest_high,
        latest_low,
    })
}

fn structure_event(
    last: &CandleItem,
    previous: &CandleItem,
    state: &StructureState,
) -> Option<SmartMoneyConceptsEvent> {
    if let Some(high) = state.latest_high {
        if previous.c <= high.level && last.c > high.level {
            let event = if state.latest_low.is_some_and(|low| low.index > high.index) {
                SmartMoneyConceptsEvent::BullishBos
            } else {
                SmartMoneyConceptsEvent::BullishChoch
            };
            return Some(event);
        }
    }
    if let Some(low) = state.latest_low {
        if previous.c >= low.level && last.c < low.level {
            let event = if state.latest_high.is_some_and(|high| high.index > low.index) {
                SmartMoneyConceptsEvent::BearishBos
            } else {
                SmartMoneyConceptsEvent::BearishChoch
            };
            return Some(event);
        }
    }
    None
}

fn liquidity_sweep_event(
    last: &CandleItem,
    state: &StructureState,
) -> Option<SmartMoneyConceptsEvent> {
    if let Some(low) = state.latest_low {
        if last.l < low.level && last.c > low.level {
            return Some(SmartMoneyConceptsEvent::BullishLiquiditySweep);
        }
    }
    if let Some(high) = state.latest_high {
        if last.h > high.level && last.c < high.level {
            return Some(SmartMoneyConceptsEvent::BearishLiquiditySweep);
        }
    }
    None
}

fn fair_value_gap_event(candles: &[CandleItem]) -> Option<SmartMoneyConceptsEvent> {
    let last = candles.last()?;
    let previous = candles.get(candles.len().checked_sub(2)?)?;
    let two_back = candles.get(candles.len().checked_sub(3)?)?;
    if last.l > two_back.h && previous.c > two_back.h {
        return Some(SmartMoneyConceptsEvent::BullishFairValueGap);
    }
    if last.h < two_back.l && previous.c < two_back.l {
        return Some(SmartMoneyConceptsEvent::BearishFairValueGap);
    }
    None
}

fn order_block_for_event(
    candles: &[CandleItem],
    state: &StructureState,
    event: SmartMoneyConceptsEvent,
) -> Option<OrderBlockZone> {
    let last_index = candles.len().saturating_sub(1);
    if event.is_bullish() {
        let start = state.latest_high?.index.min(last_index);
        let candle = candles[start..last_index]
            .iter()
            .min_by(|a, b| a.l.total_cmp(&b.l))?;
        return Some(OrderBlockZone {
            low: candle.l,
            high: candle.h,
        });
    }
    let start = state.latest_low?.index.min(last_index);
    let candle = candles[start..last_index]
        .iter()
        .max_by(|a, b| a.h.total_cmp(&b.h))?;
    Some(OrderBlockZone {
        low: candle.l,
        high: candle.h,
    })
}

fn setup_for_event(
    candles: &[CandleItem],
    state: &StructureState,
    event: SmartMoneyConceptsEvent,
) -> Option<PendingStructureSetup> {
    if matches!(
        event,
        SmartMoneyConceptsEvent::BullishFairValueGap | SmartMoneyConceptsEvent::BearishFairValueGap
    ) {
        return setup_for_fair_value_gap(candles, state, event);
    }
    let last = candles.last()?;
    let break_level = match event {
        SmartMoneyConceptsEvent::BullishLiquiditySweep => state.latest_low?.level,
        SmartMoneyConceptsEvent::BearishLiquiditySweep => state.latest_high?.level,
        SmartMoneyConceptsEvent::BullishChoch | SmartMoneyConceptsEvent::BullishBos => {
            state.latest_high?.level
        }
        SmartMoneyConceptsEvent::BearishChoch | SmartMoneyConceptsEvent::BearishBos => {
            state.latest_low?.level
        }
        SmartMoneyConceptsEvent::BullishFairValueGap
        | SmartMoneyConceptsEvent::BearishFairValueGap => return None,
        SmartMoneyConceptsEvent::None => return None,
    };
    let order_block = if matches!(
        event,
        SmartMoneyConceptsEvent::BullishLiquiditySweep
            | SmartMoneyConceptsEvent::BearishLiquiditySweep
    ) {
        OrderBlockZone {
            low: last.l,
            high: last.h,
        }
    } else {
        order_block_for_event(candles, state, event)?
    };
    let protected_low = if event == SmartMoneyConceptsEvent::BullishLiquiditySweep {
        Some(last.l)
    } else {
        state.latest_low.map(|pivot| pivot.level)
    };
    let protected_high = if event == SmartMoneyConceptsEvent::BearishLiquiditySweep {
        Some(last.h)
    } else {
        state.latest_high.map(|pivot| pivot.level)
    };
    Some(PendingStructureSetup {
        waited_candles: 0,
        event,
        break_level,
        protected_low,
        protected_high,
        order_block,
        displacement_body: candle_body(last),
        range_low: state.latest_low.map(|pivot| pivot.level),
        range_high: state.latest_high.map(|pivot| pivot.level),
    })
}

fn setup_for_fair_value_gap(
    candles: &[CandleItem],
    state: &StructureState,
    event: SmartMoneyConceptsEvent,
) -> Option<PendingStructureSetup> {
    let last = candles.last()?;
    let previous = candles.get(candles.len().checked_sub(2)?)?;
    let two_back = candles.get(candles.len().checked_sub(3)?)?;
    match event {
        SmartMoneyConceptsEvent::BullishFairValueGap => {
            let zone = OrderBlockZone {
                low: two_back.h,
                high: last.l,
            };
            if zone.high <= zone.low {
                return None;
            }
            Some(PendingStructureSetup {
                waited_candles: 0,
                event,
                break_level: zone.high,
                protected_low: Some(two_back.l.min(previous.l).min(last.l)),
                protected_high: Some(two_back.h.max(previous.h).max(last.h)),
                order_block: zone,
                displacement_body: candle_body(previous),
                range_low: state.latest_low.map(|pivot| pivot.level),
                range_high: state.latest_high.map(|pivot| pivot.level),
            })
        }
        SmartMoneyConceptsEvent::BearishFairValueGap => {
            let zone = OrderBlockZone {
                low: last.h,
                high: two_back.l,
            };
            if zone.high <= zone.low {
                return None;
            }
            Some(PendingStructureSetup {
                waited_candles: 0,
                event,
                break_level: zone.low,
                protected_low: Some(two_back.l.min(previous.l).min(last.l)),
                protected_high: Some(two_back.h.max(previous.h).max(last.h)),
                order_block: zone,
                displacement_body: candle_body(previous),
                range_low: state.latest_low.map(|pivot| pivot.level),
                range_high: state.latest_high.map(|pivot| pivot.level),
            })
        }
        _ => None,
    }
}

fn snapshot_from_setup(
    symbol: &str,
    last: &CandleItem,
    atr: f64,
    trend: TrendContext,
    setup: PendingStructureSetup,
    retest_distance_atr: f64,
) -> SmartMoneyConceptsSignalSnapshot {
    SmartMoneyConceptsSignalSnapshot {
        symbol: symbol.to_string(),
        price: last.c,
        atr,
        event: setup.event,
        break_level: setup.break_level,
        protected_low: setup.protected_low,
        protected_high: setup.protected_high,
        order_block_low: Some(setup.order_block.low),
        order_block_high: Some(setup.order_block.high),
        entry_extension_atr: (last.c - setup.break_level).abs() / atr.max(1e-9),
        retest_distance_atr,
        trend_bias: trend.bias.to_string(),
        trend_strength_pct: trend.strength_pct,
        displacement_body_atr: setup.displacement_body / atr.max(1e-9),
        range_position_pct: range_position_pct(setup.range_low, setup.range_high, last.c),
    }
}

fn candle_body(candle: &CandleItem) -> f64 {
    (candle.c - candle.o).abs()
}

fn range_position_pct(low: Option<f64>, high: Option<f64>, price: f64) -> Option<f64> {
    let low = low?;
    let high = high?;
    if high <= low {
        return None;
    }
    Some(((price - low) / (high - low) * 100.0).clamp(0.0, 100.0))
}

fn fade_snapshot(
    mut snapshot: SmartMoneyConceptsSignalSnapshot,
) -> Option<SmartMoneyConceptsSignalSnapshot> {
    snapshot.event = match snapshot.event {
        SmartMoneyConceptsEvent::BullishChoch => SmartMoneyConceptsEvent::BearishChoch,
        SmartMoneyConceptsEvent::BullishBos => SmartMoneyConceptsEvent::BearishBos,
        SmartMoneyConceptsEvent::BearishChoch => SmartMoneyConceptsEvent::BullishChoch,
        SmartMoneyConceptsEvent::BearishBos => SmartMoneyConceptsEvent::BullishBos,
        SmartMoneyConceptsEvent::BullishLiquiditySweep => {
            SmartMoneyConceptsEvent::BearishLiquiditySweep
        }
        SmartMoneyConceptsEvent::BearishLiquiditySweep => {
            SmartMoneyConceptsEvent::BullishLiquiditySweep
        }
        SmartMoneyConceptsEvent::BullishFairValueGap => {
            SmartMoneyConceptsEvent::BearishFairValueGap
        }
        SmartMoneyConceptsEvent::BearishFairValueGap => {
            SmartMoneyConceptsEvent::BullishFairValueGap
        }
        SmartMoneyConceptsEvent::None => return None,
    };
    Some(snapshot)
}

fn average_true_range(candles: &[CandleItem], period: usize) -> Option<f64> {
    if candles.len() < 2 || period == 0 {
        return None;
    }
    let start = candles.len().saturating_sub(period).max(1);
    let mut sum = 0.0;
    let mut count = 0usize;
    for index in start..candles.len() {
        let current = &candles[index];
        let previous_close = candles[index - 1].c;
        let true_range = (current.h - current.l)
            .max((current.h - previous_close).abs())
            .max((current.l - previous_close).abs());
        sum += true_range;
        count += 1;
    }
    Some(sum / count.max(1) as f64)
}

fn trend_context(
    candles: &[CandleItem],
    fast_window: usize,
    slow_window: usize,
) -> Option<TrendContext> {
    let slow_window = slow_window.max(fast_window);
    let fast_average = average_close(candles, fast_window)?;
    let slow_average = average_close(candles, slow_window)?;
    let diff = fast_average - slow_average;
    let price = candles.last()?.c.abs().max(1e-9);
    let bias = if diff > 0.0 {
        "long"
    } else if diff < 0.0 {
        "short"
    } else {
        "flat"
    };
    Some(TrendContext {
        bias,
        strength_pct: diff.abs() / price * 100.0,
    })
}

fn average_close(candles: &[CandleItem], window: usize) -> Option<f64> {
    if window == 0 || candles.len() < window {
        return None;
    }
    let start = candles.len() - window;
    let sum = candles[start..].iter().map(|item| item.c).sum::<f64>();
    Some(sum / window as f64)
}

fn trend_aligned(snapshot: &SmartMoneyConceptsSignalSnapshot) -> bool {
    match snapshot.trend_bias.as_str() {
        "long" => snapshot.event.is_bullish(),
        "short" => snapshot.event.is_bearish(),
        _ => false,
    }
}

fn atr_pct(snapshot: &SmartMoneyConceptsSignalSnapshot) -> f64 {
    snapshot.atr / snapshot.price.abs().max(1e-9) * 100.0
}

fn distance_to_zone(price: f64, low: f64, high: f64) -> f64 {
    if price < low {
        low - price
    } else if price > high {
        price - high
    } else {
        0.0
    }
}

fn distance_from_candle_to_zone(candle: &CandleItem, low: f64, high: f64) -> f64 {
    if candle.h < low {
        low - candle.h
    } else if candle.l > high {
        candle.l - high
    } else {
        0.0
    }
}

fn targets_from_r(
    price: f64,
    stop: f64,
    thresholds: &SmartMoneyConceptsThresholds,
    is_long: bool,
) -> (f64, f64, f64, f64) {
    let risk = (price - stop).abs();
    if is_long {
        (
            stop,
            price + risk * thresholds.target_r_1,
            price + risk * thresholds.target_r_2,
            price + risk * thresholds.target_r_3,
        )
    } else {
        (
            stop,
            price - risk * thresholds.target_r_1,
            price - risk * thresholds.target_r_2,
            price - risk * thresholds.target_r_3,
        )
    }
}

fn min_optional(first: Option<f64>, second: Option<f64>) -> Option<f64> {
    match (first, second) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn max_optional(first: Option<f64>, second: Option<f64>) -> Option<f64> {
    match (first, second) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}
