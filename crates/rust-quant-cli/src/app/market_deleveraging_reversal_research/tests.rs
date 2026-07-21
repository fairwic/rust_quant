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
fn args_require_frozen_manifest_and_context_cache() {
    let args = parse_deleveraging_research_args(
        [
            "--manifest",
            "/tmp/universe.json",
            "--context-cache",
            "/tmp/context.json",
        ]
        .into_iter()
        .map(str::to_owned),
    )
    .unwrap();

    assert_eq!(args.download_concurrency, 8);
    assert_eq!(args.okx_base, DEFAULT_OKX_BASE);
}

#[test]
fn red_lower_wick_sweep_is_allowed_but_needs_next_green_confirmation() {
    let mut candles = (0..HISTORY_BARS)
        .map(|index| {
            let close = 100.0 - index as f64 * 0.04;
            candle(index, close + 0.01, close + 0.1, close - 0.1, close)
        })
        .collect::<Vec<_>>();
    let sweep = candle(HISTORY_BARS, 96.2, 96.4, 95.0, 96.15);
    let confirmation = candle(HISTORY_BARS + 1, 96.1, 96.4, 96.0, 96.3);
    candles.push(sweep.clone());

    assert!(sweep.c < sweep.o);
    assert!(sweep_reclaim_shape(&candles, HISTORY_BARS));
    assert!(confirmation_passes(&sweep, &confirmation));

    let bearish_confirmation = candle(HISTORY_BARS + 1, 96.3, 96.35, 95.9, 96.0);
    assert!(!confirmation_passes(&sweep, &bearish_confirmation));
}

#[test]
fn price_tail_is_ranked_only_inside_the_active_month_members() {
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
    assert!(eligible["A-USDT-SWAP"].contains(&((HISTORY_BARS as i64 + 1) * MS_15M)));
}
