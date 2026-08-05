use super::super::{ComputedCandle, MarketVelocityEventBacktestArgs};
use super::{weekly_base_volume_v3, FilteredVolumeRsiEmaMacdSignal};

/// EMA 延续候选的收盘价最多允许离 EMA144 一个 ATR14；边界值可以入场。
pub(in super::super) const EMA144_MAX_DISTANCE_ATR: f64 = 1.0;

/// 在 v3 原始候选合并前剔除离 EMA144 过远的 EMA 延续候选。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    weekly_base_volume_v3::signal_with_ema144_distance_gate(
        candles,
        completed_count,
        args,
        EMA144_MAX_DISTANCE_ATR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::{
        market_filtered_volume_rsi_ema_macd_v3_research_args,
        market_filtered_volume_rsi_ema_macd_v5_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_strategy_type,
        BacktestCandle, MarketVelocityTradeDirection,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET, MS_15M,
    };

    /// 构造 EMA696、周成交额与成交量标记均已预热的连续已完成 K 线。
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

    fn candles() -> Vec<ComputedCandle> {
        (0..700).map(candle).collect()
    }

    fn confirm_volume(candles: &mut [ComputedCandle]) {
        candles.last_mut().expect("signal candle").candle.volume = 30.0;
    }

    fn v5_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let args = market_filtered_volume_rsi_ema_macd_v5_research_args()
            .expect("v5 research args remain valid");
        signal(candles, completed_count, &args)
    }

    fn ema_short_fixture(ema144: f64) -> Vec<ComputedCandle> {
        let mut candles = candles();
        let current = candles.last_mut().expect("signal candle");
        current.candle.open = 100.0;
        current.candle.high = 100.2;
        current.candle.low = 98.2;
        current.candle.close = 98.5;
        current.ema12 = Some(ema144 - 0.5);
        current.ema144 = Some(ema144);
        current.ema696 = Some(ema144 + 0.5);
        confirm_volume(&mut candles);
        candles
    }

    fn ema_long_fixture(ema144: f64) -> Vec<ComputedCandle> {
        let mut candles = candles();
        let current = candles.last_mut().expect("signal candle");
        current.candle.open = 100.0;
        current.candle.high = 101.8;
        current.candle.low = 99.8;
        current.candle.close = 101.5;
        current.ema12 = Some(ema144 + 0.5);
        current.ema144 = Some(ema144);
        current.ema696 = Some(ema144 - 0.5);
        confirm_volume(&mut candles);
        candles
    }

    #[test]
    fn short_at_one_atr_boundary_is_accepted_but_farther_short_is_blocked() {
        let boundary = ema_short_fixture(100.5);
        let signal = v5_signal(&boundary, boundary.len()).unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(signal.trigger, "ema_bearish_continuation_short");
        assert_eq!(signal.evidence.ema144_distance_atr, Some(1.0));
        assert_eq!(signal.evidence.ema144_max_distance_atr, Some(1.0));
        assert!(!signal.evidence.ema_candidate_blocked_by_distance);

        let far = ema_short_fixture(100.500_001);
        assert_eq!(
            v5_signal(&far, far.len()),
            Err("filtered_volume_v5_no_branch_signal")
        );
    }

    #[test]
    fn long_distance_gate_is_the_exact_mirror_of_short() {
        let near = ema_long_fixture(100.5);
        let signal = v5_signal(&near, near.len()).unwrap();
        assert_eq!(signal.direction, MarketVelocityTradeDirection::Long);
        assert_eq!(signal.trigger, "ema_bullish_continuation_long");
        assert_eq!(signal.evidence.ema144_distance_atr, Some(0.5));

        let far = ema_long_fixture(99.0);
        assert_eq!(
            v5_signal(&far, far.len()),
            Err("filtered_volume_v5_no_branch_signal")
        );
    }

    #[test]
    fn eigen_bad_short_is_far_from_ema144_and_is_blocked() {
        let mut candles = candles();
        let current = candles.last_mut().expect("signal candle");
        current.candle.open = 0.2303;
        current.candle.high = 0.2304;
        current.candle.low = 0.2256;
        current.candle.close = 0.2270;
        current.ema12 = Some(0.230687);
        current.ema144 = Some(0.236360);
        current.ema696 = Some(0.236759);
        current.rsi14 = Some(34.1212);
        current.atr14 = Some(0.0020);
        confirm_volume(&mut candles);
        let v3_args = market_filtered_volume_rsi_ema_macd_v3_research_args().unwrap();
        let v3 = weekly_base_volume_v3::signal(&candles, candles.len(), &v3_args).unwrap();

        assert_eq!(v3.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(v3.trigger, "ema_bearish_continuation_short");
        assert!(((0.236360_f64 - 0.2270) / 0.0020 - 4.68).abs() < 1e-12);
        assert_eq!(
            v5_signal(&candles, candles.len()),
            Err("filtered_volume_v5_no_branch_signal")
        );
    }

    #[test]
    fn independent_rsi_divergence_survives_a_blocked_far_ema_candidate() {
        let mut candles = ema_short_fixture(102.0);
        let reference_idx = candles.len() - 20;
        candles[reference_idx].candle.high = 102.0;
        candles[reference_idx].rsi14 = Some(75.0);
        let pivot_idx = candles.len() - 4;
        candles[pivot_idx].candle.high = 103.0;
        candles[pivot_idx].rsi14 = Some(72.0);

        let signal = v5_signal(&candles, candles.len()).unwrap();

        assert_eq!(signal.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(signal.trigger, "rsi_bearish_divergence_short");
        assert!(signal.evidence.ema_candidate_blocked_by_distance);
        assert!(signal.evidence.ema144_distance_atr.unwrap() > 1.0);
    }

    #[test]
    fn future_candle_does_not_change_completed_v5_signal() {
        let candles = ema_long_fixture(100.5);
        let expected = v5_signal(&candles, candles.len()).unwrap();
        let mut with_future = candles.clone();
        let mut future = candle(with_future.len());
        future.candle.volume = 10_000.0;
        future.candle.close = 1.0;
        with_future.push(future);

        assert_eq!(v5_signal(&with_future, candles.len()).unwrap(), expected);
    }

    #[test]
    fn v5_remains_research_only_under_the_v3_strategy_family() {
        let args = market_filtered_volume_rsi_ema_macd_v5_research_args().unwrap();
        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V5_PRESET,
        )
        .unwrap();

        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY
        );
        assert_eq!(manifest.channel, "research");
        assert_eq!(
            manifest.manifest_json["execution"]["paper_observation_eligible"],
            false
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["fast_momentum_filters"]
                ["filtered_volume_rsi_ema_macd"]["ema_branch"]["max_distance_from_ema144_atr"],
            EMA144_MAX_DISTANCE_ATR
        );
    }
}
