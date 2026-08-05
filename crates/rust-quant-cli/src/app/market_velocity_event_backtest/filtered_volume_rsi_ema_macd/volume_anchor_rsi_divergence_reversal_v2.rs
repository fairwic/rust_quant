use super::super::{ComputedCandle, MarketVelocityEventBacktestArgs, MarketVelocityTradeDirection};
use super::volume_anchor_rsi_divergence_reversal_v1;
use super::FilteredVolumeRsiEmaMacdSignal;

/// V2 逐笔证据使用独立模式名，避免与未检查锚点周期的 V1 明细混淆。
pub(crate) const ISOLATED_RSI_V2_COMPARISON_MODE: &str =
    "isolated_weekly_p90_filtered_volume_anchors_gap4_swing_reset_wick_or_next_touch";
/// V2 的单一研究假设：只接受尚未发生反方向 RSI 摆动重置的分离锚点。
pub(crate) const VOLUME_ANCHOR_RSI_V2_HYPOTHESIS: &str =
    "nearest_volume_anchor_rsi_divergence_without_intermediate_swing_reset";
/// 锚点 q 与信号 p 之间必须严格存在的已完成 15m K 线数量。
pub(crate) const MIN_INTERVENING_CANDLES: usize = 4;
/// 做多锚点周期内 RSI 高于该值代表超卖摆动已经完成，旧 q 失效。
pub(crate) const BULLISH_ANCHOR_RESET_RSI: f64 = 60.0;
/// 做空锚点周期内 RSI 低于该值代表超买摆动已经完成，旧 q 失效。
pub(crate) const BEARISH_ANCHOR_RESET_RSI: f64 = 40.0;

const GAP_INSUFFICIENT: &str = "volume_anchor_rsi_v2_anchor_gap_insufficient";
const BULLISH_SWING_RESET: &str = "volume_anchor_rsi_v2_bullish_anchor_reset_above_60";
const BEARISH_SWING_RESET: &str = "volume_anchor_rsi_v2_bearish_anchor_reset_below_40";
const INTERMEDIATE_RSI_MISSING: &str = "volume_anchor_rsi_v2_intermediate_rsi_missing";

/// 在冻结 V1 信号之上只增加锚点间隔和中间 RSI 摆动重置门禁。
///
/// V1 已经先固定最近合格 q，V2 再校验该 q；校验失败直接无信号，禁止回退到更早锚点。
pub(super) fn signal(
    candles: &[ComputedCandle],
    completed_count: usize,
    args: &MarketVelocityEventBacktestArgs,
) -> Result<FilteredVolumeRsiEmaMacdSignal, &'static str> {
    let mut signal =
        volume_anchor_rsi_divergence_reversal_v1::signal(candles, completed_count, args)?;
    let latest_idx = completed_count
        .checked_sub(1)
        .ok_or("volume_anchor_rsi_v2_not_ready")?;
    let divergence = signal
        .evidence
        .rsi_divergences
        .first_mut()
        .ok_or("volume_anchor_rsi_v2_divergence_evidence_missing")?;
    validate_anchor_cycle(
        candles,
        latest_idx,
        signal.direction,
        divergence.reference_pivot_ts_ms,
    )?;
    divergence.comparison_mode = ISOLATED_RSI_V2_COMPARISON_MODE;
    signal
        .evidence
        .isolated_family
        .as_mut()
        .ok_or("volume_anchor_rsi_v2_family_evidence_missing")?
        .hypothesis = VOLUME_ANCHOR_RSI_V2_HYPOTHESIS;
    Ok(signal)
}

/// 校验最近 q 到 p 的完整周期；边界 60/40 允许，缺失 RSI 按失败关闭处理。
fn validate_anchor_cycle(
    candles: &[ComputedCandle],
    latest_idx: usize,
    direction: MarketVelocityTradeDirection,
    reference_ts_ms: i64,
) -> Result<(), &'static str> {
    let reference_idx = candles
        .get(..latest_idx)
        .and_then(|history| {
            history
                .iter()
                .rposition(|candle| candle.candle.ts == reference_ts_ms)
        })
        .ok_or("volume_anchor_rsi_v2_reference_anchor_missing")?;
    let intervening_count = latest_idx
        .checked_sub(reference_idx + 1)
        .ok_or(GAP_INSUFFICIENT)?;
    if intervening_count < MIN_INTERVENING_CANDLES {
        return Err(GAP_INSUFFICIENT);
    }

    for candle in &candles[(reference_idx + 1)..latest_idx] {
        let rsi = candle
            .rsi14
            .filter(|value| value.is_finite())
            .ok_or(INTERMEDIATE_RSI_MISSING)?;
        match direction {
            MarketVelocityTradeDirection::Long if rsi > BULLISH_ANCHOR_RESET_RSI => {
                return Err(BULLISH_SWING_RESET);
            }
            MarketVelocityTradeDirection::Short if rsi < BEARISH_ANCHOR_RESET_RSI => {
                return Err(BEARISH_SWING_RESET);
            }
            MarketVelocityTradeDirection::Long | MarketVelocityTradeDirection::Short => {}
            MarketVelocityTradeDirection::Both => {
                return Err("volume_anchor_rsi_v2_direction_invalid");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::args::{
        market_volume_anchor_rsi_divergence_reversal_v1_research_args,
        market_volume_anchor_rsi_divergence_reversal_v2_research_args,
    };
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

    fn qualify_anchor(candles: &mut [ComputedCandle], idx: usize, rsi: f64) {
        candles[idx].candle.volume = 25.0;
        candles[idx].volume_ccy = Some(200.0);
        candles[idx].rsi14 = Some(rsi);
    }

    fn long_setup(anchor_distance: usize) -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        let reference_idx = latest_idx - anchor_distance;
        qualify_anchor(&mut candles, reference_idx, 25.0);
        candles[reference_idx].candle.low = 97.0;
        qualify_anchor(&mut candles, latest_idx, 27.0);
        candles[latest_idx].candle = BacktestCandle {
            ts: latest_idx as i64 * MS_15M,
            open: 100.0,
            high: 101.5,
            low: 96.0,
            close: 100.8,
            volume: 25.0,
        };
        candles
    }

    fn short_setup(anchor_distance: usize) -> Vec<ComputedCandle> {
        let mut candles = (0..750).map(candle).collect::<Vec<_>>();
        let latest_idx = candles.len() - 1;
        let reference_idx = latest_idx - anchor_distance;
        qualify_anchor(&mut candles, reference_idx, 75.0);
        candles[reference_idx].candle.high = 103.0;
        qualify_anchor(&mut candles, latest_idx, 73.0);
        candles[latest_idx].candle = BacktestCandle {
            ts: latest_idx as i64 * MS_15M,
            open: 100.0,
            high: 104.0,
            low: 98.5,
            close: 99.2,
            volume: 25.0,
        };
        candles
    }

    #[test]
    fn requires_four_strictly_intervening_candles_without_changing_v1() {
        let three_between = long_setup(4);
        let v1_args = market_volume_anchor_rsi_divergence_reversal_v1_research_args().unwrap();
        let v2_args = market_volume_anchor_rsi_divergence_reversal_v2_research_args().unwrap();

        assert!(volume_anchor_rsi_divergence_reversal_v1::signal(
            &three_between,
            three_between.len(),
            &v1_args
        )
        .is_ok());
        assert_eq!(
            signal(&three_between, three_between.len(), &v2_args),
            Err(GAP_INSUFFICIENT)
        );

        let four_between = long_setup(5);
        assert!(signal(&four_between, four_between.len(), &v2_args).is_ok());
    }

    #[test]
    fn bullish_boundary_60_is_allowed_but_a_higher_rsi_resets_the_anchor() {
        let args = market_volume_anchor_rsi_divergence_reversal_v2_research_args().unwrap();
        let mut boundary = long_setup(8);
        let latest_idx = boundary.len() - 1;
        boundary[latest_idx - 3].rsi14 = Some(60.0);
        assert!(signal(&boundary, boundary.len(), &args).is_ok());

        boundary[latest_idx - 3].rsi14 = Some(60.01);
        assert_eq!(
            signal(&boundary, boundary.len(), &args),
            Err(BULLISH_SWING_RESET)
        );
    }

    #[test]
    fn bearish_boundary_40_is_allowed_but_a_lower_rsi_resets_the_anchor() {
        let args = market_volume_anchor_rsi_divergence_reversal_v2_research_args().unwrap();
        let mut boundary = short_setup(8);
        let latest_idx = boundary.len() - 1;
        boundary[latest_idx - 3].rsi14 = Some(40.0);
        assert!(signal(&boundary, boundary.len(), &args).is_ok());

        boundary[latest_idx - 3].rsi14 = Some(39.99);
        assert_eq!(
            signal(&boundary, boundary.len(), &args),
            Err(BEARISH_SWING_RESET)
        );
    }

    #[test]
    fn missing_intermediate_rsi_fails_closed() {
        let args = market_volume_anchor_rsi_divergence_reversal_v2_research_args().unwrap();
        let mut candles = long_setup(8);
        let latest_idx = candles.len() - 1;
        candles[latest_idx - 2].rsi14 = None;

        assert_eq!(
            signal(&candles, candles.len(), &args),
            Err(INTERMEDIATE_RSI_MISSING)
        );
    }

    #[test]
    fn invalid_nearest_anchor_never_falls_back_to_an_older_anchor() {
        let args = market_volume_anchor_rsi_divergence_reversal_v2_research_args().unwrap();
        let mut candles = long_setup(3);
        let latest_idx = candles.len() - 1;
        qualify_anchor(&mut candles, latest_idx - 30, 24.0);
        candles[latest_idx - 30].candle.low = 98.0;

        assert_eq!(
            signal(&candles, candles.len(), &args),
            Err(GAP_INSUFFICIENT)
        );
    }
}
