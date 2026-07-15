/// 震荡结束突破下跌策略回测测试
///
/// 策略逻辑：
/// 1. 识别震荡区间：价格在一定范围内横盘整理
/// 2. 检测突破：价格突破震荡区间的下边界
/// 3. 确认下跌：有足够的动量确认突破有效
/// 4. 做空入场：在确认下跌后做空
///
/// 预期优势：
/// - 震荡区间提供明确的止损位（区间上边界）
/// - 突破下边界后有下跌空间
/// - 适合震荡后转为下跌趋势的市场环境
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[test]
#[ignore] // 默认忽略，需要显式运行
fn range_breakout_drop_btc_5m_basic_test() {
    // 使用模拟数据进行基础功能测试
    let candles = generate_simulated_range_breakout_candles();

    let strategy = RangeBreakoutDropStrategy;
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    let result = strategy.run_test("BTC-USDT-SWAP", &candles, risk);

    println!("\n========== 震荡突破下跌策略 - 基础功能测试 ==========");
    println!("测试周期: {} 根K线", candles.len());
    println!("\n--- 核心指标 ---");
    println!("总交易次数: {}", result.trade_records.len());

    let winning_trades = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss > 0.0)
        .count();
    let losing_trades = result
        .trade_records
        .iter()
        .filter(|t| t.profit_loss < 0.0)
        .count();

    println!("盈利交易: {}", winning_trades);
    println!("亏损交易: {}", losing_trades);
    println!("胜率: {:.2}%", result.win_rate);
    println!("最终资金: {:.2} USDT", result.funds);
    println!("未平仓交易: {}", result.open_trades);
    println!("被过滤信号: {}", result.filtered_signals.len());

    // 基础可行性检查
    assert!(
        result.trade_records.len() > 0 || result.filtered_signals.len() > 0,
        "应该产生交易信号或被过滤的信号"
    );

    if result.trade_records.len() > 0 {
        println!("\n✅ 策略成功产生了交易信号");

        // 显示前几笔交易详情
        println!("\n--- 交易详情（前5笔）---");
        for (i, trade) in result.trade_records.iter().take(5).enumerate() {
            println!(
                "交易 #{}: {} | 开仓: {:.2} | 平仓: {:.2} | 盈亏: {:.2}",
                i + 1,
                trade.option_type,
                trade.open_price,
                trade.close_price.unwrap_or(0.0),
                trade.profit_loss
            );
        }
    } else {
        println!("\n⚠️  所有信号都被过滤了，查看过滤原因：");
        for (i, signal) in result.filtered_signals.iter().take(5).enumerate() {
            println!(
                "信号 #{}: {} @ {:.2} | 原因: {:?}",
                i + 1,
                signal.direction,
                signal.signal_price,
                signal.filter_reasons
            );
        }
    }
}

#[test]
#[ignore]
fn range_breakout_drop_parameter_sweep() {
    let candles = generate_simulated_range_breakout_candles();

    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    println!("\n========== 参数扫描测试 ==========");
    println!("测试数据: {} 根K线", candles.len());

    let mut best_funds = 0.0;
    let mut best_tuning = None;

    // 参数组合
    let range_lookbacks = vec![15, 20, 25];
    let max_volatilities = vec![2.5, 3.0, 3.5];
    let min_breakout_body_ratios = vec![0.5, 0.55, 0.6];

    let mut test_count = 0;
    let total_tests =
        range_lookbacks.len() * max_volatilities.len() * min_breakout_body_ratios.len();

    for &lookback in &range_lookbacks {
        for &max_vol in &max_volatilities {
            for &body_ratio in &min_breakout_body_ratios {
                test_count += 1;

                let mut tuning = RangeBreakoutDropBacktestTuning::default();
                tuning.range_lookback_candles = lookback;
                tuning.max_range_volatility_pct = max_vol;
                tuning.min_breakout_body_ratio = body_ratio;

                let strategy = RangeBreakoutDropStrategy;
                let result = strategy.run_test_with_tuning("BTC-USDT-SWAP", &candles, risk, tuning);

                if result.funds > best_funds && result.trade_records.len() >= 1 {
                    best_funds = result.funds;
                    best_tuning = Some((tuning, result));
                }

                if test_count % 5 == 0 {
                    println!(
                        "进度: {}/{} ({:.1}%)",
                        test_count,
                        total_tests,
                        test_count as f64 / total_tests as f64 * 100.0
                    );
                }
            }
        }
    }

    println!("\n========== 最佳参数配置 ==========");
    if let Some((tuning, result)) = best_tuning {
        println!("震荡识别窗口: {} K线", tuning.range_lookback_candles);
        println!("最大波动率: {:.1}%", tuning.max_range_volatility_pct);
        println!("最小实体比例: {:.2}", tuning.min_breakout_body_ratio);
        println!("\n--- 回测结果 ---");
        println!("总交易: {}", result.trade_records.len());
        println!("胜率: {:.2}%", result.win_rate);
        println!("最终资金: {:.2} USDT", result.funds);
    } else {
        println!("⚠️  未找到满足条件的参数组合");
    }
}

/// 生成模拟的震荡突破K线数据
fn generate_simulated_range_breakout_candles() -> Vec<CandleItem> {
    let mut candles = Vec::new();
    let mut base_price = 50000.0;
    let mut ts = 1704067200000i64; // 2024-01-01 00:00:00

    // 第一阶段：震荡区间（30根K线）
    let range_high = base_price * 1.015; // 50750
    let range_low = base_price * 0.985; // 49250

    for i in 0..30 {
        let price = range_low + (range_high - range_low) * (i as f64 / 30.0).sin().abs();
        let volatility = 50.0;

        candles.push(CandleItem {
            ts,
            o: price,
            h: price + volatility,
            l: price - volatility,
            c: price + (volatility * 0.2 * ((i % 3) as f64 - 1.0)),
            v: 1000.0 + (i as f64 * 10.0),
            confirm: 1,
        });

        ts += 300_000; // 5分钟
    }

    // 第二阶段：突破下跌（15根K线）
    for i in 0..15 {
        let drop_ratio = 1.0 - (i as f64 * 0.004); // 每根K线下跌0.4%
        let price = range_low * drop_ratio;

        candles.push(CandleItem {
            ts,
            o: price + 20.0,
            h: price + 30.0,
            l: price - 10.0,
            c: price,
            v: 2000.0 + (i as f64 * 50.0), // 放量
            confirm: 1,
        });

        ts += 300_000;
    }

    // 第三阶段：继续下跌（10根K线）
    base_price = range_low * 0.94;
    for i in 0..10 {
        let price = base_price * (1.0 - i as f64 * 0.003);

        candles.push(CandleItem {
            ts,
            o: price + 10.0,
            h: price + 20.0,
            l: price - 15.0,
            c: price,
            v: 1500.0,
            confirm: 1,
        });

        ts += 300_000;
    }

    // 第四阶段：小幅反弹（5根K线）
    for i in 0..5 {
        let price = base_price * (1.0 + i as f64 * 0.002);

        candles.push(CandleItem {
            ts,
            o: price - 10.0,
            h: price + 10.0,
            l: price - 20.0,
            c: price,
            v: 1000.0,
            confirm: 1,
        });

        ts += 300_000;
    }

    candles
}
