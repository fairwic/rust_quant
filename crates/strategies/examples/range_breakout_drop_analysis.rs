use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::collections::HashMap;
use std::env;

/// 深度分析策略过滤原因，找出阻碍交易的真正原因
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let symbol = "BTC-USDT-SWAP";

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

    let mut candle_items: Vec<CandleItem> = Vec::new();
    for row in rows.iter().rev() {
        let ts: i64 = row.try_get("ts")?;
        candle_items.push(CandleItem {
            ts,
            o: row.try_get::<String, _>("o")?.parse::<f64>()?,
            h: row.try_get::<String, _>("h")?.parse::<f64>()?,
            l: row.try_get::<String, _>("l")?.parse::<f64>()?,
            c: row.try_get::<String, _>("c")?.parse::<f64>()?,
            v: row.try_get::<String, _>("vol")?.parse::<f64>()?,
            confirm: row.try_get::<String, _>("confirm")?.parse::<i32>()?,
        });
    }

    println!("========== 策略过滤原因深度分析 ==========\n");

    // 测试极度宽松的参数
    let tuning = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 15,     // 缩短震荡窗口
        max_range_volatility_pct: 10.0, // 大幅放宽震荡波动
        min_range_volatility_pct: 0.1,  // 降低最小波动
        min_breakout_body_ratio: 0.3,   // 降低实体比例
        min_breakout_move_atr: 0.3,     // 大幅降低突破幅度
        min_breakout_volume_mult: 1.0,  // 不要求放量
        require_bearish_ema: false,     // 取消趋势过滤
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: false,
        stop_atr_mult: 1.5,
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.5,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 20.0, // 大幅降低RSI要求
        cooldown_candles: 1,       // 最小冷却
        allow_short: true,
    };

    let strategy = RangeBreakoutDropStrategy;
    let risk_config = BasicRiskStrategyConfig::default();
    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    println!("极度宽松参数结果:");
    println!("总交易次数: {}", result.trade_records.len());
    println!("总过滤信号: {}", result.filtered_signals.len());
    println!("胜率: {:.2}%", result.win_rate * 100.0);
    println!("最终资金: {:.2}\n", result.funds);

    // 统计过滤原因
    let mut reason_counts: HashMap<String, usize> = HashMap::new();
    let mut reason_combinations: HashMap<Vec<String>, usize> = HashMap::new();

    for sig in &result.filtered_signals {
        for reason in &sig.filter_reasons {
            *reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
        let mut reasons = sig.filter_reasons.clone();
        reasons.sort();
        *reason_combinations.entry(reasons).or_insert(0) += 1;
    }

    println!("单个过滤原因统计:");
    let mut sorted_reasons: Vec<_> = reason_counts.iter().collect();
    sorted_reasons.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in sorted_reasons.iter().take(15) {
        let pct = (**count as f64 / result.filtered_signals.len() as f64) * 100.0;
        println!("  {}: {} ({:.1}%)", reason, count, pct);
    }

    println!("\n最常见的过滤原因组合 (前10):");
    let mut sorted_combinations: Vec<_> = reason_combinations.iter().collect();
    sorted_combinations.sort_by(|a, b| b.1.cmp(a.1));
    for (reasons, count) in sorted_combinations.iter().take(10) {
        let pct = (**count as f64 / result.filtered_signals.len() as f64) * 100.0;
        println!("  出现{}次 ({:.1}%):", count, pct);
        for r in reasons.iter() {
            println!("    - {}", r);
        }
        println!();
    }

    // 找出只被单一原因过滤的信号
    println!("只被单一原因过滤的信号:");
    for (reasons, count) in sorted_combinations.iter() {
        if reasons.len() == 1 {
            let pct = (**count as f64 / result.filtered_signals.len() as f64) * 100.0;
            println!("  {}: {} ({:.1}%)", reasons[0], count, pct);
        }
    }

    // 分析交易详情
    if !result.trade_records.is_empty() {
        println!("\n成功交易详情:");
        for (i, trade) in result.trade_records.iter().enumerate() {
            println!(
                "  #{}: 开仓时间={}, 价格={:.2}, 盈亏={:.2}, 类型={}",
                i + 1,
                trade.open_position_time,
                trade.open_price,
                trade.profit_loss,
                trade.option_type
            );
        }
    }

    println!("\n========================================");

    Ok(())
}
