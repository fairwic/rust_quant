use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 简化策略：移除过严条件，只保留核心突破逻辑
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

    println!("========== 简化策略回测 ==========\n");
    println!("数据: {} 4H K线，共{}根", symbol, candle_items.len());
    println!("策略逻辑: 只要求震荡识别 + 向下突破 + 阴线，移除所有严格的量化过滤\n");

    // 极简参数：移除所有严格要求
    let tuning = RangeBreakoutDropBacktestTuning {
        range_lookback_candles: 20,
        max_range_volatility_pct: 10.0, // 非常宽松
        min_range_volatility_pct: 0.1,  // 几乎不限制
        min_breakout_body_ratio: 0.2,   // 只要求最低实体比例
        min_breakout_move_atr: 0.1,     // 几乎任何向下移动都算
        min_breakout_volume_mult: 0.5,  // 不要求放量，甚至缩量也可以
        require_bearish_ema: false,     // 不要求趋势过滤
        slow_ema_period: 50,
        long_term_ema_period: 200,
        require_below_long_term_ema: false,
        stop_atr_mult: 2.0, // 止损稍微放宽
        target_r_1: 1.0,
        target_r_2: 2.0,
        target_r_3: 3.5,
        atr_period: 14,
        rsi_period: 14,
        rsi_min_before_drop: 10.0, // RSI几乎不限制
        cooldown_candles: 1,       // 最小冷却
        allow_short: true,
    };

    let strategy = RangeBreakoutDropStrategy;
    let risk_config = BasicRiskStrategyConfig::default();

    println!("执行回测...\n");
    let result = strategy.run_test_with_tuning(symbol, &candle_items, risk_config, tuning);

    println!("========== 回测结果 ==========");
    println!("总交易次数: {}", result.trade_records.len());
    println!("总过滤信号: {}", result.filtered_signals.len());
    println!("胜率: {:.2}%", result.win_rate * 100.0);
    println!("最终资金: {:.2}", result.funds);

    if result.funds > 100.0 {
        println!(
            "总盈亏: +{:.2} (+{:.2}%)",
            result.funds - 100.0,
            (result.funds - 100.0)
        );
    } else {
        println!(
            "总盈亏: {:.2} ({:.2}%)",
            result.funds - 100.0,
            (result.funds - 100.0)
        );
    }

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

        println!("\n交易统计:");
        println!(
            "  盈利交易: {} ({:.1}%)",
            winning_trades,
            (winning_trades as f64 / result.trade_records.len() as f64) * 100.0
        );
        println!(
            "  亏损交易: {} ({:.1}%)",
            losing_trades,
            (losing_trades as f64 / result.trade_records.len() as f64) * 100.0
        );
        println!(
            "  平均盈亏: {:.2}",
            total_pnl / result.trade_records.len() as f64
        );

        // 计算最大盈利和最大亏损
        let max_profit = result
            .trade_records
            .iter()
            .map(|t| t.profit_loss)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_loss = result
            .trade_records
            .iter()
            .map(|t| t.profit_loss)
            .fold(f64::INFINITY, f64::min);
        println!("  最大盈利: {:.2}", max_profit);
        println!("  最大亏损: {:.2}", max_loss);

        if result.trade_records.len() <= 20 {
            println!("\n所有交易详情:");
            for (i, trade) in result.trade_records.iter().enumerate() {
                println!(
                    "  #{}: {}时间={}, 价格={:.2}, 盈亏={:.2}",
                    i + 1,
                    if trade.profit_loss > 0.0 {
                        "✓ "
                    } else {
                        "✗ "
                    },
                    trade.open_position_time,
                    trade.open_price,
                    trade.profit_loss
                );
            }
        } else {
            println!("\n前10笔交易:");
            for (i, trade) in result.trade_records.iter().take(10).enumerate() {
                println!(
                    "  #{}: {}时间={}, 价格={:.2}, 盈亏={:.2}",
                    i + 1,
                    if trade.profit_loss > 0.0 {
                        "✓ "
                    } else {
                        "✗ "
                    },
                    trade.open_position_time,
                    trade.open_price,
                    trade.profit_loss
                );
            }
            println!("  ... 还有{}笔交易", result.trade_records.len() - 10);
        }
    }

    if !result.filtered_signals.is_empty() {
        use std::collections::HashMap;
        let mut reason_counts: HashMap<String, usize> = HashMap::new();

        for sig in &result.filtered_signals {
            for reason in &sig.filter_reasons {
                *reason_counts.entry(reason.clone()).or_insert(0) += 1;
            }
        }

        println!("\n过滤原因TOP5:");
        let mut sorted_reasons: Vec<_> = reason_counts.iter().collect();
        sorted_reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in sorted_reasons.iter().take(5) {
            let pct = (**count as f64 / result.filtered_signals.len() as f64) * 100.0;
            println!("  {}: {} ({:.1}%)", reason, count, pct);
        }
    }

    println!("\n================================");

    Ok(())
}
