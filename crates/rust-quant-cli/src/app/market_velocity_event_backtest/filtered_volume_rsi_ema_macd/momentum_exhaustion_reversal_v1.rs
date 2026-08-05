use super::super::{
    ComputedCandle, IsolatedStrategyFamilySignalEvidence, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection,
};
use super::isolated_family_common::{
    anchor_entry_evidence, current_volume_gate, fixed_one_r_signal,
    ISOLATED_FILTERED_VOLUME_MIN_RATIO,
};
use super::FilteredVolumeRsiEmaMacdSignal;

/// 动量衰竭家族只观察信号 K 之前精确 96 根已完成 K 线。
pub(crate) const MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES: usize = 96;
/// 历史首开到末收的绝对净移动至少为 8%。
pub(crate) const MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT: f64 = 8.0;
/// 落库时用于证明本家族没有混入 RSI、EMA 或平台结论的最小假设标识。
pub(crate) const MOMENTUM_EXHAUSTION_HYPOTHESIS: &str =
    "prior_96_net_move_8pct_plus_abnormal_volume_then_price_rejection";

const LONG_WICK_TRIGGER: &str = "momentum_exhaustion_lower_wick_next_open_long";
const LONG_TOUCH_TRIGGER: &str = "momentum_exhaustion_next_high_touch_long";
const SHORT_WICK_TRIGGER: &str = "momentum_exhaustion_upper_wick_next_open_short";
const SHORT_TOUCH_TRIGGER: &str = "momentum_exhaustion_next_low_touch_short";

/// 仅以历史净移动、当前异常量和价格拒绝/确认构造动量衰竭反转 setup。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    if (args.entry_min_volume_ratio - ISOLATED_FILTERED_VOLUME_MIN_RATIO).abs() > f64::EPSILON {
        return Err("momentum_exhaustion_ratio_policy_mismatch");
    }
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("momentum_exhaustion_not_ready")?;
    let latest = candles
        .get(latest_idx)
        .ok_or("momentum_exhaustion_not_ready")?;
    let (volume, weekly_volume) = current_volume_gate(candles, latest_idx)
        .map_err(|_| "momentum_exhaustion_volume_not_confirmed")?;
    let net_move_pct =
        prior_net_move_pct(candles, latest_idx).ok_or("momentum_exhaustion_history_not_ready")?;
    let direction = if net_move_pct <= -MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT {
        MarketVelocityTradeDirection::Long
    } else if net_move_pct >= MOMENTUM_EXHAUSTION_MIN_NET_MOVE_PCT {
        MarketVelocityTradeDirection::Short
    } else {
        return Err("momentum_exhaustion_net_move_not_confirmed");
    };
    let (anchor_entry, direct_wick_entry) = anchor_entry_evidence(latest, direction)?;
    let trigger = match (direction, direct_wick_entry) {
        (MarketVelocityTradeDirection::Long, true) => LONG_WICK_TRIGGER,
        (MarketVelocityTradeDirection::Long, false) => LONG_TOUCH_TRIGGER,
        (MarketVelocityTradeDirection::Short, true) => SHORT_WICK_TRIGGER,
        (MarketVelocityTradeDirection::Short, false) => SHORT_TOUCH_TRIGGER,
        (MarketVelocityTradeDirection::Both, _) => unreachable!(),
    };

    fixed_one_r_signal(
        latest,
        direction,
        trigger,
        volume,
        weekly_volume,
        Vec::new(),
        Some(anchor_entry),
        IsolatedStrategyFamilySignalEvidence {
            family: "momentum_exhaustion_reversal",
            hypothesis: MOMENTUM_EXHAUSTION_HYPOTHESIS,
            prior_96_net_move_pct: Some(net_move_pct),
            platform_breakdown: None,
            long_term_ema_confirmed: false,
            ema696_recent: Vec::new(),
        },
    )
}

/// 计算 p 前精确 96 根从首根开盘到末根收盘的有符号净变化百分比。
pub(super) fn prior_net_move_pct(candles: &[ComputedCandle], latest_idx: usize) -> Option<f64> {
    let start = latest_idx.checked_sub(MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES)?;
    let history = candles.get(start..latest_idx)?;
    if history.len() != MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES {
        return None;
    }
    let first_open = history.first()?.candle.open;
    let last_close = history.last()?.candle.close;
    if !first_open.is_finite() || first_open <= 0.0 || !last_close.is_finite() || last_close <= 0.0
    {
        return None;
    }
    Some((last_close - first_open) / first_open * 100.0)
}

#[cfg(test)]
mod tests {
    use super::super::isolated_family_common::ISOLATED_FIXED_TARGET_ATR_MULTIPLIER;
    use super::*;
    use crate::app::market_velocity_event_backtest::args::market_momentum_exhaustion_reversal_v1_research_args;
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

    fn long_setup() -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        let history_start = latest_idx - MOMENTUM_EXHAUSTION_LOOKBACK_CANDLES;
        candles[history_start].candle.open = 100.0;
        candles[latest_idx - 1].candle = BacktestCandle {
            ts: (latest_idx - 1) as i64 * MS_15M,
            open: 91.5,
            high: 92.0,
            low: 90.5,
            close: 91.0,
            volume: 10.0,
        };
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
    fn ignores_rsi_macd_and_ema_when_net_move_volume_and_rejection_are_unchanged() {
        let mut candles = long_setup();
        let args = market_momentum_exhaustion_reversal_v1_research_args().unwrap();
        let before = signal(&candles, candles.len(), &args).unwrap();
        let latest_idx = candles.len() - 1;
        candles[latest_idx].rsi14 = None;
        candles[latest_idx].macd_line = None;
        candles[latest_idx].ema12 = None;
        candles[latest_idx].ema144 = None;
        candles[latest_idx].ema169 = None;
        candles[latest_idx].ema696 = None;
        let after = signal(&candles, candles.len(), &args).unwrap();

        assert_eq!(before.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(before.trigger, after.trigger);
        assert_eq!(before.direction, after.direction);
        assert_eq!(
            before.evidence.take_profit_atr_multiplier,
            Some(ISOLATED_FIXED_TARGET_ATR_MULTIPLIER)
        );
        assert_eq!(
            before
                .evidence
                .isolated_family
                .as_ref()
                .and_then(|family| family.prior_96_net_move_pct),
            Some(-9.0)
        );
    }

    #[test]
    fn future_candle_cannot_change_frozen_setup() {
        let mut candles = long_setup();
        let completed_count = candles.len();
        let args = market_momentum_exhaustion_reversal_v1_research_args().unwrap();
        let before = signal(&candles, completed_count, &args).unwrap();
        let mut future = candle(completed_count);
        future.candle.low = 1.0;
        future.candle.high = 1_000.0;
        candles.push(future);
        let after = signal(&candles, completed_count, &args).unwrap();

        assert_eq!(before, after);
    }
}
