use super::super::{
    AnchorEntrySignalEvidence, ComputedCandle, IsolatedStrategyFamilySignalEvidence,
    MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection,
};
use super::isolated_family_common::{
    atr_target_signal, current_volume_gate, ISOLATED_FILTERED_VOLUME_MIN_RATIO,
};
use super::momentum_exhaustion_reversal_v1::{
    prior_net_move_pct, MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT,
};
use super::weekly_base_volume_v3::target_atr_multiplier_with_min_ratio;
use super::{
    FilteredVolumeRsiEmaMacdSignal, DOJI_MAX_BODY_RANGE_RATIO, REVERSAL_WICK_MIN_RANGE_RATIO,
};

/// 方向性长影线限价单最多观察 p 后连续 12 根已完成 15m K 线。
pub(crate) const MOMENTUM_EXHAUSTION_LIMIT_VALID_CANDLES: usize = 12;
/// V2 的最小假设标识；用于证明成交等待和目标分档没有混入其他指标。
pub(crate) const MOMENTUM_EXHAUSTION_V2_HYPOTHESIS: &str =
    "prior_96_net_move_8pct_plus_abnormal_volume_then_wick_extreme_limit12";

const LONG_WICK_TRIGGER: &str = "momentum_exhaustion_lower_wick_limit12_long_v2";
const LONG_TOUCH_TRIGGER: &str = "momentum_exhaustion_next_high_touch_long_v2";
const SHORT_WICK_TRIGGER: &str = "momentum_exhaustion_upper_wick_limit12_short_v2";
const SHORT_TOUCH_TRIGGER: &str = "momentum_exhaustion_next_low_touch_short_v2";

/// 冻结某个动量衰竭版本的方向影线门槛与审计标签，公共信号流程只读取该不可变策略。
#[derive(Debug, Clone, Copy)]
pub(super) struct MomentumExhaustionSignalPolicy {
    /// 对应方向影线占信号 K 完整振幅的最低比例；V2 为 60%，V3 为 55%。
    pub(super) directional_wick_min_range_ratio: f64,
    /// 写入逐笔证据的版本假设，防止不同阈值的结果被当成同一研究口径。
    pub(super) hypothesis: &'static str,
    /// 长下影满足版本门槛时使用的做多触发标签。
    pub(super) long_wick_trigger: &'static str,
    /// 做多未达到方向影线门槛时使用的下一根高点触发标签。
    pub(super) long_touch_trigger: &'static str,
    /// 长上影满足版本门槛时使用的做空触发标签。
    pub(super) short_wick_trigger: &'static str,
    /// 做空未达到方向影线门槛时使用的下一根低点触发标签。
    pub(super) short_touch_trigger: &'static str,
}

const V2_POLICY: MomentumExhaustionSignalPolicy = MomentumExhaustionSignalPolicy {
    directional_wick_min_range_ratio: REVERSAL_WICK_MIN_RANGE_RATIO,
    hypothesis: MOMENTUM_EXHAUSTION_V2_HYPOTHESIS,
    long_wick_trigger: LONG_WICK_TRIGGER,
    long_touch_trigger: LONG_TOUCH_TRIGGER,
    short_wick_trigger: SHORT_WICK_TRIGGER,
    short_touch_trigger: SHORT_TOUCH_TRIGGER,
};

/// 仅改变 V1 的方向性影线成交与目标距离，96 根、量比和周 P90 门禁保持冻结。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_policy(candles, completed_count, args, V2_POLICY)
}

/// 用版本化影线门槛执行同一动量衰竭规则，避免 V2/V3 复制并漂移其余冻结条件。
pub(super) fn signal_with_policy(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
    policy: MomentumExhaustionSignalPolicy,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - ISOLATED_FILTERED_VOLUME_MIN_RATIO).abs() > f64::EPSILON {
        return Err("momentum_exhaustion_v2_ratio_policy_mismatch");
    }
    if !policy.directional_wick_min_range_ratio.is_finite()
        || !(0.0..=1.0).contains(&policy.directional_wick_min_range_ratio)
    {
        return Err("momentum_exhaustion_v2_wick_policy_invalid");
    }
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("momentum_exhaustion_v2_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("momentum_exhaustion_v2_not_ready")?;
    let (volume, weekly_volume) = current_volume_gate(candles, latest_idx)
        .map_err(|_| "momentum_exhaustion_v2_volume_not_confirmed")?;
    let net_move_pct = prior_net_move_pct(candles, latest_idx)
        .ok_or("momentum_exhaustion_v2_history_not_ready")?;
    let direction = if net_move_pct <= -MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT {
        MarketVelocityTradeDirection::Long
    } else if net_move_pct >= MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT {
        MarketVelocityTradeDirection::Short
    } else {
        return Err("momentum_exhaustion_v2_net_move_not_confirmed");
    };
    let (anchor_entry, directional_wick) =
        anchor_entry_evidence(latest, direction, policy.directional_wick_min_range_ratio)?;
    let trigger = match (direction, directional_wick) {
        (MarketVelocityTradeDirection::Long, true) => policy.long_wick_trigger,
        (MarketVelocityTradeDirection::Long, false) => policy.long_touch_trigger,
        (MarketVelocityTradeDirection::Short, true) => policy.short_wick_trigger,
        (MarketVelocityTradeDirection::Short, false) => policy.short_touch_trigger,
        (MarketVelocityTradeDirection::Both, _) => unreachable!(),
    };
    let target_atr =
        target_atr_multiplier_with_min_ratio(volume.ratio, ISOLATED_FILTERED_VOLUME_MIN_RATIO)
            .ok_or("momentum_exhaustion_v2_target_not_ready")?;

    atr_target_signal(
        latest,
        direction,
        trigger,
        volume,
        weekly_volume,
        Vec::new(),
        Some(anchor_entry),
        IsolatedStrategyFamilySignalEvidence {
            family: "momentum_exhaustion_reversal",
            hypothesis: policy.hypothesis,
            prior_96_net_move_pct: Some(net_move_pct),
            platform_breakdown: None,
            long_term_ema_confirmed: false,
            ema696_recent: Vec::new(),
        },
        target_atr,
        false,
    )
}

/// 冻结 p 的方向性影线和限价；非方向性影线仍只允许紧邻下一根突破反转侧极值。
fn anchor_entry_evidence(
    pivot: &ComputedCandle,
    direction: MarketVelocityTradeDirection,
    directional_wick_min_range_ratio: f64,
) -> Result<(AnchorEntrySignalEvidence, bool), &'static str> {
    let range = pivot.candle.high - pivot.candle.low;
    if !range.is_finite() || range <= 0.0 {
        return Err("momentum_exhaustion_v2_pivot_range_invalid");
    }
    let body_ratio = (pivot.candle.close - pivot.candle.open).abs() / range;
    let upper_wick_ratio =
        (pivot.candle.high - pivot.candle.open.max(pivot.candle.close)).max(0.0) / range;
    let lower_wick_ratio =
        (pivot.candle.open.min(pivot.candle.close) - pivot.candle.low).max(0.0) / range;
    let (activation_price, directional_wick_ratio, opposite_wick_ratio) = match direction {
        MarketVelocityTradeDirection::Long => {
            (pivot.candle.low, lower_wick_ratio, upper_wick_ratio)
        }
        MarketVelocityTradeDirection::Short => {
            (pivot.candle.high, upper_wick_ratio, lower_wick_ratio)
        }
        MarketVelocityTradeDirection::Both => {
            return Err("momentum_exhaustion_v2_direction_invalid");
        }
    };
    let directional_wick = body_ratio > DOJI_MAX_BODY_RANGE_RATIO
        && directional_wick_ratio >= directional_wick_min_range_ratio
        && directional_wick_ratio > opposite_wick_ratio;
    let activation_price = if directional_wick {
        activation_price
    } else {
        match direction {
            MarketVelocityTradeDirection::Long => pivot.candle.high,
            MarketVelocityTradeDirection::Short => pivot.candle.low,
            MarketVelocityTradeDirection::Both => unreachable!(),
        }
    };
    if !activation_price.is_finite() || activation_price <= 0.0 {
        return Err("momentum_exhaustion_v2_activation_price_invalid");
    }

    Ok((
        AnchorEntrySignalEvidence {
            activation_mode: if directional_wick {
                "directional_wick_limit_12_candles"
            } else {
                "next_candle_intrabar_break"
            },
            pivot_body_range_ratio: body_ratio,
            pivot_directional_wick_range_ratio: directional_wick_ratio,
            pivot_opposite_wick_range_ratio: opposite_wick_ratio,
            activation_price,
            activation_candle_ts_ms: None,
            fill_price: None,
            fill_price_source: None,
            intrabar_path_policy: Some("full_15m_bar_conservative_stop_first"),
        },
        directional_wick,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::market_momentum_exhaustion_reversal_v2_research_args;
    use crate::app::market_velocity_event_backtest::filtered_volume_rsi_ema_macd::momentum_exhaustion_reversal_v1::MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES;
    use crate::app::market_velocity_event_backtest::{BacktestCandle, MS_15M};

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(100.0),
            ema: Some(100.0),
            ema12: Some(100.0),
            ema144: Some(100.0),
            ema169: Some(100.0),
            ema696: Some(100.0),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(2.0),
            rsi14: Some(50.0),
            atr14: Some(2.0),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    fn long_wick_setup() -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        let history_start = latest_idx - MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES;
        candles[history_start].candle.open = 100.0;
        candles[latest_idx - 1].candle.close = 91.0;
        candles[latest_idx].candle = BacktestCandle {
            ts: latest_idx as i64 * MS_15M,
            open: 91.0,
            high: 92.0,
            low: 87.5,
            close: 91.8,
            volume: 25.0,
        };
        candles[latest_idx].volume_ccy = Some(200.0);
        candles
    }

    #[test]
    fn lower_wick_uses_p_low_limit_and_first_volume_target_tier() {
        let candles = long_wick_setup();
        let args = market_momentum_exhaustion_reversal_v2_research_args().unwrap();
        let signal = signal(&candles, candles.len(), &args).unwrap();
        let anchor = signal.evidence.anchor_entry.as_ref().unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(anchor.activation_mode, "directional_wick_limit_12_candles");
        assert_eq!(anchor.activation_price, 87.5);
        assert_eq!(signal.evidence.take_profit_atr_multiplier, Some(2.7));
    }

    #[test]
    fn indicators_and_future_candle_do_not_change_the_frozen_setup() {
        let mut candles = long_wick_setup();
        let completed_count = candles.len();
        let args = market_momentum_exhaustion_reversal_v2_research_args().unwrap();
        let before = signal(&candles, completed_count, &args).unwrap();
        let latest_idx = completed_count - 1;
        candles[latest_idx].rsi14 = None;
        candles[latest_idx].macd_line = None;
        candles[latest_idx].ema12 = None;
        candles[latest_idx].ema144 = None;
        candles[latest_idx].ema169 = None;
        candles[latest_idx].ema696 = None;
        let mut future = candle(completed_count);
        future.candle.high = 10_000.0;
        future.candle.low = 1.0;
        candles.push(future);
        let after = signal(&candles, completed_count, &args).unwrap();

        assert_eq!(before, after);
    }
}
