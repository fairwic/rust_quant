use anyhow::Result;
use rust_quant_market::models::candle_dto::{SelectCandleReqDto, SelectTime, TimeDirect};
use rust_quant_market::models::candle_entity::CandlesEntity;
use rust_quant_market::models::candles::CandlesModel;
/// 震荡突破下跌策略 - 真实数据回测
///
/// 从数据库加载历史K线进行回测验证
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use rust_quant_strategies::strategy_common::BasicRiskStrategyConfig;
use rust_quant_strategies::CandleItem;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("\n========== 震荡突破下跌策略 - 真实数据回测 ==========\n");

    // 测试目标
    let targets = vec![("BTC-USDT-SWAP", "5m"), ("ETH-USDT-SWAP", "5m")];

    for (inst_id, timeframe) in targets {
        println!("正在回测: {} {}", inst_id, timeframe);

        match load_candles_from_db(inst_id, timeframe, 10000).await {
            Ok(candles) => {
                if candles.len() < 100 {
                    println!("  ⚠️  数据量不足: {} 根K线 (需要至少100根)", candles.len());
                    continue;
                }

                println!("  ✅ 加载了 {} 根K线", candles.len());

                // 运行回测
                run_backtest(inst_id, &candles);
                println!();
            }
            Err(e) => {
                println!("  ❌ 加载数据失败: {}", e);
                println!("     可能原因: 数据库连接失败或表不存在");
                println!();
            }
        }
    }

    println!("回测完成！");
    Ok(())
}

/// 从数据库加载K线数据
async fn load_candles_from_db(
    inst_id: &str,
    timeframe: &str,
    limit: usize,
) -> Result<Vec<CandleItem>> {
    let model = CandlesModel::new();

    let dto = SelectCandleReqDto {
        inst_id: inst_id.to_string(),
        time_interval: timeframe.to_string(),
        confirm: Some(1),  // 只要已确认的K线
        select_time: None, // 不限制时间范围
        limit,
    };

    let entities = model.get_all(dto).await?;

    // 转换为CandleItem并按时间正序排列
    let mut candles: Vec<CandleItem> = entities
        .into_iter()
        .map(|e| entity_to_candle_item(e))
        .collect();

    candles.reverse(); // 从数据库查出来是倒序，需要reverse

    Ok(candles)
}

/// 将数据库实体转换为CandleItem
fn entity_to_candle_item(entity: CandlesEntity) -> CandleItem {
    CandleItem {
        ts: entity.ts,
        o: entity.o.parse().unwrap_or(0.0),
        h: entity.h.parse().unwrap_or(0.0),
        l: entity.l.parse().unwrap_or(0.0),
        c: entity.c.parse().unwrap_or(0.0),
        v: entity.vol.parse().unwrap_or(0.0),
        confirm: entity.confirm.parse().unwrap_or(0),
    }
}

/// 运行回测
fn run_backtest(inst_id: &str, candles: &[CandleItem]) {
    let risk = BasicRiskStrategyConfig {
        max_loss_percent: 2.0,
        ..BasicRiskStrategyConfig::default()
    };

    // 默认参数回测
    println!("  --- 默认参数回测 ---");
    let result = RangeBreakoutDropStrategy.run_test(inst_id, candles, risk);
    print_result(&result);

    // 如果默认参数没有产生信号，尝试宽松参数
    if result.trade_records.is_empty() && result.filtered_signals.is_empty() {
        println!("\n  --- 宽松参数回测 ---");
        let mut tuning = RangeBreakoutDropBacktestTuning::default();
        tuning.max_range_volatility_pct = 4.0;
        tuning.min_range_volatility_pct = 0.3;
        tuning.min_breakout_body_ratio = 0.4;
        tuning.min_breakout_move_atr = 0.5;
        tuning.min_breakout_volume_mult = 1.2;
        tuning.require_bearish_ema = false;
        tuning.rsi_min_before_drop = 30.0;

        let result2 =
            RangeBreakoutDropStrategy.run_test_with_tuning(inst_id, candles, risk, tuning);
        print_result(&result2);
    }
}

/// 打印回测结果
fn print_result(result: &rust_quant_strategies::BackTestResult) {
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

    println!("    总交易: {}", result.trade_records.len());
    println!("    盈利: {} | 亏损: {}", winning, losing);
    println!("    胜率: {:.2}%", result.win_rate);
    println!("    最终资金: {:.2}", result.funds);
    println!("    被过滤: {}", result.filtered_signals.len());

    if result.filtered_signals.len() > 0 {
        println!("\n    过滤原因统计:");
        let mut reason_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for signal in &result.filtered_signals {
            for reason in &signal.filter_reasons {
                *reason_counts.entry(reason.clone()).or_insert(0) += 1;
            }
        }
        let mut sorted: Vec<_> = reason_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in sorted.iter().take(5) {
            println!("      {}: {}", reason, count);
        }
    }

    if result.trade_records.len() > 0 {
        println!("\n    前3笔交易:");
        for (i, trade) in result.trade_records.iter().take(3).enumerate() {
            println!(
                "      #{}: {} @ {:.2} | 盈亏 {:.2}",
                i + 1,
                trade.option_type,
                trade.open_price,
                trade.profit_loss
            );
        }
    }
}
