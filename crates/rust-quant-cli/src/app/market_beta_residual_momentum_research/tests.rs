use super::*;

/// 从同步对数收益构造只用于因果因子测试的连续 15m K 线。
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

/// 构造具有有限真实波幅的测试 K 线。
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

/// 生成与 BTC Beta 叠加、但尾部含独立趋势的币种收益。
fn symbol_returns(benchmark: &[f64], tail_drift: f64) -> Vec<f64> {
    benchmark
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let noise = if index % 2 == 0 { 0.0007 } else { -0.0007 };
            let drift = if index + RESIDUAL_24H_BARS >= benchmark.len() {
                tail_drift
            } else {
                0.0
            };
            0.00002 + 1.4 * value + noise + drift
        })
        .collect()
}

#[test]
fn args_only_accept_frozen_manifest_path() {
    let args =
        parse_residual_momentum_args(["--manifest".to_owned(), "/tmp/residual.json".to_owned()])
            .unwrap();
    assert_eq!(args.manifest, PathBuf::from("/tmp/residual.json"));
    assert!(parse_residual_momentum_args(["--score".to_owned(), "2".to_owned()]).is_err());
}

#[test]
fn factor_uses_only_completed_prefix_and_removes_btc_beta() {
    let benchmark_returns = (0..BETA_BARS)
        .map(|index| (index as i64 % 9 - 4) as f64 * 0.00025)
        .collect::<Vec<_>>();
    let symbol_returns = symbol_returns(&benchmark_returns, 0.00045);
    let start_ts = -MS_15M;
    let benchmark = candles_from_returns(start_ts, &benchmark_returns);
    let mut symbol = candles_from_returns(start_ts, &symbol_returns);
    let decision_candle_ts = symbol.last().unwrap().ts;
    let before = factor_at(&symbol, &benchmark, decision_candle_ts).unwrap();
    assert!((1.2..1.6).contains(&before.beta));
    assert!(before.residual_24h > 0.0);
    assert!(before.residual_6h > 0.0);
    assert!(before.score > MIN_SCORE);

    let future_open = symbol.last().unwrap().c;
    symbol.push(candle(
        decision_candle_ts + MS_15M,
        future_open,
        future_open * 0.5,
    ));
    let after = factor_at(&symbol, &benchmark, decision_candle_ts).unwrap();
    assert_eq!(before, after);
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
        candles: candles_from_returns(start_ts, &symbol_returns(&benchmark_returns, 0.00055)),
    };
    let weak = SymbolSeries {
        candles: candles_from_returns(start_ts, &symbol_returns(&benchmark_returns, 0.00025)),
    };
    let decision_ts = benchmark.candles.last().unwrap().ts + MS_15M;
    assert_eq!(decision_ts.rem_euclid(MS_8H), 0);
    let schedule = UniverseSchedule {
        version: "test".to_owned(),
        windows: vec![UniverseWindow {
            from_ms: decision_ts - MS_8H,
            to_ms: decision_ts + MS_8H,
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
    let (first_candidates, _) =
        build_candidates(&schedule, &first, EntryRule::MomentumContinuation).unwrap();
    let (second_candidates, _) =
        build_candidates(&schedule, &second, EntryRule::MomentumContinuation).unwrap();
    assert_eq!(first_candidates, second_candidates);
    assert_eq!(first_candidates.len(), 1);
    assert_eq!(first_candidates[0].symbol, "AAA-USDT-SWAP");
}

#[test]
fn same_bar_stop_and_target_collision_is_settled_as_stop() {
    let mut candles = (0..MOMENTUM_MAX_HOLDING_BARS)
        .map(|index| candle(index as i64 * MS_15M, 100.0, 100.0))
        .collect::<Vec<_>>();
    candles[0].h = 104.0;
    candles[0].l = 98.0;
    let plan = TradePlan {
        candidate: Candidate {
            symbol: "AAA-USDT-SWAP".to_owned(),
            decision_ts: 0,
            factor: FactorSnapshot {
                beta: 1.0,
                residual_24h: 0.01,
                residual_6h: 0.01,
                score: 2.0,
                atr: 0.5,
            },
            long: true,
        },
        entry_index: 0,
        entry: 100.0,
        stop: 99.0,
        target: 103.0,
        risk: 1.0,
        long: true,
    };
    let trade = settle_plan(&plan, &candles, EntryRule::MomentumContinuation).unwrap();
    assert_eq!(trade.exit_reason, "stop");
    assert_eq!(trade.gross_r, -1.0);
    assert!(trade.net_r < trade.gross_r);
}

#[test]
fn mean_reversion_requires_six_hour_turn_and_fades_twenty_four_hour_extreme() {
    let benchmark_returns = (0..BETA_BARS)
        .map(|index| (index as i64 % 13 - 6) as f64 * 0.00018)
        .collect::<Vec<_>>();
    let mut returns = symbol_returns(&benchmark_returns, 0.00055);
    for value in &mut returns[BETA_BARS - RESIDUAL_6H_BARS..] {
        *value -= 0.0016;
    }
    let start_ts = -MS_15M;
    let benchmark = candles_from_returns(start_ts, &benchmark_returns);
    let symbol = candles_from_returns(start_ts, &returns);
    let factor = factor_at(&symbol, &benchmark, symbol.last().unwrap().ts).unwrap();
    assert!(factor.residual_24h > 0.0);
    assert!(factor.residual_6h < 0.0);
    assert!(EntryRule::MeanReversion.condition_passed(factor));
    assert!(!EntryRule::MeanReversion.is_long(factor.score));
    assert!(!EntryRule::MomentumContinuation.condition_passed(factor));
}
