use super::*;

fn ohlc(ts: i64, open: f64, high: f64, low: f64, close: f64) -> BacktestCandle {
    BacktestCandle {
        ts,
        open,
        high,
        low,
        close,
        volume: 10.0,
    }
}

fn ohlcv(ts: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> BacktestCandle {
    BacktestCandle {
        ts,
        open,
        high,
        low,
        close,
        volume,
    }
}

fn fast_momentum_breakout_candles() -> Vec<BacktestCandle> {
    let closes = [
        100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 120.0, 116.0, 110.0, 104.0, 100.0,
        102.0, 104.0, 106.0, 108.0, 110.0, 112.0, 114.0, 115.0, 135.0,
    ];
    closes
        .iter()
        .enumerate()
        .map(|(idx, close)| {
            let volume = if idx == closes.len() - 1 { 30.0 } else { 10.0 };
            BacktestCandle {
                ts: MS_15M * idx as i64,
                open: close - 1.0,
                high: close + 1.0,
                low: close - 1.0,
                close: *close,
                volume,
            }
        })
        .collect()
}

#[test]
fn precomputes_previous_range_average_for_direct_kline_momentum() {
    let candles = vec![
        ohlc(0, 100.0, 101.0, 99.0, 100.5),
        ohlc(MS_15M, 100.5, 102.5, 100.0, 101.0),
        ohlc(MS_15M * 2, 101.0, 105.0, 100.0, 104.0),
        ohlc(MS_15M * 3, 104.0, 108.0, 103.0, 107.0),
    ];
    let computed = build_computed_candles(candles, 3);
    assert_eq!(computed[3].previous_range_avg, Some(3.1666666666666665));
}

#[test]
fn entry_confirmation_blocks_when_body_ratio_is_too_small() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_min_body_ratio_pct: Some(60.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = fast_momentum_breakout_candles();
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 22;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "body_ratio_not_confirmed");
}

#[test]
fn entry_confirmation_blocks_when_close_position_is_too_low() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_min_close_position_pct: Some(80.0),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = fast_momentum_breakout_candles();
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 22;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "close_position_not_confirmed");
}

#[test]
fn entry_confirmation_blocks_when_range_expansion_is_too_small() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_min_range_expansion_ratio: Some(1.5),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = fast_momentum_breakout_candles();
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 22;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(!ok);
    assert_eq!(reason, "range_expansion_not_confirmed");
}

#[test]
fn entry_confirmation_accepts_direct_kline_momentum_shape_filters() {
    let args = MarketVelocityEventBacktestArgs {
        entry_period: 3,
        entry_max_distance_pct: 50.0,
        entry_min_volume_ratio: 1.2,
        entry_min_body_ratio_pct: Some(65.0),
        entry_min_close_position_pct: Some(80.0),
        entry_min_range_expansion_ratio: Some(1.5),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let candles = vec![
        ohlcv(0, 100.0, 101.0, 99.5, 100.5, 10.0),
        ohlcv(MS_15M, 100.5, 101.5, 100.0, 101.0, 10.0),
        ohlcv(MS_15M * 2, 101.0, 102.0, 100.8, 101.5, 10.0),
        ohlcv(MS_15M * 3, 101.5, 102.2, 100.8, 101.0, 10.0),
        ohlcv(MS_15M * 4, 101.2, 112.0, 101.0, 111.8, 25.0),
    ];
    let computed = build_computed_candles(candles, args.entry_period);
    let event_ts = MS_15M * 5;
    let (ok, reason) = entry_confirmation(
        &computed,
        event_ts,
        MarketVelocityTradeDirection::Long,
        &args,
    );
    assert!(ok);
    assert_eq!(reason, "reclaim_ema");
}
