mod anchor_failed_acceptance;
#[cfg(test)]
mod base_tests;
mod composite;
mod current_additions;
mod ema_short_ablation;
mod ema_trend_long_research;
mod intent_builder;
mod intent_context;
#[cfg(test)]
mod intent_tests;
mod large_structures;
mod range_squeeze_v15;
#[cfg(test)]
mod recent_horizontal_upthrust_tests;
mod sell_climax_base_reclaim;
mod util;
mod v10;
mod v12;
mod v4;
mod v5;
mod v6;
mod v7;
mod v8;
use super::indicators::crossover;
use super::model::{
    AnchorUpthrustResearchVariant, BlockedSignal, Candle, Direction, EmaShortResearchVariant,
    EmaTrendLongResearchVariant, EntryIntent, ExitPolicy, IndicatorPoint, IndicatorSeries,
    ParityRuleVersion, SellClimaxBaseReclaimResearchVariant, SignalFamily, StopEntryIntent,
    StrictVisualBreakoutResearchVariant,
};
use super::ranges::{confirmed_anchor_range, nearest_sideways_zone, ConfirmedRange};
use super::strict_visual_breakout::{StrictVisualBreakoutSignal, StrictVisualLongEntryState};
use anchor_failed_acceptance::{
    arm_recent_horizontal_first_break, evaluate_failed_acceptance_only,
    evaluate_pending as evaluate_false_breakout_pending, FalseBreakoutPending, FalseBreakoutSignal,
    UpthrustFailedAcceptanceSignal, UpthrustRightSidePending,
};
use current_additions::{
    evaluate_bollinger_lower_reclaim_long, evaluate_effort_no_result_short,
    evaluate_ema596_reclaim_departure_long, evaluate_ema596_reclaim_departure_long_v10,
    evaluate_ema596_reclaim_departure_long_v11, BollingerLowerReclaimLongResult,
    EffortNoResultShortResult, Ema596ReclaimDepartureLongResult,
};
pub use ema_short_ablation::take_profit_tier as volume_take_profit_atr;
use ema_short_ablation::{take_profit_tier, EmaShortAblationState, EmaShortSignal};
use ema_trend_long_research::research_source_constraints_pass;
use intent_builder::build_intent;
#[cfg(test)]
use intent_builder::{preserve_transition_stop, transition_entry_target};
use intent_context::IntentContext;
use large_structures::{
    horizontal_signal as large_horizontal_signal, triangle_signal as large_triangle_signal,
};
use range_squeeze_v15::RangeSqueezeStateV15;
use util::{
    append_family, blocked, distance_ticks, is_recent, maximum, mean, minimum, prior_extremes,
    prior_values, round_down, round_up, slope,
};
use v10::{evaluate_rsi_patterns, EmaExpansionPolicyV10, SignalStateV10};
use v12::SignalStateV12;
use v4::{
    counter_trend_structure_target_v4, divergence_candle_confirmed, fresh_sideways_break,
    select_short_structure_target_v4,
};
use v5::{
    blocks_pure_rsi_neutral_v5, counter_trend_ema_age_capped_600, rsi_counter_trend_plan_v5,
    RsiCounterTrendPlanV5, EMA_ALIGNMENT_AGE_CAP,
};
use v6::{EmaTrendLongAcceptanceV6, EmaTrendLongStateV6};
use v7::{accepted_bearish_engulfing, accepted_bullish_engulfing, guard_divergence_opposing_wicks};
use v8::{fresh_slow_ema_band_reclaim, strong_bullish_volume_impulse, V8_BLOCK_REASON};
const ANCHOR_MIN_DISTANCE: usize = 5;
const ANCHOR_MAX_DISTANCE: usize = 32;
const DIVERGENCE_WEAK_MAX_GAP: usize = 7;
const EMA_EXPANSION_COOLDOWN: usize = 12;
const EMA_EXPANSION_STRUCTURE_LOOKBACK: usize = 20;
const RECENT_MACD_DIRECTION_LOOKBACK: usize = 3;
/// 当前 K 线信号计算的结果。
#[derive(Debug, Default)]
pub struct SignalEvaluation {
    pub intent: Option<EntryIntent>,
    pub stop_entry: Option<StopEntryIntent>,
    pub blocked: Vec<BlockedSignal>,
}
/// 稳健箱体突破后的接受确认状态，只保留信号时已经确定的边界与目标档位。
#[derive(Debug, Clone)]
struct ConfirmedRangePending {
    breakout_index: usize,
    range: ConfirmedRange,
    volume_ratio: f64,
    take_profit_atr: f64,
    retest_band: f64,
}
/// 箱体突破获得市场接受后交给订单意图层的冻结参数。
#[derive(Debug, Clone, Copy)]
struct AcceptedRangeSignal {
    raw_high: f64,
    volume_ratio: f64,
    take_profit_atr: f64,
}
/// 二次扫高失败时冻结的盘整低点，作为逆势空单结构目标。
#[derive(Debug, Clone, Copy)]
struct TransitionSweepSignal {
    consolidation_low: f64,
}
/// 当前确认棒相对最近完整量能锚点的 RSI 背离方向。
#[derive(Debug, Clone, Copy, Default)]
struct Divergence {
    bullish: bool,
    bearish: bool,
}
/// 仅由当前与前一根已完成 K 线确定的基础蜡烛形态。
#[derive(Debug, Clone, Copy, Default)]
struct CandlePatterns {
    bullish_engulfing: bool,
    bearish_engulfing: bool,
    long_lower_shadow: bool,
    long_upper_shadow: bool,
}
/// 只在时间向前推进时更新的 Pine `var` 状态。
#[derive(Debug, Default)]
pub struct SignalState {
    last_bullish_impulse: Option<usize>,
    last_ema12_cross_144_up: Option<usize>,
    last_ema12_cross_596_up: Option<usize>,
    last_ema_expansion_signal: Option<usize>,
    false_breakout_pending: Option<FalseBreakoutPending>,
    /// V23/V24 与旧 20 根极值假突破分离，保证更换锚区不会改变既有家族候选和退出归因。
    recent_horizontal_upthrust_pending: Option<FalseBreakoutPending>,
    /// V21 的单棒右侧确认窗口；与 V20 的冻结突破状态分离，防止延长确认期限。
    upthrust_right_side_pending: Option<UpthrustRightSidePending>,
    confirmed_range_pending: Option<ConfirmedRangePending>,
    post_breakout_guard: Option<(usize, f64)>,
    recent_bullish_transition: Vec<bool>,
    ema_compression_distance_atr: Vec<Option<f64>>,
    ema12_slope_atr: Vec<Option<f64>>,
    ema144_slope_atr: Vec<Option<f64>>,
    ema596_slope_atr: Vec<Option<f64>>,
    ema_long_expansion_state: Vec<bool>,
    ema_short_expansion_state: Vec<bool>,
    ema_trend_long_v6: EmaTrendLongStateV6,
    v10: SignalStateV10,
    v12: SignalStateV12,
    range_squeeze_v15: RangeSqueezeStateV15,
    reject_false_breakout_lower_wick: bool,
    enable_upthrust_failed_acceptance: bool,
    anchor_upthrust_research_variant: AnchorUpthrustResearchVariant,
    ema_short_ablation: EmaShortAblationState,
    ema_trend_long_research_variant: EmaTrendLongResearchVariant,
    sell_climax_base_reclaim_variant: SellClimaxBaseReclaimResearchVariant,
    strict_visual_breakout: StrictVisualLongEntryState,
    strict_visual_breakout_variant: StrictVisualBreakoutResearchVariant,
}
impl SignalState {
    /// 绑定互斥的 Research-only 变量；调用方负责保证一次只改变一个研究维度。
    pub fn new_with_research_variants(
        ema_short_variant: EmaShortResearchVariant,
        ema_trend_long_research_variant: EmaTrendLongResearchVariant,
        sell_climax_base_reclaim_variant: SellClimaxBaseReclaimResearchVariant,
        anchor_upthrust_research_variant: AnchorUpthrustResearchVariant,
        strict_visual_breakout_variant: StrictVisualBreakoutResearchVariant,
    ) -> Self {
        Self {
            ema_short_ablation: EmaShortAblationState::new(ema_short_variant),
            ema_trend_long_research_variant,
            sell_climax_base_reclaim_variant,
            anchor_upthrust_research_variant,
            strict_visual_breakout_variant,
            ..Self::default()
        }
    }
    pub fn evaluate(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        tick_size: f64,
        current_position: Option<Direction>,
        entries_enabled: bool,
        rule_version: ParityRuleVersion,
    ) -> SignalEvaluation {
        if rule_version.is_v18_composite() || rule_version.is_v19_composite() {
            return composite::evaluate(
                self,
                candles,
                indicators,
                index,
                tick_size,
                current_position,
                entries_enabled,
                rule_version,
            );
        }
        if rule_version.includes_v15_range_squeeze() {
            return self.range_squeeze_v15.evaluate(
                candles,
                indicators,
                index,
                tick_size,
                current_position,
                entries_enabled,
                rule_version,
            );
        }
        let mut result = SignalEvaluation::default();
        let Some(point) = indicators.get(index) else {
            return result;
        };
        let Some(atr) = point.atr14.filter(|value| *value > 0.0) else {
            self.push_empty_runtime_state();
            return result;
        };
        let candle = candles[index];
        let take_profit_atr = point.filtered_volume_ratio.and_then(take_profit_tier);
        let strict_visual_breakout = self
            .strict_visual_breakout_variant
            .is_enabled()
            .then(|| {
                self.strict_visual_breakout
                    .update(
                        candles,
                        index,
                        tick_size,
                        point.atr14,
                        point.volume_event,
                        point.filtered_volume_ratio,
                        take_profit_atr,
                        self.strict_visual_breakout_variant,
                    )
                    .and_then(|event| event.entry_signal())
            })
            .flatten();
        let strict_visual_long =
            strict_visual_breakout.filter(|signal| signal.direction == Direction::Long);
        let strict_visual_short =
            strict_visual_breakout.filter(|signal| signal.direction == Direction::Short);
        let strict_visual_breakout_line = strict_visual_long.map(|signal| signal.range.upper);
        let patterns = candle_patterns(candles, index);
        let mut divergence = divergence_at(candles, indicators, index);
        if rule_version.includes_v4_guards()
            && divergence.bullish
            && !divergence_candle_confirmed(candle, tick_size, Direction::Long)
        {
            divergence.bullish = false;
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Long,
                "V4_RSI_BULL_DIV_REQUIRES_BULL_BODY_OR_LOWER_WICK",
            ));
        }
        if rule_version.includes_v4_guards()
            && divergence.bearish
            && !divergence_candle_confirmed(candle, tick_size, Direction::Short)
        {
            divergence.bearish = false;
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Short,
                "V4_RSI_BEAR_DIV_REQUIRES_BEAR_BODY_OR_UPPER_WICK",
            ));
        }
        if rule_version.includes_v7_guards() {
            result.blocked.extend(guard_divergence_opposing_wicks(
                &mut divergence,
                patterns,
                candle.timestamp_ms,
            ));
        }
        if rule_version.includes_v3_guards() && divergence.bullish {
            // MACD 死叉只移除底背离家族，不能误伤同棒独立成立的其他多单结构。
            if recent_macd_death_cross(indicators, index) != Some(false) {
                divergence.bullish = false;
                result.blocked.push(blocked(
                    candle.timestamp_ms,
                    Direction::Long,
                    "V3_RSI_BULL_DIV_RECENT_MACD_DEAD_CROSS_3",
                ));
            }
        }
        let runtime = self.update_runtime_series(candles, indicators, index, atr, rule_version);
        let rsi = point.rsi14.unwrap_or(f64::NAN);
        let ema12 = point.ema12.unwrap_or(f64::NAN);
        let ema144 = point.ema144.unwrap_or(f64::NAN);
        let ema596 = point.ema596.unwrap_or(f64::NAN);
        let ema696 = point.ema696.unwrap_or(f64::NAN);
        let body_range_ratio = candle.body() / candle.range();
        let body_open_ratio = candle.body() / candle.open;
        let moderate_large_body =
            body_range_ratio >= 0.60 && body_open_ratio > 0.01 && body_open_ratio < 0.03;
        let bullish_engulfing_accepted =
            !rule_version.includes_v7_guards() || accepted_bullish_engulfing(patterns);
        let bearish_engulfing_accepted =
            !rule_version.includes_v7_guards() || accepted_bearish_engulfing(patterns);
        if rule_version.includes_v7_guards()
            && point.volume_event
            && !divergence.bearish
            && !divergence.bullish
            && rsi <= 30.0
            && patterns.bullish_engulfing
            && !bullish_engulfing_accepted
        {
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Long,
                "V7_RSI_OVERSOLD_ENGULF_REJECTS_LONG_UPPER_SHADOW",
            ));
        }
        if rule_version.includes_v7_guards()
            && point.volume_event
            && !divergence.bearish
            && !divergence.bullish
            && rsi >= 70.0
            && patterns.bearish_engulfing
            && !bearish_engulfing_accepted
        {
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Short,
                "V7_RSI_OVERBOUGHT_ENGULF_REJECTS_LONG_LOWER_SHADOW",
            ));
        }
        let legacy_rsi_pattern_long = point.volume_event
            && !divergence.bearish
            && !divergence.bullish
            && rsi <= 30.0
            && ((patterns.bullish_engulfing && bullish_engulfing_accepted)
                || patterns.long_lower_shadow);
        let legacy_rsi_pattern_short = point.volume_event
            && !divergence.bearish
            && !divergence.bullish
            && rsi >= 70.0
            && ((patterns.bearish_engulfing && bearish_engulfing_accepted)
                || patterns.long_upper_shadow);
        let v12_rsi = rule_version.includes_v12_guards().then(|| {
            self.v12.evaluate_rsi_patterns(
                candles,
                indicators,
                index,
                patterns,
                bullish_engulfing_accepted,
                bearish_engulfing_accepted,
                divergence.bearish || divergence.bullish,
            )
        });
        let v10_rsi = (!rule_version.includes_v12_guards()).then(|| {
            evaluate_rsi_patterns(
                candles,
                indicators,
                index,
                patterns,
                bullish_engulfing_accepted,
                bearish_engulfing_accepted,
                divergence.bearish || divergence.bullish,
                rule_version.includes_v11_guards(),
            )
        });
        let rsi_pattern_long = if let Some(v12_rsi) = v12_rsi {
            v12_rsi.long
        } else if rule_version.includes_v10_guards() {
            v10_rsi.is_some_and(|decision| decision.long)
        } else {
            legacy_rsi_pattern_long
        };
        let raw_rsi_pattern_short = if let Some(v12_rsi) = v12_rsi {
            v12_rsi.short
        } else if rule_version.includes_v10_guards() {
            v10_rsi.is_some_and(|decision| decision.short)
        } else {
            legacy_rsi_pattern_short
        };
        let mut rsi_pattern_short = raw_rsi_pattern_short && !runtime.recent_bullish_transition;
        let ema_long_source_baseline = point.volume_event
            && ema12 > ema144
            && ema144 > ema696
            && candle.close > ema12
            && candle.close > candle.open
            && moderate_large_body
            && rsi < 70.0;
        let ema_short_source = point.volume_event
            && ema12 < ema144
            && ema144 < ema696
            && candle.close < ema12
            && candle.close < candle.open
            && moderate_large_body
            && rsi > 30.0;
        let three_bar_long = three_bar_engulfing_long(candles, indicators, index, atr);
        let bollinger_lower_reclaim = rule_version
            .includes_current_additions()
            .then(|| evaluate_bollinger_lower_reclaim_long(candles, indicators, index, tick_size))
            .flatten();
        let effort_no_result_setup = rule_version
            .includes_current_additions()
            .then(|| evaluate_effort_no_result_short(candles, indicators, index, tick_size))
            .flatten();
        let transition_sweep = transition_liquidity_sweep(
            candles, indicators, self, index, ema12, ema144, ema596, rsi,
        );
        let research_variant = self.ema_trend_long_research_variant;
        let research_take_profit_atr = point.filtered_volume_ratio.and_then(|ratio| {
            take_profit_tier(ratio).or_else(|| {
                (research_variant.uses_take_profit_floor_25() && ratio >= 2.5).then_some(2.7)
            })
        });
        let research_body_open_min = if research_variant.uses_body_open_min_003() {
            0.003
        } else {
            0.01
        };
        let research_moderate_large_body = body_range_ratio >= 0.60
            && body_open_ratio > research_body_open_min
            && body_open_ratio < 0.03;
        let ema_long_research_source = research_variant.is_enabled()
            && point.weekly_volume_ready
            && point
                .weekly_volume_p80
                .is_some_and(|weekly_p80| candle.volume >= weekly_p80)
            && point
                .filtered_volume_ratio
                .is_some_and(|ratio| ratio >= 2.5)
            && ema12 > ema144
            && ema144 > ema696
            && candle.close > ema12
            && candle.close > candle.open
            && research_moderate_large_body
            && rsi < 70.0
            && research_take_profit_atr.is_some()
            && research_source_constraints_pass(
                research_variant,
                candles,
                index,
                candle,
                point,
                ema12,
                atr,
                body_open_ratio,
            );
        let ema_long_uses_research_source = !ema_long_source_baseline && ema_long_research_source;
        let ema_long_source_base = ema_long_source_baseline || ema_long_research_source;
        let ema_long_take_profit_atr = if ema_long_source_baseline {
            take_profit_atr
        } else {
            research_take_profit_atr
        };
        let ema_long_source_distance_atr_max = ema_long_uses_research_source
            .then_some(research_variant.source_distance_atr_max())
            .unwrap_or(1.25);
        let ema_short_signal = self.ema_short_ablation.evaluate(
            candles,
            indicators,
            index,
            ema_short_source,
            atr,
            take_profit_atr,
            entries_enabled && current_position != Some(Direction::Short),
        );
        let ema_short = ema_short_signal.is_some();
        let ema_trend_long_v6 = rule_version
            .includes_v6_guards()
            .then(|| {
                self.ema_trend_long_v6.evaluate(
                    candles,
                    indicators,
                    index,
                    ema_long_source_base,
                    ema_long_take_profit_atr,
                )
            })
            .flatten();
        let ema_trend_long_v10 = (rule_version.includes_v10_guards()
            && !rule_version.includes_v12_guards())
        .then(|| {
            self.v10.evaluate_ema_trend_long(
                candles,
                indicators,
                index,
                ema_long_source_base,
                ema_long_take_profit_atr,
                ema_long_source_distance_atr_max,
                ema_long_uses_research_source && research_variant.requires_bullish_retest(),
            )
        })
        .flatten();
        let ema_trend_long_v12 = rule_version
            .includes_v12_guards()
            .then(|| {
                self.v12.evaluate_ema_trend_long(
                    candles,
                    indicators,
                    index,
                    ema_long_source_base,
                    ema_long_take_profit_atr,
                    ema_long_source_distance_atr_max,
                )
            })
            .flatten();
        let ema_trend_long = ema_trend_long_v12
            .or(ema_trend_long_v10)
            .or(ema_trend_long_v6);
        let ema_long = if rule_version.includes_v6_guards() || rule_version.includes_v10_guards() {
            ema_trend_long.is_some()
        } else {
            ema_long_source_base
        };
        let large_horizontal = large_horizontal_signal(
            candles,
            indicators,
            index,
            atr,
            take_profit_atr,
            ema12,
            ema144,
            ema696,
            rsi,
        );
        let large_triangle = large_triangle_signal(
            candles,
            indicators,
            index,
            atr,
            take_profit_atr,
            ema12,
            ema144,
            ema696,
            rsi,
        );
        let (false_breakout, upthrust_failed_acceptance, accepted_range) = self
            .update_breakout_states(
                candles,
                indicators,
                index,
                atr,
                tick_size,
                take_profit_atr,
                ema12,
                ema144,
                ema696,
                rsi,
            );
        // 与 Pine 一致：旧 transition sweep 在重合棒上优先，V20 只统计独占的提前拒绝。
        let upthrust_failed_acceptance =
            upthrust_failed_acceptance.filter(|_| transition_sweep.is_none());
        let current_breakout_line = [
            accepted_range.map(|signal| signal.raw_high),
            large_horizontal.map(|signal| signal.0),
            strict_visual_breakout_line,
            large_triangle.map(|signal| signal.0),
        ]
        .into_iter()
        .flatten()
        .max_by(f64::total_cmp);
        self.release_post_breakout_guard(index, candle.close, transition_sweep.is_some());
        let ordinary_short_guard = self
            .post_breakout_guard
            .is_some_and(|(_, line)| candle.close > line)
            || current_breakout_line.is_some();
        rsi_pattern_short &= !ordinary_short_guard;
        if rule_version.includes_v8_guards()
            && rsi_pattern_short
            && patterns.long_upper_shadow
            && fresh_slow_ema_band_reclaim(candles, indicators, index)
        {
            rsi_pattern_short = false;
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Short,
                V8_BLOCK_REASON,
            ));
        }
        let effort_no_result = effort_no_result_setup.filter(|_| !ordinary_short_guard);
        let standard_long =
            if rule_version.includes_v6_guards() || rule_version.includes_v10_guards() {
                (point.volume_event
                    && (divergence.bullish || rsi_pattern_long)
                    && take_profit_atr.is_some())
                    || ema_long
            } else {
                point.volume_event
                    && (divergence.bullish || rsi_pattern_long || ema_long)
                    && take_profit_atr.is_some()
            };
        let standard_short = (point.volume_event
            && (divergence.bearish || rsi_pattern_short)
            && take_profit_atr.is_some())
            || ema_short;
        let existing_raw_long = standard_long
            || accepted_range.is_some()
            || large_horizontal.is_some()
            || strict_visual_long.is_some()
            || large_triangle.is_some()
            || runtime.ema_expansion_long
            || three_bar_long
            || bollinger_lower_reclaim.is_some();
        let ema596_reclaim_departure = (rule_version.includes_current_additions()
            && !existing_raw_long)
            .then(|| {
                if rule_version.includes_v12_guards() {
                    evaluate_ema596_reclaim_departure_long_v10(
                        candles, indicators, index, tick_size,
                    )
                } else if rule_version.includes_v11_guards() {
                    evaluate_ema596_reclaim_departure_long_v11(
                        candles, indicators, index, tick_size,
                    )
                } else if rule_version.includes_v10_guards() {
                    evaluate_ema596_reclaim_departure_long_v10(
                        candles, indicators, index, tick_size,
                    )
                } else {
                    evaluate_ema596_reclaim_departure_long(candles, indicators, index, tick_size)
                }
            })
            .flatten();
        let raw_long = existing_raw_long || ema596_reclaim_departure.is_some();
        let raw_short = standard_short
            || strict_visual_short.is_some()
            || false_breakout.is_some()
            || upthrust_failed_acceptance.is_some()
            || transition_sweep.is_some()
            || runtime.ema_expansion_short
            || effort_no_result.is_some();
        let divergence_only_long = standard_long
            && divergence.bullish
            && !ema_long
            && accepted_range.is_none()
            && large_horizontal.is_none()
            && strict_visual_long.is_none()
            && large_triangle.is_none()
            && !runtime.ema_expansion_long
            && !three_bar_long
            && bollinger_lower_reclaim.is_none()
            && ema596_reclaim_departure.is_none();
        let divergence_only_short = standard_short
            && divergence.bearish
            && !ema_short
            && false_breakout.is_none()
            && upthrust_failed_acceptance.is_none()
            && transition_sweep.is_none()
            && !runtime.ema_expansion_short
            && effort_no_result.is_none();
        let divergence_reversal_long = divergence_only_long && !(ema12 > ema144 && ema144 > ema696);
        let divergence_reversal_short =
            divergence_only_short && !(ema12 < ema144 && ema144 < ema696);
        let false_breakout_continuation = false_breakout.is_some()
            && runtime.slopes_ready
            && ema12 < ema144
            && ema144 < ema596
            && runtime.ema12_slope < 0.0
            && runtime.ema144_slope < 0.0
            && runtime.ema596_slope < 0.0
            && runtime.ema_short_spreads_expanding
            && candle.close < ema12;
        let short_trend_extension = runtime.ema_expansion_short || false_breakout_continuation;
        let counter_trend_long = ema12 < ema144 && ema144 < ema696;
        let counter_trend_short = ema12 > ema144 && ema144 > ema696;
        let trend_aligned_long = ema12 > ema144 && ema144 > ema696;
        let trend_aligned_short = ema12 < ema144 && ema144 < ema696;
        let v5_block_neutral_long = rule_version.includes_v5_guards()
            && blocks_pure_rsi_neutral_v5(
                divergence_only_long,
                counter_trend_long,
                trend_aligned_long,
            );
        let v5_block_neutral_short = rule_version.includes_v5_guards()
            && blocks_pure_rsi_neutral_v5(
                divergence_only_short,
                counter_trend_short,
                trend_aligned_short,
            );
        if v5_block_neutral_long {
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Long,
                "V5_PURE_RSI_BULL_DIV_REQUIRES_STRICT_EMA_REGIME",
            ));
        }
        if v5_block_neutral_short {
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Short,
                "V5_PURE_RSI_BEAR_DIV_REQUIRES_STRICT_EMA_REGIME",
            ));
        }
        let audit_long_ema_age =
            (rule_version.includes_v4_guards() && divergence_only_long && counter_trend_long)
                .then(|| counter_trend_ema_age_capped_600(indicators, index, Direction::Long));
        let audit_short_ema_age =
            (rule_version.includes_v4_guards() && divergence_only_short && counter_trend_short)
                .then(|| counter_trend_ema_age_capped_600(indicators, index, Direction::Short));
        let sideways = nearest_sideways_zone(candles, index, tick_size);
        let v5_long_plan = (rule_version.includes_v5_guards()
            && divergence_only_long
            && counter_trend_long)
            .then(|| {
                sideways.and_then(|zone| {
                    rsi_counter_trend_plan_v5(indicators, index, Direction::Long, zone, tick_size)
                })
            })
            .flatten();
        let v5_short_plan = (rule_version.includes_v5_guards()
            && divergence_only_short
            && counter_trend_short)
            .then(|| {
                sideways.and_then(|zone| {
                    rsi_counter_trend_plan_v5(indicators, index, Direction::Short, zone, tick_size)
                })
            })
            .flatten();
        let long_structure_target = sideways.map(|zone| {
            if let Some(plan) = v5_long_plan {
                plan.target_price
            } else if rule_version.includes_v4_guards() {
                counter_trend_structure_target_v4(
                    candles,
                    index,
                    atr,
                    tick_size,
                    Direction::Long,
                    zone,
                )
            } else {
                round_down(zone.high, tick_size)
            }
        });
        let short_sideways_target = sideways.map(|zone| {
            if rule_version.includes_v4_guards() {
                counter_trend_structure_target_v4(
                    candles,
                    index,
                    atr,
                    tick_size,
                    Direction::Short,
                    zone,
                )
            } else {
                round_up(zone.low, tick_size)
            }
        });
        // V4 fresh 上破优先横盘边界；非 fresh sweep 保留自身低点，旧版优先级不变。
        let short_fresh_sideways_target = sideways
            .filter(|zone| {
                rule_version.includes_v4_guards()
                    && counter_trend_short
                    && fresh_sideways_break(candles, index, Direction::Short, *zone)
            })
            .map(|zone| {
                counter_trend_structure_target_v4(
                    candles,
                    index,
                    atr,
                    tick_size,
                    Direction::Short,
                    zone,
                )
            });
        let transition_target =
            transition_sweep.map(|signal| round_up(signal.consolidation_low, tick_size));
        let short_structure_target = if let Some(plan) = v5_short_plan {
            Some(plan.target_price)
        } else if rule_version.includes_v4_guards() {
            select_short_structure_target_v4(
                short_fresh_sideways_target,
                transition_target,
                short_sideways_target,
            )
        } else {
            transition_target.or(short_sideways_target)
        };
        let counter_long_ready = strict_visual_long
            .and_then(|signal| signal.measured_move_target_price)
            .map_or(
                !counter_trend_long
                    || long_structure_target.is_some_and(|target| target > candle.close),
                |target| target > candle.close,
            );
        let counter_short_ready = if let Some(target) =
            strict_visual_short.and_then(|signal| signal.measured_move_target_price)
        {
            target < candle.close
        } else if let Some(signal) = upthrust_failed_acceptance {
            signal.frozen_target_low < candle.close
        } else if transition_sweep.is_some() {
            short_structure_target.is_some_and(|target| target < candle.close)
        } else {
            !counter_trend_short
                || short_structure_target.is_some_and(|target| target < candle.close)
        };
        if raw_long
            && counter_trend_long
            && !counter_long_ready
            && !three_bar_long
            && bollinger_lower_reclaim.is_none()
            && ema596_reclaim_departure.is_none()
        {
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Long,
                "逆势多单在信号时点没有位于成交价上方的已确认横盘目标",
            ));
        }
        if raw_short
            && !counter_short_ready
            && effort_no_result.is_none()
            && upthrust_failed_acceptance.is_none()
        {
            result.blocked.push(blocked(
                candle.timestamp_ms,
                Direction::Short,
                "逆势空单或扫高空单没有位于成交价下方的冻结结构目标",
            ));
        }
        let all_long = (raw_long && !v5_block_neutral_long && counter_long_ready)
            || three_bar_long
            || bollinger_lower_reclaim.is_some()
            || ema596_reclaim_departure.is_some();
        let all_short = (raw_short && !v5_block_neutral_short && counter_short_ready)
            || effort_no_result.is_some()
            || upthrust_failed_acceptance.is_some();
        if all_long && all_short {
            result.blocked.push(BlockedSignal {
                signal_time_ms: candle.timestamp_ms,
                direction: None,
                reason: "同一确认棒同时出现多空候选，按 Pine 冲突规则全部取消".to_string(),
            });
            return result;
        }
        if !entries_enabled || (!all_long && !all_short) {
            return result;
        }
        let direction = if all_long {
            Direction::Long
        } else {
            Direction::Short
        };
        if current_position == Some(direction) {
            return result;
        }
        // V6 接受棒无需再放量；未独立满足 RSI 量能条件时不得污染归因或覆盖冻结风险。
        let qualified_rsi_long = point.volume_event && take_profit_atr.is_some();
        let intent_divergence =
            if rule_version.includes_v6_guards() || rule_version.includes_v10_guards() {
                Divergence {
                    bullish: divergence.bullish && qualified_rsi_long,
                    bearish: divergence.bearish,
                }
            } else {
                divergence
            };
        let intent_rsi_pattern_long = rsi_pattern_long
            && (!(rule_version.includes_v6_guards() || rule_version.includes_v10_guards())
                || qualified_rsi_long);
        let deferred_ema_short = ema_short_signal.is_some_and(|signal| signal.deferred);
        let context = IntentContext {
            patterns,
            divergence: Divergence {
                bearish: intent_divergence.bearish && !deferred_ema_short,
                ..intent_divergence
            },
            rsi_pattern_long: intent_rsi_pattern_long,
            rsi_pattern_short: rsi_pattern_short && !deferred_ema_short,
            ema_long,
            ema_trend_long_v6: ema_trend_long,
            ema_short: ema_short_signal,
            three_bar_long,
            bollinger_lower_reclaim,
            ema596_reclaim_departure,
            accepted_range,
            large_horizontal_line: large_horizontal.map(|value| value.0),
            strict_visual_breakout: strict_visual_breakout
                .filter(|signal| signal.direction == direction),
            large_triangle_line: large_triangle.map(|value| value.0),
            false_breakout,
            upthrust_failed_acceptance,
            transition_sweep,
            ema_expansion_long: runtime.ema_expansion_long,
            ema_expansion_short: runtime.ema_expansion_short,
            effort_no_result,
            divergence_reversal_long,
            divergence_reversal_short,
            short_trend_extension,
            counter_trend_long,
            counter_trend_short,
            long_structure_target,
            short_structure_target,
            counter_trend_ema_age_audit: if direction == Direction::Long {
                audit_long_ema_age
            } else {
                audit_short_ema_age
            },
            v5_counter_trend_plan: if direction == Direction::Long {
                v5_long_plan
            } else {
                v5_short_plan
            },
            take_profit_atr,
        };
        let intent = build_intent(
            candles,
            indicators,
            index,
            tick_size,
            direction,
            context,
            rule_version,
        );
        if let Some(line) = intent.as_ref().and_then(|intent| intent.breakout_line) {
            if direction == Direction::Long {
                self.post_breakout_guard = Some((index, line));
            }
        }
        result.intent = intent;
        result
    }
    /// 按时间追加 EMA 交叉、斜率和压缩状态；不能跳索引或重复调用同一根 K 线。
    fn update_runtime_series(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        atr: f64,
        rule_version: ParityRuleVersion,
    ) -> RuntimePoint {
        let candle = candles[index];
        let point = &indicators.points[index];
        let (ema12, ema144, ema596) = match (point.ema12, point.ema144, point.ema596) {
            (Some(ema12), Some(ema144), Some(ema596)) => (ema12, ema144, ema596),
            _ => {
                self.push_empty_runtime_state();
                return RuntimePoint::default();
            }
        };
        let previous = index
            .checked_sub(1)
            .and_then(|previous| indicators.get(previous));
        let cross144 = previous.is_some_and(|previous| {
            crossover(
                ema12,
                ema144,
                previous.ema12.unwrap_or(f64::NAN),
                previous.ema144.unwrap_or(f64::NAN),
            )
        });
        let cross596 = previous.is_some_and(|previous| {
            crossover(
                ema12,
                ema596,
                previous.ema12.unwrap_or(f64::NAN),
                previous.ema596.unwrap_or(f64::NAN),
            )
        });
        let strong_impulse = strong_bullish_volume_impulse(candle, point, atr);
        if strong_impulse {
            self.last_bullish_impulse = Some(index);
        }
        if cross144 {
            self.last_ema12_cross_144_up = Some(index);
        }
        if cross596 {
            self.last_ema12_cross_596_up = Some(index);
        }
        let recent_transition = is_recent(self.last_bullish_impulse, index, 5)
            && is_recent(self.last_ema12_cross_144_up, index, 5)
            && is_recent(self.last_ema12_cross_596_up, index, 5)
            && ema12 > ema144
            && ema12 > ema596
            && candle.close > ema12;
        self.recent_bullish_transition.push(recent_transition);
        let compression_distance = Some((ema12 - ema596).abs() / atr);
        self.ema_compression_distance_atr.push(compression_distance);
        let ema12_slope = slope(indicators, index, atr, |point| point.ema12);
        let ema144_slope = slope(indicators, index, atr, |point| point.ema144);
        let ema596_slope = slope(indicators, index, atr, |point| point.ema596);
        self.ema12_slope_atr.push(ema12_slope);
        self.ema144_slope_atr.push(ema144_slope);
        self.ema596_slope_atr.push(ema596_slope);
        let prior_compression = prior_values(&self.ema_compression_distance_atr, index, 12);
        let compression_ready = prior_compression.is_some_and(|values| {
            mean(&values) <= 0.25 && maximum(&values) <= 0.50 && minimum(&values) <= 0.10
        });
        let previous_slopes = index.checked_sub(1).and_then(|previous_index| {
            Some((
                self.ema12_slope_atr
                    .get(previous_index)
                    .copied()
                    .flatten()?,
                self.ema144_slope_atr
                    .get(previous_index)
                    .copied()
                    .flatten()?,
                self.ema596_slope_atr
                    .get(previous_index)
                    .copied()
                    .flatten()?,
            ))
        });
        let slopes_ready = ema12_slope.is_some()
            && ema144_slope.is_some()
            && ema596_slope.is_some()
            && previous_slopes.is_some();
        let (ema12_slope_value, ema144_slope_value, ema596_slope_value) = (
            ema12_slope.unwrap_or(0.0),
            ema144_slope.unwrap_or(0.0),
            ema596_slope.unwrap_or(0.0),
        );
        let magnitudes_expanding = previous_slopes.is_some_and(|previous| {
            ema12_slope_value.abs() >= previous.0.abs() * 1.20
                && ema144_slope_value.abs() >= previous.1.abs() * 1.20
                && ema596_slope_value.abs() >= previous.2.abs() * 1.20
        });
        let previous_point = previous.cloned().unwrap_or_default();
        let previous_ema12 = previous_point.ema12.unwrap_or(f64::NAN);
        let previous_ema144 = previous_point.ema144.unwrap_or(f64::NAN);
        let previous_ema596 = previous_point.ema596.unwrap_or(f64::NAN);
        let long_spreads_expanding = ema12 - ema144 > previous_ema12 - previous_ema144
            && ema12 - ema596 > previous_ema12 - previous_ema596;
        let short_spreads_expanding = ema144 - ema12 > previous_ema144 - previous_ema12
            && ema596 - ema12 > previous_ema596 - previous_ema12;
        let raw_long_state = compression_ready
            && magnitudes_expanding
            && ema12_slope_value >= 0.05
            && ema144_slope_value >= 0.015
            && ema596_slope_value >= 0.0015
            && ema12 > ema144
            && ema12 > ema596
            && long_spreads_expanding
            && candle.close > ema12
            && candle.close > candle.open;
        let raw_short_state = compression_ready
            && magnitudes_expanding
            && ema12_slope_value <= -0.05
            && ema144_slope_value <= -0.015
            && ema596_slope_value <= -0.0015
            && ema12 < ema144
            && ema12 < ema596
            && short_spreads_expanding
            && candle.close < ema12
            && candle.close < candle.open;
        let long_state = raw_long_state
            && (!rule_version.includes_v3_guards()
                || recent_macd_death_cross(indicators, index) == Some(false));
        let short_structure_accepted =
            prior_extremes(candles, index, EMA_EXPANSION_STRUCTURE_LOOKBACK)
                .is_some_and(|(_, structure_low)| candle.close < structure_low);
        let short_state =
            raw_short_state && (!rule_version.includes_v3_guards() || short_structure_accepted);
        let v14_long_impulse_ready = slopes_ready
            && ema12_slope_value >= 0.05
            && ema144_slope_value >= 0.0
            && ema596_slope_value >= 0.0
            && ema12 > ema144
            && ema12 > ema596
            && long_spreads_expanding
            && candle.close > ema12
            && candle.close > candle.open
            && recent_macd_death_cross(indicators, index) == Some(false);
        let v14_short_impulse_ready = slopes_ready
            && ema12_slope_value <= -0.05
            && ema144_slope_value <= 0.0
            && ema596_slope_value <= 0.0
            && ema12 < ema144
            && ema12 < ema596
            && short_spreads_expanding
            && candle.close < ema12
            && candle.close < candle.open;
        let previous_long_state = self
            .ema_long_expansion_state
            .last()
            .copied()
            .unwrap_or(false);
        let previous_short_state = self
            .ema_short_expansion_state
            .last()
            .copied()
            .unwrap_or(false);
        let cooldown_ready = self
            .last_ema_expansion_signal
            .map(|last| index - last > EMA_EXPANSION_COOLDOWN)
            .unwrap_or(true);
        let (expansion_long, expansion_short) = if rule_version.includes_v14_guards() {
            let decision = self.v10.evaluate_ema_expansion_v14(
                candles,
                indicators,
                index,
                compression_ready,
                v14_long_impulse_ready,
                v14_short_impulse_ready,
                cooldown_ready,
            );
            (decision.long, decision.short)
        } else if rule_version.includes_v12_guards() {
            let decision = self.v12.evaluate_ema_expansion(
                candles,
                indicators,
                index,
                long_state,
                short_state,
                cooldown_ready,
            );
            (decision.long, decision.short)
        } else if rule_version.includes_v10_guards() {
            let policy = if rule_version.includes_v13_guards() {
                EmaExpansionPolicyV10::V13
            } else if rule_version.includes_v11_guards() {
                EmaExpansionPolicyV10::V11
            } else {
                EmaExpansionPolicyV10::V10
            };
            let decision = self.v10.evaluate_ema_expansion(
                candles,
                indicators,
                index,
                long_state,
                short_state,
                cooldown_ready,
                policy,
            );
            (decision.long, decision.short)
        } else {
            (
                cooldown_ready && long_state && !previous_long_state,
                cooldown_ready && short_state && !previous_short_state,
            )
        };
        if expansion_long || expansion_short {
            self.last_ema_expansion_signal = Some(index);
        }
        self.ema_long_expansion_state.push(long_state);
        self.ema_short_expansion_state.push(short_state);
        RuntimePoint {
            recent_bullish_transition: recent_transition,
            ema_expansion_long: expansion_long,
            ema_expansion_short: expansion_short,
            slopes_ready,
            ema12_slope: ema12_slope_value,
            ema144_slope: ema144_slope_value,
            ema596_slope: ema596_slope_value,
            ema_short_spreads_expanding: short_spreads_expanding,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// 推进假突破与箱体接受的有限等待窗口，所有边界均来自首次突破时的历史。
    fn update_breakout_states(
        &mut self,
        candles: &[Candle],
        indicators: &IndicatorSeries,
        index: usize,
        atr: f64,
        tick_size: f64,
        take_profit_atr: Option<f64>,
        ema12: f64,
        ema144: f64,
        ema696: f64,
        rsi: f64,
    ) -> (
        Option<FalseBreakoutSignal>,
        Option<UpthrustFailedAcceptanceSignal>,
        Option<AcceptedRangeSignal>,
    ) {
        let candle = candles[index];
        let uses_recent_horizontal_anchor = self
            .anchor_upthrust_research_variant
            .uses_recent_horizontal_first_break_anchor();
        let (false_signal, legacy_upthrust_failed_acceptance) = evaluate_false_breakout_pending(
            &mut self.false_breakout_pending,
            &mut self.upthrust_right_side_pending,
            candle,
            index,
            tick_size,
            self.reject_false_breakout_lower_wick,
            self.enable_upthrust_failed_acceptance && !uses_recent_horizontal_anchor,
            self.anchor_upthrust_research_variant,
        );
        let recent_horizontal_upthrust = uses_recent_horizontal_anchor
            .then(|| {
                evaluate_failed_acceptance_only(
                    &mut self.recent_horizontal_upthrust_pending,
                    candle,
                    index,
                    tick_size,
                    self.anchor_upthrust_research_variant,
                )
            })
            .flatten();
        let upthrust_failed_acceptance =
            legacy_upthrust_failed_acceptance.or(recent_horizontal_upthrust);
        let failure_bounds = prior_extremes(candles, index, 20);
        let anchor_breakout = failure_bounds.is_some_and(|(high, _)| {
            indicators.points[index].volume_event
                && take_profit_atr.is_some()
                && candle.close > high
                && candle.close > candle.open
        });
        let new_anchor_breakout = self.false_breakout_pending.is_none() && anchor_breakout;
        if new_anchor_breakout {
            let (anchor_high, anchor_low) = failure_bounds.expect("bounds checked above");
            self.false_breakout_pending = Some(FalseBreakoutPending {
                breakout_index: index,
                anchor_high,
                anchor_low,
                breakout_open: candle.open,
                breakout_high: candle.high,
                breakout_volume: candle.volume,
                volume_ratio: indicators.points[index]
                    .filtered_volume_ratio
                    .expect("volume event has ratio"),
                take_profit_atr: take_profit_atr.expect("breakout requires target tier"),
                active_parent_horizontal_anchor: None,
            });
        }
        if self.enable_upthrust_failed_acceptance && uses_recent_horizontal_anchor {
            arm_recent_horizontal_first_break(
                &mut self.recent_horizontal_upthrust_pending,
                candles,
                index,
                tick_size,
                indicators.points[index].volume_event,
                indicators.points[index].filtered_volume_ratio,
                take_profit_atr,
                self.anchor_upthrust_research_variant,
            );
        }

        let mut accepted_signal = None;
        if let Some(pending) = self.confirmed_range_pending.clone() {
            let age = index - pending.breakout_index;
            let retest = candle.low <= pending.range.raw_high + pending.retest_band
                && candle.close > pending.range.raw_high;
            let held_twice = index > 0
                && candle.close > pending.range.raw_high
                && candles[index - 1].close > pending.range.raw_high;
            let invalidated = candle.close < pending.range.upper;
            if (1..=3).contains(&age)
                && !invalidated
                && (retest || held_twice)
                && ema12 > ema144
                && ema144 > ema696
            {
                accepted_signal = Some(AcceptedRangeSignal {
                    raw_high: pending.range.raw_high,
                    volume_ratio: pending.volume_ratio,
                    take_profit_atr: pending.take_profit_atr,
                });
                self.confirmed_range_pending = None;
            } else if invalidated || age >= 3 {
                self.confirmed_range_pending = None;
            }
        }

        let previous_atr = index
            .checked_sub(1)
            .and_then(|previous| indicators.points[previous].atr14);
        let confirmed_range = previous_atr
            .and_then(|previous_atr| confirmed_anchor_range(candles, index, previous_atr));
        let confirmed_candidate = new_anchor_breakout
            && confirmed_range.is_some()
            && ema12 > ema144
            && ema144 > ema696
            && rsi < 70.0;
        if confirmed_candidate && self.confirmed_range_pending.is_none() {
            let range = confirmed_range.expect("candidate requires confirmed range");
            self.confirmed_range_pending = Some(ConfirmedRangePending {
                breakout_index: index,
                range,
                volume_ratio: indicators.points[index]
                    .filtered_volume_ratio
                    .expect("breakout requires ratio"),
                take_profit_atr: take_profit_atr.expect("breakout requires target"),
                retest_band: (range.raw_high * 0.001)
                    .max(previous_atr.expect("range requires previous ATR") * 0.25),
            });
        }

        let _ = atr;
        (false_signal, upthrust_failed_acceptance, accepted_signal)
    }

    /// 突破线守卫只在跌回冻结线或出现后续扫高确认时释放，防止过早反手做空。
    fn release_post_breakout_guard(&mut self, index: usize, close: f64, transition_sweep: bool) {
        let release = self
            .post_breakout_guard
            .is_some_and(|(signal_index, line)| {
                close <= line || (index > signal_index && transition_sweep)
            });
        if release {
            self.post_breakout_guard = None;
        }
    }

    /// 指标未就绪时仍追加空占位，保持所有运行序列与 K 线索引一一对应。
    fn push_empty_runtime_state(&mut self) {
        self.recent_bullish_transition.push(false);
        self.ema_compression_distance_atr.push(None);
        self.ema12_slope_atr.push(None);
        self.ema144_slope_atr.push(None);
        self.ema596_slope_atr.push(None);
        self.ema_long_expansion_state.push(false);
        self.ema_short_expansion_state.push(false);
    }
}

/// 一根确认棒完成后生成的运行态快照，斜率均以每 ATR 的三棒变化归一化。
#[derive(Debug, Default)]
struct RuntimePoint {
    recent_bullish_transition: bool,
    ema_expansion_long: bool,
    ema_expansion_short: bool,
    slopes_ready: bool,
    ema12_slope: f64,
    ema144_slope: f64,
    ema596_slope: f64,
    ema_short_spreads_expanding: bool,
}

/// 返回最近的完整量能锚点；最近锚点不合格时，背离层不得回退到更远锚点。
pub fn nearest_volume_anchor(indicators: &IndicatorSeries, index: usize) -> Option<(usize, usize)> {
    (ANCHOR_MIN_DISTANCE..=ANCHOR_MAX_DISTANCE).find_map(|offset| {
        index
            .checked_sub(offset)
            .filter(|anchor| indicators.points[*anchor].volume_event)
            .map(|anchor| (anchor, offset))
    })
}

/// 判断 `t、t-1、t-2` 是否刚发生 DIF 下穿 DEA；柱值缺历史时返回 `None`。
fn recent_macd_death_cross(indicators: &IndicatorSeries, index: usize) -> Option<bool> {
    let first = index.checked_sub(RECENT_MACD_DIRECTION_LOOKBACK - 1)?;
    let previous_first = first.checked_sub(1)?;
    indicators
        .points
        .get(previous_first..=index)?
        .iter()
        .map(|point| point.macd_histogram)
        .collect::<Option<Vec<_>>>()
        .map(|histograms| {
            histograms
                .windows(2)
                .any(|pair| pair[1] < 0.0 && pair[0] >= 0.0)
        })
}

/// 只比较最近完整量能锚点，并要求中间路径保持 RSI 中轴连续和独立波段幅度。
fn divergence_at(candles: &[Candle], indicators: &IndicatorSeries, index: usize) -> Divergence {
    let Some((anchor, gap)) = nearest_volume_anchor(indicators, index) else {
        return Divergence::default();
    };
    let point = &indicators.points[index];
    let anchor_point = &indicators.points[anchor];
    let Some((rsi, anchor_rsi, atr, anchor_atr)) = point
        .rsi14
        .zip(anchor_point.rsi14)
        .zip(point.atr14.zip(anchor_point.atr14))
        .map(|((rsi, anchor_rsi), (atr, anchor_atr))| (rsi, anchor_rsi, atr, anchor_atr))
    else {
        return Divergence::default();
    };
    if anchor + 1 >= index {
        return Divergence::default();
    }
    let path_rsi: Option<Vec<f64>> = indicators.points[anchor + 1..index]
        .iter()
        .map(|point| point.rsi14)
        .collect();
    let Some(path_rsi) = path_rsi else {
        return Divergence::default();
    };
    let path_price = &candles[anchor + 1..index];
    let path_rsi_min = minimum(&path_rsi);
    let path_rsi_max = maximum(&path_rsi);
    let path_price_low = path_price
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let path_price_high = path_price
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let atr_scale = atr.max(anchor_atr);
    if atr_scale <= 0.0 {
        return Divergence::default();
    }

    let current = candles[index];
    let anchor_candle = candles[anchor];
    let bearish_distance = current.high - anchor_candle.high;
    let bullish_distance = anchor_candle.low - current.low;
    let bearish_price_valid = bearish_distance / anchor_candle.high >= 0.0035
        && bearish_distance / atr_scale >= 0.50
        && (anchor_candle.high - path_price_low) / atr_scale >= 1.0;
    let bullish_price_valid = bullish_distance / anchor_candle.low >= 0.0035
        && bullish_distance / atr_scale >= 0.50
        && (path_price_high - anchor_candle.low) / atr_scale >= 1.0;
    let weak = gap <= DIVERGENCE_WEAK_MAX_GAP;
    let bearish = path_rsi_min >= 50.0
        && bearish_price_valid
        && rsi > 60.0
        && anchor_rsi >= 70.0
        && current.high > anchor_candle.high
        && rsi < anchor_rsi
        && (!weak || current.close < anchor_candle.high);
    let bullish = path_rsi_max <= 50.0
        && bullish_price_valid
        && rsi < 40.0
        && anchor_rsi <= 30.0
        && current.low < anchor_candle.low
        && rsi > anchor_rsi
        && (!weak || current.close > anchor_candle.low);
    Divergence { bullish, bearish }
}

/// 识别吞没与长影线；零振幅和十字星不会被误判为反转形态。
fn candle_patterns(candles: &[Candle], index: usize) -> CandlePatterns {
    let candle = candles[index];
    if !candle.is_valid() {
        return CandlePatterns::default();
    }
    let body_ratio = candle.body() / candle.range();
    let upper_shadow = candle.high - candle.open.max(candle.close);
    let lower_shadow = candle.open.min(candle.close) - candle.low;
    let is_doji = body_ratio <= 0.10;
    let long_upper_shadow =
        !is_doji && upper_shadow / candle.range() >= 0.60 && upper_shadow > lower_shadow;
    let long_lower_shadow =
        !is_doji && lower_shadow / candle.range() >= 0.60 && lower_shadow > upper_shadow;
    let previous = index.checked_sub(1).map(|previous| candles[previous]);
    let bullish_engulfing = previous.is_some_and(|previous| {
        previous.is_valid()
            && previous.close < previous.open
            && candle.close > candle.open
            && candle.open <= previous.close
            && candle.close >= previous.open
    });
    let bearish_engulfing = previous.is_some_and(|previous| {
        previous.is_valid()
            && previous.close > previous.open
            && candle.close < candle.open
            && candle.open >= previous.close
            && candle.close <= previous.open
    });
    CandlePatterns {
        bullish_engulfing,
        bearish_engulfing,
        long_lower_shadow,
        long_upper_shadow,
    }
}

/// 验证下跌末端三棒吞没；所有趋势、量能和位置证据都止于当前确认棒。
fn three_bar_engulfing_long(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    atr: f64,
) -> bool {
    if index < 48
        || !candles[index - 3..=index]
            .iter()
            .all(|candle| candle.is_valid())
    {
        return false;
    }
    let current = candles[index];
    let prior = &candles[index - 3..index];
    let bear_count = prior
        .iter()
        .filter(|candle| candle.close < candle.open)
        .count();
    let body_low = prior
        .iter()
        .map(|candle| candle.open.min(candle.close))
        .fold(f64::INFINITY, f64::min);
    let body_high = prior
        .iter()
        .map(|candle| candle.open.max(candle.close))
        .fold(f64::NEG_INFINITY, f64::max);
    let pattern_low = candles[index - 3..=index]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let prior_48_low = candles[index - 48..index]
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let previous = &indicators.points[index - 1];
    let point = &indicators.points[index];
    let down_regime = previous
        .ema12
        .zip(previous.ema144)
        .zip(previous.ema596)
        .is_some_and(|((ema12, ema144), ema596)| {
            ema12 < ema144
                && ema144 < ema596
                && ema12 < indicators.points[index - 4].ema12.unwrap_or(f64::NAN)
                && ema144 < indicators.points[index - 4].ema144.unwrap_or(f64::NAN)
                && ema596 < indicators.points[index - 4].ema596.unwrap_or(f64::NAN)
                && candles[index - 1].close < candles[index - 48].close
        });
    let rsi_crossed = point
        .rsi14
        .zip(previous.rsi14)
        .is_some_and(|(rsi, previous)| rsi > 50.0 && previous <= 50.0);

    current.close > current.open
        && bear_count >= 2
        && candles[index - 1].close < candles[index - 3].open
        && current.open <= body_low
        && current.close >= body_high
        && current.body() / current.range() >= 0.75
        && current.body() / atr >= 1.5
        && (current.close - current.low) / current.range() >= 0.90
        && previous.volume_event
        && point
            .filtered_volume_ratio
            .is_some_and(|ratio| ratio >= 2.5)
        && down_regime
        && pattern_low <= prior_48_low + 0.25 * atr
        && current.close > point.ema12.unwrap_or(f64::INFINITY)
        && current.close > point.bollinger_middle.unwrap_or(f64::INFINITY)
        && rsi_crossed
}

#[allow(clippy::too_many_arguments)]
/// 从最近的多头转折锚点向远处扫描扫高失败；首个合格锚点失败后禁止回退挑选。
fn transition_liquidity_sweep(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    state: &SignalState,
    index: usize,
    ema12: f64,
    ema144: f64,
    ema596: f64,
    rsi: f64,
) -> Option<TransitionSweepSignal> {
    for offset in 5..=16 {
        let Some(anchor) = index.checked_sub(offset) else {
            continue;
        };
        if anchor < 20
            || !state
                .recent_bullish_transition
                .get(anchor)
                .copied()
                .unwrap_or(false)
        {
            continue;
        }
        let prior_high = candles[anchor - 20..anchor]
            .iter()
            .map(|candle| candle.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let anchor_candle = candles[anchor];
        let anchor_patterns = candle_patterns(candles, anchor);
        let anchor_point = &indicators.points[anchor];
        let anchor_is_valid = anchor_point.volume_event
            && anchor_point.rsi14.is_some_and(|value| value >= 80.0)
            && anchor_patterns.long_upper_shadow
            && anchor_candle.high > prior_high
            && anchor_candle.close < prior_high;
        if !anchor_is_valid {
            continue;
        }
        let anchor_atr = anchor_point.atr14?;
        let touch_band = (anchor_candle.high * 0.0015).max(anchor_atr * 0.50);
        let path = &candles[anchor + 1..index];
        let consolidation_low = path
            .iter()
            .map(|candle| candle.low)
            .fold(f64::INFINITY, f64::min);
        let mut touch_groups = 0usize;
        let mut last_touch_offset = None;
        for path_offset in 1..offset {
            let sample = candles[index - path_offset];
            let touches = sample.high >= anchor_candle.high - touch_band
                && sample.high <= anchor_candle.high
                && sample.close < anchor_candle.high;
            if touches {
                if last_touch_offset
                    .map(|last| path_offset - last >= 2)
                    .unwrap_or(true)
                {
                    touch_groups += 1;
                }
                last_touch_offset = Some(path_offset);
            }
        }
        let current = candles[index];
        let signal = touch_groups >= 2
            && current.high > anchor_candle.high
            && current.close < anchor_candle.high
            && rsi >= 70.0
            && rsi < anchor_point.rsi14.unwrap_or(f64::NEG_INFINITY)
            && ema12 > ema144
            && ema12 > ema596
            && current.close > ema12
            && consolidation_low < current.close;
        if signal {
            return Some(TransitionSweepSignal { consolidation_low });
        }
        // Pine 在 offset 从近到远扫描时只冻结第一个合格锚点；该锚点失败后禁止回退。
        return None;
    }
    None
}
