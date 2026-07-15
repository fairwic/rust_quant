use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy, RangeBreakoutDropThresholds,
};
use sqlx::Row;
use std::env;

/// 从数据库读取K线数据并运行回测
///
/// 使用方法:
/// ```bash
/// DATABASE_URL="postgresql://postgres:postgres@localhost:5432/quant_core" \
/// cargo run -p rust-quant-strategies --example range_breakout_drop_backtest
/// ```
#[tokio::main]
async fn main() -> Result<()> {
    // 连接数据库
    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/quant_core".to_string());

    println!("连接数据库: {}", database_url);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // 读取BTC 4小时K线数据
    let symbol = "BTC-USDT-SWAP";

    println!("查询 {} 的K线数据...", symbol);

    // 直接使用sqlx::query而不是query!宏，避免编译时检查
    let rows = sqlx::query(
        r#"
        SELECT ts, o, h, l, c, vol, confirm
        FROM "btc-usdt-swap_candles_4h"
        WHERE confirm = '1'
        ORDER BY ts DESC
        LIMIT 2000
        "#,
    )
    .fetch_all(&pool)
    .await?;

    println!("读取到 {} 根K线数据", rows.len());

    if rows.len() < 600 {
        println!("K线数据不足600根，无法运行回测");
        return Ok(());
    }

    // 转换为CandleItem格式（需要反转顺序，因为查询是DESC）
    let mut candle_items: Vec<CandleItem> = Vec::new();
    for row in rows.iter().rev() {
        let ts: i64 = row.try_get("ts")?;
        let o: String = row.try_get("o")?;
        let h: String = row.try_get("h")?;
        let l: String = row.try_get("l")?;
        let c: String = row.try_get("c")?;
        let vol: String = row.try_get("vol")?;
        let confirm: String = row.try_get("confirm")?;

        candle_items.push(CandleItem {
            ts,
            o: o.parse::<f64>()?,
            h: h.parse::<f64>()?,
            l: l.parse::<f64>()?,
            c: c.parse::<f64>()?,
            v: vol.parse::<f64>()?,
            confirm: confirm.parse::<i32>()?,
        });
    }

    println!(
        "转换完成，K线范围: {} 到 {}",
        candle_items.first().unwrap().ts,
        candle_items.last().unwrap().ts
    );

    // 策略是零大小类型，直接使用即可
    let strategy = RangeBreakoutDropStrategy;

    // 使用默认tuning参数
    let tuning = RangeBreakoutDropBacktestTuning::default();

    // 使用默认风险配置
    let risk_config = BasicRiskStrategyConfig::default();

    println!("\n开始回测...");
    println!("策略参数: {:?}", tuning);

    // 运行回测
    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    // 输出结果
    println!("\n========== 回测结果 ==========");
    println!("交易对: {}", symbol);
    println!("K线周期: 4H");
    println!("K线数量: {}", candle_items.len());
    println!("\n总交易次数: {}", result.trade_records.len());
    println!("总过滤信号: {}", result.filtered_signals.len());
    println!("胜率: {:.2}%", result.win_rate * 100.0);
    println!("最终资金: {:.2}", result.funds);

    if !result.trade_records.is_empty() {
        let winning_trades = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .count();
        let losing_trades = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss <= 0.0)
            .count();
        let total_pnl: f64 = result.trade_records.iter().map(|t| t.profit_loss).sum();

        println!("\n盈利交易: {}", winning_trades);
        println!("亏损交易: {}", losing_trades);
        println!("总盈亏: {:.2}", total_pnl);
        println!(
            "平均盈亏: {:.2}",
            total_pnl / result.trade_records.len() as f64
        );

        // 显示前10笔交易详情
        println!("\n前10笔交易详情:");
        for (i, trade) in result.trade_records.iter().take(10).enumerate() {
            println!(
                "  #{}: 入场@{:.2}, 出场@{:.2}, 盈亏={:.2}",
                i + 1,
                trade.open_price,
                trade.close_price.unwrap_or(0.0),
                trade.profit_loss
            );
        }
    } else {
        println!("\n⚠️  没有产生任何交易");
    }

    if !result.filtered_signals.is_empty() {
        // 统计过滤原因
        use std::collections::HashMap;
        let mut reason_counts: HashMap<String, usize> = HashMap::new();

        for sig in &result.filtered_signals {
            for reason in &sig.filter_reasons {
                *reason_counts.entry(reason.clone()).or_insert(0) += 1;
            }
        }

        println!("\n过滤原因统计:");
        let mut sorted_reasons: Vec<_> = reason_counts.iter().collect();
        sorted_reasons.sort_by(|a, b| b.1.cmp(a.1));

        for (reason, count) in sorted_reasons.iter().take(10) {
            println!("  {}: {}", reason, count);
        }

        // 显示前5个过滤信号详情
        println!("\n前5个过滤信号详情:");
        for (i, sig) in result.filtered_signals.iter().take(5).enumerate() {
            println!(
                "  #{}: 时间={}, 价格={:.2}, 原因数={}",
                i + 1,
                sig.ts,
                sig.signal_price,
                sig.filter_reasons.len()
            );
            for reason in &sig.filter_reasons {
                println!("      - {}", reason);
            }
        }
    }

    println!("\n=============================");

    Ok(())
}
