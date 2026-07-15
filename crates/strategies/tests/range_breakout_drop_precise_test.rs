/// 震荡突破下跌策略 - 使用精确构造的数据进行回测验证
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[test]
#[ignore]
fn range_breakout_drop_working_example() {
    println!("\n========== 震荡突破下跌 - 工作示例 ==========\n");

    // 使用精确构造的符合策略条件的数据
    let candles = build_perfect_range_breakout_setup();

    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result = RangeBreakoutDropStrategy.run_test("BTC-USDT-SWAP", &candles, risk);

    println!("数据长度: {} 根K线", candles.len());
    println!("交易数: {}", result.trade_records.len());
    println!("胜率: {:.1}%", result.win_rate);
    println!("最终资金: {:.2}", result.funds);
    println!("总盈亏: {:.2}%", result.funds - 100.0);
    println!("被过滤: {}", result.filtered_signals.len());

    // 分析过滤原因
    if result.filtered_signals.len() > 0 {
        use std::collections::HashMap;
        let mut reason_counts: HashMap<String, usize> = HashMap::new();
        for signal in &result.filtered_signals {
            for reason in &signal.filter_reasons {
                *reason_counts.entry(reason.clone()).or_insert(0) += 1;
            }
        }

        println!("\n过滤原因统计:");
        let mut sorted: Vec<_> = reason_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in sorted.iter().take(10) {
            println!("  {}: {}", reason, count);
        }
        println!();
    }

    assert!(result.trade_records.len() > 0, "应该产生至少一笔交易");

    if result.trade_records.len() > 0 {
        println!("\n✅ 策略成功产生交易！\n");

        for (i, trade) in result.trade_records.iter().enumerate() {
            println!("交易 #{}", i + 1);
            println!("  方向: {}", trade.option_type);
            println!("  开仓价: {:.2}", trade.open_price);
            println!("  平仓价: {:.2}", trade.close_price.unwrap_or(0.0));
            println!("  盈亏: {:.2}", trade.profit_loss);
            println!();
        }

        // 评估盈利性
        let winning = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .count();
        let total_pnl = result.funds - 100.0;

        println!("【策略评估】");
        println!("盈利交易: {}/{}", winning, result.trade_records.len());

        if total_pnl > 0.0 && result.win_rate > 50.0 {
            println!("✅ 策略在测试数据上盈利！");
            println!("下一步: 在更多市场环境下测试");
        } else if total_pnl > 0.0 {
            println!("⚠️  盈利但胜率较低，需要优化入场条件");
        } else {
            println!("❌ 策略亏损，需要调整参数或逻辑");
        }
    }
}

/// 构造完美的震荡突破设置
///
/// 关键：精确计算ATR，确保突破K线满足所有条件
fn build_perfect_range_breakout_setup() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let base_price = 50000.0;

    // 预热期 (400根) - 用于EMA/ATR/RSI计算
    for i in 0..400 {
        let price = base_price + ((i % 20) as f64 - 10.0) * 50.0;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 100.0,
            l: price - 100.0,
            c: price + ((i % 5) as f64 - 2.0) * 20.0,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 开始构造震荡区间（30根K线）
    // 目标：波动率在 0.5% - 3.0% 之间
    let range_center = 50000.0;
    let range_half_width = range_center * 0.01; // 1%上下，总波动2%
    let range_high = range_center + range_half_width;
    let range_low = range_center - range_half_width;

    // 震荡K线 - 在range内来回波动
    for i in 0..30 {
        let phase = (i as f64 / 30.0) * std::f64::consts::PI * 2.0;
        let price_in_range = range_center + range_half_width * 0.8 * phase.sin();

        candles.push(CandleItem {
            ts,
            o: price_in_range,
            h: price_in_range + 50.0,
            l: price_in_range - 50.0,
            c: price_in_range + ((i % 3) as f64 - 1.0) * 20.0,
            v: 1100.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 计算预期的ATR（基于震荡期的波动）
    // 简单估计：ATR ≈ 100（因为震荡期H-L约100）
    let estimated_atr = 100.0;

    // 现在构造突破K线，精确满足所有条件：
    // 1. close < range_low ✓
    // 2. 阴线 (close < open) ✓
    // 3. 实体比例 ≥ 0.55 ✓
    // 4. 突破移动 ≥ 0.8 ATR ✓
    // 5. 成交量 ≥ 1.5倍 ✓

    let breakout_open = range_low + 50.0; // 从range边界附近开始
    let breakout_move = estimated_atr * 1.0; // 移动1.0 ATR，确保满足0.8阈值
    let breakout_close = range_low - breakout_move; // 突破下边界

    // 计算实体和影线
    let body_size = breakout_open - breakout_close; // 阴线：open > close
    let total_range = body_size / 0.6; // 实体占60%，满足≥55%
    let upper_shadow = total_range * 0.2;
    let lower_shadow = total_range * 0.2;

    candles.push(CandleItem {
        ts,
        o: breakout_open,
        h: breakout_open + upper_shadow,
        l: breakout_close - lower_shadow,
        c: breakout_close,
        v: 1100.0 * 2.0, // 2倍成交量，满足≥1.5倍
        confirm: 1,
    });
    ts += 300_000;

    // 继续下跌（10根）- 让止盈有机会触发
    let mut current_price = breakout_close;
    for _ in 0..10 {
        let drop = current_price * 0.005; // 每根跌0.5%
        candles.push(CandleItem {
            ts,
            o: current_price,
            h: current_price + 30.0,
            l: current_price - drop - 20.0,
            c: current_price - drop,
            v: 1500.0,
            confirm: 1,
        });
        current_price -= drop;
        ts += 300_000;
    }

    // 填充到超过500根
    while candles.len() < 550 {
        let noise = ((candles.len() % 7) as f64 - 3.0) * 20.0;
        candles.push(CandleItem {
            ts,
            o: current_price,
            h: current_price + 60.0,
            l: current_price - 60.0,
            c: current_price + noise,
            v: 1200.0,
            confirm: 1,
        });
        current_price += noise * 0.1;
        ts += 300_000;
    }

    candles
}

#[test]
#[ignore]
fn range_breakout_drop_multiple_scenarios() {
    println!("\n========== 多场景测试 ==========\n");

    let scenarios = vec![
        ("单次突破", build_perfect_range_breakout_setup()),
        ("连续突破", build_multiple_breakouts()),
    ];

    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    for (name, candles) in scenarios {
        println!("场景: {}", name);
        let result = RangeBreakoutDropStrategy.run_test("BTC-USDT-SWAP", &candles, risk);

        let pnl = result.funds - 100.0;
        println!(
            "  交易: {} | 胜率: {:.1}% | 盈亏: {:.2}%",
            result.trade_records.len(),
            result.win_rate,
            pnl
        );

        if pnl > 0.0 {
            println!("  ✅ 盈利");
        } else {
            println!("  ❌ 亏损");
        }
        println!();
    }
}

fn build_multiple_breakouts() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let mut price = 50000.0;

    // 预热
    for i in 0..400 {
        let p = price + ((i % 20) as f64 - 10.0) * 50.0;
        candles.push(CandleItem {
            ts,
            o: p,
            h: p + 100.0,
            l: p - 100.0,
            c: p,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 创建2个震荡-突破周期
    for cycle in 0..2 {
        // 震荡
        let range_center = price;
        let range_hw = price * 0.01;

        for i in 0..25 {
            let phase = (i as f64 / 25.0) * std::f64::consts::PI * 2.0;
            let p = range_center + range_hw * 0.8 * phase.sin();

            candles.push(CandleItem {
                ts,
                o: p,
                h: p + 50.0,
                l: p - 50.0,
                c: p,
                v: 1100.0,
                confirm: 1,
            });
            ts += 300_000;
        }

        // 突破
        let open = range_center - range_hw + 50.0;
        let close = range_center - range_hw - 100.0;
        let body = open - close;
        let total = body / 0.6;

        candles.push(CandleItem {
            ts,
            o: open,
            h: open + total * 0.2,
            l: close - total * 0.2,
            c: close,
            v: 1100.0 * 2.0,
            confirm: 1,
        });
        ts += 300_000;

        // 下跌
        price = close;
        for _ in 0..8 {
            let drop = price * 0.004;
            candles.push(CandleItem {
                ts,
                o: price,
                h: price + 30.0,
                l: price - drop - 20.0,
                c: price - drop,
                v: 1400.0,
                confirm: 1,
            });
            price -= drop;
            ts += 300_000;
        }

        // 小反弹
        for _ in 0..5 {
            let bounce = price * 0.002;
            candles.push(CandleItem {
                ts,
                o: price,
                h: price + bounce + 20.0,
                l: price - 10.0,
                c: price + bounce,
                v: 1100.0,
                confirm: 1,
            });
            price += bounce;
            ts += 300_000;
        }
    }

    // 填充
    while candles.len() < 550 {
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 60.0,
            l: price - 60.0,
            c: price,
            v: 1200.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    candles
}
