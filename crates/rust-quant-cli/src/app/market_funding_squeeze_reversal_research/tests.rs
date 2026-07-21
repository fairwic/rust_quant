use super::*;
use std::io::Write;
use zip::write::FileOptions;

fn candle(index: usize, open: f64, high: f64, low: f64, close: f64, volume: f64) -> CandleItem {
    CandleItem {
        ts: index as i64 * MS_15M,
        o: open,
        h: high,
        l: low,
        c: close,
        v: volume,
        confirm: 1,
    }
}

#[test]
fn official_funding_archive_filters_manifest_symbols_and_validates_day() {
    let day = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(
            "allswap-fundingrates-2024-07-01.csv",
            FileOptions::default(),
        )
        .unwrap();
    writer
        .write_all(
            b"instrument_name,funding_rate,funding_time\r\nBTC-USDT-SWAP,-0.0001,1719763200000\r\nETH-USDT-SWAP,0.0002,1719792000000\r\nBTC-USD-SWAP,-0.01,1719792000000\r\n",
        )
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();

    let rows = parse_funding_archive(
        &bytes,
        day,
        &BTreeSet::from(["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()]),
    )
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "BTC-USDT-SWAP");
    assert_eq!(rows[0].1.rate, -0.0001);
}

#[test]
fn funding_archive_day_rolls_at_utc_16() {
    let before_roll = Utc
        .with_ymd_and_hms(2024, 7, 1, 15, 59, 59)
        .single()
        .unwrap()
        .timestamp_millis();
    let after_roll = Utc
        .with_ymd_and_hms(2024, 7, 1, 16, 0, 0)
        .single()
        .unwrap()
        .timestamp_millis();

    assert_eq!(
        timestamp_day(before_roll).unwrap(),
        NaiveDate::from_ymd_opt(2024, 7, 1).unwrap()
    );
    assert_eq!(
        timestamp_day(after_roll).unwrap(),
        NaiveDate::from_ymd_opt(2024, 7, 2).unwrap()
    );
}

#[test]
fn funding_requires_one_completed_bar_delay_and_expires() {
    let point = FundingState {
        ts: 8 * 60 * 60 * 1_000,
        rate: -0.001,
        eligible: true,
    };

    assert_eq!(latest_funding_state(&[point], point.ts), None);
    assert_eq!(
        latest_funding_state(&[point], point.ts + MS_15M),
        Some(point)
    );
    assert_eq!(
        latest_funding_state(&[point], point.ts + FUNDING_MAX_AGE_MS + 1),
        None
    );
}

#[test]
fn red_long_lower_wick_can_pass_sweep_reclaim_shape() {
    let mut candles = (0..HISTORY_BARS)
        .map(|index| {
            let close = 100.0 - index as f64 * 0.065;
            candle(index, close + 0.02, close + 0.1, close - 0.1, close, 100.0)
        })
        .collect::<Vec<_>>();
    candles.push(candle(HISTORY_BARS, 94.0, 94.3, 93.0, 93.95, 160.0));

    assert!(candles[HISTORY_BARS].c < candles[HISTORY_BARS].o);
    assert!(reversal_signal(&candles, HISTORY_BARS).is_some());
}
