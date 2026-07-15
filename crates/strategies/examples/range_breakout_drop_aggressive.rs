use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 激进参数：快速止盈止损，增加交易频率
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

    println!("========== 激进参数回测 ==========\n");
    println!("策略逻辑:");
    println!("  - 宽松突破识别（最低价触及即可）");
    println!("  - 快速止盈（0.5R, 1.0R, 1.5R）");
    println!("  - 紧密止损（1.0 ATR）");
    println!("  - 零冷却期\n");

    // 激进参数：快速止盈止损
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
        stop_atr_mult: 1.0, // 紧密止损：1.0 ATR
        target_r_1: 0.5,    // 快速止盈：0.5R
        target_r_2: 1.0,    // 第二目标：1.0R
        target_r_3: 1.5,    // 第三目标：1.5R
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
        cooldown_candles: 0, // 零冷却期
        allow_short: true,
    };

    let strategy = RangeBreakoutDropStrategy;
    let risk_config = BasicRiskStrategyConfig::default();

    println!("执行回测...\n");
    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    println!("========== 回测结果 ==========");
    println!("总交易次数: {}", result.trade_records.len());
    println!("总过滤信号: {}", result.filtered_signals.len());

    if result.trade_records.len() > 0 {
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

        println!("胜率: {:.2}%", result.win_rate * 100.0);
        println!("最终资金: {:.2}", result.funds);
        println!(
            "总盈亏: {:.2} ({:.2}%)",
            result.funds - 100.0,
            (result.funds - 100.0)
        );

        println!("\n交易统计:");
        println!(
            "  盈利交易: {} ({:.1}%)",
            winning_trades,
            winning_trades as f64 / result.trade_records.len() as f64 * 100.0
        );
        println!(
            "  亏损交易: {} ({:.1}%)",
            losing_trades,
            losing_trades as f64 / result.trade_records.len() as f64 * 100.0
        );

        let avg_win = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss > 0.0)
            .map(|t| t.profit_loss)
            .sum::<f64>()
            / winning_trades.max(1) as f64;
        let avg_loss = result
            .trade_records
            .iter()
            .filter(|t| t.profit_loss < 0.0)
            .map(|t| t.profit_loss.abs())
            .sum::<f64>()
            / losing_trades.max(1) as f64;

        println!("  平均盈利: {:.2}", avg_win);
        println!("  平均亏损: {:.2}", avg_loss);
        if avg_loss > 0.0 {
            println!("  盈亏比: {:.2}", avg_win / avg_loss);
        }

        println!("\n前10笔交易:");
        for (i, trade) in result.trade_records.iter().take(10).enumerate() {
            let pnl_symbol = if trade.profit_loss > 0.0 {
                "✓"
            } else if trade.profit_loss < 0.0 {
                "✗"
            } else {
                "○"
            };
            println!(
                "  #{}: {} {} 入场={}, 出场={}, 价格={:.2}, 盈亏={:.2}",
                i + 1,
                pnl_symbol,
                trade.option_type,
                trade.open_position_time,
                trade
                    .close_position_time
                    .as_ref()
                    .unwrap_or(&"-".to_string()),
                trade.open_price,
                trade.profit_loss,
            );
        }
    } else {
        println!("⚠️ 没有产生任何交易");
    }

    println!("\n================================");

    Ok(())
}
