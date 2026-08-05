use super::*;
use crate::app::market_velocity_event_backtest::BacktestCandle;
use serde_json::json;

/// 构造 15 分钟测试 K；L1 仅使用时间和高低价判断成交覆盖。
fn candle(index: usize, high: f64, low: f64) -> ComputedCandle {
    ComputedCandle {
        candle: BacktestCandle {
            ts: index as i64 * MS_15M,
            open: 99.0,
            high,
            low,
            close: 99.0,
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

/// 确认 K 内即使再次触及来源极值，也不能补造成交。
#[test]
fn relimit_is_not_active_on_confirmation_candle() {
    let candles = vec![
        candle(0, 99.0, 98.0),
        candle(1, 101.0, 98.0),
        candle(2, 99.0, 98.0),
    ];
    let resolution = resolve_relimit(&candles, 2, 2, 0, "short", 100.0, &BTreeSet::new())
        .expect("resolve relimit");
    assert_eq!(resolution.entry_idx, None);
    assert_eq!(
        resolution.terminal_status,
        "relimit_not_touched_before_original_setup_expiry"
    );
}

/// 确认发生在来源 setup 第 12 根时不得重新获得 12 根有效期。
#[test]
fn original_setup_expiry_is_not_reset_after_confirmation() {
    let candles = (0..20)
        .map(|index| candle(index, 101.0, 98.0))
        .collect::<Vec<_>>();
    let resolution = resolve_relimit(&candles, 13, 12, 0, "short", 100.0, &BTreeSet::new())
        .expect("resolve exhausted relimit");
    assert_eq!(resolution.entry_idx, None);
    assert_eq!(
        resolution.terminal_status,
        "original_setup_ttl_exhausted_at_confirmation"
    );
}

/// 同一根既触及旧限价又生成新 setup 时，旧订单先按来源极值成交。
#[test]
fn touch_precedes_same_candle_replacement() {
    let candles = vec![
        candle(0, 99.0, 98.0),
        candle(1, 99.0, 98.0),
        candle(2, 101.0, 98.0),
    ];
    let replacement_ts = candles[2].candle.ts;
    let resolution = resolve_relimit(
        &candles,
        2,
        2,
        0,
        "short",
        100.0,
        &BTreeSet::from([replacement_ts]),
    )
    .expect("resolve touch before replacement");
    assert_eq!(resolution.entry_idx, Some(2));
    assert_eq!(resolution.replaced_by_ts_ms, None);
}

/// 未触价的新 setup 会在收盘替换旧订单，禁止以后续触价补造成交。
#[test]
fn replacement_is_terminal_before_a_later_touch() {
    let candles = vec![
        candle(0, 99.0, 98.0),
        candle(1, 99.0, 98.0),
        candle(2, 99.0, 98.0),
        candle(3, 101.0, 98.0),
    ];
    let replacement_ts = candles[2].candle.ts;
    let resolution = resolve_relimit(
        &candles,
        2,
        3,
        0,
        "short",
        100.0,
        &BTreeSet::from([replacement_ts]),
    )
    .expect("resolve terminal replacement");
    assert_eq!(resolution.entry_idx, None);
    assert_eq!(resolution.replaced_by_ts_ms, Some(replacement_ts));
    assert_eq!(resolution.terminal_status, "relimit_replaced_by_new_setup");
}

/// 做多必须镜像使用 low 触及来源 setup low。
#[test]
fn long_relimit_uses_low_touch() {
    let candles = vec![candle(0, 102.0, 101.0), candle(1, 102.0, 99.0)];
    let resolution = resolve_relimit(&candles, 1, 1, 0, "long", 100.0, &BTreeSet::new())
        .expect("resolve long touch");
    assert_eq!(resolution.entry_idx, Some(1));
}

/// 来源候选若混入净 R 等后验字段必须在读取前失败关闭。
#[test]
fn source_candidate_schema_rejects_outcome_fields() {
    let report = json!({"candidates": [{"symbol": "BTC-USDT-SWAP", "net_r": 1.0}]});
    let error = validate_no_outcome_candidate_fields(&report).expect_err("reject outcome field");
    assert!(error.to_string().contains("forbidden outcome field net_r"));
}
