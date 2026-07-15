/// 深度调试测试 - 逐步检查策略执行流程
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[test]
#[ignore]
fn debug_strategy_execution_step_by_step() {
    println!("\n========== 深度调试 - 逐步检查 ==========\n");

    // 生成简单场景
    let candles = generate_simple_breakout();
    println!("生成了 {} 根K线", candles.len());

    // 检查前几根和后几根K线
    println!("\n前5根K线:");
    for (i, c) in candles.iter().take(5).enumerate() {
        println!(
            "  #{}: O={:.2} H={:.2} L={:.2} C={:.2} V={:.2}",
            i, c.o, c.h, c.l, c.c, c.v
        );
    }

    println!("\n后5根K线:");
    for (i, c) in candles.iter().skip(candles.len() - 5).enumerate() {
        let idx = candles.len() - 5 + i;
        println!(
            "  #{}: O={:.2} H={:.2} L={:.2} C={:.2} V={:.2}",
            idx, c.o, c.h, c.l, c.c, c.v
        );
    }

    // 运行回测
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let tuning = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 15,
        max_range_volatility_pct: 5.0,
        min_range_volatility_pct: 0.2,
        min_breakout_body_ratio: 0.3,
        min_breakout_move_atr: 0.3,
        min_breakout_volume_mult: 1.0,
        require_bearish_ema: false,
        slow_ema_period: 20,
        long_term_ema_period: 200,
        require_below_long_term_ema: false,
        stop_atr_mult: 1.5,
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.5,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 20.0,
        cooldown_candles: 2,
        allow_short: true,
    };

    println!("\n使用超宽松参数:");
    println!("  震荡窗口: {}", tuning.range_lookback_candles);
    println!("  最大波动: {}%", tuning.max_range_volatility_pct);
    println!("  最小实体: {}", tuning.min_breakout_body_ratio);
    println!("  最小移动: {} ATR", tuning.min_breakout_move_atr);
    println!("  冷却期: {} 根", tuning.cooldown_candles);

    println!("\n开始回测...");
    let result =
        RangeBreakoutDropStrategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning);

    println!("\n回测结果:");
    println!("  交易数: {}", result.trade_records.len());
    println!("  被过滤: {}", result.filtered_signals.len());
    println!("  未平仓: {}", result.open_trades);
    println!("  最终资金: {:.2}", result.funds);

    if result.trade_records.len() > 0 {
        println!("\n交易记录:");
        for (i, trade) in result.trade_records.iter().enumerate() {
            println!(
                "  #{}: {} @ {:.2} | 盈亏 {:.2}",
                i + 1,
                trade.option_type,
                trade.open_price,
                trade.profit_loss
            );
        }
    }

    if result.filtered_signals.len() > 0 {
        println!("\n过滤信号:");
        for (i, signal) in result.filtered_signals.iter().take(5).enumerate() {
            println!(
                "  #{}: 价格 {:.2} | 原因: {:?}",
                i + 1,
                signal.signal_price,
                signal.filter_reasons
            );
        }
    }

    if result.trade_records.is_empty() && result.filtered_signals.is_empty() {
        println!("\n❌ 没有产生任何信号或过滤记录！");
        println!("这表明 generate_signal 方法可能在早期就返回了空信号。");
        println!("\n可能原因:");
        println!("  1. 数据长度不足 min_data_length()");
        println!("  2. snapshot() 返回 None");
        println!("  3. 冷却期一直在消耗");
        println!("  4. 回测框架的 min_data_length 检查");
    }

    // 手动检查最小数据长度要求
    let min_len = tuning
        .range_lookback_candles
        .max(tuning.slow_ema_period)
        .max(tuning.atr_period + 1)
        .max(tuning.rsi_period + 1);
    println!("\n最小数据长度要求: {}", min_len);
    println!("实际数据长度: {}", candles.len());
    println!("是否满足: {}", candles.len() >= min_len);
}

fn generate_simple_breakout() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let base = 50000.0;

    // 生成足够多的预热数据 (60根)
    for i in 0..60 {
        candles.push(CandleItem {
            ts,
            o: base,
            h: base + 50.0,
            l: base - 50.0,
            c: base + ((i % 5) as f64 - 2.0) * 10.0,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 明显的震荡区间 (20根)
    let range_high = base + 300.0;
    let range_low = base - 300.0;
    for i in 0..20 {
        let price = if i % 4 == 0 {
            range_high - 50.0
        } else if i % 4 == 2 {
            range_low + 50.0
        } else {
            (range_high + range_low) / 2.0
        };

        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 40.0,
            l: price - 40.0,
            c: price,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 明显突破下跌 (10根大阴线)
    let mut price = range_low - 100.0;
    for i in 0..10 {
        let drop = 200.0;
        let open = price;
        let close = price - drop;

        candles.push(CandleItem {
            ts,
            o: open,
            h: open + 20.0,
            l: close - 20.0,
            c: close,
            v: 2000.0 + (i as f64 * 100.0), // 明显放量
            confirm: 1,
        });

        price = close;
        ts += 300_000;
    }

    candles
}
