use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 详细分析交易过程
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

    println!("========== 交易详情分析 ==========\n");

    // 使用更激进的止盈止损
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
        stop_atr_mult: 1.5, // 更紧的止损
        target_r_1: 0.5,    // 更近的止盈
        target_r_2: 1.0,
        target_r_3: 2.0,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0,
        cooldown_candles: 0,
        allow_short: true,
    };

    let strategy = RangeBreakoutDropStrategy;
    let risk_config = BasicRiskStrategyConfig::default();

    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    println!("总交易次数: {}", result.trade_records.len());
    println!("总过滤信号: {}", result.filtered_signals.len());
    println!("胜率: {:.2}%", result.win_rate * 100.0);
    println!("最终资金: {:.2}", result.funds);
    println!(
        "总盈亏: {:.2} ({:.2}%)\n",
        result.funds - 100.0,
        (result.funds - 100.0)
    );

    println!("所有交易详情:");
    for (i, trade) in result.trade_records.iter().enumerate() {
        let pnl_symbol = if trade.profit_loss > 0.0 {
            "✓"
        } else {
            "✗"
        };

        println!(
            "  #{}: {} 类型={}, 入场={}, 出场={}, 开仓价={:.2}, 平仓价={:.2}, 盈亏={:.2}",
            i + 1,
            pnl_symbol,
            trade.option_type,
            trade.open_position_time,
            trade
                .close_position_time
                .as_ref()
                .unwrap_or(&"持仓中".to_string()),
            trade.open_price,
            trade.close_price.unwrap_or(0.0),
            trade.profit_loss,
        );
    }

    println!("\n================================");

    Ok(())
}
