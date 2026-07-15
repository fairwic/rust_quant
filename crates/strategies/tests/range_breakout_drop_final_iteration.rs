/// 震荡突破下跌策略 - 修复后的完整迭代测试
///
/// 生成足够长的数据（>500根）以通过回测框架的预热期检查
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[test]
#[ignore]
fn range_breakout_drop_fixed_iteration() {
    println!("\n========== 震荡突破下跌策略 - 完整迭代（修复版）==========\n");

    // 生成足够长的数据（需要>500根才能通过预热期）
    let candles = generate_long_range_breakout_scenario();
    println!("生成了 {} 根K线", candles.len());

    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    println!("\n【第1轮】默认参数");
    let result1 = RangeBreakoutDropStrategy.run_test("BTC-USDT-SWAP", &candles, risk);
    print_result("默认", &result1);

    if result1.trade_records.is_empty() {
        println!("\n【第2轮】放宽震荡识别");
        let mut tuning2 = RangeBreakoutDropBacktestTuning::default();
        tuning2.max_range_volatility_pct = 4.0;
        tuning2.min_range_volatility_pct = 0.3;
        let result2 = RangeBreakoutDropStrategy.run_test_with_tuning(
            "BTC-USDT-SWAP",
            &candles,
            risk,
            tuning2,
        );
        print_result("放宽震荡", &result2);

        if result2.trade_records.is_empty() {
            println!("\n【第3轮】降低突破要求");
            let mut tuning3 = tuning2;
            tuning3.min_breakout_body_ratio = 0.4;
            tuning3.min_breakout_move_atr = 0.5;
            tuning3.min_breakout_volume_mult = 1.2;
            let result3 = RangeBreakoutDropStrategy.run_test_with_tuning(
                "BTC-USDT-SWAP",
                &candles,
                risk,
                tuning3,
            );
            print_result("降低突破", &result3);

            if result3.trade_records.is_empty() {
                println!("\n【第4轮】关闭EMA过滤");
                let mut tuning4 = tuning3;
                tuning4.require_bearish_ema = false;
                tuning4.rsi_min_before_drop = 25.0;
                let result4 = RangeBreakoutDropStrategy.run_test_with_tuning(
                    "BTC-USDT-SWAP",
                    &candles,
                    risk,
                    tuning4,
                );
                print_result("关闭EMA", &result4);

                if result4.trade_records.len() > 0 {
                    println!("\n✅ 第4轮成功产生交易！");
                    analyze_profitability(&result4, &tuning4);
                } else {
                    println!("\n⚠️  仍未产生交易，检查过滤原因");
                    if result4.filtered_signals.len() > 0 {
                        analyze_filters(&result4);
                    }
                }
            } else {
                println!("\n✅ 第3轮成功产生交易！");
                analyze_profitability(&result3, &tuning3);
            }
        } else {
            println!("\n✅ 第2轮成功产生交易！");
            analyze_profitability(&result2, &tuning2);
        }
    } else {
        println!("\n✅ 默认参数成功产生交易！");
        analyze_profitability(&result1, &RangeBreakoutDropBacktestTuning::default());
    }
}

fn print_result(label: &str, result: &rust_quant_strategies::BackTestResult) {
    let winning = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let pnl = result.funds - 100.0;

    println!(
        "  [{}] 交易:{} 胜率:{:.1}% 盈亏:{:.2}% 过滤:{}",
        label,
        result.trade_records.len(),
        result.win_rate,
        pnl,
        result.filtered_signals.len()
    );
}

fn analyze_profitability(
    result: &rust_quant_strategies::BackTestResult,
    tuning: &RangeBreakoutDropBacktestTuning,
) {
    let pnl = result.funds - 100.0;
    let is_profitable = pnl > 5.0 && result.win_rate > 50.0;

    println!("\n【盈利性分析】");
    println!("  总盈亏: {:.2}%", pnl);
    println!("  胜率: {:.1}%", result.win_rate);
    println!("  交易数: {}", result.trade_records.len());
    println!(
        "  是否盈利: {}",
        if is_profitable { "✅ 是" } else { "❌ 否" }
    );

    if result.trade_records.len() > 0 {
        let avg_win: f64 = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .map(|t| t.profit_loss)
            .sum::<f64>()
            / result
                .trade_records
                .iter()
                .filter(|t| t.profit_loss > 0.0)
                .count()
                .max(1) as f64;

        let avg_loss: f64 = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .map(|t| t.profit_loss.abs())
            .sum::<f64>()
            / result
                .trade_records
                .iter()
                .filter(|t| t.profit_loss < 0.0)
                .count()
                .max(1) as f64;

        println!("  平均盈利: {:.2}", avg_win);
        println!("  平均亏损: {:.2}", avg_loss);
        if avg_loss > 0.0 {
            println!("  盈亏比: {:.2}", avg_win / avg_loss);
        }
    }

    println!("\n【最终配置】");
    println!("  震荡窗口: {}", tuning.range_lookback_candles);
    println!(
        "  波动范围: {:.1}%-{:.1}%",
        tuning.min_range_volatility_pct, tuning.max_range_volatility_pct
    );
    println!(
        "  突破要求: body≥{:.2} move≥{:.2}ATR vol≥{:.1}x",
        tuning.min_breakout_body_ratio,
        tuning.min_breakout_move_atr,
        tuning.min_breakout_volume_mult
    );
    println!("  EMA过滤: {}", tuning.require_bearish_ema);
    println!(
        "  止损: {:.1}ATR 止盈: {:.1}R/{:.1}R/{:.1}R",
        tuning.stop_atr_mult, tuning.target_r_1, tuning.target_r_2, tuning.target_r_3
    );
}

fn analyze_filters(result: &rust_quant_strategies::BackTestResult) {
    use std::collections::HashMap;

    let mut reason_counts: HashMap<String, usize> = HashMap::new();
    for signal in &result.filtered_signals {
        for reason in &signal.filter_reasons {
            *reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
    }

    println!("\n【过滤原因统计】(前10)");
    let mut sorted: Vec<_> = reason_counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in sorted.iter().take(10) {
        println!("  {}: {}", reason, count);
    }
}

/// 生成足够长的震荡突破场景（>500根K线）
fn generate_long_range_breakout_scenario() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let base_price = 50000.0;

    // 阶段1: 预热期（400根）- 随机波动
    let mut price = base_price;
    for i in 0..400 {
        let noise = ((i % 11) as f64 - 5.0) * 20.0;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 100.0,
            l: price - 100.0,
            c: price + noise,
            v: 1000.0 + (i % 50) as f64 * 10.0,
            confirm: 1,
        });
        price += noise * 0.05;
        ts += 300_000;
    }

    // 阶段2: 震荡区间1（30根）
    let range_center = price;
    let range_width = price * 0.012; // 约2.4%波动
    for i in 0..30 {
        let phase = (i as f64 / 30.0) * std::f64::consts::PI * 2.0;
        let p = range_center + range_width * phase.sin();

        candles.push(CandleItem {
            ts,
            o: p,
            h: p + 60.0,
            l: p - 60.0,
            c: p + ((i % 3) as f64 - 1.0) * 30.0,
            v: 1200.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 阶段3: 突破下跌1（15根强势阴线）
    price = range_center - range_width - 100.0;
    for i in 0..15 {
        let drop = price * 0.006;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 30.0,
            l: price - drop - 40.0,
            c: price - drop,
            v: 2500.0 + (i as f64 * 120.0), // 明显放量
            confirm: 1,
        });
        price -= drop;
        ts += 300_000;
    }

    // 阶段4: 下跌延续（10根）
    for _ in 0..10 {
        let drop = price * 0.003;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 20.0,
            l: price - drop - 20.0,
            c: price - drop,
            v: 1800.0,
            confirm: 1,
        });
        price -= drop;
        ts += 300_000;
    }

    // 阶段5: 小幅反弹（10根）
    for i in 0..10 {
        let bounce = price * 0.002;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + bounce + 30.0,
            l: price - 20.0,
            c: price + bounce,
            v: 1300.0,
            confirm: 1,
        });
        price += bounce;
        ts += 300_000;
    }

    // 阶段6: 震荡区间2（25根）
    let range_center2 = price;
    let range_width2 = price * 0.013;
    for i in 0..25 {
        let phase = (i as f64 / 25.0) * std::f64::consts::PI * 2.0;
        let p = range_center2 + range_width2 * phase.sin();

        candles.push(CandleItem {
            ts,
            o: p,
            h: p + 55.0,
            l: p - 55.0,
            c: p,
            v: 1250.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 阶段7: 突破下跌2（12根）
    price = range_center2 - range_width2 - 80.0;
    for i in 0..12 {
        let drop = price * 0.005;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 25.0,
            l: price - drop - 35.0,
            c: price - drop,
            v: 2300.0 + (i as f64 * 100.0),
            confirm: 1,
        });
        price -= drop;
        ts += 300_000;
    }

    // 阶段8: 填充到足够长度
    while candles.len() < 520 {
        let noise = ((candles.len() % 7) as f64 - 3.0) * 15.0;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 40.0,
            l: price - 40.0,
            c: price + noise,
            v: 1100.0,
            confirm: 1,
        });
        price += noise * 0.1;
        ts += 300_000;
    }

    candles
}
