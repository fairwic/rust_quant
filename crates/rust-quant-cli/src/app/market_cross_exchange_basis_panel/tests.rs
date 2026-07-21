use super::*;

/// 由给定对数基差构造 OKX K 线；Binance 基准价格固定为 100。
fn okx_from_basis(start_ts: i64, basis: &[f64]) -> Vec<CandleItem> {
    basis
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let price = 100.0 * value.exp();
            CandleItem {
                ts: start_ts + index as i64 * MS_15M,
                o: price,
                h: price * 1.001,
                l: price * 0.999,
                c: price,
                v: 1.0,
                confirm: 1,
            }
        })
        .collect()
}

/// 构造固定价格的 Binance 15m K 线。
fn flat_binance(start_ts: i64, bars: usize) -> Vec<BinanceCandle> {
    (0..bars)
        .map(|index| BinanceCandle {
            ts: start_ts + index as i64 * MS_15M,
            open: 100.0,
            close: 100.0,
            quote_volume: 1.0,
            taker_buy_quote_volume: 0.5,
        })
        .collect()
}

#[test]
fn args_only_expose_data_locations_and_download_concurrency() {
    let args = parse_cross_exchange_basis_panel_args([
        "--manifest".to_owned(),
        "/tmp/universe.json".to_owned(),
        "--cache-dir".to_owned(),
        "/tmp/binance".to_owned(),
    ])
    .unwrap();
    assert_eq!(args.manifest, PathBuf::from("/tmp/universe.json"));
    assert_eq!(args.cache_dir, PathBuf::from("/tmp/binance"));
    assert_eq!(args.download_concurrency, 16);
    assert!(
        parse_cross_exchange_basis_panel_args(["--z-score".to_owned(), "3".to_owned()]).is_err()
    );
}

#[test]
fn factor_uses_only_synchronized_completed_seven_day_prefix() {
    let mut basis = (0..BASIS_BARS)
        .map(|index| if index % 2 == 0 { 0.001 } else { -0.001 })
        .collect::<Vec<_>>();
    *basis.last_mut().unwrap() = 0.012;
    let start_ts = -(BASIS_BARS as i64) * MS_15M;
    let binance = flat_binance(start_ts, BASIS_BARS);
    let mut okx = okx_from_basis(start_ts, &basis);
    let decision_candle_ts = okx.last().unwrap().ts;
    let before = basis_factor_at(&okx, &binance, decision_candle_ts).unwrap();
    assert!(before.z_score > EXTREME_Z);

    let future_ts = decision_candle_ts + MS_15M;
    okx.push(okx_from_basis(future_ts, &[1.0]).remove(0));
    assert_eq!(
        before,
        basis_factor_at(&okx, &binance, decision_candle_ts).unwrap()
    );

    let mut gapped = okx[..okx.len() - 1].to_vec();
    gapped[BASIS_BARS / 2].ts += 1;
    assert!(basis_factor_at(&gapped, &binance, decision_candle_ts).is_none());
}

#[test]
fn paired_outcome_fades_the_expensive_exchange_from_next_common_open() {
    let bars = FORWARD_24H_BARS;
    let mut okx = okx_from_basis(0, &vec![0.0; bars]);
    let binance = flat_binance(0, bars);
    for candle in &mut okx[FORWARD_1H_BARS - 1..] {
        candle.c = 99.0;
    }
    let positive_z = paired_forward_returns(&okx, &binance, 0, 2.5).unwrap();
    let negative_z = paired_forward_returns(&okx, &binance, 0, -2.5).unwrap();
    assert!((positive_z.0 - 0.01).abs() < 1e-12);
    assert!((positive_z.1 - 0.01).abs() < 1e-12);
    assert!((positive_z.2 - 0.01).abs() < 1e-12);
    assert!((negative_z.0 + 0.01).abs() < 1e-12);
}

#[test]
fn selection_uses_the_largest_absolute_z_score_without_replacement() {
    let factor_start = -(BASIS_BARS as i64) * MS_15M;
    let total_bars = BASIS_BARS + FORWARD_24H_BARS;
    let binance_series = flat_binance(factor_start, total_bars);
    let mut strong_basis = (0..total_bars)
        .map(|index| if index % 2 == 0 { 0.001 } else { -0.001 })
        .collect::<Vec<_>>();
    let mut weak_basis = strong_basis.clone();
    strong_basis[BASIS_BARS - 1] = 0.015;
    weak_basis[BASIS_BARS - 1] = 0.006;
    let schedule = UniverseSchedule {
        version: "test".to_owned(),
        windows: vec![UniverseWindow {
            from_ms: 0,
            to_ms: MS_4H,
            members: ["AAA-USDT-SWAP".to_owned(), "BBB-USDT-SWAP".to_owned()]
                .into_iter()
                .collect(),
        }],
    };
    let okx = BTreeMap::from([
        (
            "AAA-USDT-SWAP".to_owned(),
            okx_from_basis(factor_start, &strong_basis),
        ),
        (
            "BBB-USDT-SWAP".to_owned(),
            okx_from_basis(factor_start, &weak_basis),
        ),
    ]);
    let binance = BTreeMap::from([
        ("AAA-USDT-SWAP".to_owned(), binance_series.clone()),
        ("BBB-USDT-SWAP".to_owned(), binance_series),
    ]);
    let (observations, stages) = build_observations(&schedule, &okx, &binance);
    assert_eq!(stages.decision_points, 1);
    assert_eq!(stages.selected_candidates, 1);
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].symbol, "AAA-USDT-SWAP");
}

#[test]
fn dislocation_factor_requires_a_synchronized_673_bar_prefix() {
    let mut basis = (0..=BASIS_BARS)
        .map(|index| if index % 2 == 0 { 0.0001 } else { -0.0001 })
        .collect::<Vec<_>>();
    *basis.last_mut().unwrap() = 0.01;
    let start_ts = -((BASIS_BARS + 1) as i64) * MS_15M;
    let okx = okx_from_basis(start_ts, &basis);
    let binance = flat_binance(start_ts, basis.len());
    let factors = dislocation_factors(&okx, &binance);
    assert_eq!(factors.len(), 1);
    assert_eq!(factors[0].0, 0);
    assert!(factors[0].1.current_deviation.abs() >= EXECUTABLE_DEVIATION);
    assert!(factors[0].1.previous_deviation.abs() < CONTROL_DEVIATION);

    let mut gapped = okx;
    gapped[BASIS_BARS / 2].ts += 1;
    assert!(dislocation_factors(&gapped, &binance).is_empty());
}

#[test]
fn executable_crossing_outranks_the_near_cost_control() {
    let factor_start = -((BASIS_BARS + 1) as i64) * MS_15M;
    let total_bars = BASIS_BARS + 1 + FORWARD_24H_BARS;
    let binance_series = flat_binance(factor_start, total_bars);
    let mut executable_basis = (0..total_bars)
        .map(|index| if index % 2 == 0 { 0.0001 } else { -0.0001 })
        .collect::<Vec<_>>();
    let mut control_basis = executable_basis.clone();
    executable_basis[BASIS_BARS] = 0.01;
    control_basis[BASIS_BARS] = 0.004;
    let schedule = UniverseSchedule {
        version: "test".to_owned(),
        windows: vec![UniverseWindow {
            from_ms: 0,
            to_ms: MS_15M,
            members: ["AAA-USDT-SWAP".to_owned(), "BBB-USDT-SWAP".to_owned()]
                .into_iter()
                .collect(),
        }],
    };
    let okx = BTreeMap::from([
        (
            "AAA-USDT-SWAP".to_owned(),
            okx_from_basis(factor_start, &executable_basis),
        ),
        (
            "BBB-USDT-SWAP".to_owned(),
            okx_from_basis(factor_start, &control_basis),
        ),
    ]);
    let binance = BTreeMap::from([
        ("AAA-USDT-SWAP".to_owned(), binance_series.clone()),
        ("BBB-USDT-SWAP".to_owned(), binance_series),
    ]);
    let (observations, stages) = build_dislocation_observations(&schedule, &okx, &binance);
    assert_eq!(stages.decision_points, 1);
    assert_eq!(stages.executable_crossings, 1);
    assert_eq!(stages.control_crossings, 1);
    assert_eq!(stages.selected_executable, 1);
    assert_eq!(observations.len(), 1);
    assert!(observations[0].executable);
    assert_eq!(observations[0].symbol, "AAA-USDT-SWAP");
}
