use super::*;

/// 由收盘序列构造连续且已确认的 15m K 线。
fn candles(start_ts: i64, closes: &[f64]) -> Vec<CandleItem> {
    closes
        .iter()
        .enumerate()
        .map(|(index, close)| CandleItem {
            ts: start_ts + index as i64 * MS_15M,
            o: *close,
            h: *close * 1.001,
            l: *close * 0.999,
            c: *close,
            v: 1.0,
            confirm: 1,
        })
        .collect()
}

#[test]
fn args_only_accept_the_frozen_manifest_path() {
    let args = parse_cross_sectional_momentum_args([
        "--manifest".to_owned(),
        "/tmp/momentum.json".to_owned(),
    ])
    .unwrap();
    assert_eq!(args.manifest, PathBuf::from("/tmp/momentum.json"));
    assert!(
        parse_cross_sectional_momentum_args(["--lookback".to_owned(), "48".to_owned()]).is_err()
    );
}

#[test]
fn factor_ignores_future_candles_and_rejects_a_gap() {
    let start_ts = -(LOOKBACK_BARS as i64) * MS_15M;
    let closes = (0..=LOOKBACK_BARS)
        .map(|index| 100.0 + index as f64 * 0.1)
        .collect::<Vec<_>>();
    let mut values = candles(start_ts, &closes);
    let decision_candle_ts = values.last().unwrap().ts;
    let before = return_24h_at(&values, decision_candle_ts).unwrap();
    values.push(candles(decision_candle_ts + MS_15M, &[1.0]).remove(0));
    assert_eq!(before, return_24h_at(&values, decision_candle_ts).unwrap());
    let mut gapped = values[..values.len() - 1].to_vec();
    gapped[LOOKBACK_BARS / 2].ts += 1;
    assert!(return_24h_at(&gapped, decision_candle_ts).is_none());
}

#[test]
fn leg_outcome_enters_at_next_open_and_uses_fixed_completed_closes() {
    let mut values = candles(0, &vec![100.0; FORWARD_24H_BARS]);
    values[FORWARD_8H_BARS - 1].c = 105.0;
    values[FORWARD_24H_BARS - 1].c = 110.0;
    let outcome = leg_outcome(&values, 0).unwrap();
    assert!((outcome.forward_8h - 0.05).abs() < 1e-12);
    assert!((outcome.forward_24h - 0.10).abs() < 1e-12);
}

#[test]
fn middle_control_indices_are_distinct_from_extreme_ranks() {
    let length = 60usize;
    let control_long = ((length - 1) as f64 * 0.25).floor() as usize;
    let control_short = ((length - 1) as f64 * 0.75).floor() as usize;
    assert_eq!(control_long, 14);
    assert_eq!(control_short, 44);
    assert_ne!(control_long, 0);
    assert_ne!(control_short, length - 1);
}

#[test]
fn spread_summary_uses_equal_notional_long_minus_short() {
    let observation = SpreadObservation {
        decision_ts: 0,
        long_symbol: "AAA-USDT-SWAP".to_owned(),
        short_symbol: "BBB-USDT-SWAP".to_owned(),
        long_outcome: LegOutcome {
            forward_8h: 0.03,
            forward_24h: 0.05,
        },
        short_outcome: LegOutcome {
            forward_8h: -0.02,
            forward_24h: -0.04,
        },
        control_long_outcome: LegOutcome {
            forward_8h: 0.01,
            forward_24h: 0.01,
        },
        control_short_outcome: LegOutcome {
            forward_8h: 0.0,
            forward_24h: 0.0,
        },
    };
    let summary = summarize_spread(&[observation], false);
    assert!((summary.mean_forward_8h.unwrap() - 0.05).abs() < 1e-12);
    assert!((summary.mean_forward_24h.unwrap() - 0.09).abs() < 1e-12);
}

#[test]
fn event_clustering_is_anchored_instead_of_chaining_all_decisions() {
    let template = SpreadObservation {
        decision_ts: 0,
        long_symbol: "AAA-USDT-SWAP".to_owned(),
        short_symbol: "BBB-USDT-SWAP".to_owned(),
        long_outcome: LegOutcome {
            forward_8h: 0.0,
            forward_24h: 0.0,
        },
        short_outcome: LegOutcome {
            forward_8h: 0.0,
            forward_24h: 0.0,
        },
        control_long_outcome: LegOutcome {
            forward_8h: 0.0,
            forward_24h: 0.0,
        },
        control_short_outcome: LegOutcome {
            forward_8h: 0.0,
            forward_24h: 0.0,
        },
    };
    let observations = [
        template.clone(),
        SpreadObservation {
            decision_ts: MS_8H,
            ..template.clone()
        },
        SpreadObservation {
            decision_ts: 2 * MS_8H,
            ..template
        },
    ];
    assert_eq!(effective_events(&observations), 2);
}
