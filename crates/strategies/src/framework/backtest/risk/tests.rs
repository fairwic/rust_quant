use super::*;
#[test]
fn compute_targets_prefers_tightest_stop_loss_and_nearest_tp_long() {
    let position = TradePosition {
        trade_side: TradeSide::Long,
        open_price: 100.0,
        position_nums: 1.0,
        signal_kline_stop_close_price: Some(95.0),
        move_stop_open_price: Some(98.0),
        atr_take_ratio_profit_price: Some(120.0),
        long_signal_take_profit_price: Some(110.0),
        ..TradePosition::default()
    };
    let candle = CandleItem {
        o: 100.0,
        h: 105.0,
        l: 99.0,
        c: 102.0,
        v: 1.0,
        ts: 1,
        confirm: 1,
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 0.05,
        ..Default::default()
    };
    let targets = compute_current_targets(&position, &candle, &risk);
    assert_eq!(targets.stop_loss, Some(98.0));
    assert_eq!(targets.take_profit, Some(110.0));
}
#[test]
fn compute_targets_prefers_tightest_stop_loss_and_nearest_tp_short() {
    let position = TradePosition {
        trade_side: TradeSide::Short,
        open_price: 100.0,
        position_nums: 1.0,
        signal_kline_stop_close_price: Some(106.0),
        move_stop_open_price: Some(103.0),
        atr_take_ratio_profit_price: Some(80.0),
        short_signal_take_profit_price: Some(90.0),
        ..TradePosition::default()
    };
    let candle = CandleItem {
        o: 100.0,
        h: 101.0,
        l: 95.0,
        c: 97.0,
        v: 1.0,
        ts: 1,
        confirm: 1,
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 0.05,
        ..Default::default()
    };
    let targets = compute_current_targets(&position, &candle, &risk);
    assert_eq!(targets.stop_loss, Some(103.0));
    assert_eq!(targets.take_profit, Some(90.0));
}

#[test]
fn base_stop_uses_max_loss_when_signal_stop_is_looser() {
    let position = TradePosition {
        trade_side: TradeSide::Long,
        open_price: 100.0,
        position_nums: 1.0,
        signal_kline_stop_close_price: Some(95.0),
        ..TradePosition::default()
    };
    let candle = CandleItem {
        o: 100.0,
        h: 101.0,
        l: 94.0,
        c: 96.0,
        v: 1.0,
        ts: 1,
        confirm: 1,
    };
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 0.02,
        dynamic_max_loss: Some(false),
        ..Default::default()
    };
    let result =
        check_base_protective_stop(&ExitContext::new(&position, &candle), &position, &risk);
    assert!(matches!(
        result,
        ExitResult::Exit {
            price,
            reason: "最大亏损止损"
        } if (price - 98.0).abs() < 1e-9
    ));
}
#[test]
fn effective_max_loss_keeps_default_entry_mismatch_tightening() {
    let position = TradePosition {
        trade_side: TradeSide::Long,
        open_price: 100.0,
        entry_kline_amplitude: Some(0.04),
        entry_kline_close_pos: Some(0.4),
        ..TradePosition::default()
    };
    let candle = CandleItem {
        o: 100.0,
        h: 101.0,
        l: 99.0,
        c: 100.0,
        v: 1.0,
        ts: 1,
        confirm: 1,
    };
    let ctx = ExitContext::new(&position, &candle);
    assert_eq!(
        compute_effective_max_loss(&position, &ctx, 0.05, true),
        0.03
    );
}
#[test]
fn effective_max_loss_can_tighten_entry_amplitude_without_direction_mismatch() {
    let position = TradePosition {
        trade_side: TradeSide::Long,
        open_price: 100.0,
        entry_kline_amplitude: Some(0.04),
        entry_kline_close_pos: Some(0.8),
        ..TradePosition::default()
    };
    let candle = CandleItem {
        o: 100.0,
        h: 101.0,
        l: 99.0,
        c: 100.0,
        v: 1.0,
        ts: 1,
        confirm: 1,
    };
    let ctx = ExitContext::new(&position, &candle);
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 0.05,
        dynamic_entry_require_direction_mismatch: Some(false),
        dynamic_entry_amp_threshold: Some(0.03),
        dynamic_entry_loss_percent: Some(0.035),
        ..Default::default()
    };
    assert_eq!(
        compute_effective_max_loss_with_config(
            &position,
            &ctx,
            risk.max_loss_percent,
            risk.dynamic_max_loss.unwrap_or(true),
            &risk,
        ),
        0.035
    );
}
