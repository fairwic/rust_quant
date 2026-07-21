use super::*;

/// 从同步对数收益构造连续的 15m K 线。
fn candles_from_returns(start_ts: i64, returns: &[f64]) -> Vec<CandleItem> {
    let mut candles = Vec::with_capacity(returns.len() + 1);
    let mut close = 100.0_f64;
    candles.push(candle(start_ts, close, close));
    for (index, value) in returns.iter().enumerate() {
        let open = close;
        close *= value.exp();
        candles.push(candle(start_ts + (index as i64 + 1) * MS_15M, open, close));
    }
    candles
}

/// 构造有限且已确认的测试 K 线。
fn candle(ts: i64, open: f64, close: f64) -> CandleItem {
    CandleItem {
        ts,
        o: open,
        h: open.max(close) * 1.001,
        l: open.min(close) * 0.999,
        c: close,
        v: 1.0,
        confirm: 1,
    }
}

/// 生成带 BTC Beta、交替特质噪声和尾部残差转向的币种收益。
fn reverted_symbol_returns(benchmark: &[f64], strength: f64) -> Vec<f64> {
    benchmark
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let noise = if index % 2 == 0 { 0.0002 } else { -0.0002 };
            let mut drift = 0.0;
            if index + RESIDUAL_24H_BARS >= benchmark.len() {
                drift += strength;
            }
            if index + RESIDUAL_6H_BARS >= benchmark.len() {
                drift -= strength * 2.5;
            }
            0.00002 + 1.4 * value + noise + drift
        })
        .collect()
}

/// 构造直接结算测试使用的冻结候选。
fn candidate(decision_ts: i64, long_residual: bool, beta: f64) -> Candidate {
    Candidate {
        symbol: "AAA-USDT-SWAP".to_owned(),
        decision_ts,
        factor: FactorSnapshot {
            beta,
            residual_24h: if long_residual { -0.03 } else { 0.03 },
            residual_6h: if long_residual { 0.01 } else { -0.01 },
            score: if long_residual { -2.0 } else { 2.0 },
            residual_std_15m: 0.00125,
        },
        long_residual,
    }
}

#[test]
fn args_only_accept_frozen_manifest_path() {
    let args =
        parse_beta_hedged_residual_args(["--manifest".to_owned(), "/tmp/pair.json".to_owned()])
            .unwrap();
    assert_eq!(args.manifest, PathBuf::from("/tmp/pair.json"));
    assert!(parse_beta_hedged_residual_args(["--score".to_owned(), "2".to_owned()]).is_err());
}

#[test]
fn factor_uses_only_synchronized_completed_prefix() {
    let benchmark_returns = (0..BETA_BARS)
        .map(|index| (index as i64 % 13 - 6) as f64 * 0.00018)
        .collect::<Vec<_>>();
    let symbol_returns = reverted_symbol_returns(&benchmark_returns, 0.00055);
    let start_ts = -MS_15M;
    let benchmark = candles_from_returns(start_ts, &benchmark_returns);
    let mut symbol = candles_from_returns(start_ts, &symbol_returns);
    let decision_candle_ts = symbol.last().unwrap().ts;
    let before = factor_at(&symbol, &benchmark, decision_candle_ts).unwrap();
    assert!((1.2..1.6).contains(&before.beta));
    assert!(before.residual_24h > 0.0);
    assert!(before.residual_6h < 0.0);
    assert!(before.score.abs() >= MIN_SCORE);

    let future_open = symbol.last().unwrap().c;
    symbol.push(candle(
        decision_candle_ts + MS_15M,
        future_open,
        future_open * 0.5,
    ));
    assert_eq!(
        before,
        factor_at(&symbol, &benchmark, decision_candle_ts).unwrap()
    );

    let mut gapped = symbol[..symbol.len() - 1].to_vec();
    gapped[BETA_BARS / 2].ts += 1;
    assert!(factor_at(&gapped, &benchmark, decision_candle_ts).is_none());
}

#[test]
fn candidate_selection_is_independent_of_symbol_insertion_order() {
    let benchmark_returns = (0..BETA_BARS)
        .map(|index| (index as i64 % 11 - 5) as f64 * 0.0002)
        .collect::<Vec<_>>();
    let start_ts = -MS_15M;
    let benchmark = SymbolSeries {
        candles: candles_from_returns(start_ts, &benchmark_returns),
    };
    let strong = SymbolSeries {
        candles: candles_from_returns(
            start_ts,
            &reverted_symbol_returns(&benchmark_returns, 0.0007),
        ),
    };
    let weak = SymbolSeries {
        candles: candles_from_returns(
            start_ts,
            &reverted_symbol_returns(&benchmark_returns, 0.00045),
        ),
    };
    let decision_ts = benchmark.candles.last().unwrap().ts + MS_15M;
    assert_eq!(decision_ts.rem_euclid(MS_4H), 0);
    let schedule = UniverseSchedule {
        version: "test".to_owned(),
        windows: vec![UniverseWindow {
            from_ms: decision_ts - MS_4H,
            to_ms: decision_ts + MS_4H,
            members: ["AAA-USDT-SWAP".to_owned(), "BBB-USDT-SWAP".to_owned()]
                .into_iter()
                .collect(),
        }],
    };
    let first = BTreeMap::from([
        (BENCHMARK.to_owned(), benchmark.clone()),
        ("AAA-USDT-SWAP".to_owned(), strong.clone()),
        ("BBB-USDT-SWAP".to_owned(), weak.clone()),
    ]);
    let second = BTreeMap::from([
        ("BBB-USDT-SWAP".to_owned(), weak),
        ("AAA-USDT-SWAP".to_owned(), strong),
        (BENCHMARK.to_owned(), benchmark),
    ]);
    let (first_candidates, _) = build_candidates(&schedule, &first).unwrap();
    let (second_candidates, _) = build_candidates(&schedule, &second).unwrap();
    assert_eq!(first_candidates, second_candidates);
    assert_eq!(first_candidates.len(), 1);
    assert_eq!(first_candidates[0].symbol, "AAA-USDT-SWAP");
}

#[test]
fn beta_hedge_removes_the_frozen_common_market_move() {
    let beta = 1.4;
    let btc_entry = 100.0;
    let btc_exit = 102.0;
    let symbol_entry = 50.0;
    let symbol_exit = symbol_entry * (1.0 + beta * (btc_exit / btc_entry - 1.0));
    let residual = pair_return(true, beta, symbol_entry, symbol_exit, btc_entry, btc_exit).unwrap();
    assert!(residual.abs() < 1e-12);
}

#[test]
fn close_trigger_exits_at_next_common_open_and_charges_four_fills() {
    let start = 7 * 60 * 60 * 1_000 + 45 * 60 * 1_000;
    let symbol = vec![
        candle(start, 100.0, 100.0),
        candle(start + MS_15M, 100.0, 98.0),
        candle(start + 2 * MS_15M, 97.5, 97.5),
    ];
    let benchmark = vec![
        candle(start, 100.0, 100.0),
        candle(start + MS_15M, 100.0, 100.0),
        candle(start + 2 * MS_15M, 100.0, 100.0),
    ];
    let plan = PairPlan {
        candidate: candidate(start, true, 1.0),
        symbol_entry_index: 0,
        btc_entry_index: 0,
        symbol_entry: 100.0,
        btc_entry: 100.0,
        risk_return: 0.01,
        long_residual: true,
    };
    let trade = settle_pair(&plan, &symbol, &benchmark).unwrap();
    assert_eq!(trade.exit_reason, "stop");
    assert_eq!(trade.trigger_ts, start + 2 * MS_15M);
    assert_eq!(trade.exit_ts, start + 2 * MS_15M);
    assert!((trade.trigger_spread_r + 2.0).abs() < 1e-12);
    assert!((trade.gross_r + 2.5).abs() < 1e-12);

    // 四次成交成本为 2 * 8bps * (1 + beta)，并跨过一个 UTC 8h 资金时点。
    assert!((trade.cost_r - 0.34).abs() < 1e-12);
    assert!((trade.net_r + 2.84).abs() < 1e-12);
}

#[test]
fn missing_next_common_open_rejects_an_otherwise_triggered_trade() {
    let symbol = vec![candle(0, 100.0, 100.0), candle(MS_15M, 100.0, 98.0)];
    let benchmark = vec![candle(0, 100.0, 100.0), candle(MS_15M, 100.0, 100.0)];
    let plan = PairPlan {
        candidate: candidate(0, true, 1.0),
        symbol_entry_index: 0,
        btc_entry_index: 0,
        symbol_entry: 100.0,
        btc_entry: 100.0,
        risk_return: 0.01,
        long_residual: true,
    };
    assert!(settle_pair(&plan, &symbol, &benchmark).is_none());
}
