use super::*;
use crate::app::market_velocity_event_backtest::BacktestCandle;

/// 构造测试用完整 K 线；只覆盖入场后止损和目标路径。
fn candle(ts: i64, open: f64, high: f64, low: f64, close: f64) -> ComputedCandle {
    ComputedCandle {
        candle: BacktestCandle {
            ts,
            open,
            high,
            low,
            close,
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

/// 两侧虽有不同入场价，但初始风险距离必须同为 1.5 倍来源 ATR。
#[test]
fn entry_plans_share_frozen_atr_risk_distance() {
    let baseline = entry_plan(0, 0, 100.0, 2.0, 2.7, MarketVelocityTradeDirection::Short)
        .expect("baseline plan");
    let variant = entry_plan(1, 1, 99.0, 2.0, 2.7, MarketVelocityTradeDirection::Short)
        .expect("variant plan");
    assert!(approx_equal(
        (baseline.stop_price - baseline.entry_price).abs(),
        3.0
    ));
    assert!(approx_equal(
        (variant.stop_price - variant.entry_price).abs(),
        3.0
    ));
}

/// 缺少 tick 顺序时，同棒同时触发止损和目标必须按止损退出。
#[test]
fn same_bar_stop_and_target_uses_stop_first() {
    let candles = vec![candle(0, 100.0, 104.0, 94.0, 100.0)];
    let plan = EntryPlan {
        entry_ts_ms: 0,
        entry_idx: 0,
        entry_price: 100.0,
        stop_price: 103.0,
        target_price: 95.0,
    };
    let path =
        simulate_exit(&candles, plan, MarketVelocityTradeDirection::Short).expect("exit path");
    assert_eq!(path.exit_reason, "both_hit_stop_first");
    assert_eq!(path.exit_price, 103.0);
}

/// 量比分档沿用来源 V2 的 2.7、3.6、4.5 ATR 三档。
#[test]
fn target_tiers_match_source_v2() {
    assert_eq!(target_atr_multiplier(2.5), Some(2.7));
    assert_eq!(target_atr_multiplier(4.0), Some(3.6));
    assert_eq!(target_atr_multiplier(6.0), Some(4.5));
    assert_eq!(target_atr_multiplier(2.49), None);
}

/// 双边名义成本应让毛 1R 的净结果严格低于 1R。
#[test]
fn net_r_deducts_entry_and_exit_costs() {
    let value = net_r(100.0, 97.0, 3.0, MarketVelocityTradeDirection::Short);
    assert!(value < 1.0);
    assert!(value > 0.9);
}
