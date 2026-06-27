use super::*;
use std::collections::HashMap;
#[test]
fn framework_equity_split_report_splits_by_entry_time() {
    fn winning_candles(entry_ts: i64) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for index in 0..505 {
            candles.push(ohlc(MS_15M * index, 100.0, 101.0, 99.0, 100.0));
        }
        candles.push(ohlc(entry_ts, 100.0, 101.0, 99.0, 100.0));
        candles.push(ohlc(entry_ts + MS_15M, 106.0, 106.5, 105.0, 106.0));
        candles
    }
    let confirmed = vec![
        confirmed_event(1, "EARLY-USDT-SWAP", MS_15M * 505, "2026-06-01T00:00:00Z"),
        confirmed_event(2, "LATE-USDT-SWAP", MS_15M * 605, "2026-06-02T00:00:00Z"),
    ];
    let candles_by_symbol = HashMap::from([
        ("EARLY-USDT-SWAP".to_string(), winning_candles(MS_15M * 505)),
        ("LATE-USDT-SWAP".to_string(), winning_candles(MS_15M * 605)),
    ]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let splits = build_framework_equity_split_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(splits.len(), 2);
    assert_eq!(splits[0].label, "early");
    assert_eq!(splits[0].report.total_open_trades, 1);
    assert_eq!(splits[1].label, "late");
    assert_eq!(splits[1].report.total_open_trades, 1);
    assert!(splits[0].end_entry_ts < splits[1].start_entry_ts);
}
#[test]
fn framework_equity_quartile_report_splits_by_entry_time() {
    fn winning_candles(entry_ts: i64) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for index in 0..505 {
            candles.push(ohlc(MS_15M * index, 100.0, 101.0, 99.0, 100.0));
        }
        candles.push(ohlc(entry_ts, 100.0, 101.0, 99.0, 100.0));
        candles.push(ohlc(entry_ts + MS_15M, 106.0, 106.5, 105.0, 106.0));
        candles
    }
    let confirmed = vec![
        confirmed_event(1, "Q1-USDT-SWAP", MS_15M * 505, "2026-06-01T00:00:00Z"),
        confirmed_event(2, "Q2-USDT-SWAP", MS_15M * 605, "2026-06-02T00:00:00Z"),
        confirmed_event(3, "Q3-USDT-SWAP", MS_15M * 705, "2026-06-03T00:00:00Z"),
        confirmed_event(4, "Q4-USDT-SWAP", MS_15M * 805, "2026-06-04T00:00:00Z"),
    ];
    let candles_by_symbol = HashMap::from([
        ("Q1-USDT-SWAP".to_string(), winning_candles(MS_15M * 505)),
        ("Q2-USDT-SWAP".to_string(), winning_candles(MS_15M * 605)),
        ("Q3-USDT-SWAP".to_string(), winning_candles(MS_15M * 705)),
        ("Q4-USDT-SWAP".to_string(), winning_candles(MS_15M * 805)),
    ]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let quartiles =
        build_framework_equity_quartile_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(quartiles.len(), 4);
    for (index, quartile) in quartiles.iter().enumerate() {
        assert_eq!(quartile.label, ["q1", "q2", "q3", "q4"][index]);
        assert_eq!(quartile.report.total_open_trades, 1);
    }
    assert!(quartiles
        .windows(2)
        .all(|items| items[0].end_entry_ts < items[1].start_entry_ts));
}
#[test]
fn framework_equity_symbol_window_report_shows_top_symbols_by_quartile() {
    fn candles(entry_ts: i64, exit_close: f64) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for index in 0..505 {
            candles.push(ohlc(MS_15M * index, 100.0, 101.0, 99.0, 100.0));
        }
        candles.push(ohlc(entry_ts, 100.0, 101.0, 99.0, 100.0));
        candles.push(ohlc(
            entry_ts + MS_15M,
            exit_close,
            exit_close + 0.5,
            exit_close - 1.0,
            exit_close,
        ));
        candles
    }
    let confirmed = vec![
        confirmed_event(1, "Q1-LOW-USDT-SWAP", MS_15M * 505, "2026-06-01T00:00:00Z"),
        confirmed_event(2, "Q1-TOP-USDT-SWAP", MS_15M * 506, "2026-06-01T01:00:00Z"),
        confirmed_event(3, "Q2-TOP-USDT-SWAP", MS_15M * 605, "2026-06-02T00:00:00Z"),
        confirmed_event(4, "Q2-LOW-USDT-SWAP", MS_15M * 606, "2026-06-02T01:00:00Z"),
        confirmed_event(5, "Q3-TOP-USDT-SWAP", MS_15M * 705, "2026-06-03T00:00:00Z"),
        confirmed_event(6, "Q3-LOW-USDT-SWAP", MS_15M * 706, "2026-06-03T01:00:00Z"),
        confirmed_event(7, "Q4-TOP-USDT-SWAP", MS_15M * 805, "2026-06-04T00:00:00Z"),
        confirmed_event(8, "Q4-LOW-USDT-SWAP", MS_15M * 806, "2026-06-04T01:00:00Z"),
    ];
    let candles_by_symbol = HashMap::from([
        ("Q1-LOW-USDT-SWAP".to_string(), candles(MS_15M * 505, 104.0)),
        ("Q1-TOP-USDT-SWAP".to_string(), candles(MS_15M * 506, 106.0)),
        ("Q2-TOP-USDT-SWAP".to_string(), candles(MS_15M * 605, 106.0)),
        ("Q2-LOW-USDT-SWAP".to_string(), candles(MS_15M * 606, 104.0)),
        ("Q3-TOP-USDT-SWAP".to_string(), candles(MS_15M * 705, 106.0)),
        ("Q3-LOW-USDT-SWAP".to_string(), candles(MS_15M * 706, 104.0)),
        ("Q4-TOP-USDT-SWAP".to_string(), candles(MS_15M * 805, 106.0)),
        ("Q4-LOW-USDT-SWAP".to_string(), candles(MS_15M * 806, 104.0)),
    ]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let windows = super::super::equity::build_framework_equity_symbol_window_reports(
        &confirmed,
        &candles_by_symbol,
        2.0,
        &args,
        1,
    );
    assert_eq!(windows.len(), 4);
    assert_eq!(windows[0].split.label, "q1");
    assert_eq!(windows[0].top_symbols[0].symbol, "Q1-TOP-USDT-SWAP");
    assert_eq!(windows[1].top_symbols[0].symbol, "Q2-TOP-USDT-SWAP");
    assert_eq!(windows[2].top_symbols[0].symbol, "Q3-TOP-USDT-SWAP");
    assert_eq!(windows[3].top_symbols[0].symbol, "Q4-TOP-USDT-SWAP");
}
#[test]
fn framework_equity_trigger_report_groups_by_entry_trigger() {
    fn winning_candles(entry_ts: i64) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for index in 0..505 {
            candles.push(ohlc(MS_15M * index, 100.0, 101.0, 99.0, 100.0));
        }
        candles.push(ohlc(entry_ts, 100.0, 101.0, 99.0, 100.0));
        candles.push(ohlc(entry_ts + MS_15M, 106.0, 106.5, 105.0, 106.0));
        candles
    }
    let mut breakout = confirmed_event(
        1,
        "BREAKOUT-USDT-SWAP",
        MS_15M * 505,
        "2026-06-01T00:00:00Z",
    );
    breakout.trigger = "breakout_previous_high".to_string();
    let mut reclaim = confirmed_event(2, "RECLAIM-USDT-SWAP", MS_15M * 605, "2026-06-02T00:00:00Z");
    reclaim.trigger = "reclaim_ema".to_string();
    let confirmed = vec![breakout, reclaim];
    let candles_by_symbol = HashMap::from([
        (
            "BREAKOUT-USDT-SWAP".to_string(),
            winning_candles(MS_15M * 505),
        ),
        (
            "RECLAIM-USDT-SWAP".to_string(),
            winning_candles(MS_15M * 605),
        ),
    ]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let triggers =
        build_framework_equity_trigger_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(triggers.len(), 2);
    assert_eq!(triggers[0].trigger, "breakout_previous_high");
    assert_eq!(triggers[0].report.total_open_trades, 1);
    assert_eq!(triggers[1].trigger, "reclaim_ema");
    assert_eq!(triggers[1].report.total_open_trades, 1);
}
#[test]
fn framework_equity_feature_report_groups_generic_event_features() {
    fn winning_candles(entry_ts: i64) -> Vec<BacktestCandle> {
        let mut candles = Vec::new();
        for index in 0..505 {
            candles.push(ohlc(MS_15M * index, 100.0, 101.0, 99.0, 100.0));
        }
        candles.push(ohlc(entry_ts, 100.0, 101.0, 99.0, 100.0));
        candles.push(ohlc(entry_ts + MS_15M, 106.0, 106.5, 105.0, 106.0));
        candles
    }
    let mut early = confirmed_event(1, "EARLY-USDT-SWAP", MS_15M * 505, "2026-06-01T00:00:00Z");
    early.event.delta_rank = 12;
    early.event.price_change_pct = 3.0;
    let mut middle = confirmed_event(2, "MID-USDT-SWAP", MS_15M * 605, "2026-06-02T00:00:00Z");
    middle.event.delta_rank = 30;
    middle.event.price_change_pct = 12.0;
    let mut late = confirmed_event(3, "LATE-USDT-SWAP", MS_15M * 705, "2026-06-03T00:00:00Z");
    late.event.delta_rank = 55;
    late.event.price_change_pct = 25.0;
    let confirmed = vec![early, middle, late];
    let candles_by_symbol = HashMap::from([
        ("EARLY-USDT-SWAP".to_string(), winning_candles(MS_15M * 505)),
        ("MID-USDT-SWAP".to_string(), winning_candles(MS_15M * 605)),
        ("LATE-USDT-SWAP".to_string(), winning_candles(MS_15M * 705)),
    ]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let reports = super::super::equity::build_framework_equity_feature_reports(
        &confirmed,
        &candles_by_symbol,
        2.0,
        &args,
    );
    let report_for = |feature: &str, bucket: &str| {
        reports
            .iter()
            .find(|report| report.feature == feature && report.bucket == bucket)
            .unwrap()
    };
    assert_eq!(
        report_for("delta_rank", "12_24").report.total_open_trades,
        1
    );
    assert!(reports.iter().all(|report| report.feature != "new_rank"));
    assert_eq!(
        report_for("price_change_pct", "20_plus")
            .report
            .total_open_trades,
        1
    );
}
#[test]
fn framework_equity_concentration_report_removes_top_positive_symbols() {
    let report = FrameworkEquityReport {
        target_r: 3.3,
        initial_fund_per_symbol: 100.0,
        min_trades: 3,
        total_open_trades: 7,
        total_profit: 40.0,
        win_rate: Some(60.0),
        trade_sharpe: None,
        max_drawdown_pct: 5.0,
        meets_min_trades: true,
        symbols: vec![
            FrameworkEquitySymbolReport {
                symbol: "SMALL-USDT-SWAP".to_string(),
                open_trades: 1,
                final_fund: 105.0,
                profit: 5.0,
                wins: 1,
                losses: 0,
                trade_sharpe: None,
                max_drawdown_pct: 1.0,
            },
            FrameworkEquitySymbolReport {
                symbol: "TOP-USDT-SWAP".to_string(),
                open_trades: 2,
                final_fund: 130.0,
                profit: 30.0,
                wins: 2,
                losses: 0,
                trade_sharpe: None,
                max_drawdown_pct: 2.0,
            },
            FrameworkEquitySymbolReport {
                symbol: "LOSS-USDT-SWAP".to_string(),
                open_trades: 4,
                final_fund: 105.0,
                profit: 5.0,
                wins: 0,
                losses: 4,
                trade_sharpe: None,
                max_drawdown_pct: 5.0,
            },
        ],
    };
    let reports = build_framework_equity_concentration_reports(&report);
    assert_eq!(reports[0].removed_top_positive, 1);
    assert_eq!(reports[0].removed_symbols, vec!["TOP-USDT-SWAP"]);
    assert_eq!(reports[0].removed_profit, 30.0);
    assert_eq!(reports[0].removed_share_pct, Some(75.0));
    assert_eq!(reports[0].remaining_total_profit, 10.0);
    assert_eq!(reports[0].remaining_open_trades, 5);
    assert_eq!(reports[0].remaining_win_rate, Some(20.0));
    assert!(reports[0].remaining_meets_min_trades);
}
#[test]
fn framework_equity_report_applies_profit_protection_stop_updates() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 100.5, 99.5, 100.0));
    candles.push(ohlc(entry_ts + MS_15M, 102.8, 103.2, 100.8, 102.7));
    candles.push(ohlc(entry_ts + MS_15M * 2, 100.8, 101.0, 100.5, 99.0));
    let confirmed = vec![confirmed_event(
        1,
        "PROTECT-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("PROTECT-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        profit_protect_after_r: Some(1.0),
        profit_protect_stop_r: 0.2,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let report = build_framework_equity_report(&confirmed, &candles_by_symbol, 3.0, &args);
    assert_eq!(report.total_open_trades, 1);
    assert_eq!(report.symbols[0].wins, 1);
    assert_eq!(report.symbols[0].losses, 0);
    assert!(report.total_profit > 0.0);
}
#[test]
fn framework_equity_report_skips_profit_protection_when_candle_closes_below_new_stop() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 100.5, 99.5, 100.0));
    candles.push(ohlc(entry_ts + MS_15M, 102.0, 106.2, 100.8, 102.0));
    candles.push(ohlc(entry_ts + MS_15M * 2, 99.0, 100.0, 96.5, 97.0));
    let confirmed = vec![confirmed_event(
        1,
        "PROTECT-CLOSE-BELOW-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("PROTECT-CLOSE-BELOW-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        profit_protect_after_r: Some(2.0),
        profit_protect_stop_r: 1.0,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let report = build_framework_equity_report(&confirmed, &candles_by_symbol, 3.0, &args);
    assert_eq!(report.total_open_trades, 1);
    assert_eq!(report.symbols[0].wins, 0);
    assert_eq!(report.symbols[0].losses, 1);
    assert!(report.total_profit < 0.0);
}
#[test]
fn framework_equity_trade_report_maps_closed_trade_to_rank_event() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 100.5, 99.5, 100.0));
    candles.push(ohlc(entry_ts + MS_15M, 106.0, 106.5, 105.0, 106.0));
    let mut event = confirmed_event(
        42,
        "TRADE-REPORT-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    );
    event.event.new_rank = 4;
    event.event.delta_rank = 21;
    event.event.price_change_pct = 54.09;
    event.trigger = "reclaim_ema".to_string();
    let confirmed = vec![event];
    let candles_by_symbol = HashMap::from([("TRADE-REPORT-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let reports = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].event_id, 42);
    assert_eq!(reports[0].symbol, "TRADE-REPORT-USDT-SWAP");
    assert_eq!(reports[0].trigger, "reclaim_ema");
    assert_eq!(reports[0].new_rank, 4);
    assert_eq!(reports[0].delta_rank, 21);
    assert_eq!(reports[0].price_change_pct, 54.09);
    assert_eq!(reports[0].outcome, "win");
    assert_eq!(reports[0].open_price, 100.0);
    assert!(reports[0].signal_open_position_time.ends_with("+08:00"));
    assert!(reports[0].open_position_time.ends_with("+08:00"));
    assert!(reports[0]
        .close_position_time
        .as_ref()
        .is_some_and(|value| value.ends_with("+08:00")));
    assert!(reports[0].close_price.unwrap() > reports[0].open_price);
    assert!(reports[0].quantity > 0.0);
    assert!(reports[0].profit_loss > 0.0);
}
#[test]
fn framework_equity_trade_report_keeps_retest_trade_present_in_realistic_sequence() {
    let entry_ts = 1_781_393_400_000;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 0.19, 0.191, 0.189, 0.19));
    }
    candles.extend([
        ohlc(entry_ts, 0.1908, 0.1923, 0.1905, 0.1910),
        ohlc(entry_ts + MS_15M, 0.1909, 0.1921, 0.1898, 0.1916),
        ohlc(entry_ts + MS_15M * 2, 0.1916, 0.1920, 0.1909, 0.1920),
        ohlc(entry_ts + MS_15M * 3, 0.1921, 0.1929, 0.1899, 0.1905),
        ohlc(entry_ts + MS_15M * 4, 0.1903, 0.1926, 0.1901, 0.1926),
        ohlc(entry_ts + MS_15M * 5, 0.1926, 0.1950, 0.1918, 0.1947),
        ohlc(entry_ts + MS_15M * 6, 0.1946, 0.1966, 0.1944, 0.1952),
        ohlc(entry_ts + MS_15M * 7, 0.1951, 0.1971, 0.1951, 0.1970),
        ohlc(entry_ts + MS_15M * 8, 0.1969, 0.1999, 0.1962, 0.1996),
        ohlc(entry_ts + MS_15M * 9, 0.1995, 0.2017, 0.1990, 0.2000),
        ohlc(entry_ts + MS_15M * 10, 0.2003, 0.2041, 0.1994, 0.2037),
        ohlc(entry_ts + MS_15M * 11, 0.2037, 0.2050, 0.2027, 0.2046),
    ]);
    let mut event = confirmed_event(
        77,
        "REAL-SEQUENCE-USDT-SWAP",
        entry_ts,
        "2026-06-13T23:23:03Z",
    );
    event.entry_price = 0.1908;
    event.trigger = "reclaim_ema+retest_after_signal+fvg_fallback".to_string();
    let confirmed = vec![event];
    let candles_by_symbol = HashMap::from([("REAL-SEQUENCE-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.04,
        ignore_entry_signal_updates_while_open: true,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let report = build_framework_equity_report(&confirmed, &candles_by_symbol, 1.8, &args);
    let trades = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 1.8, &args);
    assert_eq!(report.total_open_trades, 1);
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].event_id, 77);
    assert_eq!(trades[0].outcome, "win");
    assert!(trades[0].profit_loss > 0.0);
}
#[test]
fn framework_equity_trade_report_keeps_same_candle_full_close_trade() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 108.0, 99.5, 107.0));
    let confirmed = vec![confirmed_event(
        78,
        "SAME-CANDLE-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("SAME-CANDLE-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let reports = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].event_id, 78);
    assert_eq!(reports[0].outcome, "win");
}
#[test]
fn framework_equity_trade_report_keeps_trade_when_symbol_history_is_shorter_than_500_candles() {
    let entry_ts = MS_15M * 5;
    let mut candles = Vec::new();
    for index in 0..5 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.extend([
        ohlc(entry_ts, 100.0, 101.0, 99.5, 100.0),
        ohlc(entry_ts + MS_15M, 101.0, 102.0, 100.8, 101.8),
        ohlc(entry_ts + MS_15M * 2, 101.8, 104.2, 101.7, 104.0),
        ohlc(entry_ts + MS_15M * 3, 104.0, 106.5, 103.8, 106.0),
    ]);
    let confirmed = vec![confirmed_event(
        79,
        "SHORT-HISTORY-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("SHORT-HISTORY-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let report = build_framework_equity_report(&confirmed, &candles_by_symbol, 2.0, &args);
    let trades = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(report.total_open_trades, 1);
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].event_id, 79);
    assert_eq!(trades[0].outcome, "win");
}
#[test]
fn framework_equity_trade_report_applies_early_no_profit_exit() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 101.0, 99.5, 100.4));
    candles.push(ohlc(entry_ts + MS_15M, 100.4, 101.0, 99.0, 99.8));
    candles.push(ohlc(entry_ts + MS_15M * 2, 99.8, 108.0, 99.0, 107.0));
    let confirmed = vec![confirmed_event(
        43,
        "EARLY-EXIT-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("EARLY-EXIT-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        early_exit_no_profit_candles: Some(1),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let reports = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].event_id, 43);
    assert_eq!(reports[0].close_type, "early_exit_no_profit");
    assert_eq!(reports[0].outcome, "loss");
    assert!(reports[0].profit_loss < 0.0);
    assert!(reports[0].profit_loss > -1.0);
}
#[test]
fn framework_equity_trade_report_keeps_risk_close_type_before_early_exit_signal() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 101.0, 99.5, 100.4));
    candles.push(ohlc(entry_ts + MS_15M, 100.4, 101.0, 96.0, 99.8));
    let confirmed = vec![confirmed_event(
        44,
        "EARLY-EXIT-STOP-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("EARLY-EXIT-STOP-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        early_exit_no_profit_candles: Some(1),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let reports = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].event_id, 44);
    assert_eq!(reports[0].close_type, "Signal_Kline_Stop_Loss");
    assert_eq!(reports[0].outcome, "loss");
}
#[test]
fn framework_equity_trade_report_can_ignore_same_symbol_entry_updates_while_open() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 100.5, 99.5, 100.0));
    candles.push(ohlc(entry_ts + MS_15M, 104.0, 105.0, 103.0, 104.0));
    candles.push(ohlc(entry_ts + MS_15M * 2, 100.5, 101.0, 96.0, 97.0));
    let first = confirmed_event(
        50,
        "STRICT-UPDATE-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    );
    let mut second = confirmed_event(
        51,
        "STRICT-UPDATE-USDT-SWAP",
        entry_ts + MS_15M,
        "2026-06-01T00:15:00Z",
    );
    second.entry_price = 104.0;
    let confirmed = vec![first, second];
    let candles_by_symbol = HashMap::from([("STRICT-UPDATE-USDT-SWAP".to_string(), candles)]);
    let default_args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let updated_reports =
        build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &default_args);
    assert_eq!(updated_reports.len(), 1);
    assert_eq!(updated_reports[0].event_id, 50);
    assert!(updated_reports[0].profit_loss > 0.0);

    let strict_args = MarketVelocityEventBacktestArgs {
        ignore_entry_signal_updates_while_open: true,
        ..default_args
    };
    let strict_reports =
        build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &strict_args);
    assert_eq!(strict_reports.len(), 1);
    assert_eq!(strict_reports[0].event_id, 50);
    assert_eq!(strict_reports[0].close_type, "Signal_Kline_Stop_Loss");
    assert!(strict_reports[0].profit_loss < 0.0);
}
#[test]
fn framework_equity_trade_report_expands_runner_close_legs() {
    let entry_ts = MS_15M * 505;
    let mut candles = Vec::new();
    for index in 0..505 {
        candles.push(ohlc(MS_15M * index, 100.0, 100.5, 99.5, 100.0));
    }
    candles.push(ohlc(entry_ts, 100.0, 100.5, 99.5, 100.0));
    candles.push(ohlc(entry_ts + MS_15M, 106.0, 106.5, 105.0, 106.0));
    candles.push(ohlc(entry_ts + MS_15M * 2, 112.0, 112.5, 111.0, 112.0));
    let confirmed = vec![confirmed_event(
        45,
        "RUNNER-USDT-SWAP",
        entry_ts,
        "2026-06-01T00:00:00Z",
    )];
    let candles_by_symbol = HashMap::from([("RUNNER-USDT-SWAP".to_string(), candles)]);
    let args = MarketVelocityEventBacktestArgs {
        min_trades: 1,
        stop_loss_pct: 0.03,
        runner_target_r: Some(4.0),
        runner_fraction: 0.3,
        runner_stop_r: 0.0,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let reports = build_framework_equity_trade_reports(&confirmed, &candles_by_symbol, 2.0, &args);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].event_id, 45);
    assert_eq!(reports[0].close_legs.len(), 2);
    assert_eq!(
        reports[0].close_legs[0].close_type,
        "runner_base_target_hit"
    );
    assert!(!reports[0].close_legs[0].full_close);
    assert_eq!(reports[0].close_legs[1].close_type, "runner_target_hit");
    assert!(reports[0].close_legs[1].full_close);
    assert!(reports[0].profit_loss > 0.0);
    assert_eq!(
        reports[0].profit_loss,
        reports[0]
            .close_legs
            .iter()
            .map(|leg| leg.profit_loss)
            .sum::<f64>()
    );
}
#[test]
fn framework_equity_trade_report_builds_legacy_backtest_detail_payload() {
    let report = FrameworkEquityTradeReport {
        target_r: 2.4,
        symbol: "DETAIL-USDT-SWAP".to_string(),
        event_id: 77,
        detected_at: "2026-06-01T02:25:36Z".to_string(),
        entry_ts: MS_15M * 505,
        signal_open_position_time: "2026-06-01 10:25:36".to_string(),
        open_position_time: "1970-01-04 07:15:00".to_string(),
        close_position_time: Some("1970-01-04 07:30:00".to_string()),
        open_price: 100.0,
        close_price: Some(107.2),
        close_type: "LongTakeProfit".to_string(),
        signal_status: 1,
        profit_loss: 6.43,
        quantity: 1.0,
        outcome: "win",
        trigger: "reclaim_ema".to_string(),
        new_rank: 8,
        delta_rank: 19,
        price_change_pct: 43.2,
        close_legs: Vec::new(),
    };
    let args = MarketVelocityEventBacktestArgs {
        stop_loss_pct: 0.03,
        entry_period: 20,
        paper_outcome_entry_rule_version:
            "rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1".to_string(),
        ..MarketVelocityEventBacktestArgs::default()
    };
    let details = build_market_velocity_backtest_details(&report, 123, &args).unwrap();
    let open = &details[0];
    let close = &details[1];
    let signal_value = serde_json::from_str::<serde_json::Value>(&close.signal_value).unwrap();
    assert_eq!(details.len(), 2);
    assert_eq!(open.back_test_id, 123);
    assert_eq!(open.inst_id, "DETAIL-USDT-SWAP");
    assert_eq!(open.strategy_type, "market_velocity_episode");
    assert_eq!(open.timeframe, "15m");
    assert_eq!(open.option_type, "long");
    assert_eq!(
        open.signal_open_position_time.as_deref(),
        Some("2026-06-01 10:25:36")
    );
    assert_eq!(open.open_position_time, "1970-01-04 07:15:00");
    assert_eq!(open.close_position_time, "1970-01-04 07:15:00");
    assert_eq!(open.open_price, "100");
    assert_eq!(open.close_price, None);
    assert_eq!(open.profit_loss, "0");
    assert_eq!(open.quantity, "1");
    assert_eq!(open.full_close, "false");
    assert_eq!(open.close_type, "");
    assert_eq!(close.option_type, "close");
    assert_eq!(
        close.signal_open_position_time.as_deref(),
        Some("2026-06-01 10:25:36")
    );
    assert_eq!(close.open_position_time, "1970-01-04 07:15:00");
    assert_eq!(close.close_position_time, "1970-01-04 07:30:00");
    assert_eq!(close.close_price.as_deref(), Some("107.2"));
    assert_eq!(close.profit_loss, "6.43");
    assert_eq!(close.full_close, "true");
    assert_eq!(close.close_type, "LongTakeProfit");
    assert_eq!(close.signal_status, 1);
    assert_eq!(close.win_nums, 1);
    assert_eq!(close.loss_nums, 0);
    assert_eq!(close.signal_result, "market_velocity_framework_replay");
    assert_eq!(signal_value["rank_event_id"], 77);
    assert_eq!(signal_value["entry_trigger"], "reclaim_ema");
    assert_eq!(signal_value["target_r"], 2.4);
    assert_eq!(
        signal_value["entry_rule_version"],
        args.paper_outcome_entry_rule_version
    );
}
#[test]
fn framework_equity_trade_report_builds_runner_legacy_backtest_detail_payload() {
    let report = FrameworkEquityTradeReport {
        target_r: 2.4,
        symbol: "RUNNER-DETAIL-USDT-SWAP".to_string(),
        event_id: 88,
        detected_at: "2026-06-01T02:25:36Z".to_string(),
        entry_ts: MS_15M * 505,
        signal_open_position_time: "2026-06-01 10:25:36".to_string(),
        open_position_time: "1970-01-04 07:15:00".to_string(),
        close_position_time: Some("1970-01-04 07:45:00".to_string()),
        open_price: 100.0,
        close_price: Some(112.0),
        close_type: "runner_base_target_hit+runner_target_hit".to_string(),
        signal_status: 1,
        profit_loss: 4.0,
        quantity: 1.0,
        outcome: "win",
        trigger: "breakout_previous_high".to_string(),
        new_rank: 6,
        delta_rank: 50,
        price_change_pct: 21.0,
        close_legs: vec![
            FrameworkEquityCloseLegReport {
                close_ts: MS_15M * 506,
                close_position_time: "1970-01-04 07:30:00".to_string(),
                close_price: 107.2,
                close_type: "runner_base_target_hit".to_string(),
                profit_loss: 3.0,
                quantity: 0.7,
                full_close: false,
                exit_reason: "runner_base_target_hit".to_string(),
                result_r: 2.4,
            },
            FrameworkEquityCloseLegReport {
                close_ts: MS_15M * 507,
                close_position_time: "1970-01-04 07:45:00".to_string(),
                close_price: 112.0,
                close_type: "runner_target_hit".to_string(),
                profit_loss: 1.0,
                quantity: 0.3,
                full_close: true,
                exit_reason: "runner_target_hit".to_string(),
                result_r: 4.0,
            },
        ],
    };
    let args = MarketVelocityEventBacktestArgs {
        stop_loss_pct: 0.03,
        runner_target_r: Some(4.0),
        runner_fraction: 0.3,
        runner_stop_r: 0.0,
        ..MarketVelocityEventBacktestArgs::default()
    };
    let details = build_market_velocity_backtest_details(&report, 456, &args).unwrap();
    assert_eq!(details.len(), 3);
    assert_eq!(details[0].option_type, "long");
    assert_eq!(details[1].option_type, "close");
    assert_eq!(details[1].full_close, "false");
    assert_eq!(details[1].quantity, "0.7");
    assert_eq!(details[1].close_type, "runner_base_target_hit");
    assert_eq!(details[2].option_type, "close");
    assert_eq!(details[2].full_close, "true");
    assert_eq!(details[2].quantity, "0.3");
    assert_eq!(details[2].close_type, "runner_target_hit");
    let signal_value = serde_json::from_str::<serde_json::Value>(&details[2].signal_value).unwrap();
    assert_eq!(signal_value["rank_event_id"], 88);
    assert_eq!(signal_value["exit_reason"], "runner_target_hit");
    assert_eq!(signal_value["runner_target_r"], 4.0);
    assert_eq!(signal_value["runner_fraction"], 0.3);
}
fn confirmed_event(id: i64, symbol: &str, entry_ts: i64, detected_at: &str) -> ConfirmedEvent {
    ConfirmedEvent {
        event: RadarEvent {
            id,
            exchange: "okx".to_string(),
            symbol: symbol.to_string(),
            ts: entry_ts - MS_15M,
            detected_at: detected_at.to_string(),
            new_rank: 10,
            delta_rank: 20,
            current_price: 100.0,
            price_change_pct: 3.0,
        },
        entry_ts,
        entry_price: 100.0,
        entry_idx: 505,
        trigger: "breakout_previous_high".to_string(),
    }
}
