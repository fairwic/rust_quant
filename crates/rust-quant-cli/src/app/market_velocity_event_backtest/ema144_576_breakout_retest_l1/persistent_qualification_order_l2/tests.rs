use super::*;

/// 跳空穿越限价只能获得开盘改善，不能使用盘中极值挑选更优成交。
#[test]
fn gap_aware_limit_fill_is_directionally_conservative() {
    assert_eq!(gap_aware_limit_fill(98.0, 100.0, L2Direction::Long), 98.0);
    assert_eq!(gap_aware_limit_fill(102.0, 100.0, L2Direction::Long), 100.0);
    assert_eq!(
        gap_aware_limit_fill(102.0, 100.0, L2Direction::Short),
        102.0
    );
    assert_eq!(gap_aware_limit_fill(98.0, 100.0, L2Direction::Short), 100.0);
}

/// 风险价格严格复用 4% 止损和 0.52R 目标，并保持多空镜像。
#[test]
fn risk_prices_use_frozen_15m_momentum_contract() {
    let (long_stop, long_target) =
        risk_prices(100.0, L2Direction::Long, V6_TARGET_R).expect("long risk");
    let (short_stop, short_target) =
        risk_prices(100.0, L2Direction::Short, V6_TARGET_R).expect("short risk");
    assert!(approx_equal(long_stop, 96.0));
    assert!(approx_equal(long_target, 102.08));
    assert!(approx_equal(short_stop, 104.0));
    assert!(approx_equal(short_target, 97.92));
}

/// V9 只能改变目标倍数，4% 止损必须与 V6 保持一致。
#[test]
fn risk_prices_support_frozen_v9_target2r_contract() {
    let (long_stop, long_target) =
        risk_prices(100.0, L2Direction::Long, V9_TARGET_R).expect("long risk");
    let (short_stop, short_target) =
        risk_prices(100.0, L2Direction::Short, V9_TARGET_R).expect("short risk");
    assert!(approx_equal(long_stop, 96.0));
    assert!(approx_equal(long_target, 108.0));
    assert!(approx_equal(short_stop, 104.0));
    assert!(approx_equal(short_target, 92.0));
}

/// 同一 K 同时穿越止损和目标时必须让上层选择止损路径。
#[test]
fn same_bar_protection_reports_both_hits() {
    assert_eq!(
        exit_hits(103.0, 95.0, 96.0, 102.08, L2Direction::Long),
        (true, true)
    );
    assert_eq!(
        exit_hits(105.0, 97.0, 104.0, 97.92, L2Direction::Short),
        (true, true)
    );
}

/// 压力成本只减少 R，不改变冻结成交、风险或退出价格。
#[test]
fn target_trade_deducts_round_trip_cost_in_r() {
    let entry = EntryPlan {
        candidate_id: "BTC:1:long".to_owned(),
        symbol: "BTC-USDT-SWAP".to_owned(),
        direction: L2Direction::Long,
        signal_ts_ms: 1,
        entry_idx: 0,
        anchor_ema144: 99.4,
        anchor_atr14: 2.0,
        limit_price: 100.0,
        entry_price: 100.0,
        stop_price: 96.0,
        target_price: 102.08,
    };
    let record = build_trade_record(
        entry,
        ExitPath {
            complete: true,
            exit_ts_ms: 2,
            exit_price: 102.08,
            exit_reason: "target_hit",
        },
    );
    assert!(approx_equal(record.gross_r, 0.52));
    assert!(record.cost_r > 0.0);
    assert!(approx_equal(record.net_r, record.gross_r - record.cost_r));
}

/// 任一镜像方向成本后不盈利时，联合 L2 门禁必须停止。
#[test]
fn decision_rejects_a_negative_mirror_direction() {
    let positive = EmaRetestL2Performance {
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
    let negative = EmaRetestL2Performance {
        sum_r: -1.0,
        expectancy_r: -0.05,
        profit_factor: Some(0.9),
        ..positive.clone()
    };
    let coverage = EmaRetestL2Coverage {
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
        returned_symbol_count: 60,
        eligible_symbol_count: 44,
        excluded_symbol_count: 16,
        blockers: BTreeMap::new(),
    };
    let concentration = EmaRetestL2Concentration {
        net_r_after_removing_top_two_trades: 2.0,
        net_r_after_removing_top_event: 2.0,
        max_symbol_positive_r_share_pct: Some(20.0),
        max_event_positive_r_share_pct: Some(20.0),
        net_r_by_symbol: BTreeMap::new(),
        net_r_by_month: BTreeMap::new(),
        net_r_by_direction: BTreeMap::from([("long", 5.0), ("short", -1.0)]),
    };
    let by_direction = BTreeMap::from([("long", positive.clone()), ("short", negative)]);

    let decision = decide_l2(
        &coverage,
        &positive,
        &positive,
        &by_direction,
        &concentration,
        true,
        V6_CANDIDATE_KEY,
    );

    assert_eq!(decision.status, "stop");
    assert_eq!(
        decision.gates.get("both_directions_cost_adjusted_positive"),
        Some(&false)
    );
}
