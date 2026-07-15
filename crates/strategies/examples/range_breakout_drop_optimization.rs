use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 参数优化 - 在200EMA过滤基础上继续优化
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
        LIMIT 5000
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

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           参数优化 - 争取实现正收益                           ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("📊 测试数据: {}根K线\n", candle_items.len());

    let risk_config = BasicRiskStrategyConfig::default();

    // 测试配置组合
    let configs = vec![
        (
            "基线（200EMA）",
            RangeBreakoutDropBacktestTuning {
                range_lookback_candles: 20,
                max_range_volatility_pct: 10.0,
                min_range_volatility_pct: 0.1,
                min_breakout_body_ratio: 0.2,
                min_breakout_move_atr: 0.1,
                min_breakout_volume_mult: 0.5,
                require_bearish_ema: false,
                slow_ema_period: 50,
                long_term_ema_period: 200,
                require_below_long_term_ema: true,
                stop_atr_mult: 1.5,
                target_r_1: 0.8,
                target_r_2: 1.6,
                target_r_3: 2.4,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            },
        ),
        (
            "缩小震荡范围",
            RangeBreakoutDropBacktestTuning {
                range_lookback_candles: 20,
                max_range_volatility_pct: 5.0, // 从10降到5
                min_range_volatility_pct: 0.5, // 从0.1提到0.5
                min_breakout_body_ratio: 0.2,
                min_breakout_move_atr: 0.1,
                min_breakout_volume_mult: 0.5,
                require_bearish_ema: false,
                slow_ema_period: 50,
                long_term_ema_period: 200,
                require_below_long_term_ema: true,
                stop_atr_mult: 1.5,
                target_r_1: 0.8,
                target_r_2: 1.6,
                target_r_3: 2.4,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            },
        ),
        (
            "更严格突破",
            RangeBreakoutDropBacktestTuning {
                range_lookback_candles: 20,
                max_range_volatility_pct: 10.0,
                min_range_volatility_pct: 0.1,
                min_breakout_body_ratio: 0.4,  // 从0.2提到0.4
                min_breakout_move_atr: 0.3,    // 从0.1提到0.3
                min_breakout_volume_mult: 1.0, // 从0.5提到1.0
                require_bearish_ema: false,
                slow_ema_period: 50,
                long_term_ema_period: 200,
                require_below_long_term_ema: true,
                stop_atr_mult: 1.5,
                target_r_1: 0.8,
                target_r_2: 1.6,
                target_r_3: 2.4,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            },
        ),
        (
            "双EMA过滤",
            RangeBreakoutDropBacktestTuning {
                range_lookback_candles: 20,
                max_range_volatility_pct: 10.0,
                min_range_volatility_pct: 0.1,
                min_breakout_body_ratio: 0.2,
                min_breakout_move_atr: 0.1,
                min_breakout_volume_mult: 0.5,
                require_bearish_ema: true, // 开启50EMA
                slow_ema_period: 50,
                long_term_ema_period: 200,
                require_below_long_term_ema: true, // 保持200EMA
                stop_atr_mult: 1.5,
                target_r_1: 0.8,
                target_r_2: 1.6,
                target_r_3: 2.4,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            },
        ),
        (
            "更紧止损",
            RangeBreakoutDropBacktestTuning {
                range_lookback_candles: 20,
                max_range_volatility_pct: 10.0,
                min_range_volatility_pct: 0.1,
                min_breakout_body_ratio: 0.2,
                min_breakout_move_atr: 0.1,
                min_breakout_volume_mult: 0.5,
                require_bearish_ema: false,
                slow_ema_period: 50,
                long_term_ema_period: 200,
                require_below_long_term_ema: true,
                stop_atr_mult: 1.2, // 从1.5降到1.2
                target_r_1: 0.8,
                target_r_2: 1.6,
                target_r_3: 2.4,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            },
        ),
        (
            "更近止盈",
            RangeBreakoutDropBacktestTuning {
                range_lookback_candles: 20,
                max_range_volatility_pct: 10.0,
                min_range_volatility_pct: 0.1,
                min_breakout_body_ratio: 0.2,
                min_breakout_move_atr: 0.1,
                min_breakout_volume_mult: 0.5,
                require_bearish_ema: false,
                slow_ema_period: 50,
                long_term_ema_period: 200,
                require_below_long_term_ema: true,
                stop_atr_mult: 1.5,
                target_r_1: 0.5, // 从0.8降到0.5
                target_r_2: 1.0,
                target_r_3: 1.5,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            },
        ),
    ];

    let mut results = Vec::new();

    for (name, tuning) in configs {
        let strategy = RangeBreakoutDropStrategy;
        let result =
            strategy.run_test_with_tuning(symbol, &candle_items, risk_config.clone(), tuning);

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

            results.push((
                name,
                trades,
                result.win_rate,
                result.funds - 100.0,
                profit_factor,
                result.funds,
            ));
        }
    }

    // 按总盈亏排序
    results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  优化结果（按盈亏排序）                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!(
        "{:<20} {:<10} {:<10} {:<12} {:<10} {:<12}",
        "配置", "交易数", "胜率", "总盈亏", "盈亏比", "最终资金"
    );
    println!("{}", "-".repeat(80));

    for (i, (name, trades, wr, pnl, pf, funds)) in results.iter().enumerate() {
        let marker = if *pnl > 0.0 {
            "✅"
        } else if i == 0 {
            "🏆"
        } else if i < 3 {
            "⭐"
        } else {
            "  "
        };
        println!(
            "{} {:<18} {:<10} {:<10.1}% {:<12.2} {:<10.2} {:<12.2}",
            marker,
            name,
            trades,
            wr * 100.0,
            pnl,
            pf,
            funds
        );
    }

    if let Some((best_name, _, best_wr, best_pnl, best_pf, _)) = results.first() {
        println!("\n🏆 最佳配置: {}", best_name);
        println!("  胜率: {:.1}%", best_wr * 100.0);
        println!("  总盈亏: {:.2}%", best_pnl);
        println!("  盈亏比: {:.2}", best_pf);

        if *best_pnl > 0.0 {
            println!("\n🎉 成功实现正收益！");
        } else {
            println!("\n⚠️  仍未实现正收益，需要继续优化");
        }
    }

    println!("\n================================");

    Ok(())
}
