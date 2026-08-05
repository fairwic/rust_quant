use super::*;
use crate::app::market_velocity_event_backtest::BacktestCandle;
use std::collections::HashMap;

/// 构造只保留 OHLC 的最小派生 K，供 L2 成交与退出合同测试。
fn candle(ts: i64, open: f64, high: f64, low: f64, close: f64) -> ComputedCandle {
    ComputedCandle {
        candle: BacktestCandle {
            ts,
            open,
            high,
            low,
            close,
            volume: 1.0,
        },
        volume_ccy: None,
        sma: None,
        ema: None,
        ema12: None,
        ema144: None,
        ema169: None,
        ema696: None,
        previous_volume_avg: None,
        previous_range_avg: None,
        rsi14: None,
        atr14: None,
        bollinger_middle: None,
        bollinger_upper: None,
        bollinger_lower: None,
        bollinger_bandwidth_pct: None,
        macd_line: None,
        macd_signal_line: None,
        macd_histogram: None,
    }
}

/// 构造仅含 BTC 15m 序列的最小数据集。
fn data_with_btc(candles: Vec<ComputedCandle>) -> BacktestDataSet {
    BacktestDataSet {
        historical_universe_version: None,
        pairs: Vec::new(),
        candles_15m: HashMap::new(),
        candles_1h: HashMap::new(),
        candles_4h: HashMap::new(),
        candles_15m_computed: HashMap::from([("BTC-USDT-SWAP".to_owned(), candles)]),
        candles_4h_computed: HashMap::new(),
        events: Vec::new(),
    }
}

/// 构造已经在信号收盘可见的 V10 多头候选。
fn candidate(signal_ts_ms: i64) -> V2Candidate {
    V2Candidate {
        symbol: "BTC-USDT-SWAP".to_owned(),
        direction: "long",
        setup_ts_ms: signal_ts_ms - 4 * MS_15M,
        breakout_ts_ms: signal_ts_ms - 3 * MS_15M,
        rearmed_ts_ms: signal_ts_ms - MS_15M,
        signal_ts_ms,
        signal_month_utc: "2026-01".to_owned(),
        prior_relation_age_bars: 144,
        prior_price_side_bars: 140,
        prior_price_side_ratio: 140.0 / 144.0,
        bars_since_breakout: 3,
        bars_since_rearm: 1,
        cross_phase: "post_cross_retest",
        ema144: 100.0,
        ema576: 99.0,
        atr14: 2.0,
        retest_extreme_to_ema144_atr: 0.1,
        close_to_ema144_directional_atr: 0.2,
        execution_status: "signal_confirmed_next_bar_open_not_evaluated_l1",
    }
}

/// 构造冻结 100 元入场、4% 止损与 0.52R 目标的测试计划。
fn entry_plan(entry_ts_ms: i64) -> EntryPlan {
    EntryPlan {
        candidate_id: "BTC-USDT-SWAP:0:long".to_owned(),
        symbol: "BTC-USDT-SWAP".to_owned(),
        direction: L2Direction::Long,
        setup_ts_ms: -4 * MS_15M,
        breakout_ts_ms: -3 * MS_15M,
        rearmed_ts_ms: -MS_15M,
        signal_ts_ms: 0,
        cross_phase: "post_cross_retest",
        signal_ema144: 100.0,
        signal_ema576: 99.0,
        signal_atr14: 2.0,
        retest_extreme_to_ema144_atr: 0.1,
        close_to_ema144_directional_atr: 0.2,
        entry_idx: 1,
        entry_ts_ms,
        entry_price: 100.0,
        initial_risk: 4.0,
        stop_price: 96.0,
        target_price: 102.08,
    }
}

/// 回踩确认 K 的收盘不能成交，实际价格必须来自下一根连续 K 的开盘。
#[test]
fn resolve_entry_uses_only_next_contiguous_candle_open() {
    let data = data_with_btc(vec![
        candle(0, 99.0, 101.0, 98.0, 100.0),
        candle(MS_15M, 101.0, 102.0, 100.0, 101.5),
    ]);
    let entry = resolve_entry(
        &data,
        candidate(0),
        InitialRiskPolicy::FixedFourPercent,
        TargetRiskPolicy::FixedGrossR,
        EntryRiskGatePolicy::AllowAnyPositiveRisk,
    )
    .expect("next open entry");

    assert_eq!(entry.entry_ts_ms, MS_15M);
    assert!(approx_equal(entry.entry_price, 101.0));
    assert!(approx_equal(entry.stop_price, 96.96));
}

/// 下一根 K 缺失时不得向后寻找另一根开盘补造成交。
#[test]
fn resolve_entry_rejects_non_contiguous_next_candle() {
    let data = data_with_btc(vec![
        candle(0, 99.0, 101.0, 98.0, 100.0),
        candle(2 * MS_15M, 101.0, 102.0, 100.0, 101.5),
    ]);

    assert_eq!(
        resolve_entry(
            &data,
            candidate(0),
            InitialRiskPolicy::FixedFourPercent,
            TargetRiskPolicy::FixedGrossR,
            EntryRiskGatePolicy::AllowAnyPositiveRisk,
        )
        .expect_err("gap must block"),
        "next_entry_candle_not_contiguous"
    );
}

/// 4% 止损与 0.52R 目标必须保持多空镜像。
#[test]
fn risk_prices_keep_frozen_mirror_contract() {
    let (long_stop, long_target) = risk_prices(100.0, L2Direction::Long).expect("long risk");
    let (short_stop, short_target) = risk_prices(100.0, L2Direction::Short).expect("short risk");

    assert!(approx_equal(long_stop, 96.0));
    assert!(approx_equal(long_target, 102.08));
    assert!(approx_equal(short_stop, 104.0));
    assert!(approx_equal(short_target, 97.92));
}

/// EMA144 结构止损必须使用信号时指标，并以实际入场到止损的距离定义 0.52R。
#[test]
fn structural_risk_prices_use_signal_time_ema_and_atr() {
    let (long_stop, long_target) = risk_prices_for_candidate(
        101.0,
        L2Direction::Long,
        100.0,
        2.0,
        InitialRiskPolicy::SignalEma144AtrBuffer(0.30),
        TargetRiskPolicy::FixedGrossR,
    )
    .expect("long structural risk");
    let (short_stop, short_target) = risk_prices_for_candidate(
        99.0,
        L2Direction::Short,
        100.0,
        2.0,
        InitialRiskPolicy::SignalEma144AtrBuffer(0.30),
        TargetRiskPolicy::FixedGrossR,
    )
    .expect("short structural risk");

    assert!(approx_equal(long_stop, 99.4));
    assert!(approx_equal(long_target, 101.832));
    assert!(approx_equal(short_stop, 100.6));
    assert!(approx_equal(short_target, 98.168));
}

/// V26 的 1ATR 缓冲必须在 EMA144 外侧保持多空镜像，并降低同一入场的成本 R。
#[test]
fn one_atr_structural_buffer_is_mirrored_and_widens_initial_risk() {
    let (old_long_stop, _) = risk_prices_for_candidate(
        101.0,
        L2Direction::Long,
        100.0,
        2.0,
        InitialRiskPolicy::SignalEma144AtrBuffer(0.30),
        TargetRiskPolicy::NetAfterCostsR(2.0),
    )
    .expect("old long structural risk");
    let (long_stop, _) = risk_prices_for_candidate(
        101.0,
        L2Direction::Long,
        100.0,
        2.0,
        InitialRiskPolicy::SignalEma144AtrBuffer(1.00),
        TargetRiskPolicy::NetAfterCostsR(2.0),
    )
    .expect("V26 long structural risk");
    let (short_stop, _) = risk_prices_for_candidate(
        99.0,
        L2Direction::Short,
        100.0,
        2.0,
        InitialRiskPolicy::SignalEma144AtrBuffer(1.00),
        TargetRiskPolicy::NetAfterCostsR(2.0),
    )
    .expect("V26 short structural risk");
    let old_risk = 101.0 - old_long_stop;
    let new_risk = 101.0 - long_stop;

    assert!(approx_equal(long_stop, 98.0));
    assert!(approx_equal(short_stop, 102.0));
    assert!(approx_equal(new_risk, 3.0));
    assert!(
        stop_cost_r_for_prices(101.0, long_stop, new_risk).unwrap()
            < stop_cost_r_for_prices(101.0, old_long_stop, old_risk).unwrap()
    );
}

/// 0.50R 成本上限必须在多空镜像风险下包含边界，并拒绝任何更高值。
#[test]
fn entry_risk_gate_is_inclusive_and_mirrored() {
    let long_risk = 2.0 * 100.0 * PER_SIDE_COST_RATE / (0.50 + PER_SIDE_COST_RATE);
    let long_stop = 100.0 - long_risk;
    let short_risk = 2.0 * 100.0 * PER_SIDE_COST_RATE / (0.50 - PER_SIDE_COST_RATE);
    let short_stop = 100.0 + short_risk;

    assert!(validate_entry_risk_gate(
        100.0,
        long_stop,
        long_risk,
        EntryRiskGatePolicy::MaxStopCostR(0.50),
    )
    .is_ok());
    assert!(validate_entry_risk_gate(
        100.0,
        short_stop,
        short_risk,
        EntryRiskGatePolicy::MaxStopCostR(0.50),
    )
    .is_ok());
    assert_eq!(
        validate_entry_risk_gate(
            100.0,
            long_stop + 0.01,
            long_risk - 0.01,
            EntryRiskGatePolicy::MaxStopCostR(0.50),
        ),
        Err("stop_cost_r_above_max")
    );
}

/// 旧版本的显式无门禁政策必须保留原有正风险机会，不改变 V10～V15 行为。
#[test]
fn allow_any_positive_risk_preserves_old_entry_behavior() {
    assert!(validate_entry_risk_gate(
        100.0,
        99.99,
        0.01,
        EntryRiskGatePolicy::AllowAnyPositiveRisk,
    )
    .is_ok());
}

/// 被成本门禁拒绝的候选不进入成交列表，因此同 setup 后续合格候选仍可成为首笔成交。
#[test]
fn rejected_cost_gate_candidate_does_not_consume_setup() {
    let data = data_with_btc(vec![
        candle(0, 100.0, 100.2, 99.8, 100.0),
        candle(MS_15M, 100.0, 100.2, 99.8, 100.0),
        candle(2 * MS_15M, 100.0, 100.2, 99.8, 100.0),
    ]);
    let mut rejected = candidate(0);
    rejected.ema144 = 100.3;
    let mut later = candidate(MS_15M);
    later.setup_ts_ms = rejected.setup_ts_ms;
    later.ema144 = 99.0;

    assert_eq!(
        resolve_entry(
            &data,
            rejected,
            InitialRiskPolicy::SignalEma144AtrBuffer(0.30),
            TargetRiskPolicy::NetAfterCostsR(2.0),
            EntryRiskGatePolicy::MaxStopCostR(0.50),
        )
        .expect_err("narrow structural risk must be rejected before fill"),
        "stop_cost_r_above_max"
    );
    let later_entry = resolve_entry(
        &data,
        later,
        InitialRiskPolicy::SignalEma144AtrBuffer(0.30),
        TargetRiskPolicy::NetAfterCostsR(2.0),
        EntryRiskGatePolicy::MaxStopCostR(0.50),
    )
    .expect("later qualifying opportunity remains eligible");
    assert_eq!(later_entry.setup_ts_ms, -4 * MS_15M);
    assert_eq!(later_entry.entry_ts_ms, 2 * MS_15M);
}

/// 净 2R 目标必须把双边 8bps 成本反解进目标价，并保持多空镜像结算。
#[test]
fn net_target_prices_settle_to_requested_r_after_costs() {
    for (entry, direction, ema144) in [
        (101.0, L2Direction::Long, 100.0),
        (99.0, L2Direction::Short, 100.0),
    ] {
        let (stop, target) = risk_prices_for_candidate(
            entry,
            direction,
            ema144,
            2.0,
            InitialRiskPolicy::SignalEma144AtrBuffer(0.30),
            TargetRiskPolicy::NetAfterCostsR(2.0),
        )
        .expect("cost-adjusted net target");
        let risk = match direction {
            L2Direction::Long => entry - stop,
            L2Direction::Short => stop - entry,
        };
        let gross_r = match direction {
            L2Direction::Long => (target - entry) / risk,
            L2Direction::Short => (entry - target) / risk,
        };
        let cost_r = (entry + target) * PER_SIDE_COST_RATE / risk;

        assert!(approx_equal(gross_r - cost_r, 2.0));
    }
}

/// 同一 setup 首笔真实成交后，即使已经平仓，后续信号也不得再次开仓。
#[test]
fn first_filled_setup_policy_blocks_later_reentry() {
    let data = data_with_btc(vec![
        candle(0, 100.0, 100.0, 100.0, 100.0),
        candle(MS_15M, 100.0, 103.0, 99.0, 102.0),
        candle(2 * MS_15M, 100.0, 100.0, 100.0, 100.0),
        candle(3 * MS_15M, 100.0, 103.0, 99.0, 102.0),
    ]);
    let first = entry_plan(MS_15M);
    let mut second = entry_plan(3 * MS_15M);
    second.candidate_id = "BTC-USDT-SWAP:1800000:long".to_owned();
    second.signal_ts_ms = 2 * MS_15M;
    second.entry_idx = 3;
    let mut blockers = BTreeMap::new();

    let trades = simulate_with_symbol_lock(
        &data,
        vec![first, second],
        &mut blockers,
        SetupEntryPolicy::FirstFilledPerSetup,
    );

    assert_eq!(trades.len(), 1);
    assert_eq!(blockers.get("setup_already_filled_once"), Some(&1));
}

/// 入场 K 同时穿过止损和目标时必须走保守止损路径。
#[test]
fn same_entry_bar_conflict_uses_stop_first() {
    let entry = entry_plan(MS_15M);
    let candles = vec![
        candle(0, 100.0, 100.0, 100.0, 100.0),
        candle(MS_15M, 100.0, 103.0, 95.0, 101.0),
    ];
    let path = simulate_exit(&candles, &entry).expect("exit path");

    assert_eq!(path.exit_reason, "both_hit_stop_first");
    assert!(approx_equal(path.exit_price, 96.0));
}

/// 压力成本只从冻结退出的毛 R 中扣除，不能改变成交或目标价格。
#[test]
fn target_trade_deducts_round_trip_cost() {
    let record = build_trade_record(
        entry_plan(MS_15M),
        ExitPath {
            complete: true,
            exit_ts_ms: 2 * MS_15M,
            exit_price: 102.08,
            exit_reason: "target_hit",
        },
    );

    assert!(approx_equal(record.gross_r, TARGET_R));
    assert!(record.cost_r > 0.0);
    assert!(approx_equal(record.net_r, record.gross_r - record.cost_r));
}

/// 任一镜像方向成本后不盈利时，联合 L2 门禁必须停止。
#[test]
fn decision_rejects_negative_mirror_direction() {
    let positive = V10L2Performance {
        trades: 20,
        positive_r: 10.0,
        negative_r_abs: 5.0,
        sum_r: 5.0,
        expectancy_r: 0.25,
        profit_factor: Some(2.0),
        win_rate_pct: 70.0,
        trade_sharpe: Some(2.0),
        max_drawdown_r: 2.0,
    };
    let negative = V10L2Performance {
        sum_r: -1.0,
        expectancy_r: -0.05,
        profit_factor: Some(0.9),
        ..positive.clone()
    };
    let coverage = V10L2Coverage {
        l1_candidates: EXPECTED_L1_CANDIDATES,
        resolved_candidates: EXPECTED_L1_CANDIDATES,
        executed_trades: 40,
        completed_trades: 40,
        incomplete_trades: 0,
        completed_by_direction: BTreeMap::from([("long", 20), ("short", 20)]),
        completed_symbol_count: 10,
        completed_month_count: 8,
        completed_effective_market_events: 20,
        completed_trades_per_month: 5.0,
        returned_symbol_count: EXPECTED_RETURNED_SYMBOLS,
        eligible_symbol_count: EXPECTED_ELIGIBLE_SYMBOLS,
        excluded_symbol_count: EXPECTED_EXCLUDED_SYMBOLS,
        blockers: BTreeMap::new(),
        exit_reasons: BTreeMap::new(),
    };
    let concentration = V10L2Concentration {
        net_r_after_removing_top_two_trades: 2.0,
        net_r_after_removing_top_event: 2.0,
        max_symbol_positive_r_share_pct: Some(20.0),
        max_event_positive_r_share_pct: Some(20.0),
        net_r_by_symbol: BTreeMap::new(),
        net_r_by_month: BTreeMap::new(),
        net_r_by_direction: BTreeMap::from([("long", 5.0), ("short", -1.0)]),
        net_r_by_asset_group: BTreeMap::new(),
    };
    let by_direction = BTreeMap::from([("long", positive.clone()), ("short", negative)]);

    let decision = decide_l2(
        &coverage,
        &positive,
        &positive,
        &by_direction,
        &concentration,
        true,
        true,
    );

    assert_eq!(decision.status, "stop");
    assert_eq!(
        decision.gates.get("both_directions_cost_adjusted_positive"),
        Some(&false)
    );
}
