use super::super::{ComputedCandle, MarketVelocityEventBacktestArgs};
use super::{weekly_base_volume_v3, FilteredVolumeRsiEmaMacdSignal};

/// 冲突缓冲使用最近 12 根已完成 15m K 线的收盘价。
pub(in super::super) const BOLLINGER_CONFLICT_PERIOD: usize = 12;
/// 上下轨固定为总体标准差的 2.6 倍，避免把现有 20/2 布林带配置混入本版本。
pub(in super::super) const BOLLINGER_CONFLICT_STDDEV_MULTIPLIER: f64 = 2.6;

/// 在 v3 原始候选之后叠加布林反向候选；布林带自身没有创建交易的权限。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    weekly_base_volume_v3::signal_with_bollinger_conflict(
        candles,
        completed_count,
        args,
        BOLLINGER_CONFLICT_PERIOD,
        BOLLINGER_CONFLICT_STDDEV_MULTIPLIER,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::computed_candles::bollinger_bands_from_closes;
    use crate::app::market_velocity_event_backtest::{
        market_filtered_volume_rsi_ema_macd_v3_research_args,
        market_filtered_volume_rsi_ema_macd_v4_research_args,
        market_velocity_paper_strategy_preset_manifest, market_velocity_strategy_type,
        BacktestCandle, MarketVelocityTradeDirection,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY,
        MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET, MS_15M,
    };

    /// 构造已经完成 EMA696 与周成交量预热的连续样本，测试只改动信号附近字段。
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

    fn v4_signal(
        candles: &[ComputedCandle],
        completed_count: usize,
    ) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
        let args = market_filtered_volume_rsi_ema_macd_v4_research_args()
            .expect("v4 research args remain valid");
        signal(candles, completed_count, &args)
    }

    /// 复刻 back_test_detail 2195456 的 12 根收盘与信号形态，防止规则再次追空下轨。
    fn eigen_short_fixture() -> Vec<ComputedCandle> {
        let mut candles = candles();
        let closes = [
            0.2311, 0.2317, 0.2314, 0.2307, 0.2319, 0.2337, 0.2325, 0.2318, 0.2321, 0.2305, 0.2303,
            0.2270,
        ];
        let start = candles.len() - closes.len();
        for (offset, close) in closes.into_iter().enumerate() {
            candles[start + offset].candle.close = close;
        }
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
        candles
    }

    #[test]
    fn eigen_lower_band_touch_blocks_existing_ema_short() {
        let candles = eigen_short_fixture();
        let v3_args = market_filtered_volume_rsi_ema_macd_v3_research_args().unwrap();
        let v3 = weekly_base_volume_v3::signal(&candles, candles.len(), &v3_args).unwrap();
        let closes = candles[candles.len() - BOLLINGER_CONFLICT_PERIOD..]
            .iter()
            .map(|candle| candle.candle.close)
            .collect::<Vec<_>>();
        let bands =
            bollinger_bands_from_closes(&closes, BOLLINGER_CONFLICT_STDDEV_MULTIPLIER).unwrap();

        assert_eq!(v3.direction, MarketVelocityTradeDirection::Short);
        assert_eq!(v3.trigger, "ema_bearish_continuation_short");
        assert!((bands.lower - 0.227177390623257).abs() < 1e-12);
        assert!(candles.last().unwrap().candle.low <= bands.lower);
        assert_eq!(
            v4_signal(&candles, candles.len()),
            Err("filtered_volume_v4_direction_conflict")
        );
    }

    #[test]
    fn upper_band_touch_mirrors_the_long_conflict_rule() {
        let mut candles = candles();
        let start = candles.len() - BOLLINGER_CONFLICT_PERIOD;
        let last_idx = candles.len() - 1;
        for candle in &mut candles[start..last_idx] {
            candle.candle.close = 100.0;
        }
        let current = candles.last_mut().unwrap();
        current.candle.open = 99.5;
        current.candle.high = 101.7;
        current.candle.low = 99.4;
        current.candle.close = 101.5;
        current.ema12 = Some(100.5);
        current.ema144 = Some(100.0);
        current.ema696 = Some(99.5);
        confirm_volume(&mut candles);

        assert_eq!(
            v4_signal(&candles, candles.len()),
            Err("filtered_volume_v4_direction_conflict")
        );
    }

    #[test]
    fn bollinger_touch_without_an_original_branch_cannot_open_a_trade() {
        let mut candles = eigen_short_fixture();
        let current = candles.last_mut().unwrap();
        current.ema12 = Some(100.0);
        current.ema144 = Some(100.0);
        current.ema696 = Some(100.0);

        assert_eq!(
            v4_signal(&candles, candles.len()),
            Err("filtered_volume_v4_no_branch_signal")
        );
    }

    #[test]
    fn exact_lower_band_touch_is_inclusive_and_future_candles_are_ignored() {
        let mut candles = candles();
        let closes = [
            100.6, 99.4, 100.6, 99.4, 100.6, 99.4, 100.6, 99.4, 100.6, 99.4, 100.6, 99.0,
        ];
        let start = candles.len() - closes.len();
        for (offset, close) in closes.into_iter().enumerate() {
            candles[start + offset].candle.close = close;
        }
        let bands =
            bollinger_bands_from_closes(&closes, BOLLINGER_CONFLICT_STDDEV_MULTIPLIER).unwrap();
        let current = candles.last_mut().unwrap();
        current.candle.open = 100.5;
        current.candle.high = 100.6;
        current.candle.low = bands.lower;
        current.candle.close = 99.0;
        current.ema12 = Some(99.5);
        current.ema144 = Some(100.0);
        current.ema696 = Some(101.0);
        confirm_volume(&mut candles);
        let completed_count = candles.len();

        assert_eq!(
            v4_signal(&candles, completed_count),
            Err("filtered_volume_v4_direction_conflict")
        );
        let mut future = candle(completed_count);
        future.candle.high = 1_000.0;
        future.candle.low = 0.01;
        candles.push(future);
        assert_eq!(
            v4_signal(&candles, completed_count),
            Err("filtered_volume_v4_direction_conflict")
        );
    }

    #[test]
    fn v4_has_an_independent_research_identity_but_keeps_the_v3_strategy_family() {
        let args = market_filtered_volume_rsi_ema_macd_v4_research_args().unwrap();
        assert_eq!(
            market_velocity_strategy_type(&args),
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V3_STRATEGY_KEY
        );
        let manifest = market_velocity_paper_strategy_preset_manifest(
            MARKET_FILTERED_VOLUME_RSI_EMA_MACD_V4_PRESET,
        )
        .unwrap();
        assert_eq!(manifest.channel, "research");
        assert_eq!(
            manifest.manifest_json["parameters"]["fast_momentum_filters"]
                ["filtered_volume_rsi_ema_macd"]["bollinger_conflict_buffer"]["period"],
            BOLLINGER_CONFLICT_PERIOD
        );
        assert_eq!(
            manifest.manifest_json["parameters"]["fast_momentum_filters"]
                ["filtered_volume_rsi_ema_macd"]["bollinger_conflict_buffer"]
                ["standard_deviation_multiplier"],
            BOLLINGER_CONFLICT_STDDEV_MULTIPLIER
        );
    }
}
