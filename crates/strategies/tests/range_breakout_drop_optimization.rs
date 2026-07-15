/// 震荡突破下跌策略 - 最终迭代优化
///
/// 根据过滤原因反馈，逐步放宽参数直到产生交易，然后评估盈利性
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;
use std::collections::HashMap;

#[test]
#[ignore]
fn range_breakout_drop_final_optimization() {
    println!("\n========== 震荡突破下跌策略 - 最终优化 ==========\n");

    let candles = generate_realistic_scenario();
    println!("生成了 {} 根K线数据\n", candles.len());

    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    // 根据过滤原因，主要问题是：
    // 1. BREAKOUT_NOT_CONFIRMED - 需要降低突破确认要求
    // 2. BREAKOUT_VOLUME_TOO_LOW - 需要降低成交量要求
    // 3. BREAKOUT_MOVE_TOO_SMALL - 需要降低移动距离要求
    // 4. RANGE_TOO_VOLATILE - 需要提高波动容忍度
    // 5. NOT_IN_RANGING_MODE - 需要放宽震荡识别

    println!("【迭代1】极度宽松参数 - 确保能产生交易");
    let mut tuning1 = RangeBreakoutDropBacktestTuning::default();
    tuning1.max_range_volatility_pct = 8.0; // 大幅提高波动容忍
    tuning1.min_range_volatility_pct = 0.1; // 降低最小波动
    tuning1.min_breakout_body_ratio = 0.2; // 大幅降低实体要求
    tuning1.min_breakout_move_atr = 0.2; // 大幅降低移动要求
    tuning1.min_breakout_volume_mult = 0.8; // 甚至允许缩量
    tuning1.require_bearish_ema = false; // 关闭EMA过滤
    tuning1.rsi_min_before_drop = 15.0; // 降低RSI限制
    tuning1.cooldown_candles = 1; // 减少冷却期

    let result1 =
        RangeBreakoutDropStrategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning1);
    print_detailed_result("极度宽松", &result1, &tuning1);

    if result1.trade_records.len() > 0 {
        println!("\n✅ 成功产生交易！现在开始收紧参数优化策略\n");

        println!("【迭代2】适度收紧 - 提高质量");
        let mut tuning2 = tuning1;
        tuning2.max_range_volatility_pct = 5.0;
        tuning2.min_breakout_body_ratio = 0.35;
        tuning2.min_breakout_move_atr = 0.4;
        tuning2.min_breakout_volume_mult = 1.0;

        let result2 = RangeBreakoutDropStrategy.run_test_with_tuning(
            "BTC-USDT-SWAP",
            &candles,
            risk,
            tuning2,
        );
        print_detailed_result("适度收紧", &result2, &tuning2);

        println!("\n【迭代3】进一步优化 - 平衡频次与质量");
        let mut tuning3 = tuning2;
        tuning3.max_range_volatility_pct = 4.0;
        tuning3.min_breakout_body_ratio = 0.45;
        tuning3.min_breakout_move_atr = 0.5;
        tuning3.min_breakout_volume_mult = 1.2;
        tuning3.rsi_min_before_drop = 30.0;
        tuning3.cooldown_candles = 3;

        let result3 = RangeBreakoutDropStrategy.run_test_with_tuning(
            "BTC-USDT-SWAP",
            &candles,
            risk,
            tuning3,
        );
        print_detailed_result("平衡优化", &result3, &tuning3);

        // 比较三个版本
        println!("\n========== 优化结果对比 ==========");
        compare_results(&[
            ("极度宽松", &result1, &tuning1),
            ("适度收紧", &result2, &tuning2),
            ("平衡优化", &result3, &tuning3),
        ]);

        // 选择最优配置
        let best = select_best(&[(result1, tuning1), (result2, tuning2), (result3, tuning3)]);
        println!("\n🏆 推荐配置: {}", best.0);
        println!("原因: {}", best.1);
    } else {
        println!("\n❌ 即使极度宽松参数也未产生交易");
        println!("这说明数据生成逻辑或策略核心逻辑存在根本问题");
        analyze_why_no_trades(&result1);
    }
}

fn print_detailed_result(
    label: &str,
    result: &rust_quant_strategies::BackTestResult,
    tuning: &RangeBreakoutDropBacktestTuning,
) {
    let pnl = result.funds - 100.0;
    let winning = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let losing = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss < 0.0)
        .count();

    println!("  配置: {}", label);
    println!("    交易数: {}", result.trade_records.len());
    println!("    盈利/亏损: {}/{}", winning, losing);
    println!("    胜率: {:.1}%", result.win_rate);
    println!("    总盈亏: {:.2}%", pnl);
    println!("    被过滤: {}", result.filtered_signals.len());

    if result.trade_records.len() > 0 {
        let total_profit: f64 = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .map(|t| t.profit_loss)
            .sum();
        let total_loss: f64 = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .map(|t| t.profit_loss.abs())
            .sum();
        let avg_win = if winning > 0 {
            total_profit / winning as f64
        } else {
            0.0
        };
        let avg_loss = if losing > 0 {
            total_loss / losing as f64
        } else {
            0.0
        };

        println!("    平均盈利: {:.2}", avg_win);
        println!("    平均亏损: {:.2}", avg_loss);
        if avg_loss > 0.0 {
            println!("    盈亏比: {:.2}:1", avg_win / avg_loss);
        }
    }

    println!(
        "    参数: vol={:.1}% body={:.2} move={:.2}ATR vol_mult={:.1}x",
        tuning.max_range_volatility_pct,
        tuning.min_breakout_body_ratio,
        tuning.min_breakout_move_atr,
        tuning.min_breakout_volume_mult
    );
}

fn compare_results(
    results: &[(
        &str,
        &rust_quant_strategies::BackTestResult,
        &RangeBreakoutDropBacktestTuning,
    )],
) {
    println!(
        "\n  {:15} | {:6} | {:8} | {:10} | {:8}",
        "配置", "交易数", "胜率%", "总盈亏%", "被过滤"
    );
    println!("  {}", "-".repeat(70));

    for (label, result, _) in results {
        let pnl = result.funds - 100.0;
        println!(
            "  {:15} | {:6} | {:8.1} | {:10.2} | {:8}",
            label,
            result.trade_records.len(),
            result.win_rate,
            pnl,
            result.filtered_signals.len()
        );
    }
}

fn select_best(
    results: &[(
        rust_quant_strategies::BackTestResult,
        RangeBreakoutDropBacktestTuning,
    )],
) -> (String, String) {
    let mut best_idx = 0;
    let mut best_score = f64::NEG_INFINITY;

    for (i, (result, _)) in results.iter().enumerate() {
        if result.trade_records.len() < 3 {
            continue; // 交易数太少不考虑
        }

        let pnl = result.funds - 100.0;
        // 综合评分：盈亏 * 0.5 + 胜率 * 0.3 + log(交易数) * 10
        let score =
            pnl * 0.5 + result.win_rate * 0.3 + (result.trade_records.len() as f64).ln() * 10.0;

        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    let labels = vec!["极度宽松", "适度收紧", "平衡优化"];
    let (result, _) = &results[best_idx];

    let reason = if result.funds > 105.0 && result.win_rate > 55.0 {
        format!(
            "盈利性好（{}%盈亏，{:.1}%胜率）",
            result.funds - 100.0,
            result.win_rate
        )
    } else if result.trade_records.len() > 10 {
        format!("交易频次合理（{}笔交易）", result.trade_records.len())
    } else {
        "相对最优（但整体表现一般）".to_string()
    };

    (labels[best_idx].to_string(), reason)
}

fn analyze_why_no_trades(result: &rust_quant_strategies::BackTestResult) {
    let mut reason_counts: HashMap<String, usize> = HashMap::new();
    for signal in &result.filtered_signals {
        for reason in &signal.filter_reasons {
            *reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
    }

    println!("\n【根本原因分析】");
    let mut sorted: Vec<_> = reason_counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    for (reason, count) in sorted.iter().take(5) {
        println!("  {} 出现 {} 次", reason, count);
    }
}

/// 生成更真实的震荡突破场景
fn generate_realistic_scenario() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut ts = 1704067200000i64;
    let base_price = 50000.0;

    // 预热期 (400根)
    let mut price = base_price;
    for i in 0..400 {
        let noise = ((i % 13) as f64 - 6.0) * 25.0;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 120.0,
            l: price - 120.0,
            c: price + noise,
            v: 1000.0 + (i % 100) as f64 * 15.0,
            confirm: 1,
        });
        price += noise * 0.08;
        ts += 300_000;
    }

    // 震荡区间 (35根) - 更明显的震荡
    let range_center = price;
    let range_half_width = price * 0.008; // 0.8%上下，总共1.6%
    for i in 0..35 {
        let phase = (i as f64 / 35.0) * std::f64::consts::PI * 3.0; // 3个周期
        let p = range_center + range_half_width * phase.sin();

        candles.push(CandleItem {
            ts,
            o: p,
            h: p + range_half_width * 0.3,
            l: p - range_half_width * 0.3,
            c: p + ((i % 4) as f64 - 1.5) * 30.0,
            v: 1100.0 + (i % 10) as f64 * 20.0,
            confirm: 1,
        });
        ts += 300_000;
    }

    // 突破下跌 (20根强势阴线)
    price = range_center - range_half_width - 150.0;
    for i in 0..20 {
        let drop_pct = 0.008; // 每根跌0.8%
        let drop = price * drop_pct;
        let body_ratio = 0.7; // 70%实体

        let open = price;
        let close = price - drop;
        let range_size = drop / body_ratio;
        let upper_shadow = range_size * 0.15;
        let lower_shadow = range_size - drop - upper_shadow;

        candles.push(CandleItem {
            ts,
            o: open,
            h: open + upper_shadow,
            l: close - lower_shadow,
            c: close,
            v: 1500.0 + (i as f64 * 80.0), // 明显放量
            confirm: 1,
        });

        price = close;
        ts += 300_000;
    }

    // 继续下跌 (15根)
    for i in 0..15 {
        let drop = price * 0.004;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 40.0,
            l: price - drop - 50.0,
            c: price - drop,
            v: 1300.0 + (i as f64 * 30.0),
            confirm: 1,
        });
        price -= drop;
        ts += 300_000;
    }

    // 填充到520根
    while candles.len() < 520 {
        let noise = ((candles.len() % 9) as f64 - 4.0) * 20.0;
        candles.push(CandleItem {
            ts,
            o: price,
            h: price + 60.0,
            l: price - 60.0,
            c: price + noise,
            v: 1200.0,
            confirm: 1,
        });
        price += noise * 0.1;
        ts += 300_000;
    }

    candles
}
