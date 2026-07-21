use super::*;

fn candle(index: usize, open: f64, high: f64, low: f64, close: f64) -> CandleItem {
    CandleItem {
        ts: index as i64 * MS_15M,
        o: open,
        h: high,
        l: low,
        c: close,
        v: 100.0,
        confirm: 1,
    }
}

#[test]
fn args_require_manifest_and_metrics_cache() {
    let args = parse_flow_flip_research_args(
        [
            "--manifest",
            "/tmp/universe.json",
            "--metrics-cache",
            "/tmp/metrics.json",
        ]
        .into_iter()
        .map(str::to_owned),
    )
    .unwrap();

    assert_eq!(args.download_concurrency, 16);
    assert_eq!(args.binance_data_base, DEFAULT_BINANCE_DATA_BASE);
}

#[test]
fn price_confirmation_requires_recent_new_low_and_four_bar_breakout() {
    let mut candles = (0..HISTORY_BARS + LOW_MEMORY_BARS)
        .map(|index| candle(index, 100.0, 100.2, 99.8, 100.0))
        .collect::<Vec<_>>();
    let low_index = HISTORY_BARS + 1;
    candles[low_index].l = 99.0;
    let index = candles.len() - 1;
    candles[index] = candle(index, 100.0, 101.0, 99.9, 100.8);

    assert!(recent_new_low(&candles, index));
    assert!(bullish_breakout(&candles, index));

    candles[index].c = 100.1;
    assert!(!bullish_breakout(&candles, index));
}

#[test]
fn price_tail_uses_only_active_cross_section() {
    let symbols = ["A", "B", "C", "D", "E"]
        .into_iter()
        .map(|base| format!("{base}-USDT-SWAP"))
        .collect::<BTreeSet<_>>();
    let schedule = UniverseSchedule {
        version: "fixture".to_owned(),
        windows: vec![UniverseWindow {
            from_ms: HISTORY_BARS as i64 * MS_15M,
            to_ms: (HISTORY_BARS as i64 + 2) * MS_15M,
            members: symbols.clone(),
        }],
    };
    let candles = symbols
        .iter()
        .enumerate()
        .map(|(rank, symbol)| {
            let mut values = (0..=HISTORY_BARS)
                .map(|index| candle(index, 100.0, 100.1, 99.9, 100.0))
                .collect::<Vec<_>>();
            values[HISTORY_BARS].c = 90.0 + rank as f64;
            (symbol.clone(), values)
        })
        .collect::<BTreeMap<_, _>>();

    let (eligible, blocked) = build_price_tail_states(&schedule, &candles);

    assert_eq!(blocked, 0);
    assert_eq!(eligible.values().map(BTreeSet::len).sum::<usize>(), 1);
}

#[test]
fn acceptance_waits_for_first_completed_retest_and_invalidates_on_structure_loss() {
    let setup_index = HISTORY_BARS + LOW_MEMORY_BARS;
    let mut candles = (0..=setup_index + ACCEPTANCE_WAIT_BARS)
        .map(|index| candle(index, 100.4, 100.8, 100.3, 100.5))
        .collect::<Vec<_>>();
    candles[setup_index - 3].l = 99.0;
    candles[setup_index] = candle(setup_index, 100.4, 101.2, 100.3, 101.0);
    candles[setup_index + 1] = candle(setup_index + 1, 100.7, 101.0, 100.4, 100.6);
    candles[setup_index + 2] = candle(setup_index + 2, 100.4, 101.0, 100.2, 100.9);

    assert!(matches!(
        acceptance_decision(&candles, setup_index),
        AcceptanceDecision::Accepted(index) if index == setup_index + 2
    ));

    candles[setup_index + 1] = candle(setup_index + 1, 100.0, 100.1, 98.7, 98.9);
    assert!(matches!(
        acceptance_decision(&candles, setup_index),
        AcceptanceDecision::Invalidated(index) if index == setup_index + 1
    ));
}
