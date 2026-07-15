use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropSignalSnapshot, RangeBreakoutDropStrategy, RangeBreakoutDropThresholds,
};

fn main() {
    println!("========== 测试evaluate函数 ==========\n");

    // 测试1：收盘价突破 + 阴线 - 应该通过
    let snapshot1 = RangeBreakoutDropSignalSnapshot {
        exchange: "binance".to_string(),
        symbol: "BTC-USDT-SWAP".to_string(),
        price: 49000.0,
        range_high: 50000.0,
        range_low: 49500.0,
        range_volatility_pct: 1.0,
        in_ranging_mode: true,
        breakout_confirmed: true,
        is_close_breakout: true,
        breakout_body_ratio: 0.7,
        breakout_move_atr: 1.2,
        breakout_volume_mult: 2.0,
        slow_ema: 50000.0,
        price_below_ema: true,
        long_term_ema: 50000.0,
        price_below_long_term_ema: true,
        atr: 200.0,
        rsi: 50.0,
        candle_direction: -1,
    };

    let thresholds = RangeBreakoutDropThresholds {
        range_lookback_candles: 20,
        max_range_volatility_pct: 10.0,
        min_range_volatility_pct: 0.1,
        min_breakout_body_ratio: 0.2,
        min_breakout_move_atr: 0.1,
        min_breakout_volume_mult: 0.5,
        require_bearish_ema: false,
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: false,
        stop_atr_mult: 2.0,
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.5,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
    };

    let decision1 = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot1);
    println!("测试1 - 收盘价突破+阴线:");
    println!("  动作: {:?}", decision1.action);
    println!("  原因: {:?}\n", decision1.reasons);

    // 测试2：最低价触及 + 阳线 - 应该通过（不要求阴线）
    let snapshot2 = RangeBreakoutDropSignalSnapshot {
        breakout_confirmed: true,
        is_close_breakout: false, // 最低价触及
        candle_direction: 1,      // 阳线
        ..snapshot1.clone()
    };

    let decision2 = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot2);
    println!("测试2 - 最低价触及+阳线:");
    println!("  动作: {:?}", decision2.action);
    println!("  原因: {:?}\n", decision2.reasons);

    // 测试3：收盘价突破 + 阳线 - 应该被过滤
    let snapshot3 = RangeBreakoutDropSignalSnapshot {
        is_close_breakout: true, // 收盘价突破
        candle_direction: 1,     // 阳线
        ..snapshot1.clone()
    };

    let decision3 = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot3);
    println!("测试3 - 收盘价突破+阳线:");
    println!("  动作: {:?}", decision3.action);
    println!("  原因: {:?}\n", decision3.reasons);

    // 测试4：未突破 - 应该被过滤
    let snapshot4 = RangeBreakoutDropSignalSnapshot {
        breakout_confirmed: false,
        ..snapshot1.clone()
    };

    let decision4 = RangeBreakoutDropStrategy::evaluate(&thresholds, &snapshot4);
    println!("测试4 - 未突破:");
    println!("  动作: {:?}", decision4.action);
    println!("  原因: {:?}\n", decision4.reasons);

    println!("================================");
}
