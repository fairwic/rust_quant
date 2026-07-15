/// 震荡突破下跌策略 - 完整迭代测试
///
/// 使用更真实的模拟数据进行策略迭代和优化
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[test]
#[ignore]
fn range_breakout_drop_full_iteration() {
    println!("\n========== 震荡突破下跌策略 - 完整迭代优化 ==========\n");

    // 生成多个市场场景的数据
    let scenarios = vec![
        ("震荡后突破下跌", generate_range_breakout_scenario()),
        ("连续震荡多周期", generate_multiple_range_scenario()),
        ("震荡+假突破+真突破", generate_fake_and_real_breakout()),
    ];

    for (name, candles) in &scenarios {
        println!("场景: {}", name);
        println!("K线数量: {}", candles.len());

        // 第一轮：测试默认参数
        println!("\n【第1轮】默认参数测试");
        let result1 = test_with_params(candles, RangeBreakoutDropBacktestTuning::default());
        print_iteration_result(&result1, 1);

        if !result1.is_profitable {
            println!("\n【第2轮】放宽震荡识别");
            let mut tuning2 = RangeBreakoutDropBacktestTuning::default();
            tuning2.max_range_volatility_pct = 4.0;
            tuning2.min_range_volatility_pct = 0.3;
            let result2 = test_with_params(candles, tuning2);
            print_iteration_result(&result2, 2);

            if !result2.is_profitable {
                println!("\n【第3轮】降低突破确认要求");
                let mut tuning3 = tuning2;
                tuning3.min_breakout_body_ratio = 0.45;
                tuning3.min_breakout_move_atr = 0.6;
                tuning3.min_breakout_volume_mult = 1.3;
                let result3 = test_with_params(candles, tuning3);
                print_iteration_result(&result3, 3);

                if !result3.is_profitable {
                    println!("\n【第4轮】关闭趋势过滤");
                    let mut tuning4 = tuning3;
                    tuning4.require_bearish_ema = false;
                    tuning4.rsi_min_before_drop = 25.0;
                    let result4 = test_with_params(candles, tuning4);
                    print_iteration_result(&result4, 4);

                    if !result4.is_profitable {
                        println!("\n【第5轮】调整止损止盈比例");
                        let mut tuning5 = tuning4;
                        tuning5.stop_atr_mult = 1.2;
                        tuning5.target_r_1 = 1.2;
                        tuning5.target_r_2 = 2.5;
                        tuning5.target_r_3 = 4.0;
                        let result5 = test_with_params(candles, tuning5);
                        print_iteration_result(&result5, 5);

                        if result5.is_profitable {
                            println!("\n✅ 找到盈利配置！");
                            print_final_config(&tuning5);
                        } else {
                            println!("\n⚠️  该场景下策略难以盈利，需要重新设计或换市场环境");
                        }
                    } else {
                        println!("\n✅ 第4轮迭代成功！");
                        print_final_config(&tuning4);
                    }
                } else {
                    println!("\n✅ 第3轮迭代成功！");
                    print_final_config(&tuning3);
                }
            } else {
                println!("\n✅ 第2轮迭代成功！");
                print_final_config(&tuning2);
            }
        } else {
            println!("\n✅ 默认参数已盈利！");
        }

        println!("\n{}", "=".repeat(60));
    }

    // 参数扫描寻找最优配置
    println!("\n【参数扫描】寻找最优配置");
    let best_result = parameter_sweep(&scenarios[0].1);
    if let Some((config, result)) = best_result {
        println!("\n🎯 最优配置:");
        print_final_config(&config);
        println!("\n最优结果:");
        println!("  总盈亏: {:.2}%", result.pnl_percent);
        println!("  胜率: {:.1}%", result.win_rate);
        println!("  交易次数: {}", result.trades);
    }
}

struct IterationResult {
    trades: usize,
    win_rate: f64,
    pnl_percent: f64,
    is_profitable: bool,
    filtered: usize,
}

fn test_with_params(
    candles: &[CandleItem],
    tuning: RangeBreakoutDropBacktestTuning,
) -> IterationResult {
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result =
        RangeBreakoutDropStrategy.run_test_with_tuning("BTC-USDT-SWAP", candles, risk, tuning);

    let winning = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let win_rate = if result.trade_records.len() > 0 {
        (winning as f64 / result.trade_records.len() as f64) * 100.0
    } else {
        0.0
    };

    let pnl_percent = (result.funds - 100.0) / 100.0 * 100.0;

    IterationResult {
        trades: result.trade_records.len(),
        win_rate,
        pnl_percent,
        is_profitable: pnl_percent > 5.0 && win_rate > 50.0,
        filtered: result.filtered_signals.len(),
    }
}

fn print_iteration_result(result: &IterationResult, round: usize) {
    println!("  轮次 #{}", round);
    println!("    交易数: {}", result.trades);
    println!("    胜率: {:.1}%", result.win_rate);
    println!("    总盈亏: {:.2}%", result.pnl_percent);
    println!("    被过滤: {}", result.filtered);
    println!(
        "    盈利?: {}",
        if result.is_profitable { "✅" } else { "❌" }
    );
}

fn print_final_config(tuning: &RangeBreakoutDropBacktestTuning) {
    println!("  震荡窗口: {}", tuning.range_lookback_candles);
    println!("  最大波动: {:.1}%", tuning.max_range_volatility_pct);
    println!("  最小波动: {:.1}%", tuning.min_range_volatility_pct);
    println!("  最小实体: {:.2}", tuning.min_breakout_body_ratio);
    println!("  最小移动: {:.2} ATR", tuning.min_breakout_move_atr);
    println!("  最小成交量: {:.1}x", tuning.min_breakout_volume_mult);
    println!("  EMA过滤: {}", tuning.require_bearish_ema);
    println!("  RSI阈值: {:.0}", tuning.rsi_min_before_drop);
    println!("  止损: {:.1} ATR", tuning.stop_atr_mult);
    println!(
        "  止盈: {:.1}R / {:.1}R / {:.1}R",
        tuning.target_r_1, tuning.target_r_2, tuning.target_r_3
    );
}

fn parameter_sweep(
    candles: &[CandleItem],
) -> Option<(RangeBreakoutDropBacktestTuning, IterationResult)> {
    let mut best_config = None;
    let mut best_pnl = f64::NEG_INFINITY;

    let configs = vec![
        (20, 3.0, 0.55, 0.8, 1.5),
        (20, 3.5, 0.50, 0.7, 1.4),
        (25, 3.0, 0.55, 0.8, 1.5),
        (20, 4.0, 0.50, 0.6, 1.3),
        (15, 3.5, 0.55, 0.8, 1.5),
    ];

    for (lookback, max_vol, body, move_atr, vol_mult) in configs {
        let mut tuning = RangeBreakoutDropBacktestTuning::default();
        tuning.range_lookback_candles = lookback;
        tuning.max_range_volatility_pct = max_vol;
        tuning.min_breakout_body_ratio = body;
        tuning.min_breakout_move_atr = move_atr;
        tuning.min_breakout_volume_mult = vol_mult;
        tuning.require_bearish_ema = false;

        let result = test_with_params(candles, tuning);

        if result.pnl_percent > best_pnl && result.trades >= 3 {
            best_pnl = result.pnl_percent;
            best_config = Some((tuning, result));
        }
    }

    best_config
}

/// 生成震荡后突破下跌场景
fn generate_range_breakout_scenario() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let base_price = 50000.0;

    // 预热期 (50根)
    for i in 0..50 {
        candles.push(CandleItem {
            ts,
            o: base_price,
            h: base_price + 100.0,
            l: base_price - 100.0,
            c: base_price + ((i % 7) as f64 - 3.0) * 20.0,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 震荡区间 (30根) - 波动率约2%
    let range_center = base_price;
    let range_width = base_price * 0.01; // 1%上下
    for i in 0..30 {
        let phase = (i as f64 / 30.0) * std::f64::consts::PI * 2.0;
        let price = range_center + range_width * phase.sin();

        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 40.0,
            l: price - 40.0,
            c: price + ((i % 3) as f64 - 1.0) * 20.0,
            v: 1200.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 突破下跌 (15根强势阴线)
    let mut current = range_center - range_width - 100.0;
    for i in 0..15 {
        let drop = current * 0.006; // 每根跌0.6%
        candles.push(CandleItem {
            ts,
            o: current,
            h: current + 20.0,
            l: current - drop - 30.0,
            c: current - drop,
            v: 2500.0 + (i as f64 * 100.0), // 放量
            confirm: 1,
        });
        current -= drop;
        ts += 300_000;
    }

    // 下跌延续 (10根)
    for _ in 0..10 {
        let drop = current * 0.003;
        candles.push(CandleItem {
            ts,
            o: current,
            h: current + 15.0,
            l: current - drop - 10.0,
            c: current - drop,
            v: 1800.0,
            confirm: 1,
        });
        current -= drop;
        ts += 300_000;
    }

    candles
}

/// 生成多个震荡周期
fn generate_multiple_range_scenario() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let mut price = 50000.0;

    // 预热
    for i in 0..50 {
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 100.0,
            l: price - 100.0,
            c: price + ((i % 5) as f64 - 2.0) * 30.0,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 3个震荡-突破周期
    for _ in 0..3 {
        // 震荡
        let range_center = price;
        let range_width = price * 0.012;
        for i in 0..25 {
            let phase = (i as f64 / 25.0) * std::f64::consts::PI * 2.0;
            let p = range_center + range_width * phase.sin();
            candles.push(CandleItem {
                ts,
                o: p,
                h: p + 50.0,
                l: p - 50.0,
                c: p + ((i % 3) as f64 - 1.0) * 25.0,
                v: 1200.0,
                confirm: 1,
            });
            ts += 300_000;
        }

        // 突破下跌
        for i in 0..10 {
            let drop = price * 0.005;
            candles.push(CandleItem {
                ts,
                o: price,
                h: price + 20.0,
                l: price - drop - 20.0,
                c: price - drop,
                v: 2200.0 + (i as f64 * 80.0),
                confirm: 1,
            });
            price -= drop;
            ts += 300_000;
        }
    }

    candles
}

/// 生成假突破和真突破场景
fn generate_fake_and_real_breakout() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let base_price = 50000.0;

    // 预热
    for i in 0..50 {
        candles.push(CandleItem {
            ts,
            o: base_price,
            h: base_price + 100.0,
            l: base_price - 100.0,
            c: base_price + ((i % 5) as f64 - 2.0) * 30.0,
            v: 1000.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 震荡
    let range_center = base_price;
    let range_width = base_price * 0.01;
    for i in 0..25 {
        let phase = (i as f64 / 25.0) * std::f64::consts::PI * 2.0;
        let price = range_center + range_width * phase.sin();
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 40.0,
            l: price - 40.0,
            c: price,
            v: 1200.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 假突破 (快速回抽)
    let mut price = range_center - range_width - 50.0;
    for i in 0..3 {
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 30.0,
            l: price - 100.0,
            c: price - 50.0 + (i as f64 * 40.0), // 最后回到区间内
            v: 1400.0,
            confirm: 1,
        });
        price = price - 50.0 + (i as f64 * 40.0);
        ts += 300_000;
    }

    // 继续震荡
    for i in 0..10 {
        let phase = (i as f64 / 10.0) * std::f64::consts::PI;
        let p = range_center + range_width * 0.5 * phase.sin();
        candles.push(CandleItem {
            ts,
            o: p,
            h: p + 40.0,
            l: p - 40.0,
            c: p,
            v: 1200.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 真突破 (持续下跌)
    price = range_center - range_width - 80.0;
    for i in 0..12 {
        let drop = price * 0.006;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 15.0,
            l: price - drop - 25.0,
            c: price - drop,
            v: 2400.0 + (i as f64 * 90.0),
            confirm: 1,
        });
        price -= drop;
        ts += 300_000;
    }

    candles
}
