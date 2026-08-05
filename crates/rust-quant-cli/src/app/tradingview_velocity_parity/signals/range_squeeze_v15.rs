use super::SignalEvaluation;
use crate::app::tradingview_velocity_parity::model::{
    BlockedSignal, Candle, Direction, EntryIntent, ExitPolicy, IndicatorSeries, ParityRuleVersion,
    SignalFamily, StopEntryIntent,
};
use crate::app::tradingview_velocity_parity::ranges::{range_squeeze_box_v15, RangeSqueezeBoxV15};

const BREAKOUT_VOLUME_MULTIPLIER: f64 = 1.75;
const BREAKOUT_BODY_RATIO_MIN: f64 = 0.60;
const BREAKOUT_BUFFER_ATR: f64 = 0.15;
const BREAKOUT_BUFFER_RATIO: f64 = 0.001;
const MAX_BREAKOUT_DISTANCE_ATR: f64 = 0.75;
const ACCEPTANCE_WINDOW: usize = 4;
const RETEST_BAND_ATR: f64 = 0.20;
const STOP_BUFFER_ATR: f64 = 0.15;
const MAX_INITIAL_RISK_ATR: f64 = 1.50;
const MINIMUM_REWARD_RISK: f64 = 1.80;
const V16_TRIGGER_WINDOW: usize = 3;
const V16_MIN_INITIAL_RISK_ATR: f64 = 0.35;
const V16_PROJECTED_COST_BPS_PER_SIDE: f64 = 8.0;
const V16_MAX_PROJECTED_COST_R: f64 = 0.30;

/// 突破棒只冻结一次结构；后续接受棒不能移动上下沿、ATR 或中位量。
#[derive(Debug, Clone, Copy)]
struct PendingBreakoutV15 {
    direction: Direction,
    breakout_index: usize,
    range: RangeSqueezeBoxV15,
    source_atr: f64,
    source_volume_ratio: f64,
}

/// V15 独立状态机；不读取或更新旧组合策略的 EMA 压缩状态。
#[derive(Debug, Default)]
pub(super) struct RangeSqueezeStateV15 {
    pending: Option<PendingBreakoutV15>,
}

impl RangeSqueezeStateV15 {
    /// 先处理冻结突破的失效/接受，再允许当前棒建立新的真实箱体突破。
    pub(super) fn evaluate(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        tick_size: f64,
        current_position: Option<Direction>,
        entries_enabled: bool,
        rule_version: ParityRuleVersion,
    ) -> SignalEvaluation {
        let mut evaluation = SignalEvaluation::default();
        let Some(point) = indicators.get(index) else {
            return evaluation;
        };
        let Some(atr) = point.atr14.filter(|value| *value > 0.0) else {
            return evaluation;
        };
        let candle = candles[index];

        if current_position.is_some() {
            self.pending = None;
            return evaluation;
        }
        if let Some(pending) = self.pending {
            if index > pending.breakout_index {
                let age = index - pending.breakout_index;
                let invalidated = match pending.direction {
                    Direction::Long => candle.close <= pending.range.upper,
                    Direction::Short => candle.close >= pending.range.lower,
                };
                if invalidated {
                    evaluation.blocked.push(blocked(
                        candle.timestamp_ms,
                        pending.direction,
                        "V15_BREAKOUT_CLOSE_REENTERED_FROZEN_BOX",
                    ));
                    self.pending = None;
                    return evaluation;
                }
                if age > ACCEPTANCE_WINDOW {
                    evaluation.blocked.push(blocked(
                        candle.timestamp_ms,
                        pending.direction,
                        "V15_BREAKOUT_ACCEPTANCE_WINDOW_EXPIRED",
                    ));
                    self.pending = None;
                    return evaluation;
                } else if retested(candle, pending) {
                    if entries_enabled {
                        if rule_version.uses_right_side_trigger() {
                            match right_side_stop_entry(
                                candle,
                                point.rsi14,
                                index,
                                tick_size,
                                pending,
                                rule_version,
                            ) {
                                Ok(stop_entry) => evaluation.stop_entry = Some(stop_entry),
                                Err(reason) => evaluation.blocked.push(blocked(
                                    candle.timestamp_ms,
                                    pending.direction,
                                    reason,
                                )),
                            }
                        } else {
                            evaluation.intent =
                                accepted_intent(candle, point.rsi14, index, tick_size, pending);
                            if evaluation.intent.is_none() {
                                evaluation.blocked.push(blocked(
                                    candle.timestamp_ms,
                                    pending.direction,
                                    "V15_ACCEPTANCE_FAILED_STRUCTURAL_RISK_OR_REWARD",
                                ));
                            }
                        }
                    }
                    self.pending = None;
                    return evaluation;
                } else {
                    return evaluation;
                }
            }
        }

        if self.pending.is_none() {
            self.pending = breakout(candles, index, atr, tick_size);
        }
        evaluation
    }
}

/// V16/V17 把接受棒冻结为有限窗口 stop entry；经济门禁由规则版本显式决定。
fn right_side_stop_entry(
    candle: Candle,
    rsi: Option<f64>,
    index: usize,
    tick_size: f64,
    pending: PendingBreakoutV15,
    rule_version: ParityRuleVersion,
) -> Result<StopEntryIntent, &'static str> {
    let (trigger, stop, target, boundary, family) = match pending.direction {
        Direction::Long => (
            round_up(candle.high + tick_size, tick_size),
            round_down(
                candle.low.min(pending.range.upper) - STOP_BUFFER_ATR * pending.source_atr,
                tick_size,
            ),
            round_down(pending.range.upper + pending.range.height, tick_size),
            pending.range.upper,
            if rule_version.uses_v16_economic_gates() {
                SignalFamily::RangeSqueezeRightSideTriggerLong
            } else {
                SignalFamily::RangeSqueezeRightSideTriggerAblationLong
            },
        ),
        Direction::Short => (
            round_down(candle.low - tick_size, tick_size),
            round_up(
                candle.high.max(pending.range.lower) + STOP_BUFFER_ATR * pending.source_atr,
                tick_size,
            ),
            round_up(pending.range.lower - pending.range.height, tick_size),
            pending.range.lower,
            if rule_version.uses_v16_economic_gates() {
                SignalFamily::RangeSqueezeRightSideTriggerShort
            } else {
                SignalFamily::RangeSqueezeRightSideTriggerAblationShort
            },
        ),
    };
    let risk = (trigger - stop).abs();
    let reward = pending.direction.gross_pnl(trigger, target);
    if rule_version.uses_v16_economic_gates() {
        if risk < V16_MIN_INITIAL_RISK_ATR * pending.source_atr {
            return Err("V16_TRIGGER_RISK_BELOW_0_35_ATR");
        }
        if risk > MAX_INITIAL_RISK_ATR * pending.source_atr {
            return Err("V16_TRIGGER_RISK_ABOVE_1_50_ATR");
        }
        if reward <= 0.0 || reward / risk < MINIMUM_REWARD_RISK {
            return Err("V16_TRIGGER_REWARD_RISK_BELOW_1_80");
        }
        let projected_roundtrip_cost = 2.0 * trigger * V16_PROJECTED_COST_BPS_PER_SIDE / 10_000.0;
        if projected_roundtrip_cost / risk > V16_MAX_PROJECTED_COST_R {
            return Err("V16_TRIGGER_PROJECTED_COST_ABOVE_0_30R");
        }
    } else {
        if !v15_acceptance_valid(
            candle.close,
            stop,
            target,
            pending.source_atr,
            pending.direction,
        ) {
            return Err("V17_ACCEPTANCE_FAILED_V15_BASELINE");
        }
        if risk <= 0.0 || risk > MAX_INITIAL_RISK_ATR * pending.source_atr {
            return Err("V17_TRIGGER_INVALID_STRUCTURAL_RISK");
        }
        if reward <= 0.0 {
            return Err("V17_TRIGGER_INVALID_TARGET_DIRECTION");
        }
    }

    Ok(StopEntryIntent {
        intent: EntryIntent {
            signal_index: index,
            signal_time_ms: candle.timestamp_ms,
            direction: pending.direction,
            families: vec![family],
            signal_close: candle.close,
            signal_atr: pending.source_atr,
            stop_price: Some(stop),
            stop_ticks: None,
            target_price: Some(target),
            target_ticks: None,
            activation_ticks: None,
            exit_policy: ExitPolicy::RangeSqueezeStaged,
            counter_trend: false,
            signal_counter_trend_ema_age_bars_capped_600: None,
            counter_trend_structure_breakout_line: None,
            anchor_upthrust_target_consumption_ratio: None,
            active_parent_horizontal_anchor: None,
            strict_visual_range_length_bars: None,
            strict_visual_range_height: None,
            strict_visual_short_range_one_r_target: None,
            strict_visual_breakout_candle_extreme_stop: false,
            volume_ratio: Some(pending.source_volume_ratio),
            rsi,
            breakout_line: Some(boundary),
        },
        trigger_price: trigger,
        expires_at_index: index + V16_TRIGGER_WINDOW,
    })
}

/// 当前棒必须是方向实体、相对箱体中位量放大且不能离冻结边界过远。
fn breakout(
    candles: &[Candle],
    index: usize,
    atr: f64,
    tick_size: f64,
) -> Option<PendingBreakoutV15> {
    let candle = candles[index];
    let range = range_squeeze_box_v15(candles, index, atr)?;
    if range.median_volume <= 0.0 || candle.range() <= 0.0 {
        return None;
    }
    let volume_ratio = candle.volume / range.median_volume;
    if volume_ratio < BREAKOUT_VOLUME_MULTIPLIER
        || candle.body() / candle.range() < BREAKOUT_BODY_RATIO_MIN
    {
        return None;
    }

    let upper_buffer = (atr * BREAKOUT_BUFFER_ATR)
        .max(range.upper * BREAKOUT_BUFFER_RATIO)
        .max(tick_size);
    let lower_buffer = (atr * BREAKOUT_BUFFER_ATR)
        .max(range.lower * BREAKOUT_BUFFER_RATIO)
        .max(tick_size);
    let direction = if candle.close > candle.open
        && candle.close > range.upper + upper_buffer
        && (candle.close - range.upper) / atr <= MAX_BREAKOUT_DISTANCE_ATR
    {
        Direction::Long
    } else if candle.close < candle.open
        && candle.close < range.lower - lower_buffer
        && (range.lower - candle.close) / atr <= MAX_BREAKOUT_DISTANCE_ATR
    {
        Direction::Short
    } else {
        return None;
    };
    Some(PendingBreakoutV15 {
        direction,
        breakout_index: index,
        range,
        source_atr: atr,
        source_volume_ratio: volume_ratio,
    })
}

/// 接受棒必须进入冻结边界附近，同时收盘仍留在突破方向一侧。
fn retested(candle: Candle, pending: PendingBreakoutV15) -> bool {
    match pending.direction {
        Direction::Long => {
            candle.low <= pending.range.upper + RETEST_BAND_ATR * pending.source_atr
                && candle.close > pending.range.upper
        }
        Direction::Short => {
            candle.high >= pending.range.lower - RETEST_BAND_ATR * pending.source_atr
                && candle.close < pending.range.lower
        }
    }
}

/// 接受棒冻结结构止损和等高目标；预期 R 只用该完成棒收盘作成交前门禁。
fn accepted_intent(
    candle: Candle,
    rsi: Option<f64>,
    index: usize,
    tick_size: f64,
    pending: PendingBreakoutV15,
) -> Option<EntryIntent> {
    let (stop, target, boundary, family) = match pending.direction {
        Direction::Long => (
            round_down(
                candle.low.min(pending.range.upper) - STOP_BUFFER_ATR * pending.source_atr,
                tick_size,
            ),
            round_down(pending.range.upper + pending.range.height, tick_size),
            pending.range.upper,
            SignalFamily::RangeSqueezeBreakAcceptanceLong,
        ),
        Direction::Short => (
            round_up(
                candle.high.max(pending.range.lower) + STOP_BUFFER_ATR * pending.source_atr,
                tick_size,
            ),
            round_up(pending.range.lower - pending.range.height, tick_size),
            pending.range.lower,
            SignalFamily::RangeSqueezeBreakAcceptanceShort,
        ),
    };
    if !v15_acceptance_valid(
        candle.close,
        stop,
        target,
        pending.source_atr,
        pending.direction,
    ) {
        return None;
    }

    Some(EntryIntent {
        signal_index: index,
        signal_time_ms: candle.timestamp_ms,
        direction: pending.direction,
        families: vec![family],
        signal_close: candle.close,
        signal_atr: pending.source_atr,
        stop_price: Some(stop),
        stop_ticks: None,
        target_price: Some(target),
        target_ticks: None,
        activation_ticks: None,
        exit_policy: ExitPolicy::RangeSqueezeStaged,
        counter_trend: false,
        signal_counter_trend_ema_age_bars_capped_600: None,
        counter_trend_structure_breakout_line: None,
        anchor_upthrust_target_consumption_ratio: None,
        active_parent_horizontal_anchor: None,
        strict_visual_range_length_bars: None,
        strict_visual_range_height: None,
        strict_visual_short_range_one_r_target: None,
        strict_visual_breakout_candle_extreme_stop: false,
        volume_ratio: Some(pending.source_volume_ratio),
        rsi,
        breakout_line: Some(boundary),
    })
}

/// V15 的接受资格只使用接受棒收盘时已完成的信息，供 V15/V17 共用。
fn v15_acceptance_valid(
    entry_price: f64,
    stop: f64,
    target: f64,
    source_atr: f64,
    direction: Direction,
) -> bool {
    let risk = (entry_price - stop).abs();
    let reward = direction.gross_pnl(entry_price, target);
    risk > 0.0
        && risk <= MAX_INITIAL_RISK_ATR * source_atr
        && reward > 0.0
        && reward / risk >= MINIMUM_REWARD_RISK
}

/// 记录冻结突破在当时可见信息下的单一阻塞原因。
fn blocked(timestamp_ms: i64, direction: Direction, reason: &str) -> BlockedSignal {
    BlockedSignal {
        signal_time_ms: timestamp_ms,
        direction: Some(direction),
        reason: reason.to_owned(),
    }
}

/// 多头保护与目标向下对齐交易所 tick，避免生成不可成交价格。
fn round_down(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).floor() * tick_size
}

/// 空头保护与目标向上对齐交易所 tick，避免生成不可成交价格。
fn round_up(price: f64, tick_size: f64) -> f64 {
    (price / tick_size).ceil() * tick_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tradingview_velocity_parity::model::{IndicatorPoint, IndicatorSeries};

    fn candles_with_long_break_and_acceptance() -> Vec<Candle> {
        let mut candles = (0..48)
            .map(|index| {
                let recent = index >= 43;
                let upper_touch = index % 4 == 0;
                let lower_touch = index % 4 == 2;
                Candle {
                    timestamp_ms: index as i64 * 900_000,
                    open: 100.0,
                    high: if recent {
                        100.4
                    } else if upper_touch {
                        101.0
                    } else {
                        100.6
                    },
                    low: if recent {
                        99.6
                    } else if lower_touch {
                        99.0
                    } else {
                        99.4
                    },
                    close: if index % 2 == 0 { 100.2 } else { 99.8 },
                    volume: if recent { 40.0 } else { 100.0 },
                }
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            timestamp_ms: 48 * 900_000,
            open: 100.0,
            high: 101.7,
            low: 99.9,
            close: 101.5,
            volume: 200.0,
        });
        candles.push(Candle {
            timestamp_ms: 49 * 900_000,
            open: 101.4,
            high: 101.6,
            low: 101.1,
            close: 101.4,
            volume: 80.0,
        });
        candles
    }

    fn indicators(length: usize) -> IndicatorSeries {
        IndicatorSeries {
            points: (0..length)
                .map(|_| IndicatorPoint {
                    atr14: Some(1.0),
                    rsi14: Some(55.0),
                    ..IndicatorPoint::default()
                })
                .collect(),
        }
    }

    #[test]
    fn breakout_only_arms_and_next_completed_retest_creates_intent() {
        let candles = candles_with_long_break_and_acceptance();
        let indicators = indicators(candles.len());
        let mut state = RangeSqueezeStateV15::default();

        let source = state.evaluate(
            &candles,
            &indicators,
            48,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV15,
        );
        assert!(source.intent.is_none());
        let accepted = state.evaluate(
            &candles,
            &indicators,
            49,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV15,
        );
        let intent = accepted.intent.expect("accepted next-bar signal");

        assert_eq!(intent.direction, Direction::Long);
        assert_eq!(
            intent.families,
            vec![SignalFamily::RangeSqueezeBreakAcceptanceLong]
        );
        assert!((intent.stop_price.expect("stop") - 100.8).abs() < 1e-9);
        assert_eq!(intent.target_price, Some(103.0));
        assert_eq!(intent.breakout_line, Some(101.0));
        assert_eq!(intent.exit_policy, ExitPolicy::RangeSqueezeStaged);
    }

    #[test]
    fn close_back_inside_frozen_box_invalidates_without_rearming() {
        let mut candles = candles_with_long_break_and_acceptance();
        candles[49].close = 100.9;
        candles[49].low = 100.8;
        let indicators = indicators(candles.len());
        let mut state = RangeSqueezeStateV15::default();

        state.evaluate(
            &candles,
            &indicators,
            48,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV15,
        );
        let invalidated = state.evaluate(
            &candles,
            &indicators,
            49,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV15,
        );

        assert!(invalidated.intent.is_none());
        assert!(invalidated
            .blocked
            .iter()
            .any(|blocked| blocked.reason == "V15_BREAKOUT_CLOSE_REENTERED_FROZEN_BOX"));
    }

    #[test]
    fn v16_acceptance_arms_micro_break_instead_of_next_open_market_entry() {
        let mut candles = candles_with_long_break_and_acceptance();
        candles[49].high = 101.4;
        candles[49].close = 101.3;
        let indicators = indicators(candles.len());
        let mut state = RangeSqueezeStateV15::default();

        state.evaluate(
            &candles,
            &indicators,
            48,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV16,
        );
        let accepted = state.evaluate(
            &candles,
            &indicators,
            49,
            0.1,
            None,
            true,
            ParityRuleVersion::CandidateV16,
        );
        let stop_entry = accepted.stop_entry.expect("V16 stop entry");

        assert!(accepted.intent.is_none());
        assert_eq!(stop_entry.intent.direction, Direction::Long);
        assert_eq!(
            stop_entry.intent.families,
            vec![SignalFamily::RangeSqueezeRightSideTriggerLong]
        );
        assert!((stop_entry.trigger_price - 101.5).abs() < 1e-9);
        assert_eq!(stop_entry.expires_at_index, 52);
    }

    #[test]
    fn v17_keeps_v15_acceptance_but_removes_v16_minimum_risk_gate() {
        let mut candles = candles_with_long_break_and_acceptance();
        candles[49].open = 101.02;
        candles[49].high = 101.10;
        candles[49].low = 101.00;
        candles[49].close = 101.05;
        let indicators = indicators(candles.len());

        let mut v16 = RangeSqueezeStateV15::default();
        v16.evaluate(
            &candles,
            &indicators,
            48,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV16,
        );
        let v16_evaluation = v16.evaluate(
            &candles,
            &indicators,
            49,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV16,
        );
        assert!(v16_evaluation.stop_entry.is_none());
        assert!(v16_evaluation
            .blocked
            .iter()
            .any(|blocked| blocked.reason == "V16_TRIGGER_RISK_BELOW_0_35_ATR"));

        let mut v17 = RangeSqueezeStateV15::default();
        v17.evaluate(
            &candles,
            &indicators,
            48,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV17,
        );
        let v17_evaluation = v17.evaluate(
            &candles,
            &indicators,
            49,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV17,
        );
        let stop_entry = v17_evaluation.stop_entry.expect("V17 stop entry");

        assert_eq!(
            stop_entry.intent.families,
            vec![SignalFamily::RangeSqueezeRightSideTriggerAblationLong]
        );
        assert!((stop_entry.trigger_price - 101.11).abs() < 1e-9);

        let mut v18 = RangeSqueezeStateV15::default();
        v18.evaluate(
            &candles,
            &indicators,
            48,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV18,
        );
        let v18_evaluation = v18.evaluate(
            &candles,
            &indicators,
            49,
            0.01,
            None,
            true,
            ParityRuleVersion::CandidateV18,
        );
        let v18_stop_entry = v18_evaluation.stop_entry.expect("V18 stop entry");
        assert_eq!(v18_stop_entry.intent.families, stop_entry.intent.families);
        assert!((v18_stop_entry.trigger_price - stop_entry.trigger_price).abs() < 1e-9);
    }
}
