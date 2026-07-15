use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 参数网格搜索 - 寻找最优配置
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

    println!("========== 参数网格搜索 ==========\n");
    println!("数据: BTC-USDT-SWAP 4H K线，共{}根\n", candle_items.len());

    // 定义参数网格
    let stop_atr_mults = vec![0.8, 1.0, 1.2, 1.5];
    let target_r1s = vec![0.3, 0.5, 0.8, 1.0];

    let mut results = Vec::new();

    let risk_config = BasicRiskStrategyConfig::default();

    // 网格搜索
    for stop_atr_mult in &stop_atr_mults {
        for target_r1 in &target_r1s {
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
                stop_atr_mult: *stop_atr_mult,
                target_r_1: *target_r1,
                target_r_2: target_r1 * 2.0,
                target_r_3: target_r1 * 3.0,
                atr_period: 14,
                rsi_period: 14,
                rsi_min_before_drop: 10.0,
                cooldown_candles: 0,
                allow_short: true,
            };

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
                    *stop_atr_mult,
                    *target_r1,
                    trades,
                    result.win_rate,
                    result.funds - 100.0,
                    profit_factor,
                    result.funds,
                ));
            }
        }
    }

    // 按总盈亏排序
    results.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());

    println!("========== 参数搜索结果（按总盈亏排序）==========\n");
    println!(
        "{:<10} {:<10} {:<8} {:<10} {:<12} {:<12} {:<12}",
        "止损ATR", "止盈R1", "交易数", "胜率", "总盈亏", "盈亏比", "最终资金"
    );
    println!("{}", "-".repeat(80));

    for (i, (stop, target, trades, wr, pnl, pf, funds)) in results.iter().enumerate() {
        let marker = if i == 0 {
            "🏆"
        } else if i < 3 {
            "⭐"
        } else {
            "  "
        };
        println!(
            "{} {:<10.1} {:<10.1} {:<8} {:<10.1}% {:<12.2} {:<12.2} {:<12.2}",
            marker,
            stop,
            target,
            trades,
            wr * 100.0,
            pnl,
            pf,
            funds
        );
    }

    println!("\n最优参数配置：");
    if let Some((stop, target, trades, wr, pnl, pf, funds)) = results.first() {
        println!("  止损: {:.1} ATR", stop);
        println!("  止盈目标1: {:.1}R", target);
        println!("  止盈目标2: {:.1}R", target * 2.0);
        println!("  止盈目标3: {:.1}R", target * 3.0);
        println!("  交易数: {}", trades);
        println!("  胜率: {:.1}%", wr * 100.0);
        println!("  总盈亏: {:.2}%", pnl);
        println!("  盈亏比: {:.2}", pf);
        println!("  最终资金: {:.2}", funds);
    }

    println!("\n================================");

    Ok(())
}
