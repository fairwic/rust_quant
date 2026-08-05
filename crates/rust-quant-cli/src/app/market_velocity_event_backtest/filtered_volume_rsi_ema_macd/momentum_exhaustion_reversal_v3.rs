use super::super::{ComputedCandle, MarketVelocityEventBacktestArgs};
use super::momentum_exhaustion_reversal_v2::{signal_with_policy, MomentumExhaustionSignalPolicy};
use super::FilteredVolumeRsiEmaMacdSignal;

/// V3 只把方向影线占完整振幅的最低比例从 V2 的 60% 调整为 55%。
pub(crate) const MOMENTUM_EXHAUSTION_V3_WICK_MIN_RANGE_RATIO: f64 = 0.55;
/// V3 的最小假设标识，明确其余动量、成交量、成交与风险合同均继承冻结 V2。
pub(crate) const MOMENTUM_EXHAUSTION_V3_HYPOTHESIS: &str =
    "prior_96_net_move_8pct_plus_abnormal_volume_then_wick55_extreme_limit12";

const V3_POLICY: MomentumExhaustionSignalPolicy = MomentumExhaustionSignalPolicy {
    directional_wick_min_range_ratio: MOMENTUM_EXHAUSTION_V3_WICK_MIN_RANGE_RATIO,
    hypothesis: MOMENTUM_EXHAUSTION_V3_HYPOTHESIS,
    long_wick_trigger: "momentum_exhaustion_lower_wick_limit12_long_v3",
    long_touch_trigger: "momentum_exhaustion_next_high_touch_long_v3",
    short_wick_trigger: "momentum_exhaustion_upper_wick_limit12_short_v3",
    short_touch_trigger: "momentum_exhaustion_next_low_touch_short_v3",
};

/// 使用 55% 方向影线阈值生成 V3 信号，其余判断复用 V2 的同一纯策略流程。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    signal_with_policy(candles, completed_count, args, V3_POLICY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::{
        market_momentum_exhaustion_reversal_v2_research_args,
        market_momentum_exhaustion_reversal_v3_research_args,
    };
    use crate::app::market_velocity_event_backtest::{BacktestCandle, MS_15M};

    fn candle(idx: usize) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: idx as i64 * MS_15M,
                open: 0.55,
                high: 0.56,
                low: 0.54,
                close: 0.55,
                volume: 10.0,
            },
            volume_ccy: Some(100.0),
            sma: Some(0.55),
            ema: Some(0.55),
            ema12: Some(0.55),
            ema144: Some(0.55),
            ema169: Some(0.55),
            ema696: Some(0.55),
            previous_volume_avg: Some(10.0),
            previous_range_avg: Some(0.02),
            rsi14: Some(50.0),
            atr14: Some(0.01),
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: Some(0.0),
            macd_signal_line: Some(0.0),
            macd_histogram: Some(0.0),
        }
    }

    /// 复刻明细 2268104 的 p 形态：上影线占完整振幅约 56.64%。
    fn virtual_short_setup() -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let pivot_idx = candles.len() - 1;
        candles[pivot_idx - 96].candle.open = 0.53;
        candles[pivot_idx - 1].candle.close = 0.637;
        candles[pivot_idx].candle = BacktestCandle {
            ts: pivot_idx as i64 * MS_15M,
            open: 0.6337,
            high: 0.6452,
            low: 0.6309,
            close: 0.6371,
            volume: 25.0,
        };
        candles[pivot_idx].volume_ccy = Some(200.0);
        candles
    }

    #[test]
    fn virtual_56pct_upper_wick_changes_only_v3_entry_mode() {
        let candles = virtual_short_setup();
        let v2_args = market_momentum_exhaustion_reversal_v2_research_args().unwrap();
        let v3_args = market_momentum_exhaustion_reversal_v3_research_args().unwrap();
        let v2 = super::super::momentum_exhaustion_reversal_v2::signal(
            &candles,
            candles.len(),
            &v2_args,
        )
        .unwrap();
        let v3 = signal(&candles, candles.len(), &v3_args).unwrap();
        let v2_anchor = v2.evidence.anchor_entry.as_ref().unwrap();
        let v3_anchor = v3.evidence.anchor_entry.as_ref().unwrap();

        assert_eq!(v2.trigger, "momentum_exhaustion_next_low_touch_short_v2");
        assert_eq!(v2_anchor.activation_mode, "next_candle_intrabar_break");
        assert_eq!(v2_anchor.activation_price, 0.6309);
        assert_eq!(
            v3.trigger,
            "momentum_exhaustion_upper_wick_limit12_short_v3"
        );
        assert_eq!(
            v3_anchor.activation_mode,
            "directional_wick_limit_12_candles"
        );
        assert_eq!(v3_anchor.activation_price, 0.6452);
        assert!(
            (v3_anchor.pivot_directional_wick_range_ratio - 0.566_433_566_433_565_6).abs() < 1e-12
        );
    }
}
