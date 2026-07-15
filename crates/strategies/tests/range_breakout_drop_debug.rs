/// 震荡结束突破下跌策略 - 调试测试
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy, RangeBreakoutDropThresholds,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[test]
#[ignore]
fn range_breakout_drop_debug_test() {
    let candles = generate_test_candles();

    println!("\n========== 策略调试测试 ==========");
    println!("生成了 {} 根K线", candles.len());

    // 手动测试策略逻辑
    let tuning = RangeBreakoutDropBacktestTuning::default();
    let thresholds = tuning.thresholds();

    println!("\n策略参数:");
    println!("  震荡识别窗口: {} K线", thresholds.range_lookback_candles);
    println!("  最大波动率: {:.1}%", thresholds.max_range_volatility_pct);
    println!("  最小波动率: {:.1}%", thresholds.min_range_volatility_pct);
    println!(
        "  最小突破实体比例: {:.2}",
        thresholds.min_breakout_body_ratio
    );
    println!("  最小突破移动ATR: {:.2}", thresholds.min_breakout_move_atr);
    println!(
        "  最小突破成交量倍数: {:.2}",
        thresholds.min_breakout_volume_mult
    );

    // 运行回测
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let strategy = RangeBreakoutDropStrategy;
    let result = strategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning);

    println!("\n回测结果:");
    println!("  总交易: {}", result.trade_records.len());
    println!("  被过滤信号: {}", result.filtered_signals.len());
    println!("  胜率: {:.2}%", result.win_rate);
    println!("  最终资金: {:.2}", result.funds);

    if result.filtered_signals.len() > 0 {
        println!("\n被过滤的信号（前10个）:");
        for (i, signal) in result.filtered_signals.iter().take(10).enumerate() {
            println!(
                "  信号 #{}: 价格 {:.2}, 原因: {:?}",
                i + 1,
                signal.signal_price,
                signal.filter_reasons
            );
        }
    }

    if result.trade_records.len() > 0 {
        println!("\n交易记录:");
        for (i, trade) in result.trade_records.iter().enumerate() {
            println!(
                "  交易 #{}: {} | 开仓 {:.2} | 平仓 {:.2} | 盈亏 {:.2}",
                i + 1,
                trade.option_type,
                trade.open_price,
                trade.close_price.unwrap_or(0.0),
                trade.profit_loss
            );
        }
    }
}

/// 生成更大规模的测试数据（包含多个震荡-突破周期）
fn generate_test_candles() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;

    // 预热期：生成足够的历史数据供指标计算（100根K线）
    let mut price = 50000.0;
    for i in 0..100 {
        let noise = ((i % 7) as f64 - 3.0) * 10.0;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 50.0 + noise.abs(),
            l: price - 50.0 - noise.abs(),
            c: price + noise,
            v: 1000.0 + (i as f64 * 5.0),
            confirm: 1,
        });
        price += noise * 0.1;
        ts += 300_000;
    }

    // 第一个震荡-突破周期
    let range_center = price;
    let range_high = range_center * 1.012; // 震荡幅度 2.4%
    let range_low = range_center * 0.988;

    // 震荡期（25根K线）
    for i in 0..25 {
        let phase = (i as f64 / 25.0) * std::f64::consts::PI * 2.0;
        let range_price = range_low + (range_high - range_low) * ((phase.sin() + 1.0) / 2.0);

        candles.push(CandleItem {
            ts,
            o: range_price,
            h: range_price + 30.0,
            l: range_price - 30.0,
            c: range_price + ((i % 3) as f64 - 1.0) * 15.0,
            v: 1200.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 突破下跌（10根阴线，带放量）
    let mut current_price = range_low - 50.0; // 突破起点
    for i in 0..10 {
        let drop = current_price * 0.005; // 每根下跌0.5%
        let open = current_price;
        let close = current_price - drop;

        candles.push(CandleItem {
            ts,
            o: open,
            h: open + 20.0,
            l: close - 10.0,
            c: close,
            v: 2000.0 + (i as f64 * 100.0), // 放量
            confirm: 1,
        });

        current_price = close;
        ts += 300_000;
    }

    // 继续下跌（5根K线）
    for i in 0..5 {
        let drop = current_price * 0.003;
        let open = current_price;
        let close = current_price - drop;

        candles.push(CandleItem {
            ts,
            o: open,
            h: open + 15.0,
            l: close - 5.0,
            c: close,
            v: 1800.0,
            confirm: 1,
        });

        current_price = close;
        ts += 300_000;
    }

    candles
}

#[test]
#[ignore]
fn range_breakout_drop_relaxed_params_test() {
    let candles = generate_test_candles();

    println!("\n========== 宽松参数测试 ==========");

    // 使用更宽松的参数
    let mut tuning = RangeBreakoutDropBacktestTuning::default();
    tuning.range_lookback_candles = 20;
    tuning.max_range_volatility_pct = 4.0; // 放宽震荡判定
    tuning.min_range_volatility_pct = 0.3;
    tuning.min_breakout_body_ratio = 0.4; // 降低实体要求
    tuning.min_breakout_move_atr = 0.5; // 降低突破幅度要求
    tuning.min_breakout_volume_mult = 1.2; // 降低成交量要求
    tuning.require_bearish_ema = false; // 关闭EMA过滤
    tuning.rsi_min_before_drop = 30.0; // 降低RSI要求

    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 3.0,
        ..BasicRiskStrategyConfig::default()
    };

    let strategy = RangeBreakoutDropStrategy;
    let result = strategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning);

    println!("总交易: {}", result.trade_records.len());
    println!("被过滤: {}", result.filtered_signals.len());
    println!("胜率: {:.2}%", result.win_rate);
    println!("最终资金: {:.2}", result.funds);

    if result.trade_records.len() > 0 {
        println!("\n✅ 宽松参数下产生了交易信号");
        for (i, trade) in result.trade_records.iter().take(3).enumerate() {
            println!(
                "  交易 #{}: {} | 盈亏 {:.2}",
                i + 1,
                trade.option_type,
                trade.profit_loss
            );
        }
    }

    if result.filtered_signals.len() > 0 {
        println!("\n被过滤的信号:");
        for (i, signal) in result.filtered_signals.iter().take(5).enumerate() {
            println!("  #{}: 原因 {:?}", i + 1, signal.filter_reasons);
        }
    }
}
