use anyhow::Result;
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::Row;
use std::env;

/// 从数据库读取K线数据并运行回测，测试多组参数
#[tokio::main]
async fn main() -> Result<()> {
    // 连接数据库
    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    println!("连接数据库: {}", database_url);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let symbol = "BTC-USDT-SWAP";

    println!("查询 {} 的K线数据...", symbol);

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
        "转换完成，K线范围: {} 到 {}\n",
        candle_items.first().unwrap().ts,
        candle_items.last().unwrap().ts
    );

    // 定义多组测试参数
    let test_configs = vec![
        ("默认参数", RangeBreakoutDropBacktestTuning::default()),
        (
            "宽松突破",
            RangeBreakoutDropBacktestTuning {
                min_breakout_move_atr: 0.5,    // 从0.8降到0.5
                min_breakout_body_ratio: 0.4,  // 从0.55降到0.4
                min_breakout_volume_mult: 1.2, // 从1.5降到1.2
                ..RangeBreakoutDropBacktestTuning::default()
            },
        ),
        (
            "宽松震荡",
            RangeBreakoutDropBacktestTuning {
                max_range_volatility_pct: 5.0, // 从3.0增加到5.0
                min_range_volatility_pct: 0.3, // 从0.5降到0.3
                ..RangeBreakoutDropBacktestTuning::default()
            },
        ),
        (
            "全面宽松",
            RangeBreakoutDropBacktestTuning {
                max_range_volatility_pct: 5.0,
                min_range_volatility_pct: 0.3,
                min_breakout_move_atr: 0.5,
                min_breakout_body_ratio: 0.4,
                min_breakout_volume_mult: 1.2,
                require_bearish_ema: false, // 不要求趋势过滤
                rsi_min_before_drop: 30.0,  // 从40降到30
                cooldown_candles: 3,        // 从6降到3
                ..RangeBreakoutDropBacktestTuning::default()
            },
        ),
    ];

    let risk_config = BasicRiskStrategyConfig::default();

    for (name, tuning) in test_configs {
        println!("========== {} ==========", name);
        println!("参数: {:?}\n", tuning);

        let strategy = RangeBreakoutDropStrategy;
        let result =
            strategy.run_test_with_tuning(symbol, &candle_items, risk_config.clone(), tuning);

        println!("总交易次数: {}", result.trade_records.len());
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

            println!("盈利交易: {}", winning_trades);
            println!("亏损交易: {}", losing_trades);
            println!("总盈亏: {:.2}", total_pnl);
            println!(
                "平均盈亏: {:.2}",
                total_pnl / result.trade_records.len() as f64
            );

            if result.trade_records.len() <= 10 {
                println!("\n所有交易详情:");
                for (i, trade) in result.trade_records.iter().enumerate() {
                    println!(
                        "  #{}: 入场@{:.2}, 出场@{:.2}, 盈亏={:.2}",
                        i + 1,
                        trade.open_price,
                        trade.close_price.unwrap_or(0.0),
                        trade.profit_loss
                    );
                }
            }
        }

        println!();
    }

    Ok(())
}
