use super::*;

/// 按信号家族优先级冻结止损、目标和退出政策，实际入场价留到下一根开盘确定。
pub(super) fn build_intent(
    candles: &[Candle],
    indicators: &IndicatorSeries,
    index: usize,
    tick_size: f64,
    direction: Direction,
    context: IntentContext,
    rule_version: ParityRuleVersion,
) -> Option<EntryIntent> {
    let candle = candles[index];
    let point = &indicators.points[index];
    let atr = point.atr14?;
    let signal_atr = context
        .ema_trend_long_v6
        .map(|signal| signal.source_atr)
        .or_else(|| context.ema_short.map(|signal| signal.source_atr))
        .or_else(|| {
            context
                .strict_visual_breakout
                .map(|signal| signal.source_atr)
        })
        .unwrap_or(atr);
    let stop_ticks = distance_ticks(1.5 * signal_atr, tick_size, true);
    let mut families = Vec::new();
    let mut absolute_stop = None;
    let mut target_price = None;
    let mut target_ticks = None;
    let mut activation_ticks = None;
    let mut exit_policy = ExitPolicy::Fixed;
    let counter_trend = match direction {
        Direction::Long => context.counter_trend_long,
        Direction::Short => context.counter_trend_short,
    };

    match direction {
        Direction::Long => {
            append_family(
                &mut families,
                context.divergence.bullish,
                SignalFamily::RsiBullishDivergence,
            );
            append_family(
                &mut families,
                context.rsi_pattern_long,
                SignalFamily::RsiOversoldPattern,
            );
            append_family(&mut families, context.ema_long, SignalFamily::EmaTrendLong);
            append_family(
                &mut families,
                context.accepted_range.is_some(),
                SignalFamily::ConfirmedRangeAcceptanceLong,
            );
            append_family(
                &mut families,
                context.large_horizontal_line.is_some(),
                SignalFamily::LargeHorizontalRangeBreakLong,
            );
            append_family(
                &mut families,
                context.strict_visual_breakout.is_some(),
                SignalFamily::StrictVisualConsolidationBreakLong,
            );
            append_family(
                &mut families,
                context.large_triangle_line.is_some(),
                SignalFamily::LargeAscendingTriangleBreakLong,
            );
            append_family(
                &mut families,
                context.ema_expansion_long,
                SignalFamily::EmaCompressionExpansionLong,
            );
            append_family(
                &mut families,
                context.three_bar_long,
                SignalFamily::ThreeBarBullishEngulfingLong,
            );
            append_family(
                &mut families,
                context.bollinger_lower_reclaim.is_some(),
                SignalFamily::BollingerLowerReclaimLong,
            );
            append_family(
                &mut families,
                context.ema596_reclaim_departure.is_some(),
                SignalFamily::Ema596ReclaimDepartureLong,
            );

            if let Some(reclaim) = context.bollinger_lower_reclaim {
                absolute_stop = Some(reclaim.stop_price);
                target_price = Some(reclaim.target_price);
                exit_policy = ExitPolicy::BollingerLowerReclaim;
            } else if let Some(departure) = context.ema596_reclaim_departure {
                absolute_stop = Some(departure.stop_price);
                target_price = Some(departure.target_price);
                exit_policy = ExitPolicy::Ema596ReclaimDeparture;
            } else if context.three_bar_long {
                let pattern_low = candles[index - 3..=index]
                    .iter()
                    .map(|candle| candle.low)
                    .fold(f64::INFINITY, f64::min);
                absolute_stop = Some(round_down(pattern_low, tick_size));
                let risk_ticks = distance_ticks(candle.close - pattern_low, tick_size, true);
                activation_ticks = Some(risk_ticks);
                target_ticks = Some(((risk_ticks as f64 * 1.5).floor() as i64).max(1));
                exit_policy = ExitPolicy::ThreeBarEngulfing;
            } else if let Some(target) = context
                .strict_visual_breakout
                .and_then(|signal| signal.measured_move_target_price)
            {
                target_price = Some(round_down(target, tick_size));
            } else if counter_trend {
                target_price = context.long_structure_target;
                exit_policy =
                    counter_trend_exit_policy(rule_version, context.v5_counter_trend_plan);
            } else if context.divergence_reversal_long {
                activation_ticks = Some(stop_ticks);
                target_ticks = Some(((stop_ticks as f64 * 1.5).floor() as i64).max(1));
                exit_policy = ExitPolicy::DivergenceRegime;
            } else if context
                .strict_visual_breakout
                .is_some_and(|signal| signal.short_range_one_r_target)
            {
                // V4 只接管既有 Fixed 分支，并以同一入场意图已经冻结的初始止损定义 1R。
                target_ticks = Some(stop_ticks);
            } else {
                let take_profit_atr = context
                    .accepted_range
                    .map(|signal| signal.take_profit_atr)
                    .or_else(|| {
                        context
                            .ema_trend_long_v6
                            .map(|signal| signal.source_take_profit_atr)
                    })
                    .or_else(|| {
                        context
                            .strict_visual_breakout
                            .map(|signal| signal.source_take_profit_atr)
                    })
                    .or(context.take_profit_atr)
                    .or(context.ema_expansion_long.then_some(3.0))?;
                target_ticks = Some(distance_ticks(
                    take_profit_atr * signal_atr,
                    tick_size,
                    false,
                ));
            }

            if context.bollinger_lower_reclaim.is_none()
                && context.ema596_reclaim_departure.is_none()
                && !context.three_bar_long
                && context.rsi_pattern_long
            {
                let stop = if context.patterns.bullish_engulfing
                    && (!rule_version.includes_v7_guards()
                        || accepted_bullish_engulfing(context.patterns))
                {
                    candles[index - 1].low.min(candle.low)
                } else {
                    candle.low
                };
                absolute_stop = Some(round_down(stop, tick_size));
            }
        }
        Direction::Short => {
            append_family(
                &mut families,
                context.divergence.bearish,
                SignalFamily::RsiBearishDivergence,
            );
            append_family(
                &mut families,
                context.rsi_pattern_short,
                SignalFamily::RsiOverboughtPattern,
            );
            append_family(
                &mut families,
                context.ema_short.is_some(),
                SignalFamily::EmaTrendShort,
            );
            append_family(
                &mut families,
                context.strict_visual_breakout.is_some(),
                SignalFamily::StrictVisualConsolidationBreakShort,
            );
            append_family(
                &mut families,
                context.false_breakout.is_some(),
                SignalFamily::AnchorFalseBreakShort,
            );
            append_family(
                &mut families,
                context
                    .upthrust_failed_acceptance
                    .is_some_and(|signal| !signal.right_side_confirmed),
                SignalFamily::AnchorUpthrustFailedAcceptanceShort,
            );
            append_family(
                &mut families,
                context
                    .upthrust_failed_acceptance
                    .is_some_and(|signal| signal.right_side_confirmed),
                SignalFamily::AnchorUpthrustFailedAcceptanceRightSideShort,
            );
            append_family(
                &mut families,
                context.transition_sweep.is_some(),
                SignalFamily::TransitionLiquiditySweepShort,
            );
            append_family(
                &mut families,
                context.ema_expansion_short,
                SignalFamily::EmaCompressionExpansionShort,
            );
            append_family(
                &mut families,
                context.effort_no_result.is_some(),
                SignalFamily::EffortNoResultShort,
            );

            if let Some(effort) = context.effort_no_result {
                absolute_stop = Some(effort.stop_price);
                activation_ticks = Some(effort.activation_ticks);
                target_ticks = Some(effort.target_ticks);
                exit_policy = ExitPolicy::EffortNoResult;
            } else if let Some(sweep) = context.transition_sweep {
                absolute_stop = Some(round_up(candle.high, tick_size));
                target_price = transition_entry_target(
                    rule_version,
                    context.short_structure_target,
                    Some(round_up(sweep.consolidation_low, tick_size)),
                );
                exit_policy =
                    counter_trend_exit_policy(rule_version, context.v5_counter_trend_plan);
            } else if let Some(failed_acceptance) = context.upthrust_failed_acceptance {
                absolute_stop = Some(round_up(failed_acceptance.frozen_stop_high, tick_size));
                target_price = Some(round_up(failed_acceptance.frozen_target_low, tick_size));
                exit_policy = ExitPolicy::Fixed;
            } else if let Some(target) = context
                .strict_visual_breakout
                .and_then(|signal| signal.measured_move_target_price)
            {
                target_price = Some(round_up(target, tick_size));
            } else if counter_trend {
                target_price = context.short_structure_target;
                exit_policy =
                    counter_trend_exit_policy(rule_version, context.v5_counter_trend_plan);
            } else if context.divergence_reversal_short {
                activation_ticks = Some(stop_ticks);
                target_ticks = Some(((stop_ticks as f64 * 1.5).floor() as i64).max(1));
                exit_policy = ExitPolicy::DivergenceRegime;
            } else if context.short_trend_extension {
                let base_target = context
                    .false_breakout
                    .map(|signal| signal.take_profit_atr)
                    .or(context.take_profit_atr)
                    .or(context.ema_expansion_short.then_some(3.0))?;
                activation_ticks = Some(distance_ticks(base_target * atr, tick_size, false));
                target_ticks = Some(distance_ticks(8.0 * atr, tick_size, false));
                exit_policy = ExitPolicy::ShortTrendExtension;
            } else {
                let take_profit_atr = context
                    .false_breakout
                    .map(|signal| signal.take_profit_atr)
                    .or_else(|| {
                        context
                            .ema_short
                            .map(|signal| signal.source_take_profit_atr)
                    })
                    .or(context.take_profit_atr)
                    .or(context.ema_expansion_short.then_some(3.0))?;
                target_ticks = Some(distance_ticks(
                    take_profit_atr * signal_atr,
                    tick_size,
                    false,
                ));
            }

            if context.effort_no_result.is_some() {
                // ENR 的两高结构止损优先于同棒偶然出现的旧 RSI/假突破形态。
            } else if preserve_transition_stop(rule_version, context.transition_sweep.is_some()) {
                // V4 与 Pine 一致：transition sweep 的信号棒高点优先于同棒 false breakout。
            } else if let Some(failed_acceptance) = context.upthrust_failed_acceptance {
                absolute_stop = Some(round_up(failed_acceptance.frozen_stop_high, tick_size));
            } else if let Some(false_breakout) = context.false_breakout {
                absolute_stop = Some(round_up(false_breakout.frozen_high, tick_size));
            } else if context.rsi_pattern_short && context.transition_sweep.is_none() {
                let stop = if context.patterns.bearish_engulfing
                    && (!rule_version.includes_v7_guards()
                        || accepted_bearish_engulfing(context.patterns))
                {
                    candles[index - 1].high.max(candle.high)
                } else {
                    candle.high
                };
                absolute_stop = Some(round_up(stop, tick_size));
            }
        }
    }

    let strict_visual_breakout_candle_extreme_stop = context
        .strict_visual_breakout
        .and_then(|signal| signal.breakout_candle_extreme_stop_price);
    let strict_visual_breakout_candle_extreme_stop_min_ticks = context
        .strict_visual_breakout
        .and_then(|signal| signal.breakout_candle_extreme_stop_min_atr_multiple)
        .map(|multiple| distance_ticks(multiple * signal_atr, tick_size, true));
    if let Some(stop) = strict_visual_breakout_candle_extreme_stop {
        // V11/V12 必须覆盖同棒偶然命中的旧形态止损，否则样本不会共享同一结构合同。
        absolute_stop = Some(stop);
    }

    let strict_visual_boundary = context
        .strict_visual_breakout
        .map(|signal| match direction {
            Direction::Long => signal.range.upper,
            Direction::Short => signal.range.lower,
        });
    let breakout_line = [
        context.accepted_range.map(|signal| signal.raw_high),
        context.large_horizontal_line,
        strict_visual_boundary,
        context.large_triangle_line,
    ]
    .into_iter()
    .flatten()
    .max_by(f64::total_cmp);
    let display_volume_ratio = match direction {
        Direction::Long => context
            .ema596_reclaim_departure
            .map(|signal| signal.display_volume_ratio)
            .or_else(|| {
                context
                    .accepted_range
                    .map(|signal| signal.volume_ratio)
                    .or_else(|| {
                        context
                            .ema_trend_long_v6
                            .map(|signal| signal.source_volume_ratio)
                    })
                    .or_else(|| {
                        context
                            .strict_visual_breakout
                            .filter(|signal| signal.volume_gate_applied)
                            .map(|signal| signal.source_volume_ratio)
                    })
                    .or_else(|| {
                        context
                            .bollinger_lower_reclaim
                            .and_then(|signal| signal.display_volume_ratio)
                    })
                    .or(point.filtered_volume_ratio)
            }),
        Direction::Short => context
            .effort_no_result
            .and_then(|signal| signal.display_volume_ratio)
            .or_else(|| {
                context
                    .upthrust_failed_acceptance
                    .map(|signal| signal.breakout_volume_ratio)
            })
            .or_else(|| context.false_breakout.map(|signal| signal.volume_ratio))
            .or_else(|| context.ema_short.map(|signal| signal.source_volume_ratio))
            .or_else(|| {
                context
                    .strict_visual_breakout
                    .filter(|signal| signal.volume_gate_applied)
                    .map(|signal| signal.source_volume_ratio)
            })
            .or(point.filtered_volume_ratio),
    };

    Some(EntryIntent {
        signal_index: index,
        signal_time_ms: candle.timestamp_ms,
        direction,
        families,
        signal_close: candle.close,
        signal_atr,
        stop_price: absolute_stop,
        stop_ticks: strict_visual_breakout_candle_extreme_stop_min_ticks
            .or_else(|| absolute_stop.is_none().then_some(stop_ticks)),
        target_price,
        target_ticks,
        activation_ticks,
        exit_policy,
        counter_trend,
        signal_counter_trend_ema_age_bars_capped_600: if rule_version.includes_v4_guards() {
            context.counter_trend_ema_age_audit
        } else {
            None
        },
        counter_trend_structure_breakout_line: context
            .v5_counter_trend_plan
            .map(|plan| plan.structure_breakout_line),
        anchor_upthrust_target_consumption_ratio: context
            .upthrust_failed_acceptance
            .and_then(|signal| signal.target_consumption_ratio),
        active_parent_horizontal_anchor: context
            .upthrust_failed_acceptance
            .and_then(|signal| signal.active_parent_horizontal_anchor),
        strict_visual_range_length_bars: context
            .strict_visual_breakout
            .map(|signal| signal.range.length_bars),
        strict_visual_range_height: context.strict_visual_range_height(),
        strict_visual_short_range_one_r_target: context
            .strict_visual_breakout
            .map(|signal| signal.short_range_one_r_target),
        strict_visual_breakout_candle_extreme_stop: strict_visual_breakout_candle_extreme_stop
            .is_some(),
        volume_ratio: display_volume_ratio,
        rsi: context
            .ema_short
            .and_then(|signal| signal.source_rsi)
            .or(point.rsi14),
        breakout_line,
    })
}

/// V5 只把达到 600 根的纯 RSI 严格逆势仓切到成熟退出；年轻分支继续使用 V4 保护。
fn counter_trend_exit_policy(
    rule_version: ParityRuleVersion,
    v5_plan: Option<RsiCounterTrendPlanV5>,
) -> ExitPolicy {
    if rule_version.includes_v5_guards()
        && v5_plan.is_some_and(|plan| plan.ema_alignment_age >= EMA_ALIGNMENT_AGE_CAP)
    {
        ExitPolicy::RsiCounterTrendAgeV5
    } else if rule_version.includes_v4_guards() {
        ExitPolicy::CounterTrendStructureV4
    } else {
        ExitPolicy::CounterTrendStructure
    }
}

/// V4 复用已完成 fresh 优先级选择的结构目标；旧版本继续冻结 transition 自身目标。
pub(super) fn transition_entry_target(
    rule_version: ParityRuleVersion,
    selected_structure_target: Option<f64>,
    frozen_transition_target: Option<f64>,
) -> Option<f64> {
    if rule_version.includes_v4_guards() {
        selected_structure_target
    } else {
        frozen_transition_target
    }
}

/// V4 修正 Pine/Rust 重叠优先级；冻结 V1～V3 继续沿用各自历史回放语义。
pub(super) fn preserve_transition_stop(
    rule_version: ParityRuleVersion,
    transition_sweep_present: bool,
) -> bool {
    rule_version.includes_v4_guards() && transition_sweep_present
}
