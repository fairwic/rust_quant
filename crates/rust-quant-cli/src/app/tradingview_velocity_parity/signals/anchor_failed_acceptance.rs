use super::v7::false_breakout_short_has_long_lower_wick;
use crate::app::tradingview_velocity_parity::model::{
    AnchorUpthrustResearchVariant, Candle, HorizontalAnchorEvidence,
};
use crate::app::tradingview_velocity_parity::ranges::{
    active_parent_horizontal_upside_breakout_zone, nearest_fresh_horizontal_upside_breakout_zone,
    nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency,
};

pub(super) const FALSE_BREAKOUT_WINDOW: usize = 8;
const FAILED_ACCEPTANCE_WINDOW: usize = 2;
const CLOSE_LOCATION_MAX: f64 = 0.25;
const REJECTION_VOLUME_MULTIPLE_MIN: f64 = 0.50;
const SIGNAL_CLOSE_REWARD_RISK_MIN: f64 = 1.50;

/// 放量突破后等待失败确认的冻结锚区；所有边界都来自突破棒收盘时。
#[derive(Debug, Clone)]
pub(super) struct FalseBreakoutPending {
    pub(super) breakout_index: usize,
    pub(super) anchor_high: f64,
    pub(super) anchor_low: f64,
    /// 已完成突破棒的开盘价；V27 用它判断上涨实体是否被确认收盘完全否定。
    pub(super) breakout_open: f64,
    pub(super) breakout_high: f64,
    pub(super) breakout_volume: f64,
    pub(super) volume_ratio: f64,
    pub(super) take_profit_atr: f64,
    /// V26～V30 在突破棒收盘时冻结的父横盘证据；旧版本为 `None`。
    pub(super) active_parent_horizontal_anchor: Option<HorizontalAnchorEvidence>,
}

/// 原有“收盘跌破冻结下沿”假突破空单的风险参数。
#[derive(Debug, Clone, Copy)]
pub(super) struct FalseBreakoutSignal {
    pub(super) frozen_high: f64,
    pub(super) volume_ratio: f64,
    pub(super) take_profit_atr: f64,
}

/// 扫高后快速跌回冻结上沿的失败接受空单参数。
#[derive(Debug, Clone, Copy)]
pub(super) struct UpthrustFailedAcceptanceSignal {
    pub(super) frozen_stop_high: f64,
    pub(super) frozen_target_low: f64,
    pub(super) breakout_volume_ratio: f64,
    /// `true` 表示信号经过 V21 紧邻下一根完成棒确认，用于保持 V20/V21 家族可审计。
    pub(super) right_side_confirmed: bool,
    /// 确认棒相对 setup 收盘已消耗的冻结结构奖励比例；V20 即时信号为 `None`。
    pub(super) target_consumption_ratio: Option<f64>,
    /// V26～V30 透传到候选账本的因果父横盘证据；其他版本为 `None`。
    pub(super) active_parent_horizontal_anchor: Option<HorizontalAnchorEvidence>,
}

/// V21 在 V20 拒绝棒收盘时冻结的唯一一根右侧确认窗口。
#[derive(Debug, Clone, Copy)]
pub(super) struct UpthrustRightSidePending {
    /// V20 拒绝 setup 的 K 线索引；只允许 `setup_index + 1` 完成确认。
    setup_index: usize,
    /// V20 setup 收盘；V22 以此为起点衡量确认棒是否提前走完过多目标空间。
    setup_close: f64,
    /// 拒绝棒低点；V21 要求下一根收盘真实跌破，而不是仅盘中扫过。
    rejection_low: f64,
    /// setup 时冻结的结构止损；等待期间触及即取消，不能事后移动。
    frozen_stop_high: f64,
    /// 突破时冻结的锚区下沿，继续作为结构止盈与最低 1.5R 校验基准。
    frozen_target_low: f64,
    /// 原突破棒量比，仅透传至信号证据，确认棒不得重算或替换。
    breakout_volume_ratio: f64,
}

/// 在首次放量收盘上破最近横盘时冻结 V23～V30 状态；已存在 pending 时绝不移动原边界。
#[allow(clippy::too_many_arguments)]
pub(super) fn arm_recent_horizontal_first_break(
    pending: &mut Option<FalseBreakoutPending>,
    candles: &[Candle],
    index: usize,
    tick_size: f64,
    volume_event: bool,
    volume_ratio: Option<f64>,
    take_profit_atr: Option<f64>,
    variant: AnchorUpthrustResearchVariant,
) {
    let candle = candles[index];
    if pending.is_some()
        || !volume_event
        || take_profit_atr.is_none()
        || candle.close <= candle.open
    {
        return;
    }
    let (anchor_high, anchor_low, active_parent_horizontal_anchor) = if variant
        .uses_active_parent_horizontal_anchor()
    {
        let limit = variant
            .horizontal_direction_efficiency_max()
            .expect("V26-V29 freeze the V25B direction-efficiency limit");
        let Some(anchor) =
            active_parent_horizontal_upside_breakout_zone(candles, index, tick_size, limit)
        else {
            return;
        };
        let anchor_height = anchor.high - anchor.low;
        let normalized_breakout_excess =
            (anchor_height > 0.0).then_some((candle.close - anchor.high) / anchor_height);
        let (Some(start), Some(end)) = (
            candles.get(anchor.start_index),
            candles.get(anchor.end_index),
        ) else {
            return;
        };
        (
            anchor.high,
            anchor.low,
            Some(HorizontalAnchorEvidence {
                start_time_ms: start.timestamp_ms,
                end_time_ms: end.timestamp_ms,
                length_bars: anchor.length_bars,
                upper: anchor.high,
                lower: anchor.low,
                direction_efficiency: anchor.direction_efficiency,
                breakout_time_ms: candle.timestamp_ms,
                breakout_close: candle.close,
                breakout_excess_ticks: (candle.close - anchor.high) / tick_size,
                breakout_open: variant
                    .requires_breakout_body_rejection()
                    .then_some(candle.open),
                confirmation_close: None,
                breakout_body_rejection_depth_ticks: None,
                normalized_breakout_body_rejection_depth: None,
                normalized_breakout_excess: variant
                    .normalized_breakout_excess_max()
                    .and(normalized_breakout_excess),
                edge_transition_count: variant
                    .minimum_horizontal_edge_transitions()
                    .map(|_| anchor.edge_transition_count),
            }),
        )
    } else {
        let anchor = match variant.horizontal_direction_efficiency_max() {
            Some(limit) => nearest_fresh_horizontal_upside_breakout_zone_with_direction_efficiency(
                candles,
                index,
                tick_size,
                Some(limit),
            ),
            None => nearest_fresh_horizontal_upside_breakout_zone(candles, index, tick_size),
        };
        let Some(anchor) = anchor else {
            return;
        };
        (anchor.high, anchor.low, None)
    };
    *pending = Some(FalseBreakoutPending {
        breakout_index: index,
        anchor_high,
        anchor_low,
        breakout_open: candle.open,
        breakout_high: candle.high,
        breakout_volume: candle.volume,
        volume_ratio: volume_ratio.expect("volume event has ratio"),
        take_profit_atr: take_profit_atr.expect("breakout requires target tier"),
        active_parent_horizontal_anchor,
    });
}

/// 只推进第 1～2 根扫高失败窗口，不把横盘下沿跌破泄漏成旧 `AnchorFalseBreakShort`。
///
/// V23～V30 使用独立 pending；窗口结束即释放，使后续真正的新横盘突破可以重新取得资格。
pub(super) fn evaluate_failed_acceptance_only(
    pending: &mut Option<FalseBreakoutPending>,
    candle: Candle,
    index: usize,
    tick_size: f64,
    variant: AnchorUpthrustResearchVariant,
) -> Option<UpthrustFailedAcceptanceSignal> {
    let mut unused_right_side_pending = None;
    let (_, signal) = evaluate_pending(
        pending,
        &mut unused_right_side_pending,
        candle,
        index,
        tick_size,
        true,
        true,
        variant,
    );
    if pending.as_ref().is_some_and(|snapshot| {
        index.saturating_sub(snapshot.breakout_index) >= FAILED_ACCEPTANCE_WINDOW
    }) {
        *pending = None;
    }
    signal
}

/// 推进同一个冻结突破状态，V20 可提前确认失败，旧版本仍只等跌破下沿。
pub(super) fn evaluate_pending(
    pending: &mut Option<FalseBreakoutPending>,
    right_side_pending: &mut Option<UpthrustRightSidePending>,
    candle: Candle,
    index: usize,
    tick_size: f64,
    reject_late_lower_wick: bool,
    enable_failed_acceptance: bool,
    variant: AnchorUpthrustResearchVariant,
) -> (
    Option<FalseBreakoutSignal>,
    Option<UpthrustFailedAcceptanceSignal>,
) {
    if let Some(snapshot) = *right_side_pending {
        let age = index.saturating_sub(snapshot.setup_index);
        if age == 1 {
            let risk = snapshot.frozen_stop_high - candle.close;
            let reward = candle.close - snapshot.frozen_target_low;
            let reward_risk = if risk > 0.0 && reward > 0.0 {
                reward / risk
            } else {
                0.0
            };
            let original_reward = snapshot.setup_close - snapshot.frozen_target_low;
            let consumed_reward = snapshot.setup_close - candle.close;
            let target_consumption_ratio = if original_reward > 0.0 && consumed_reward >= 0.0 {
                Some(consumed_reward / original_reward)
            } else {
                None
            };
            // V22 只在确认棒收盘时比较冻结距离，不能读取下一根实际成交开盘或后续路径。
            let within_target_consumption_cap = target_consumption_ratio.is_some_and(|ratio| {
                ratio.is_finite()
                    && variant
                        .target_consumption_cap()
                        .is_none_or(|cap| ratio <= cap)
            });
            let confirmed = candle.is_valid()
                && candle.high < snapshot.frozen_stop_high
                && candle.close < snapshot.rejection_low
                && reward_risk >= SIGNAL_CLOSE_REWARD_RISK_MIN
                && within_target_consumption_cap;
            *right_side_pending = None;
            if confirmed {
                return (
                    None,
                    Some(UpthrustFailedAcceptanceSignal {
                        frozen_stop_high: snapshot.frozen_stop_high,
                        frozen_target_low: snapshot.frozen_target_low,
                        breakout_volume_ratio: snapshot.breakout_volume_ratio,
                        right_side_confirmed: true,
                        target_consumption_ratio,
                        active_parent_horizontal_anchor: None,
                    }),
                );
            }
        } else if age > 1 {
            *right_side_pending = None;
        }
    }

    let Some(snapshot) = pending.clone() else {
        return (None, None);
    };
    let age = index.saturating_sub(snapshot.breakout_index);

    if enable_failed_acceptance && (1..=FAILED_ACCEPTANCE_WINDOW).contains(&age) {
        let range = candle.high - candle.low;
        let close_location = if range > 0.0 {
            (candle.close - candle.low) / range
        } else {
            f64::INFINITY
        };
        let rejection_volume_multiple = if snapshot.breakout_volume > 0.0 {
            candle.volume / snapshot.breakout_volume
        } else {
            0.0
        };
        let stop_high = candle.high.max(snapshot.breakout_high) + tick_size;
        let risk = stop_high - candle.close;
        let reward = candle.close - snapshot.anchor_low;
        let reward_risk = if risk > 0.0 && reward > 0.0 {
            reward / risk
        } else {
            0.0
        };
        let anchor_height = snapshot.anchor_high - snapshot.anchor_low;
        let normalized_body_rejection_depth = if anchor_height > 0.0 {
            Some((snapshot.breakout_open - candle.close) / anchor_height)
        } else {
            None
        };
        let failed_acceptance_before_normalized_depth = candle.is_valid()
            && (!variant.requires_breakout_high_sweep() || candle.high > snapshot.breakout_high)
            && (!variant.requires_breakout_body_rejection()
                || candle.close <= snapshot.breakout_open)
            && candle.close < candle.open
            && candle.close < snapshot.anchor_high
            && close_location <= CLOSE_LOCATION_MAX
            && rejection_volume_multiple >= REJECTION_VOLUME_MULTIPLE_MIN
            && reward_risk >= SIGNAL_CLOSE_REWARD_RISK_MIN;
        let normalized_depth_accepted = variant
            .normalized_breakout_body_rejection_min()
            .is_none_or(|minimum| {
                normalized_body_rejection_depth.is_some_and(|depth| depth >= minimum)
            });
        let normalized_breakout_excess_accepted = variant
            .normalized_breakout_excess_max()
            .is_none_or(|maximum| {
                snapshot
                    .active_parent_horizontal_anchor
                    .and_then(|evidence| evidence.normalized_breakout_excess)
                    .is_some_and(|excess| excess <= maximum)
            });
        let edge_transitions_accepted =
            variant
                .minimum_horizontal_edge_transitions()
                .is_none_or(|minimum| {
                    snapshot
                        .active_parent_horizontal_anchor
                        .and_then(|evidence| evidence.edge_transition_count)
                        .is_some_and(|count| count >= minimum)
                });
        if failed_acceptance_before_normalized_depth
            && !(normalized_depth_accepted
                && normalized_breakout_excess_accepted
                && edge_transitions_accepted)
        {
            // V28～V30 都是 V27 首个确认身份的严格过滤器；必须先保持同一 pending，再在 V27
            // 本应发信号的位置消费 setup，避免提前拒绝或继续等待制造基线中不存在的新身份。
            *pending = None;
            return (None, None);
        }
        let failed_acceptance = failed_acceptance_before_normalized_depth
            && normalized_depth_accepted
            && normalized_breakout_excess_accepted
            && edge_transitions_accepted;
        if failed_acceptance {
            *pending = None;
            if variant.requires_right_side_confirmation() {
                *right_side_pending = Some(UpthrustRightSidePending {
                    setup_index: index,
                    setup_close: candle.close,
                    rejection_low: candle.low,
                    frozen_stop_high: stop_high,
                    frozen_target_low: snapshot.anchor_low,
                    breakout_volume_ratio: snapshot.volume_ratio,
                });
                return (None, None);
            }
            let active_parent_horizontal_anchor =
                snapshot
                    .active_parent_horizontal_anchor
                    .map(|mut evidence| {
                        if variant.requires_breakout_body_rejection() {
                            evidence.confirmation_close = Some(candle.close);
                            evidence.breakout_body_rejection_depth_ticks =
                                Some((snapshot.breakout_open - candle.close) / tick_size);
                        }
                        if variant.normalized_breakout_body_rejection_min().is_some() {
                            evidence.normalized_breakout_body_rejection_depth =
                                normalized_body_rejection_depth;
                        }
                        evidence
                    });
            return (
                None,
                Some(UpthrustFailedAcceptanceSignal {
                    frozen_stop_high: stop_high,
                    frozen_target_low: snapshot.anchor_low,
                    breakout_volume_ratio: snapshot.volume_ratio,
                    right_side_confirmed: false,
                    target_consumption_ratio: None,
                    active_parent_horizontal_anchor,
                }),
            );
        }
    }

    if (1..=FALSE_BREAKOUT_WINDOW).contains(&age)
        && candle.close < candle.open
        && candle.close < snapshot.anchor_low
    {
        let signal = (!reject_late_lower_wick || !false_breakout_short_has_long_lower_wick(candle))
            .then_some(FalseBreakoutSignal {
                frozen_high: snapshot.anchor_high,
                volume_ratio: snapshot.volume_ratio,
                take_profit_atr: snapshot.take_profit_atr,
            });
        *pending = None;
        return (signal, None);
    }

    if age >= FALSE_BREAKOUT_WINDOW {
        *pending = None;
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle {
            timestamp_ms: 0,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    fn btc_pending() -> Option<FalseBreakoutPending> {
        Some(FalseBreakoutPending {
            breakout_index: 100,
            anchor_high: 64_256.2,
            anchor_low: 63_639.0,
            breakout_open: 64_200.0,
            breakout_high: 64_276.8,
            breakout_volume: 2_716.4044,
            volume_ratio: 5.81,
            take_profit_atr: 3.6,
            active_parent_horizontal_anchor: None,
        })
    }

    #[test]
    fn v20_confirms_btc_failed_acceptance_on_first_completed_bar() {
        let mut pending = btc_pending();
        let mut right_side = None;
        let (_, signal) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_276.7, 64_398.0, 64_138.9, 64_156.0, 1_784.2595),
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::Baseline,
        );
        let signal = signal.expect("08:15 completed bar should confirm failed acceptance");
        assert!(pending.is_none());
        assert!(right_side.is_none());
        assert!(!signal.right_side_confirmed);
        assert_eq!(signal.target_consumption_ratio, None);
        assert_eq!(signal.frozen_stop_high, 64_398.1);
        assert_eq!(signal.frozen_target_low, 63_639.0);
    }

    #[test]
    fn v27_rejects_a_shallow_close_back_above_the_breakout_open() {
        let shallow_rejection = candle(64_270.0, 64_300.0, 64_200.0, 64_220.0, 1_784.2595);
        let mut v26_pending = btc_pending();
        let mut v26_right_side = None;
        let (_, v26_signal) = evaluate_pending(
            &mut v26_pending,
            &mut v26_right_side,
            shallow_rejection,
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::ActiveParentHorizontal,
        );
        assert!(v26_signal.is_some());

        let mut v27_pending = btc_pending();
        let mut v27_right_side = None;
        let (_, v27_signal) = evaluate_pending(
            &mut v27_pending,
            &mut v27_right_side,
            shallow_rejection,
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection,
        );
        assert!(v27_signal.is_none());
        assert!(v27_pending.is_some());
    }

    #[test]
    fn v28_requires_ten_percent_of_parent_range_body_rejection() {
        let moderate_rejection = candle(64_276.7, 64_398.0, 64_138.9, 64_156.0, 1_784.2595);
        let mut v27_pending = btc_pending();
        let mut v27_right_side = None;
        let (_, v27_signal) = evaluate_pending(
            &mut v27_pending,
            &mut v27_right_side,
            moderate_rejection,
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::ActiveParentHorizontalBreakoutBodyRejection,
        );
        assert!(v27_signal.is_some());

        let mut shallow_v28_pending = btc_pending();
        let mut shallow_v28_right_side = None;
        let (_, shallow_v28_signal) = evaluate_pending(
            &mut shallow_v28_pending,
            &mut shallow_v28_right_side,
            moderate_rejection,
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
        );
        assert!(shallow_v28_signal.is_none());
        assert!(shallow_v28_pending.is_none());
        let (_, deferred_v28_signal) = evaluate_pending(
            &mut shallow_v28_pending,
            &mut shallow_v28_right_side,
            candle(64_276.7, 64_398.0, 64_090.0, 64_120.0, 1_784.2595),
            102,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
        );
        assert!(deferred_v28_signal.is_none());

        let mut deep_v28_pending = btc_pending();
        let mut deep_v28_right_side = None;
        let (_, deep_v28_signal) = evaluate_pending(
            &mut deep_v28_pending,
            &mut deep_v28_right_side,
            candle(64_276.7, 64_398.0, 64_090.0, 64_120.0, 1_784.2595),
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::ActiveParentHorizontalNormalizedBodyRejection10Pct,
        );
        assert!(deep_v28_signal.is_some());
    }

    #[test]
    fn v21_waits_for_the_next_completed_close_below_the_rejection_low() {
        let mut pending = btc_pending();
        let mut right_side = None;
        let (_, setup_signal) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_276.7, 64_398.0, 64_138.9, 64_156.0, 1_784.2595),
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::RightSideConfirmation,
        );
        assert!(setup_signal.is_none());
        assert!(pending.is_none());
        assert!(right_side.is_some());

        let (_, confirmed) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_150.0, 64_200.0, 64_000.0, 64_100.0, 1_200.0),
            102,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::RightSideConfirmation,
        );
        let confirmed = confirmed.expect("next completed close below setup low should confirm");
        assert!(confirmed.right_side_confirmed);
        assert!((confirmed.target_consumption_ratio.expect("ratio") - 56.0 / 517.0).abs() < 1e-12);
        assert!(right_side.is_none());
        assert_eq!(confirmed.frozen_stop_high, 64_398.1);
        assert_eq!(confirmed.frozen_target_low, 63_639.0);
    }

    #[test]
    fn v21_cancels_when_confirmation_bar_touches_the_frozen_stop() {
        let mut pending = None;
        let mut right_side = Some(UpthrustRightSidePending {
            setup_index: 101,
            setup_close: 64_156.0,
            rejection_low: 64_138.9,
            frozen_stop_high: 64_398.1,
            frozen_target_low: 63_639.0,
            breakout_volume_ratio: 5.81,
        });
        let (_, confirmed) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_150.0, 64_398.1, 64_000.0, 64_100.0, 1_200.0),
            102,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::RightSideConfirmation,
        );
        assert!(confirmed.is_none());
        assert!(right_side.is_none());
    }

    #[test]
    fn v21_does_not_extend_confirmation_beyond_the_next_bar() {
        let mut pending = None;
        let mut right_side = Some(UpthrustRightSidePending {
            setup_index: 101,
            setup_close: 64_156.0,
            rejection_low: 64_138.9,
            frozen_stop_high: 64_398.1,
            frozen_target_low: 63_639.0,
            breakout_volume_ratio: 5.81,
        });
        let (_, confirmed) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_160.0, 64_220.0, 64_100.0, 64_150.0, 1_200.0),
            102,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::RightSideConfirmation,
        );
        assert!(confirmed.is_none());
        assert!(right_side.is_none());
    }

    #[test]
    fn v19_does_not_create_the_early_signal() {
        let mut pending = btc_pending();
        let mut right_side = None;
        let (late, early) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_276.7, 64_398.0, 64_138.9, 64_156.0, 1_784.2595),
            101,
            0.1,
            true,
            false,
            AnchorUpthrustResearchVariant::Baseline,
        );
        assert!(late.is_none());
        assert!(early.is_none());
        assert!(pending.is_some());
    }

    #[test]
    fn early_signal_requires_enough_reward_to_frozen_lower_boundary() {
        let mut pending = btc_pending();
        let mut right_side = None;
        pending.as_mut().expect("fixture must exist").anchor_low = 64_100.0;
        let (_, signal) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(64_276.7, 64_398.0, 64_230.0, 64_240.0, 1_784.2595),
            101,
            0.1,
            true,
            true,
            AnchorUpthrustResearchVariant::Baseline,
        );
        assert!(signal.is_none());
        assert!(pending.is_some());
    }

    #[test]
    fn legacy_lower_boundary_failure_remains_available() {
        let mut pending = btc_pending();
        let mut right_side = None;
        let (signal, early) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(63_700.0, 63_720.0, 63_500.0, 63_600.0, 2_000.0),
            104,
            0.1,
            true,
            false,
            AnchorUpthrustResearchVariant::Baseline,
        );
        assert!(signal.is_some());
        assert!(early.is_none());
        assert!(pending.is_none());
    }

    fn target_consumption_confirmation(
        variant: AnchorUpthrustResearchVariant,
    ) -> Option<UpthrustFailedAcceptanceSignal> {
        let mut pending = None;
        let mut right_side = Some(UpthrustRightSidePending {
            setup_index: 10,
            setup_close: 100.0,
            rejection_low: 99.0,
            frozen_stop_high: 110.0,
            frozen_target_low: 0.0,
            breakout_volume_ratio: 4.0,
        });
        let (_, signal) = evaluate_pending(
            &mut pending,
            &mut right_side,
            candle(90.0, 100.0, 60.0, 67.0, 1_000.0),
            11,
            0.1,
            true,
            true,
            variant,
        );
        signal
    }

    #[test]
    fn v22_rejects_confirmation_after_more_than_twenty_five_percent_consumption() {
        assert!(target_consumption_confirmation(
            AnchorUpthrustResearchVariant::TargetConsumptionCap25
        )
        .is_none());
    }

    #[test]
    fn v22_primary_cap_includes_confirmation_exactly_at_thirty_three_percent() {
        let signal =
            target_consumption_confirmation(AnchorUpthrustResearchVariant::TargetConsumptionCap33)
                .expect("33% boundary is inclusive");
        assert!((signal.target_consumption_ratio.expect("ratio") - 0.33).abs() < 1e-12);
    }
}
