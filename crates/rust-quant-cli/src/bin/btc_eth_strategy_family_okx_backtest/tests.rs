use super::*;
use std::str::FromStr;

fn candle_entity(ts: i64) -> CandlesEntity {
    CandlesEntity {
        id: None,
        ts,
        o: "100".to_string(),
        h: "105".to_string(),
        l: "99".to_string(),
        c: "104".to_string(),
        vol: "10".to_string(),
        vol_ccy: "11".to_string(),
        confirm: "1".to_string(),
        created_at: None,
        updated_at: None,
    }
}

fn cli_scalper_impulse_pullback_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = (0..count)
        .map(|i| {
            let open = start + i as f64 * 2.0;
            let close = open + 1.2;
            CandleItem {
                o: open,
                h: close + 0.8,
                l: open - 0.8,
                c: close,
                v: 2_000.0 + i as f64,
                ts: 1_783_000_000_000 + i as i64 * 300_000,
                confirm: 1,
            }
        })
        .collect::<Vec<_>>();
    for (i, candle) in candles.iter_mut().enumerate() {
        if i == 520 {
            candle.o = start + 1_040.0;
            candle.c = candle.o + 120.0;
            candle.h = candle.c + 8.0;
            candle.l = candle.o - 8.0;
            candle.v *= 4.0;
        } else if i == 521 {
            candle.o = start + 1_160.0;
            candle.c = candle.o - 34.0;
            candle.h = candle.o + 10.0;
            candle.l = candle.c - 10.0;
            candle.v *= 1.7;
        } else if i == 522 {
            candle.o = start + 1_126.0;
            candle.c = candle.o + 18.0;
            candle.h = candle.c + 7.0;
            candle.l = candle.o - 11.0;
            candle.v *= 1.5;
        } else if i > 522 {
            let open = start + 1_144.0 + (i - 522) as f64 * 28.0;
            candle.o = open;
            candle.c = open + 18.0;
            candle.h = candle.c + 9.0;
            candle.l = candle.o - 9.0;
            candle.v *= 1.2;
        }
    }
    candles
}

fn cli_scalper_short_window_impulse_pullback_candles(count: usize, start: f64) -> Vec<CandleItem> {
    let mut candles = (0..count)
        .map(|i| {
            let open = start + i as f64 * 2.0;
            CandleItem {
                o: open,
                h: open + 3.0,
                l: open - 3.0,
                c: open + 1.0,
                v: 2_000.0,
                ts: 1_783_000_000_000 + i as i64 * 60_000,
                confirm: 1,
            }
        })
        .collect::<Vec<_>>();
    let high_regime_start = count.saturating_sub(48);
    let short_cycle_start = count.saturating_sub(34);
    for (i, candle) in candles.iter_mut().enumerate() {
        if i >= high_regime_start && i < short_cycle_start {
            candle.o = start + 5_000.0 - (i - high_regime_start) as f64 * 8.0;
            candle.c = candle.o - 3.0;
            candle.h = candle.o + 5.0;
            candle.l = candle.c - 5.0;
        } else if i >= short_cycle_start {
            let open = start + (i - short_cycle_start) as f64 * 5.0;
            candle.o = open;
            candle.c = open + 2.0;
            candle.h = candle.c + 2.0;
            candle.l = candle.o - 2.0;
        }
    }
    let impulse = count.saturating_sub(3);
    candles[impulse].o = start + 140.0;
    candles[impulse].c = start + 200.0;
    candles[impulse].h = start + 204.0;
    candles[impulse].l = start + 136.0;
    candles[impulse].v = 8_000.0;
    candles[impulse + 1].o = start + 200.0;
    candles[impulse + 1].c = start + 178.0;
    candles[impulse + 1].h = start + 204.0;
    candles[impulse + 1].l = start + 172.0;
    candles[impulse + 1].v = 3_200.0;
    candles[impulse + 2].o = start + 178.0;
    candles[impulse + 2].c = start + 210.0;
    candles[impulse + 2].h = start + 214.0;
    candles[impulse + 2].l = start + 172.0;
    candles[impulse + 2].v = 3_000.0;
    candles
}

fn eth_volume_reversal_candles(trigger_ts: i64) -> Vec<CandleItem> {
    let count = 720;
    let start_ts = trigger_ts - (count as i64 - 1) * 300_000;
    let mut candles = (0..count)
        .map(|i| CandleItem {
            o: 1_600.0,
            h: 1_602.0,
            l: 1_598.0,
            c: 1_600.0,
            v: 1_000.0,
            ts: start_ts + i as i64 * 300_000,
            confirm: 1,
        })
        .collect::<Vec<_>>();
    let trigger = candles.last_mut().expect("trigger candle");
    trigger.o = 1_572.0;
    trigger.h = 1_574.0;
    trigger.l = 1_552.0;
    trigger.c = 1_564.0;
    trigger.v = 3_500.0;
    candles
}

fn eth_volume_reversal_weak_compact_rebound_candles(trigger_ts: i64) -> Vec<CandleItem> {
    let mut candles = eth_volume_reversal_candles(trigger_ts);
    let trigger = candles.last_mut().expect("trigger candle");
    trigger.o = 1_563.0;
    trigger.h = 1_566.0;
    trigger.l = 1_556.0;
    trigger.c = 1_564.0;
    trigger.v = 3_500.0;
    candles
}

fn eth_volume_reversal_fib_candles(trigger_ts: i64) -> Vec<CandleItem> {
    let mut candles = eth_volume_reversal_candles(trigger_ts);
    let trigger_index = candles.len() - 1;
    candles[trigger_index].o = 1_570.0;
    candles[trigger_index].h = 1_572.0;
    candles[trigger_index].l = 1_558.0;
    candles[trigger_index].c = 1_566.0;
    candles[trigger_index].v = 3_500.0;

    let fib_low_index = trigger_index - 120;
    candles[fib_low_index].l = 1_552.0;
    candles[fib_low_index].o = 1_560.0;
    candles[fib_low_index].c = 1_558.0;
    candles[fib_low_index].h = 1_562.0;

    let morning_high_index = candles
        .iter()
        .position(|candle| candle.ts >= 1_782_874_800_000)
        .expect("morning high candle");
    candles[morning_high_index].h = 1_602.0;
    candles[morning_high_index].o = 1_585.0;
    candles[morning_high_index].c = 1_598.0;
    candles
}

fn eth_volume_reversal_inverted_v_candles(trigger_ts: i64) -> Vec<CandleItem> {
    let count = 720;
    let start_ts = trigger_ts - (count as i64 - 1) * 300_000;
    let mut candles = (0..count)
        .map(|i| CandleItem {
            o: 1_620.0,
            h: 1_622.0,
            l: 1_618.0,
            c: 1_620.0,
            v: 1_000.0,
            ts: start_ts + i as i64 * 300_000,
            confirm: 1,
        })
        .collect::<Vec<_>>();
    let base = candles.len() - 8;
    let leg = [
        (1_620.0, 1_626.0, 1_619.0, 1_625.0, 1_400.0),
        (1_625.0, 1_633.0, 1_624.0, 1_632.0, 1_800.0),
        (1_632.0, 1_642.0, 1_631.0, 1_640.0, 2_200.0),
        (1_640.0, 1_650.0, 1_638.0, 1_647.0, 2_800.0),
        (1_647.0, 1_660.0, 1_635.0, 1_638.0, 3_800.0),
        (1_638.0, 1_640.0, 1_610.0, 1_612.0, 4_500.0),
        (1_612.0, 1_618.0, 1_602.0, 1_608.0, 900.0),
        (1_608.0, 1_612.0, 1_598.0, 1_602.0, 2_200.0),
    ];
    for (offset, (o, h, l, c, v)) in leg.into_iter().enumerate() {
        let candle = &mut candles[base + offset];
        candle.o = o;
        candle.h = h;
        candle.l = l;
        candle.c = c;
        candle.v = v;
    }
    candles
}

fn eth_volume_reversal_inverted_v_snapback_candles(trigger_ts: i64) -> Vec<CandleItem> {
    let mut candles = eth_volume_reversal_inverted_v_candles(trigger_ts);
    let snapback = candles.len() - 2;
    candles[snapback].o = 1_612.0;
    candles[snapback].h = 1_636.0;
    candles[snapback].l = 1_608.0;
    candles[snapback].c = 1_632.0;
    candles[snapback].v = 3_200.0;
    candles
}

fn eth_volume_reversal_soft_short_contraction_candles(trigger_ts: i64) -> Vec<CandleItem> {
    let mut candles = eth_volume_reversal_inverted_v_candles(trigger_ts);
    let base = candles.len() - 8;
    let leg = [
        (1_620.0, 1_624.0, 1_619.0, 1_623.0, 1_150.0),
        (1_623.0, 1_628.0, 1_622.0, 1_627.0, 1_250.0),
        (1_627.0, 1_633.0, 1_626.0, 1_632.0, 1_300.0),
        (1_632.0, 1_641.0, 1_631.0, 1_638.0, 1_450.0),
        (1_638.0, 1_641.0, 1_634.0, 1_636.0, 1_700.0),
        (1_636.0, 1_638.0, 1_620.0, 1_624.0, 2_600.0),
        (1_624.0, 1_626.0, 1_614.0, 1_620.0, 700.0),
        (1_620.0, 1_622.0, 1_610.0, 1_616.0, 1_500.0),
    ];
    for (offset, (o, h, l, c, v)) in leg.into_iter().enumerate() {
        let candle = &mut candles[base + offset];
        candle.o = o;
        candle.h = h;
        candle.l = l;
        candle.c = c;
        candle.v = v;
    }
    candles
}

fn eth_volume_reversal_soft_short_active_confirm_volume_candles(
    trigger_ts: i64,
) -> Vec<CandleItem> {
    let mut candles = eth_volume_reversal_soft_short_contraction_candles(trigger_ts);
    let confirm = candles.len() - 2;
    candles[confirm].v = 1_900.0;
    candles
}

fn eth_volume_reversal_soft_short_rearms_after_active_confirm_candles(
    trigger_ts: i64,
) -> Vec<CandleItem> {
    let mut candles = eth_volume_reversal_soft_short_contraction_candles(trigger_ts);
    let active_confirm = candles.len() - 2;
    candles[active_confirm].o = 1_624.0;
    candles[active_confirm].h = 1_626.0;
    candles[active_confirm].l = 1_605.0;
    candles[active_confirm].c = 1_610.0;
    candles[active_confirm].v = 3_200.0;

    let rearm_confirm = candles.len() - 1;
    candles[rearm_confirm].o = 1_610.0;
    candles[rearm_confirm].h = 1_612.0;
    candles[rearm_confirm].l = 1_600.0;
    candles[rearm_confirm].c = 1_606.0;
    candles[rearm_confirm].v = 700.0;
    candles
}

fn eth_volume_reversal_entry(result: &BackTestResult) -> &TradeRecord {
    result
        .trade_records
        .iter()
        .find(|record| !record.full_close)
        .expect("entry record")
}

fn eth_volume_reversal_entry_value(result: &BackTestResult) -> serde_json::Value {
    serde_json::from_str(
        eth_volume_reversal_entry(result)
            .signal_value
            .as_deref()
            .expect("entry signal value"),
    )
    .expect("entry signal json")
}

fn eth_volume_reversal_short_entry(result: &BackTestResult) -> &TradeRecord {
    result
        .trade_records
        .iter()
        .find(|record| record.option_type == "short")
        .expect("short entry record")
}

#[test]
fn parses_cli_defaults_and_limit() {
    let args = parse_args(Vec::<String>::new()).unwrap();

    assert_eq!(args.limit, DEFAULT_LIMIT);
    assert_eq!(args.risk_percent, 2.0);
    assert_eq!(args.trade_fee_rate, None);
    assert!(!args.debug_trades);
    assert!(!args.scan_breakdown);
    assert!(!args.scan_exhaustion);
    assert!(!args.scan_micro);
    assert!(!args.scan_volume_reversal);
    assert!(!args.scan_scalper);
    assert!(!args.scan_scalper_narrow);
    assert!(!args.diagnose_scalper);
    assert!(!args.diagnose_volume_reversal);
    assert!(!args.use_market_context);
    assert!(!args.backfill_okx_market_context);
    assert_eq!(args.case_label, None);

    let args = parse_args(["--limit".to_string(), "1000".to_string()]).unwrap();
    assert_eq!(args.limit, 1000);

    let args = parse_args(["--trade-fee-rate".to_string(), "0.0005".to_string()]).unwrap();
    assert_eq!(args.trade_fee_rate, Some(0.0005));

    let args = parse_args(["--debug-trades".to_string()]).unwrap();
    assert!(args.debug_trades);

    let args = parse_args(["--scan-exhaustion".to_string()]).unwrap();
    assert!(args.scan_exhaustion);

    let args = parse_args(["--scan-breakdown".to_string()]).unwrap();
    assert!(args.scan_breakdown);

    let args = parse_args(["--scan-scalper".to_string()]).unwrap();
    assert!(args.scan_scalper);

    let args = parse_args(["--scan-scalper-narrow".to_string()]).unwrap();
    assert!(args.scan_scalper_narrow);

    let args = parse_args(["--scan-micro".to_string()]).unwrap();
    assert!(args.scan_micro);

    let args = parse_args(["--scan-volume-reversal".to_string()]).unwrap();
    assert!(args.scan_volume_reversal);

    let args = parse_args(["--diagnose-scalper".to_string()]).unwrap();
    assert!(args.diagnose_scalper);

    let args = parse_args(["--diagnose-volume-reversal".to_string()]).unwrap();
    assert!(args.diagnose_volume_reversal);

    let args = parse_args(["--use-market-context".to_string()]).unwrap();
    assert!(args.use_market_context);

    let args = parse_args(["--backfill-okx-market-context".to_string()]).unwrap();
    assert!(args.backfill_okx_market_context);

    let args = parse_args(["--case-label".to_string(), "scalper_btc_1m".to_string()]).unwrap();
    assert_eq!(args.case_label.as_deref(), Some("scalper_btc_1m"));
}

#[test]
fn strategy_type_accepts_eth_volume_reversal_dual_research_key() {
    assert_eq!(
        StrategyType::from_str("eth_volume_reversal_dual_5m_v1_research"),
        Ok(StrategyType::EthVolumeReversalDual5mV1Research)
    );
    assert_eq!(
        StrategyType::EthVolumeReversalDual5mV1Research.as_str(),
        "eth_volume_reversal_dual_5m_v1_research"
    );
}

#[test]
fn strategy_type_accepts_btc_volume_reversal_dual_research_key() {
    assert_eq!(
        StrategyType::from_str("btc_volume_reversal_dual_5m_v1_research"),
        Ok(StrategyType::BtcVolumeReversalDual5mV1Research)
    );
    assert_eq!(
        StrategyType::BtcVolumeReversalDual5mV1Research.as_str(),
        "btc_volume_reversal_dual_5m_v1_research"
    );
}

#[test]
fn eth_volume_reversal_enters_on_spike_without_waiting_for_support_reclaim() {
    let candles = eth_volume_reversal_candles(1_782_869_700_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry_value = eth_volume_reversal_entry_value(&result);

    assert_eq!(result.open_trades, 1);
    assert_eq!(eth_volume_reversal_entry(&result).open_price, 1_564.0);
    assert_eq!(
        entry_value["reasons"][0].as_str(),
        Some("ETH_VOLUME_REVERSAL_5M_SPIKE")
    );
    assert_eq!(
        entry_value["entry_mode"].as_str(),
        Some("left_utc_after_one")
    );
}

#[test]
fn eth_volume_reversal_uses_trigger_low_ema696_target_and_10x_leverage() {
    let candles = eth_volume_reversal_candles(1_782_869_700_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry = eth_volume_reversal_entry(&result);
    let entry_value = eth_volume_reversal_entry_value(&result);

    assert!((entry.quantity - (100.0 / 1_564.0) * 10.0).abs() < 1e-9);
    assert_eq!(entry_value["stop_price"].as_f64(), Some(1_552.0));
    assert_eq!(entry_value["target_source"].as_str(), Some("ema696"));
    assert!(entry_value["target_price"].as_f64().unwrap() > 1_564.0);
}

#[test]
fn eth_volume_reversal_can_filter_when_ema696_room_is_too_small() {
    let candles = eth_volume_reversal_candles(1_782_869_700_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m_with_tuning(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        volume_reversal_5m::EthVolumeReversal5mTuning {
            min_ema_distance_pct: Some(99.0),
            ..Default::default()
        },
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn eth_volume_reversal_default_uses_shape_stability_filters() {
    let tuning = volume_reversal_5m::EthVolumeReversal5mTuning::default();

    assert_eq!(tuning.volume_spike_mult, 3.0);
    assert_eq!(tuning.min_rebound_close_pos, 0.50);
    assert_eq!(tuning.weak_rebound_body_pct, Some(0.12));
    assert_eq!(tuning.weak_rebound_range_pct, Some(0.80));
    assert_eq!(tuning.max_stop_pct, Some(0.012));
    assert_eq!(tuning.min_ema_distance_pct, Some(1.5));
    assert_eq!(tuning.min_target_r, 1.5);
    assert!(tuning.use_utc_day_fib);
    assert!(!tuning.tiered_take_profit);
    assert!(tuning.allow_utc_after_one);
    assert!(tuning.allow_us_premarket_fib);
    assert!(!tuning.allow_beijing_midnight);
}

#[test]
fn eth_volume_reversal_default_rejects_beijing_midnight_after_shape_diagnosis() {
    let candles = eth_volume_reversal_candles(1_782_922_200_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn eth_volume_reversal_rejects_weak_compact_left_rebound() {
    let candles = eth_volume_reversal_weak_compact_rebound_candles(1_782_869_700_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(result.open_trades, 0);
}

#[test]
fn eth_volume_reversal_dual_waits_for_short_continuation_after_inverted_v() {
    let candles = eth_volume_reversal_inverted_v_candles(1_782_922_200_000);
    let long_only = volume_reversal_5m::run_eth_volume_reversal_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let dual = volume_reversal_5m::run_eth_volume_reversal_dual_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry = eth_volume_reversal_short_entry(&dual);
    let entry_value: serde_json::Value =
        serde_json::from_str(entry.signal_value.as_deref().expect("short signal value"))
            .expect("short signal json");

    assert_eq!(long_only.open_trades, 0);
    assert_eq!(dual.open_trades, 1);
    assert_eq!(entry.open_price, 1_608.0);
    assert_eq!(
        entry_value["entry_mode"].as_str(),
        Some("short_beijing_inverted_v_confirmed")
    );
    assert_eq!(entry_value["stop_price"].as_f64(), Some(1_640.0));
    assert_eq!(entry_value["target_r"].as_f64(), Some(1.5));
    assert_eq!(
        entry_value["reasons"][0].as_str(),
        Some("ETH_VOLUME_REVERSAL_DUAL_5M_INVERTED_V_SHORT")
    );
}

#[test]
fn eth_volume_reversal_dual_rejects_short_when_next_candle_snaps_back() {
    let candles = eth_volume_reversal_inverted_v_snapback_candles(1_782_922_200_000);
    let dual = volume_reversal_5m::run_eth_volume_reversal_dual_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(dual.open_trades, 0);
}

#[test]
fn eth_volume_reversal_dual_accepts_soft_short_when_confirmation_volume_contracts() {
    let candles = eth_volume_reversal_soft_short_contraction_candles(1_782_922_200_000);
    let dual = volume_reversal_5m::run_eth_volume_reversal_dual_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry = eth_volume_reversal_short_entry(&dual);
    let entry_value: serde_json::Value =
        serde_json::from_str(entry.signal_value.as_deref().expect("short signal value"))
            .expect("short signal json");

    assert_eq!(dual.open_trades, 1);
    assert_eq!(entry.open_price, 1_620.0);
    assert_eq!(entry_value["target_r"].as_f64(), Some(1.5));
    assert_eq!(
        entry_value["entry_mode"].as_str(),
        Some("short_beijing_inverted_v_confirmed")
    );
    assert!(
        entry_value["confirmation_volume_ratio"].as_f64().unwrap() <= 0.35,
        "soft short requires confirmation volume contraction"
    );
}

#[test]
fn eth_volume_reversal_dual_rejects_soft_short_when_confirmation_volume_stays_active() {
    let candles = eth_volume_reversal_soft_short_active_confirm_volume_candles(1_782_922_200_000);
    let dual = volume_reversal_5m::run_eth_volume_reversal_dual_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert_eq!(dual.open_trades, 0);
}

#[test]
fn eth_volume_reversal_dual_rearms_after_active_confirmation_candle() {
    let candles =
        eth_volume_reversal_soft_short_rearms_after_active_confirm_candles(1_782_922_200_000);
    let dual = volume_reversal_5m::run_eth_volume_reversal_dual_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry = eth_volume_reversal_short_entry(&dual);
    let entry_value: serde_json::Value =
        serde_json::from_str(entry.signal_value.as_deref().expect("short signal value"))
            .expect("short signal json");

    assert_eq!(dual.open_trades, 1);
    assert_eq!(entry.open_price, 1_606.0);
    assert_eq!(entry_value["trigger_price"].as_f64(), Some(1_610.0));
    assert!(
        entry_value["confirmation_volume_ratio"].as_f64().unwrap() <= 0.35,
        "the second confirmation should be the low-volume continuation candle"
    );
}

#[test]
fn eth_volume_reversal_dual_persists_as_independent_strategy_type() {
    let case = StrategyCase {
        label: "eth_volume_reversal_dual_5m",
        symbol: "ETH-USDT-SWAP",
        period: "5m",
        family: StrategyFamily::EthVolumeReversalDual5m,
    };

    assert_eq!(
        strategy_type_for_persistence(&case),
        Some(StrategyType::EthVolumeReversalDual5mV1Research)
    );
}

#[test]
fn eth_volume_reversal_can_research_beijing_midnight_when_enabled() {
    let candles = eth_volume_reversal_candles(1_782_922_200_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m_with_tuning(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        volume_reversal_5m::EthVolumeReversal5mTuning {
            allow_utc_after_one: false,
            allow_us_premarket_fib: false,
            allow_beijing_midnight: true,
            ..Default::default()
        },
    );
    let entry_value = eth_volume_reversal_entry_value(&result);

    assert_eq!(result.open_trades, 1);
    assert_eq!(
        entry_value["entry_mode"].as_str(),
        Some("left_beijing_after_midnight")
    );
}

#[test]
fn eth_volume_reversal_enters_immediately_on_fib_retracement_spike() {
    let candles = eth_volume_reversal_fib_candles(1_782_910_200_000);
    let result = volume_reversal_5m::run_eth_volume_reversal_5m(
        "ETH-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry_value = eth_volume_reversal_entry_value(&result);

    assert_eq!(result.open_trades, 1);
    assert_eq!(
        entry_value["entry_mode"].as_str(),
        Some("right_us_premarket_fib")
    );
    assert_eq!(
        entry_value["target_source"].as_str(),
        Some("prior_utc_morning_impulse")
    );
    assert!(entry_value["fib_0236"].as_f64().unwrap() >= 1_558.0);
}

#[test]
fn strategy_cases_include_1m_scalper_for_short_cycle_frequency() {
    let labels = strategy_cases()
        .iter()
        .map(|case| case.label)
        .collect::<Vec<_>>();

    assert!(labels.contains(&"scalper_btc_1m"));
    assert!(labels.contains(&"scalper_eth_1m"));
    assert!(labels.contains(&"micro_scalper_btc_1m"));
    assert!(labels.contains(&"micro_scalper_eth_1m"));
    assert!(labels.contains(&"eth_volume_reversal_5m"));
    assert!(labels.contains(&"btc_volume_reversal_dual_5m"));
    assert!(labels.contains(&"sol_volume_reversal_dual_5m"));
    assert!(labels.contains(&"breakdown_btc_5m"));
    assert!(labels.contains(&"breakdown_eth_5m"));
    assert!(labels.contains(&"exhaustion_btc_5m"));
    assert!(labels.contains(&"exhaustion_eth_5m"));
}

#[test]
fn eth_volume_reversal_persistence_uses_research_strategy_type() {
    let case = strategy_cases_for_filter(Some("eth_volume_reversal_5m"), false).unwrap()[0].clone();

    assert_eq!(
        strategy_type_for_persistence(&case).unwrap(),
        StrategyType::EthVolumeReversal5mV1Research
    );
}

#[test]
fn run_report_backtests_retains_trade_records_for_persistence() {
    let case = strategy_cases_for_filter(Some("eth_volume_reversal_5m"), false).unwrap()[0].clone();
    let loaded = LoadedCase {
        case,
        candles: eth_volume_reversal_candles(1_782_869_700_000),
        context: BacktestMarketContext::default(),
        context_required: false,
    };

    let runs = run_report_backtests(&[loaded], 2.0, None, ReportTuningOverrides::default());

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].report.label, "eth_volume_reversal_5m");
    assert!(!runs[0].result.trade_records.is_empty());
}

#[test]
fn eth_volume_reversal_persistence_risk_config_records_10x_contract() {
    let risk = risk_config_for_persistence(
        StrategyFamily::EthVolumeReversal5m,
        strategy_family_risk_config(2.0, None),
    );

    assert_eq!(risk.position_leverage, Some(10.0));
    assert_eq!(risk.is_used_signal_k_line_stop_loss, Some(true));
    assert_eq!(risk.dynamic_max_loss, Some(false));
}

#[test]
fn micro_scalper_1m_runs_existing_backtest_pipeline() {
    let candles = cli_scalper_impulse_pullback_candles(560, 100_000.0);
    let result = run_micro_scalper_1m(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );

    assert!(result.open_trades > 0);
    assert!(!result.trade_records.is_empty());
}

#[test]
fn micro_scalper_scan_tunings_are_fee_aware_without_short_cycle_trade_cap() {
    let tunings = micro_scalper_scan_tunings();

    assert!(tunings.len() > 20);
    assert!(tunings.iter().any(|tuning| !tuning.allow_short));
    assert!(tunings.iter().any(|tuning| tuning.target_r_2 >= 2.5));
    assert!(tunings.iter().any(|tuning| tuning.cooldown_candles <= 4));
}

#[test]
fn volume_reversal_scan_tunings_cover_day_fib_without_tiered_take_profit() {
    let tunings = volume_reversal_5m::volume_reversal_scan_tunings();

    assert!(tunings.iter().any(|tuning| tuning.use_utc_day_fib));
    assert!(tunings.iter().all(|tuning| !tuning.tiered_take_profit));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.target_r_override.is_some()));
}

#[test]
fn volume_reversal_scan_tunings_cover_no_tiered_low_r_winrate_candidates() {
    let tunings = volume_reversal_5m::volume_reversal_scan_tunings();

    assert!(tunings
        .iter()
        .any(|tuning| { !tuning.tiered_take_profit && tuning.target_r_override == Some(1.0) }));
    assert!(tunings
        .iter()
        .any(|tuning| { !tuning.tiered_take_profit && tuning.target_r_override == Some(1.2) }));
    assert!(tunings.iter().any(|tuning| {
        tuning.allow_utc_after_one
            && !tuning.allow_us_premarket_fib
            && !tuning.allow_beijing_midnight
    }));
    assert!(tunings.iter().any(|tuning| {
        !tuning.allow_utc_after_one
            && !tuning.allow_us_premarket_fib
            && tuning.allow_beijing_midnight
    }));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.min_ema_distance_pct == Some(1.0)));
}

#[test]
fn btc_volume_reversal_frequency_scan_tunings_cover_more_trade_candidates() {
    let tunings = volume_reversal_5m::btc_volume_reversal_frequency_scan_tunings();

    assert!(tunings.iter().any(|tuning| {
        tuning.volume_spike_mult == 4.0
            && tuning.target_r_override == Some(3.0)
            && tuning.weak_rebound_body_pct == None
            && tuning.weak_rebound_range_pct == None
    }));
    assert!(tunings.iter().any(|tuning| {
        tuning.volume_spike_mult == 3.5
            && tuning.target_r_override == Some(2.5)
            && tuning.min_ema_distance_pct == Some(1.0)
    }));
    assert!(tunings.iter().any(|tuning| tuning.cooldown_candles <= 4));
    assert!(tunings.iter().all(|tuning| {
        tuning.allow_utc_after_one
            && tuning.allow_us_premarket_fib
            && !tuning.allow_beijing_midnight
    }));
}

#[test]
fn parses_btc_volume_reversal_frequency_scan_flag() {
    let args = parse_args([
        "--scan-btc-volume-reversal".to_string(),
        "--limit".to_string(),
        "50000".to_string(),
    ])
    .unwrap();

    assert!(args.scan_btc_volume_reversal);
    assert_eq!(args.limit, 50_000);
}

#[test]
fn volume_reversal_diagnostic_tunings_compare_utc_only_against_utc_bj() {
    let tunings = volume_reversal_5m::volume_reversal_diagnostic_tunings();

    assert!(tunings.iter().any(|(label, tuning)| {
        *label == "utc_only_3r"
            && !tuning.tiered_take_profit
            && tuning.target_r_override == Some(3.0)
            && tuning.allow_utc_after_one
            && !tuning.allow_us_premarket_fib
            && !tuning.allow_beijing_midnight
    }));
    assert!(tunings.iter().any(|(label, tuning)| {
        *label == "utc_bj_3r"
            && !tuning.tiered_take_profit
            && tuning.target_r_override == Some(3.0)
            && tuning.allow_utc_after_one
            && !tuning.allow_us_premarket_fib
            && tuning.allow_beijing_midnight
    }));
    assert!(tunings.iter().any(|(label, tuning)| {
        *label == "bj_only_3r"
            && !tuning.tiered_take_profit
            && tuning.target_r_override == Some(3.0)
            && !tuning.allow_utc_after_one
            && !tuning.allow_us_premarket_fib
            && tuning.allow_beijing_midnight
    }));
}

#[test]
fn strategy_family_risk_config_keeps_trade_fee_separate_from_funding() {
    let default_risk = strategy_family_risk_config(2.0, None);
    let explicit_fee_risk = strategy_family_risk_config(2.0, Some(0.0005));

    assert_eq!(default_risk.max_loss_percent, 2.0);
    assert_eq!(default_risk.trade_fee_rate, None);
    assert_eq!(explicit_fee_risk.max_loss_percent, 2.0);
    assert_eq!(explicit_fee_risk.trade_fee_rate, Some(0.0005));
}

#[test]
fn strategy_case_filter_keeps_only_requested_label() {
    let cases = strategy_cases_for_filter(Some("scalper_btc_1m"), false).unwrap();

    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].label, "scalper_btc_1m");
    assert!(strategy_cases_for_filter(Some("missing_case"), false).is_err());
}

#[test]
fn default_case_filter_excludes_failed_research_micro_scalper() {
    let default_labels = strategy_cases_for_filter(None, false)
        .unwrap()
        .into_iter()
        .map(|case| case.label)
        .collect::<Vec<_>>();
    let research_labels = strategy_cases_for_filter(None, true)
        .unwrap()
        .into_iter()
        .map(|case| case.label)
        .collect::<Vec<_>>();
    let explicit_micro = strategy_cases_for_filter(Some("micro_scalper_btc_1m"), false).unwrap();

    assert!(!default_labels.contains(&"micro_scalper_btc_1m"));
    assert!(!default_labels.contains(&"micro_scalper_eth_1m"));
    assert!(!default_labels.contains(&"btc_volume_reversal_dual_5m"));
    assert!(!default_labels.contains(&"sol_volume_reversal_dual_5m"));
    assert!(research_labels.contains(&"micro_scalper_btc_1m"));
    assert!(research_labels.contains(&"micro_scalper_eth_1m"));
    assert!(research_labels.contains(&"btc_volume_reversal_dual_5m"));
    assert!(research_labels.contains(&"sol_volume_reversal_dual_5m"));
    assert_eq!(explicit_micro[0].label, "micro_scalper_btc_1m");
}

#[test]
fn alt_symbol_volume_reversal_cases_persist_as_dual_research_type() {
    let sol_case =
        strategy_cases_for_filter(Some("sol_volume_reversal_dual_5m"), false).unwrap()[0].clone();

    assert_eq!(sol_case.symbol, "SOL-USDT-SWAP");
    assert!(matches!(
        sol_case.family,
        StrategyFamily::EthVolumeReversalDual5m
    ));
    assert_eq!(
        strategy_type_for_persistence(&sol_case).unwrap(),
        StrategyType::EthVolumeReversalDual5mV1Research
    );
}

#[test]
fn btc_volume_reversal_case_uses_dedicated_research_type() {
    let btc_case =
        strategy_cases_for_filter(Some("btc_volume_reversal_dual_5m"), false).unwrap()[0].clone();

    assert_eq!(btc_case.symbol, "BTC-USDT-SWAP");
    assert!(matches!(
        btc_case.family,
        StrategyFamily::BtcVolumeReversalDual5m
    ));
    assert_eq!(
        strategy_type_for_persistence(&btc_case).unwrap(),
        StrategyType::BtcVolumeReversalDual5mV1Research
    );
}

#[test]
fn btc_volume_reversal_hybrid_case_uses_dedicated_research_type() {
    let btc_case =
        strategy_cases_for_filter(Some("btc_volume_reversal_hybrid_5m"), false).unwrap()[0].clone();

    assert_eq!(btc_case.symbol, "BTC-USDT-SWAP");
    assert!(matches!(
        btc_case.family,
        StrategyFamily::BtcVolumeReversalHybrid5m
    ));
    assert_eq!(
        strategy_type_for_persistence(&btc_case).unwrap(),
        StrategyType::BtcVolumeReversalHybrid5mV1Research
    );
}

#[test]
fn btc_volume_reversal_tuning_keeps_mechanics_and_limits_weak_bounces() {
    let eth = volume_reversal_5m::EthVolumeReversal5mTuning::default();
    let btc = volume_reversal_5m::btc_volume_reversal_5m_tuning();

    assert_eq!(btc.volume_window, eth.volume_window);
    assert_eq!(btc.ema_window, eth.ema_window);
    assert_eq!(btc.sweep_lookback, eth.sweep_lookback);
    assert_eq!(btc.fib_lookback, eth.fib_lookback);
    assert_eq!(btc.volume_spike_mult, 4.0);
    assert_eq!(
        btc.min_downside_excursion_pct,
        eth.min_downside_excursion_pct
    );
    assert_eq!(btc.min_rebound_close_pos, eth.min_rebound_close_pos);
    assert_eq!(btc.weak_rebound_body_pct, Some(0.20));
    assert_eq!(btc.weak_rebound_range_pct, Some(1.00));
    assert_eq!(btc.target_r_override, Some(3.0));
    assert_eq!(btc.min_target_r, 3.0);
    assert_eq!(btc.max_stop_pct, eth.max_stop_pct);
    assert_eq!(btc.allow_utc_after_one, eth.allow_utc_after_one);
    assert_eq!(btc.allow_us_premarket_fib, eth.allow_us_premarket_fib);
    assert_eq!(btc.allow_beijing_midnight, eth.allow_beijing_midnight);
}

#[test]
fn btc_volume_reversal_hybrid_shorts_confirmed_failed_weak_rebound() {
    let mut candles = eth_volume_reversal_weak_compact_rebound_candles(1_782_869_700_000);
    candles.last_mut().expect("trigger candle").v = 4_500.0;
    let pending = volume_reversal_5m::failed_weak_rebound_short_setup(
        &candles,
        volume_reversal_5m::btc_volume_reversal_5m_tuning(),
        volume_reversal_5m::BtcFailedWeakReboundShortTuning::default(),
    )
    .expect("weak rebound pending short setup");
    candles.push(CandleItem {
        o: 1_563.0,
        h: 1_565.0,
        l: 1_548.0,
        c: 1_550.0,
        v: 800.0,
        ts: 1_782_870_000_000,
        confirm: 1,
    });
    assert!(
        volume_reversal_5m::confirmed_failed_weak_rebound_short_signal(
            &candles,
            pending,
            volume_reversal_5m::BtcFailedWeakReboundShortTuning::default(),
        )
        .is_some()
    );

    let result = volume_reversal_5m::run_btc_volume_reversal_hybrid_5m(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
    );
    let entry = eth_volume_reversal_entry(&result);
    let entry_value: serde_json::Value =
        serde_json::from_str(entry.signal_value.as_deref().expect("hybrid signal value"))
            .expect("hybrid signal json");

    assert_eq!(result.open_trades, 1);
    assert_eq!(entry.option_type, "short");
    assert_eq!(
        entry_value["entry_mode"].as_str(),
        Some("short_failed_weak_rebound_confirmed")
    );
    assert_eq!(entry_value["target_r"].as_f64(), Some(1.5));
}

#[test]
fn scalper_scan_tunings_cover_optional_oi_confirmation_filter() {
    let tunings = scalper_scan_tunings();

    assert!(tunings.iter().any(|tuning| tuning.require_oi_confirmation));
    assert!(tunings.iter().any(|tuning| !tuning.require_oi_confirmation));
}

#[test]
fn scalper_scan_tunings_cover_short_cycle_trend_windows() {
    let tunings = scalper_scan_tunings();

    assert!(tunings
        .iter()
        .any(|tuning| tuning.trend_fast_window == 13 && tuning.trend_slow_window == 34));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.trend_fast_window == 20 && tuning.trend_slow_window == 48));
}

#[test]
fn scalper_narrow_scan_tunings_stay_small_and_short_cycle_focused() {
    let tunings = scalper_narrow_scan_tunings();

    assert!(tunings.len() <= 128);
    assert!(tunings
        .iter()
        .all(|tuning| tuning.trend_fast_window == 13 && tuning.trend_slow_window == 34));
    assert!(tunings.iter().any(|tuning| tuning.allow_short));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.min_directional_ratio_48 < 0.25));
}

#[test]
fn scalper_raw_candidate_sort_prefers_frequency_before_pnl() {
    let tuning = BtcEthLiquidityScalperBacktestTuning::default();
    let mut candidates = vec![
        ScalperScanCandidateReport {
            tuning,
            entries: 0,
            wins: 0,
            losses: 0,
            win_rate_pct: 0.0,
            pnl: 0.0,
            max_drawdown_pct: 0.0,
            trades_per_day: 0.0,
            early_win_rate_pct: 0.0,
            early_pnl: 0.0,
            late_win_rate_pct: 0.0,
            late_pnl: 0.0,
            remove_top5_pnl: 0.0,
            filtered_reason_counts: Vec::new(),
        },
        ScalperScanCandidateReport {
            tuning,
            entries: 100,
            wins: 44,
            losses: 32,
            win_rate_pct: 57.89,
            pnl: -1.5,
            max_drawdown_pct: 1.7,
            trades_per_day: 7.14,
            early_win_rate_pct: 64.0,
            early_pnl: -0.7,
            late_win_rate_pct: 51.0,
            late_pnl: -0.8,
            remove_top5_pnl: -2.0,
            filtered_reason_counts: Vec::new(),
        },
    ];

    sort_scalper_raw_candidates(&mut candidates);

    assert_eq!(candidates[0].entries, 100);
}

#[test]
fn merge_filtered_reason_counts_orders_by_frequency() {
    let reports = vec![
        CaseReport {
            label: "a".to_string(),
            candles: 0,
            entries: 0,
            closed: 0,
            wins: 0,
            losses: 0,
            win_rate_pct: 0.0,
            pnl: 0.0,
            final_funds: 100.0,
            max_drawdown_pct: 0.0,
            days: 0.0,
            trades_per_day: 0.0,
            trades: Vec::new(),
            filtered_signals: 0,
            filtered_reason_counts: vec![("LOW".to_string(), 1), ("HIGH".to_string(), 3)],
            filtered_signal_snapshots: Vec::new(),
        },
        CaseReport {
            label: "b".to_string(),
            candles: 0,
            entries: 0,
            closed: 0,
            wins: 0,
            losses: 0,
            win_rate_pct: 0.0,
            pnl: 0.0,
            final_funds: 100.0,
            max_drawdown_pct: 0.0,
            days: 0.0,
            trades_per_day: 0.0,
            trades: Vec::new(),
            filtered_signals: 0,
            filtered_reason_counts: vec![("LOW".to_string(), 2)],
            filtered_signal_snapshots: Vec::new(),
        },
    ];

    assert_eq!(
        merge_filtered_reason_counts(&reports),
        vec![("HIGH".to_string(), 3), ("LOW".to_string(), 3)]
    );
}

#[test]
fn merge_filtered_reason_counts_excludes_confirmed_signal_metadata() {
    let reports = vec![CaseReport {
        label: "a".to_string(),
        candles: 0,
        entries: 0,
        closed: 0,
        wins: 0,
        losses: 0,
        win_rate_pct: 0.0,
        pnl: 0.0,
        final_funds: 100.0,
        max_drawdown_pct: 0.0,
        days: 0.0,
        trades_per_day: 0.0,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: vec![
            ("BTC_ETH_LIQUIDITY_SCALP_CONFIRMED".to_string(), 3),
            ("STOP_PRICE:100.0".to_string(), 3),
            ("OI_NOT_CONFIRMED_REDUCE_SIZE".to_string(), 2),
            ("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 5),
        ],
        filtered_signal_snapshots: Vec::new(),
    }];

    assert_eq!(
        merge_filtered_reason_counts(&reports),
        vec![("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 5)]
    );
}

#[test]
fn scalper_filter_counts_ignore_non_scalper_baseline_reports() {
    let non_scalper = vec![CaseReport {
        label: "exhaustion_btc_5m".to_string(),
        candles: 0,
        entries: 0,
        closed: 0,
        wins: 0,
        losses: 0,
        win_rate_pct: 0.0,
        pnl: 0.0,
        final_funds: 100.0,
        max_drawdown_pct: 0.0,
        days: 0.0,
        trades_per_day: 0.0,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: vec![("EXHAUSTION_FADE_SHORT_V1_CONFIRMED".to_string(), 10)],
        filtered_signal_snapshots: Vec::new(),
    }];
    let scalper = vec![CaseReport {
        label: "scalper_btc_1m".to_string(),
        candles: 0,
        entries: 0,
        closed: 0,
        wins: 0,
        losses: 0,
        win_rate_pct: 0.0,
        pnl: 0.0,
        final_funds: 100.0,
        max_drawdown_pct: 0.0,
        days: 0.0,
        trades_per_day: 0.0,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: vec![("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 3)],
        filtered_signal_snapshots: Vec::new(),
    }];

    assert_eq!(
        scalper_filter_counts(&non_scalper, &scalper),
        vec![("MICROSTRUCTURE_CONFIRMATION_MISSING".to_string(), 3)]
    );
}

#[test]
fn scalper_candidate_summary_ignores_profitable_non_scalper_reports() {
    let non_scalper = vec![scan_case_report("breakdown_eth_5m", 20, 15, 5, 100.0)];
    let scalper = vec![scan_case_report("scalper_btc_1m", 4, 1, 3, -2.0)];

    let summary = summarize_scalper_candidate_reports(&non_scalper, &scalper);

    assert_eq!(summary.entries, 4);
    assert_eq!(summary.wins, 1);
    assert_eq!(summary.losses, 3);
    assert_eq!(summary.win_rate_pct, 25.0);
    assert_eq!(summary.pnl, -2.0);
}

#[test]
fn breakdown_candidate_summary_ignores_profitable_non_breakdown_reports() {
    let non_breakdown = vec![scan_case_report("exhaustion_eth_5m", 30, 20, 10, 80.0)];
    let breakdown = vec![scan_case_report("breakdown_btc_5m", 5, 2, 3, -1.5)];

    let summary = summarize_breakdown_candidate_reports(&non_breakdown, &breakdown);

    assert_eq!(summary.entries, 5);
    assert_eq!(summary.wins, 2);
    assert_eq!(summary.losses, 3);
    assert_eq!(summary.win_rate_pct, 40.0);
    assert_eq!(summary.pnl, -1.5);
}

#[test]
fn exhaustion_candidate_summary_ignores_profitable_non_exhaustion_reports() {
    let non_exhaustion = vec![scan_case_report("breakdown_eth_5m", 30, 20, 10, 80.0)];
    let exhaustion = vec![scan_case_report("exhaustion_btc_5m", 5, 2, 3, -1.5)];

    let summary = summarize_exhaustion_candidate_reports(&non_exhaustion, &exhaustion);

    assert_eq!(summary.entries, 5);
    assert_eq!(summary.wins, 2);
    assert_eq!(summary.losses, 3);
    assert_eq!(summary.win_rate_pct, 40.0);
    assert_eq!(summary.pnl, -1.5);
}

#[test]
fn build_report_attaches_entry_snapshot_debug_to_closed_trades() {
    let entry = trade_record(
        "short",
        "2026-06-01 00:00:00",
        false,
        100.0,
        0.0,
        Some(
            serde_json::json!({
                "price": 100.0,
                "failed_reclaim_high": 102.0,
                "atr_15m": 1.5,
                "oi_growth_pct": 4.0,
                "funding_rate": 0.0004,
                "long_short_ratio": 1.3,
                "taker_buy_volume": 2.0,
                "taker_sell_volume": 5.0
            })
            .to_string(),
        ),
        Some(
            serde_json::json!({
                "reasons": ["STOP_PRICE:102.5", "TARGET_R_1:1"]
            })
            .to_string(),
        ),
    );
    let close = trade_record(
        "close",
        "2026-06-01 00:00:00",
        true,
        100.0,
        -0.2,
        None,
        None,
    );
    let result = BackTestResult {
        funds: 99.8,
        open_trades: 1,
        trade_records: vec![entry, close],
        ..BackTestResult::default()
    };
    let candles = vec![CandleItem {
        o: 100.0,
        h: 101.0,
        l: 99.0,
        c: 100.0,
        v: 1.0,
        ts: 1_783_000_000_000,
        confirm: 1,
    }];

    let report = build_report("exhaustion_btc_5m", &candles, &result);
    let trade = report.trades.first().expect("closed trade debug");
    let snapshot = trade.entry_snapshot.expect("entry snapshot debug");

    assert_eq!(
        trade.entry_reasons,
        vec!["STOP_PRICE:102.5", "TARGET_R_1:1"]
    );
    assert!((snapshot.stop_distance_pct - 2.0).abs() < 1e-9);
    assert!((snapshot.atr_pct - 1.5).abs() < 1e-9);
    assert!((snapshot.oi_growth_pct - 4.0).abs() < 1e-9);
    assert!((snapshot.funding_rate - 0.0004).abs() < 1e-12);
    assert!((snapshot.long_short_ratio - 1.3).abs() < 1e-9);
    assert!((snapshot.taker_sell_buy_ratio - 2.5).abs() < 1e-9);
}

#[test]
fn parses_volume_reversal_entry_shape_snapshot_debug() {
    let snapshot = parse_entry_snapshot_debug(
        &serde_json::json!({
            "price": 1_564.0,
            "stop_price": 1_552.0,
            "target_r": 3.0,
            "ema696": 1_600.0,
            "volume_multiple": 5.2,
            "downside_excursion_pct": 1.1,
            "rebound_close_pos": 0.72,
            "candle_range_pct": 1.5,
            "body_pct": 0.4,
            "lower_wick_pct": 0.9,
            "upper_wick_pct": 0.2
        })
        .to_string(),
    )
    .expect("volume reversal snapshot");

    assert!((snapshot.stop_distance_pct - 0.7672634271099744).abs() < 1e-9);
    assert!((snapshot.target_r - 3.0).abs() < 1e-9);
    assert!((snapshot.ema_distance_pct - 2.301790281329923).abs() < 1e-9);
    assert!((snapshot.volume_multiple - 5.2).abs() < 1e-9);
    assert!((snapshot.downside_excursion_pct - 1.1).abs() < 1e-9);
    assert!((snapshot.rebound_close_pos - 0.72).abs() < 1e-9);
    assert!((snapshot.lower_wick_pct - 0.9).abs() < 1e-9);
    assert!((snapshot.upper_wick_pct - 0.2).abs() < 1e-9);
}

#[test]
fn build_report_attaches_filtered_signal_snapshot_debug() {
    let signal_snapshot = serde_json::json!({
        "price": 100.0,
        "failed_reclaim_high": 103.0,
        "atr_15m": 2.0,
        "oi_growth_pct": 0.2,
        "funding_rate": -0.0001,
        "long_short_ratio": 0.9,
        "taker_buy_volume": 4.0,
        "taker_sell_volume": 5.0
    })
    .to_string();
    let result: BackTestResult = serde_json::from_value(serde_json::json!({
        "funds": 0.0,
        "win_rate": 0.0,
        "open_trades": 0,
        "trade_records": [],
        "filtered_signals": [],
        "dynamic_config_logs": [],
        "audit_trail": {
            "run_id": "",
            "signal_snapshots": [{
                "ts": 1_783_000_000_000i64,
                "payload": serde_json::json!({
                    "single_value": signal_snapshot
                }).to_string(),
                "filtered": true,
                "filter_reasons": ["OI_GROWTH_MISSING"]
            }],
            "risk_decisions": [],
            "order_decisions": []
        }
    }))
    .expect("backtest result");
    let candles = vec![CandleItem {
        o: 100.0,
        h: 101.0,
        l: 99.0,
        c: 100.0,
        v: 1.0,
        ts: 1_783_000_000_000,
        confirm: 1,
    }];

    let report = build_report("breakdown_btc_5m", &candles, &result);
    let filtered = report
        .filtered_signal_snapshots
        .first()
        .expect("filtered signal debug");

    assert_eq!(filtered.reasons, vec!["OI_GROWTH_MISSING"]);
    assert_eq!(filtered.ts, 1_783_000_000_000);
    assert!((filtered.snapshot.stop_distance_pct - 3.0).abs() < 1e-9);
    assert!((filtered.snapshot.atr_pct - 2.0).abs() < 1e-9);
    assert!((filtered.snapshot.oi_growth_pct - 0.2).abs() < 1e-9);
    assert!((filtered.snapshot.funding_rate + 0.0001).abs() < 1e-12);
    assert!((filtered.snapshot.long_short_ratio - 0.9).abs() < 1e-9);
    assert!((filtered.snapshot.taker_sell_buy_ratio - 1.25).abs() < 1e-9);
}

#[test]
fn short_scan_candidate_requires_profit_after_removing_top_trades() {
    let fragile = ScanCandidateReport {
        tuning: BearShortStackBacktestTuning::default(),
        entries: 30,
        wins: 20,
        losses: 10,
        win_rate_pct: 66.67,
        pnl: 8.0,
        max_drawdown_pct: 4.0,
        trades_per_day: 1.0,
        early_win_rate_pct: 60.0,
        early_pnl: 2.0,
        late_win_rate_pct: 70.0,
        late_pnl: 6.0,
        remove_top5_pnl: -0.1,
    };

    assert!(!short_scan_candidate_meets_constraints(&fragile));
}

#[test]
fn short_scan_candidate_rejects_active_subcase_below_win_rate() {
    let reports = vec![
        scan_case_report("exhaustion_btc_5m", 20, 16, 4, 5.0),
        scan_case_report("exhaustion_eth_5m", 20, 10, 10, 1.0),
    ];
    let summary = ScanCandidateReport {
        tuning: BearShortStackBacktestTuning::default(),
        entries: 40,
        wins: 26,
        losses: 14,
        win_rate_pct: 65.0,
        pnl: 6.0,
        max_drawdown_pct: 2.0,
        trades_per_day: 1.0,
        early_win_rate_pct: 60.0,
        early_pnl: 1.0,
        late_win_rate_pct: 70.0,
        late_pnl: 5.0,
        remove_top5_pnl: 1.0,
    };

    assert!(short_scan_candidate_meets_constraints(&summary));
    assert!(!short_candidate_reports_meet_constraints(
        &summary, &reports
    ));
}

#[test]
fn format_case_reports_shows_subcase_frequency_win_rate_and_pnl() {
    let reports = vec![
        scan_case_report_with_pnls("breakdown_btc_5m", &[0.45, 0.30, -0.80]),
        scan_case_report("breakdown_eth_5m", 0, 0, 0, 0.0),
        scan_case_report_with_pnls("breakdown_btc_15m", &[0.25, -0.50, -0.25]),
    ];

    assert_eq!(
        format_case_reports(&reports),
        "breakdown_btc_15m:e3/wr33.33/pnl-0.5000/aw0.2500/al-0.3750;breakdown_btc_5m:e3/wr66.67/pnl-0.0500/aw0.3750/al-0.8000;breakdown_eth_5m:e0/wr0.00/pnl0.0000/aw0.0000/al0.0000"
    );
}

#[test]
fn scalper_setup_diagnostics_count_confirmed_and_failed_windows() {
    let diagnostics = scalper_setup_diagnostics(
        &cli_scalper_impulse_pullback_candles(560, 100_000.0),
        BtcEthLiquidityScalperBacktestTuning::default(),
    );

    assert!(diagnostics.samples > 0);
    assert!(diagnostics.confirmed > 0);
    assert_eq!(diagnostics.classified_windows(), diagnostics.samples);
}

#[test]
fn scalper_setup_diagnostics_respect_short_cycle_trend_windows() {
    let candles = cli_scalper_short_window_impulse_pullback_candles(560, 100_000.0);
    let default_diagnostics =
        scalper_setup_diagnostics(&candles, BtcEthLiquidityScalperBacktestTuning::default());
    let short_window_diagnostics = scalper_setup_diagnostics(
        &candles,
        BtcEthLiquidityScalperBacktestTuning {
            trend_fast_window: 13,
            trend_slow_window: 34,
            ..Default::default()
        },
    );

    assert_eq!(default_diagnostics.confirmed, 0);
    assert!(short_window_diagnostics.confirmed > 0);
}

#[test]
fn scalper_setup_diagnostics_explain_flat_market_rejections() {
    let candles = (0..560)
        .map(|i| CandleItem {
            o: 100.0,
            h: 100.5,
            l: 99.5,
            c: 100.0,
            v: 1_000.0,
            ts: 1_783_000_000_000 + i as i64 * 60_000,
            confirm: 1,
        })
        .collect::<Vec<_>>();
    let diagnostics =
        scalper_setup_diagnostics(&candles, BtcEthLiquidityScalperBacktestTuning::default());

    assert_eq!(diagnostics.confirmed, 0);
    assert!(diagnostics.reason_count("NO_TREND") > 0);
    assert_eq!(diagnostics.classified_windows(), diagnostics.samples);
}

#[test]
fn scalper_diagnostic_reasons_are_sorted_by_frequency() {
    let diagnostics = ScalperSetupDiagnostics {
        samples: 6,
        confirmed: 0,
        reasons: BTreeMap::from([("A_LOW", 1), ("Z_HIGH", 5)]),
    };

    assert_eq!(
        format_scalper_diagnostic_reasons(&diagnostics),
        "Z_HIGH:5,A_LOW:1"
    );
}

#[test]
fn breakdown_scan_tunings_stay_in_context_neighborhood() {
    let tunings = breakdown_scan_tunings();

    assert!(tunings.len() <= 512);
    assert_eq!(
        context_breakdown_tuning(),
        BearShortStackBacktestTuning::default()
    );
    assert!(!tunings.contains(&context_breakdown_tuning()));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.breakdown_initial_move_range_mult < 0.90));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.breakdown_min_volume_mult < 1.20));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.breakdown_stop_atr_buffer < 0.35));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.breakdown_target_r_2 > 1.6));
}

#[test]
fn exhaustion_scan_tunings_cover_exit_r_targets() {
    let tunings = exhaustion_scan_tunings();

    assert!(tunings.len() <= 512);
    assert!(tunings
        .iter()
        .any(|tuning| tuning.exhaustion_target_r_2 > 1.6));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.exhaustion_target_r_1 >= 1.0));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.exhaustion_stop_atr_buffer < 0.5));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.exhaustion_stop_atr_buffer > 0.5));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.exhaustion_min_rejection_atr >= 1.4));
    assert!(tunings.iter().any(|tuning| tuning.cooldown_candles < 12));
    assert!(tunings
        .iter()
        .any(|tuning| tuning.exhaustion_min_rejection_atr < 1.4));
}

#[test]
fn candle_entity_conversion_uses_sharded_table_entity_shape() {
    let item =
        candle_entity_to_item(&candle_entity(1_700_000_000_000), "BTC-USDT-SWAP", "5m").unwrap();

    assert_eq!(item.ts, 1_700_000_000_000);
    assert_eq!(item.o, 100.0);
    assert_eq!(item.h, 105.0);
    assert_eq!(item.l, 99.0);
    assert_eq!(item.c, 104.0);
    assert_eq!(item.v, 11.0);
    assert_eq!(item.confirm, 1);
}

#[test]
fn candle_span_days_uses_first_and_last_timestamp() {
    let candles = vec![
        CandleItem {
            ts: 1_700_000_000_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
        CandleItem {
            ts: 1_700_086_400_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
    ];

    assert_eq!(candle_span_days(&candles), 1.0);
}

#[test]
fn market_context_backfill_windows_cover_range_with_fixed_window_size() {
    let windows = market_context_backfill_windows(1_000, 3_500, 1_000);

    assert_eq!(
        windows,
        vec![(1_000, 1_999), (2_000, 2_999), (3_000, 3_500)]
    );
}

#[test]
fn market_context_symbol_base_uses_okx_swap_base_coin() {
    assert_eq!(okx_base_coin("BTC-USDT-SWAP"), "BTC");
    assert_eq!(okx_base_coin("ETH-USDT-SWAP"), "ETH");
}

#[test]
fn run_loaded_case_requires_market_context_instead_of_placeholder_snapshot() {
    let case = StrategyCase {
        label: "scalper_btc_5m",
        symbol: "BTC-USDT-SWAP",
        period: "5m",
        family: StrategyFamily::Scalper,
    };
    let candles = cli_scalper_impulse_pullback_candles(560, 100_000.0);
    let baseline = LoadedCase {
        case: case.clone(),
        candles: candles.clone(),
        context: BacktestMarketContext::default(),
        context_required: false,
    };
    let guarded = LoadedCase {
        case,
        candles,
        context: BacktestMarketContext::default(),
        context_required: true,
    };
    let risk = BasicRiskStrategyConfig::default();

    let baseline_result = run_loaded_case(
        &baseline,
        risk,
        Some(BtcEthLiquidityScalperBacktestTuning {
            allow_synthetic_market_context: true,
            ..Default::default()
        }),
        None,
    );
    let guarded_result = run_loaded_case(&guarded, risk, None, None);

    assert!(baseline_result.open_trades > 0);
    assert_eq!(guarded_result.open_trades, 0);
}

#[test]
fn context_breakdown_keeps_strict_default_until_validated() {
    let context_tuning = bear_tuning_for_context_run(StrategyFamily::Breakdown, None);
    let default_tuning = BearShortStackBacktestTuning::default();

    assert_eq!(context_tuning, default_tuning);
    assert_eq!(default_tuning.cooldown_candles, 12);
    assert_eq!(default_tuning.breakdown_initial_move_range_mult, 1.35);
    assert_eq!(
        bear_tuning_for_context_run(StrategyFamily::Exhaustion, None),
        BearShortStackBacktestTuning {
            cooldown_candles: 12,
            exhaustion_new_high_range_mult: 1.25,
            exhaustion_min_body_ratio: 0.30,
            exhaustion_min_volume_mult: 1.30,
            exhaustion_min_rejection_atr: 1.40,
            exhaustion_stop_atr_buffer: 0.35,
            exhaustion_target_r_1: 1.0,
            exhaustion_target_r_2: 2.0,
            ..Default::default()
        }
    );

    let provided = BearShortStackBacktestTuning {
        cooldown_candles: 4,
        ..Default::default()
    };
    assert_eq!(
        bear_tuning_for_context_run(StrategyFamily::Breakdown, Some(provided)),
        provided
    );
}

#[test]
fn report_tunings_keep_breakdown_and_exhaustion_separate() {
    let breakdown = context_breakdown_tuning();
    let exhaustion = BearShortStackBacktestTuning {
        cooldown_candles: 24,
        exhaustion_new_high_range_mult: 1.25,
        ..Default::default()
    };
    let tunings = ReportTuningOverrides {
        breakdown: Some(breakdown),
        exhaustion: Some(exhaustion),
        ..Default::default()
    };

    assert_eq!(
        bear_tuning_for_report_family(StrategyFamily::Breakdown, tunings),
        Some(breakdown)
    );
    assert_eq!(
        bear_tuning_for_report_family(StrategyFamily::Exhaustion, tunings),
        Some(exhaustion)
    );
    assert_eq!(
        bear_tuning_for_report_family(StrategyFamily::Scalper, tunings),
        None
    );
}

#[test]
fn builds_backtest_context_from_market_snapshot_series() {
    let candles = vec![
        CandleItem {
            ts: 1_700_000_300_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
        CandleItem {
            ts: 1_700_000_600_000,
            o: 0.0,
            h: 0.0,
            l: 0.0,
            c: 0.0,
            v: 0.0,
            confirm: 1,
        },
    ];
    let series = MarketContextSnapshotSeries {
        funding: vec![metric_snapshot("funding_rate", 1_700_000_000_000, 0.0001)],
        open_interest: vec![
            metric_snapshot("open_interest_volume", 1_700_000_000_000, 100.0),
            metric_snapshot("open_interest_volume", 1_700_000_300_000, 103.0),
        ],
        taker: vec![taker_snapshot(1_700_000_300_000, 12.0, 8.0)],
        long_short: vec![long_short_snapshot(1_700_000_300_000, 1.2)],
    };

    let context = build_backtest_market_context(&candles, &series);

    assert_eq!(context.scalper.len(), 2);
    assert_eq!(context.bear.len(), 2);
    assert!((context.scalper[0].oi_expansion_pct - 3.0).abs() < 1e-9);
    assert_eq!(context.scalper[0].taker_buy_volume, 12.0);
    assert_eq!(context.bear[0].long_short_ratio, 1.2);
}

fn metric_snapshot(metric_type: &str, metric_time: i64, value: f64) -> ExternalMarketSnapshot {
    let mut snapshot = ExternalMarketSnapshot::new(
        "okx".to_string(),
        "BTC-USDT-SWAP".to_string(),
        metric_type.to_string(),
        metric_time,
    );
    if metric_type == "funding_rate" {
        snapshot.funding_rate = Some(value);
    } else {
        snapshot.open_interest = Some(value);
    }
    snapshot
}

fn taker_snapshot(metric_time: i64, buy: f64, sell: f64) -> ExternalMarketSnapshot {
    let mut snapshot = ExternalMarketSnapshot::new(
        "okx".to_string(),
        "BTC-USDT-SWAP".to_string(),
        "taker_volume".to_string(),
        metric_time,
    );
    snapshot.raw_payload = Some(serde_json::json!({
        "buy_volume": buy,
        "sell_volume": sell
    }));
    snapshot
}

fn long_short_snapshot(metric_time: i64, ratio: f64) -> ExternalMarketSnapshot {
    let mut snapshot = ExternalMarketSnapshot::new(
        "okx".to_string(),
        "BTC-USDT-SWAP".to_string(),
        "long_short_ratio".to_string(),
        metric_time,
    );
    snapshot.long_short_ratio = Some(ratio);
    snapshot
}

fn scan_case_report(
    label: &str,
    entries: usize,
    wins: usize,
    losses: usize,
    pnl: f64,
) -> CaseReport {
    CaseReport {
        label: label.to_string(),
        candles: 0,
        entries,
        closed: wins + losses,
        wins,
        losses,
        win_rate_pct: ratio_pct(wins, wins + losses),
        pnl,
        final_funds: 100.0 + pnl,
        max_drawdown_pct: 1.0,
        days: 1.0,
        trades_per_day: entries as f64,
        trades: Vec::new(),
        filtered_signals: 0,
        filtered_reason_counts: Vec::new(),
        filtered_signal_snapshots: Vec::new(),
    }
}

fn scan_case_report_with_pnls(label: &str, pnls: &[f64]) -> CaseReport {
    let wins = pnls.iter().filter(|pnl| **pnl > 0.0).count();
    let losses = pnls.iter().filter(|pnl| **pnl < 0.0).count();
    let pnl = pnls.iter().sum::<f64>();
    let mut report = scan_case_report(label, pnls.len(), wins, losses, pnl);
    report.trades = pnls
        .iter()
        .map(|pnl| ClosedTradeDebug {
            open_time: String::new(),
            close_time: None,
            open_price: 0.0,
            close_price: None,
            pnl: *pnl,
            close_type: String::new(),
            entry_snapshot: None,
            entry_reasons: Vec::new(),
        })
        .collect();
    report
}

fn trade_record(
    option_type: &str,
    open_time: &str,
    full_close: bool,
    open_price: f64,
    profit_loss: f64,
    signal_value: Option<String>,
    signal_result: Option<String>,
) -> TradeRecord {
    TradeRecord {
        option_type: option_type.to_string(),
        open_position_time: open_time.to_string(),
        signal_open_position_time: None,
        close_position_time: Some(open_time.to_string()),
        open_price,
        signal_status: 0,
        close_price: Some(open_price),
        profit_loss,
        quantity: 1.0,
        full_close,
        close_type: String::new(),
        win_num: 0,
        loss_num: 0,
        signal_value,
        signal_result,
        stop_loss_source: None,
        stop_loss_update_history: None,
    }
}
