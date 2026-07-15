use anyhow::Result;
use chrono::Datelike;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 分段回测 - 分析不同时期的表现
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let symbol = "BTC-USDT-SWAP";

    // 获取所有数据
    let rows = sqlx::query(
        r#"
        SELECT ts, o, h, l, c, vol, confirm
        FROM "btc-usdt-swap_candles_4h"
        WHERE confirm = '1'
        ORDER BY ts DESC
        LIMIT 8000
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut all_candles: Vec<CandleItem> = Vec::new();
    for row in rows.iter().rev() {
        let ts: i64 = row.try_get("ts")?;
        all_candles.push(CandleItem {
            ts,
            o: row.try_get::<String, _>("o")?.parse::<f64>()?,
            h: row.try_get::<String, _>("h")?.parse::<f64>()?,
            l: row.try_get::<String, _>("l")?.parse::<f64>()?,
            c: row.try_get::<String, _>("c")?.parse::<f64>()?,
            v: row.try_get::<String, _>("vol")?.parse::<f64>()?,
            confirm: row.try_get::<String, _>("confirm")?.parse::<i32>()?,
        });
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  分段回测 - 时期表现分析                      ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // 最优参数
    let tuning = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 20,
        max_range_volatility_pct: 10.0,
        min_range_volatility_pct: 0.1,
        min_breakout_body_ratio: 0.2,
        min_breakout_move_atr: 0.1,
        min_breakout_volume_mult: 0.5,
        require_bearish_ema: false,
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: false,
        stop_atr_mult: 1.5,
        target_r_1: 0.8,
        target_r_2: 1.6,
        target_r_3: 2.4,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
        cooldown_candles: 0,
        allow_short: true,
    };

    let risk_config = BasicRiskStrategyConfig::default();

    // 分段测试：每2000根一段
    let segments = vec![
        (0, 2000, "2022-2024 早期"),
        (2000, 4000, "2024-2025 中期"),
        (4000, 6000, "2025-2026 后期"),
        (6000, 8000, "2020-2022 最早期"),
    ];

    println!("📊 分段回测结果:\n");
    println!(
        "{:<15} {:<12} {:<10} {:<10} {:<12} {:<10}",
        "时期", "K线数", "交易数", "胜率", "总盈亏", "盈亏比"
    );
    println!("{}", "-".repeat(75));

    for (start, end, label) in segments {
        if end > all_candles.len() {
            continue;
        }

        let segment_candles = &all_candles[start..end];
        let strategy = RangeBreakoutDropStrategy;
        let result = strategy.run_test_with_tuning(
            symbol,
            segment_candles,
            risk_config.clone(),
            tuning.clone(),
        );

        let trades = result.trade_records.len();
        if trades > 0 {
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

            let avg_win = if winning > 0 {
                result
                    .trade_records
                    .iter()
                    .filter(|t| t.profit_loss > 0.0)
                    .map(|t| t.profit_loss)
                    .sum::<f64>()
                    / winning as f64
            } else {
                0.0
            };

            let avg_loss = if losing > 0 {
                result
                    .trade_records
                    .iter()
                    .filter(|t| t.profit_loss < 0.0)
                    .map(|t| t.profit_loss.abs())
                    .sum::<f64>()
                    / losing as f64
            } else {
                0.0
            };

            let profit_factor = if avg_loss > 0.0 {
                avg_win / avg_loss
            } else {
                0.0
            };

            let pnl_marker = if result.funds > 100.0 { "✅" } else { "❌" };

            println!(
                "{} {:<13} {:<12} {:<10} {:<10.1}% {:<12.2} {:<10.2}",
                pnl_marker,
                label,
                segment_candles.len(),
                trades,
                result.win_rate * 100.0,
                result.funds - 100.0,
                profit_factor
            );
        } else {
            println!(
                "⚪ {:<13} {:<12} {:<10} {:<10} {:<12} {:<10}",
                label,
                segment_candles.len(),
                0,
                "-",
                "-",
                "-"
            );
        }
    }

    println!("\n💡 分析结论:\n");

    // 测试最近2000根（原始成功的数据段）
    let recent_2000 = &all_candles[0..2000];
    let strategy = RangeBreakoutDropStrategy;
    let result_recent =
        strategy.run_test_with_tuning(symbol, recent_2000, risk_config.clone(), tuning.clone());

    println!("🔍 最近2000根详细分析:");
    println!("  总盈亏: {:.2}%", result_recent.funds - 100.0);
    println!("  胜率: {:.1}%", result_recent.win_rate * 100.0);
    println!("  交易数: {}", result_recent.trade_records.len());

    if let (Some(first), Some(last)) = (recent_2000.first(), recent_2000.last()) {
        let first_price = first.c;
        let last_price = last.c;
        let price_change = ((last_price - first_price) / first_price) * 100.0;

        println!("\n📉 市场趋势分析:");
        println!("  起始价格: ${:.2}", first_price);
        println!("  结束价格: ${:.2}", last_price);
        println!("  价格变化: {:.2}%", price_change);

        if price_change < -10.0 {
            println!("  ✅ 下跌趋势明显（策略适合）");
        } else if price_change > 10.0 {
            println!("  ❌ 上涨趋势（做空策略不适合）");
        } else {
            println!("  ⚠️  震荡行情");
        }
    }

    println!("\n================================");

    Ok(())
}
