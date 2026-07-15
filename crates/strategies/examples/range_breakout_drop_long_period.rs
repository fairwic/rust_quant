use anyhow::Result;
use chrono::Datelike;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 长周期回测 - 使用5000根K线（约2年数据）
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let symbol = "BTC-USDT-SWAP";
    let candle_limit = 5000;

    let rows = sqlx::query(&format!(
        r#"
            SELECT ts, o, h, l, c, vol, confirm
            FROM "btc-usdt-swap_candles_4h"
            WHERE confirm = '1'
            ORDER BY ts DESC
            LIMIT {}
            "#,
        candle_limit
    ))
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

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              长周期回测 - 扩展历史数据验证                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("📊 测试数据:");
    println!("  品种: {}", symbol);
    println!("  周期: 4小时");
    println!("  K线数: {}根", candle_items.len());
    println!(
        "  时间跨度: 约{:.1}天 ({:.1}年)",
        candle_items.len() as f64 * 4.0 / 24.0,
        candle_items.len() as f64 * 4.0 / 24.0 / 365.0
    );

    if let (Some(first), Some(last)) = (candle_items.first(), candle_items.last()) {
        let first_time = chrono::DateTime::from_timestamp(first.ts / 1000, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Invalid".to_string());
        let last_time = chrono::DateTime::from_timestamp(last.ts / 1000, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Invalid".to_string());
        println!("  时间范围: {} 至 {}\n", first_time, last_time);
    }

    // 使用优化后的最佳参数
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

    println!("⚙️  使用最优参数配置:");
    println!("  止损: {:.1} ATR", tuning.stop_atr_mult);
    println!(
        "  止盈: {:.1}R / {:.1}R / {:.1}R\n",
        tuning.target_r_1, tuning.target_r_2, tuning.target_r_3
    );

    let strategy = RangeBreakoutDropStrategy;
    let risk_config = BasicRiskStrategyConfig::default();

    println!("🔄 执行长周期回测...\n");
    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                     回测结果                                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let trades = result.trade_records.len();
    println!("📈 整体表现:");
    println!(
        "  总盈亏: {:.2}% (${:.2} → ${:.2})",
        (result.funds - 100.0),
        100.0,
        result.funds
    );
    println!("  最终资金: ${:.2}", result.funds);
    println!(
        "  年化收益率: {:.2}%",
        (result.funds - 100.0) / (candle_items.len() as f64 * 4.0 / 24.0 / 365.0)
    );

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
        let breakeven = trades - winning - losing;

        println!("\n🎯 交易统计:");
        println!("  总交易数: {}", trades);
        println!(
            "  盈利: {} ({:.1}%)",
            winning,
            winning as f64 / trades as f64 * 100.0
        );
        println!(
            "  亏损: {} ({:.1}%)",
            losing,
            losing as f64 / trades as f64 * 100.0
        );
        println!(
            "  盈亏平衡: {} ({:.1}%)",
            breakeven,
            breakeven as f64 / trades as f64 * 100.0
        );
        println!("  胜率: {:.1}%", result.win_rate * 100.0);
        println!(
            "  平均每年交易: {:.1}笔",
            trades as f64 / (candle_items.len() as f64 * 4.0 / 24.0 / 365.0)
        );

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

        let max_win = result
            .trade_records
            .iter()
            .map(|t| t.profit_loss)
            .fold(f64::NEG_INFINITY, f64::max);

        let max_loss = result
            .trade_records
            .iter()
            .map(|t| t.profit_loss)
            .fold(f64::INFINITY, f64::min);

        println!("\n💰 盈亏分析:");
        println!("  平均盈利: ${:.2}", avg_win);
        println!("  平均亏损: ${:.2}", avg_loss);
        println!(
            "  盈亏比: {:.2}",
            if avg_loss > 0.0 {
                avg_win / avg_loss
            } else {
                0.0
            }
        );
        println!("  最大盈利: ${:.2}", max_win);
        println!("  最大亏损: ${:.2}", max_loss);

        println!("\n📊 与2000根K线对比:");
        println!("  短周期(2000根): 24笔交易, +45.87%, 胜率41.7%, 盈亏比4.59");
        println!(
            "  长周期({}根): {}笔交易, {:.2}%, 胜率{:.1}%, 盈亏比{:.2}",
            candle_items.len(),
            trades,
            result.funds - 100.0,
            result.win_rate * 100.0,
            if avg_loss > 0.0 {
                avg_win / avg_loss
            } else {
                0.0
            }
        );

        // 分析交易分布
        println!("\n📅 交易时间分布:");
        let mut yearly_trades: std::collections::HashMap<i32, usize> =
            std::collections::HashMap::new();
        for trade in &result.trade_records {
            if let Ok(dt) =
                chrono::DateTime::parse_from_str(&trade.open_position_time, "%Y-%m-%d %H:%M:%S")
            {
                *yearly_trades.entry(dt.year()).or_insert(0) += 1;
            }
        }
        let mut years: Vec<_> = yearly_trades.keys().collect();
        years.sort();
        for year in years {
            println!("  {}: {}笔", year, yearly_trades[year]);
        }

        println!("\n📋 最近10笔交易:");
        for (i, trade) in result.trade_records.iter().rev().take(10).enumerate() {
            let pnl_symbol = if trade.profit_loss > 0.0 {
                "✅"
            } else if trade.profit_loss < 0.0 {
                "❌"
            } else {
                "⚪"
            };
            println!(
                "  {} #{}: {} 入场={} 价格={:.2} 盈亏=${:.2}",
                pnl_symbol,
                trades - i,
                trade.option_type,
                &trade.open_position_time[0..16],
                trade.open_price,
                trade.profit_loss,
            );
        }
    } else {
        println!("⚠️ 没有产生任何交易");
    }

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                   长周期验证结论                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    if trades > 0 {
        let avg_loss = if result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .count()
            > 0
        {
            result
                .trade_records
                .iter()
                .filter(|t| t.profit_loss < 0.0)
                .map(|t| t.profit_loss.abs())
                .sum::<f64>()
                / result
                    .trade_records
                    .iter()
                    .filter(|t| t.profit_loss < 0.0)
                    .count() as f64
        } else {
            0.0
        };
        let avg_win = if result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .count()
            > 0
        {
            result
                .trade_records
                .iter()
                .filter(|t| t.profit_loss > 0.0)
                .map(|t| t.profit_loss)
                .sum::<f64>()
                / result
                    .trade_records
                    .iter()
                    .filter(|t| t.profit_loss > 0.0)
                    .count() as f64
        } else {
            0.0
        };

        if result.funds > 100.0 && result.win_rate > 0.3 {
            println!("✅ 策略在长周期数据上表现稳定");
            println!("   - 总体盈利: {:.2}%", result.funds - 100.0);
            println!("   - 胜率合理: {:.1}%", result.win_rate * 100.0);
            println!(
                "   - 盈亏比: {:.2}",
                if avg_loss > 0.0 {
                    avg_win / avg_loss
                } else {
                    0.0
                }
            );
        } else if result.funds > 95.0 {
            println!("⚠️ 策略在长周期数据上表现一般");
            println!("   需要进一步优化参数或逻辑");
        } else {
            println!("❌ 策略在长周期数据上表现不佳");
            println!("   建议重新审视策略逻辑");
        }
    }

    println!("\n================================");

    Ok(())
}
