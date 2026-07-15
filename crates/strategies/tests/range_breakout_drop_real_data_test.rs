use anyhow::{anyhow, Context, Result};
use rust_quant_common::CandleItem;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::range_breakout_drop::{
    RangeBreakoutDropBacktestTuning, RangeBreakoutDropStrategy,
};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::env;

const MIN_BACKTEST_CANDLES: usize = 600;

/// 使用 quant_core 中的 BTC 4H K 线验证当前公开回测接口。
#[tokio::test]
#[ignore = "requires QUANT_CORE_DATABASE_URL and local quant_core candle tables"]
async fn test_range_breakout_drop_with_real_btc_data() -> Result<()> {
    let pool = connect_quant_core().await?;
    let candles = load_confirmed_candles(&pool, "BTC-USDT-SWAP", 2_000).await?;
    if candles.len() < MIN_BACKTEST_CANDLES {
        eprintln!(
            "skipping range breakout smoke: only {} BTC candles",
            candles.len()
        );
        return Ok(());
    }

    let result = RangeBreakoutDropStrategy.run_test_with_tuning(
        "BTC-USDT-SWAP",
        &candles,
        BasicRiskStrategyConfig::default(),
        RangeBreakoutDropBacktestTuning::default(),
    );

    assert!(result.funds.is_finite());
    assert!(result.win_rate.is_finite());
    assert!(result
        .trade_records
        .iter()
        .all(|trade| trade.profit_loss.is_finite()));
    Ok(())
}

/// 使用相同默认参数验证 BTC、ETH、SOL 三个训练币种的数据兼容性。
#[tokio::test]
#[ignore = "requires QUANT_CORE_DATABASE_URL and local quant_core candle tables"]
async fn test_range_breakout_drop_multiple_symbols() -> Result<()> {
    let pool = connect_quant_core().await?;

    for symbol in ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"] {
        let candles = load_confirmed_candles(&pool, symbol, 1_000).await?;
        if candles.len() < MIN_BACKTEST_CANDLES {
            eprintln!("skipping {symbol}: only {} candles", candles.len());
            continue;
        }

        let result = RangeBreakoutDropStrategy.run_test_with_tuning(
            symbol,
            &candles,
            BasicRiskStrategyConfig::default(),
            RangeBreakoutDropBacktestTuning::default(),
        );
        assert!(result.funds.is_finite(), "{symbol} funds must be finite");
        assert!(
            result.win_rate.is_finite(),
            "{symbol} win rate must be finite"
        );
    }

    Ok(())
}

/// 只接受 Core 专用连接变量，避免测试误连 quant_web。
async fn connect_quant_core() -> Result<PgPool> {
    let database_url = env::var("QUANT_CORE_DATABASE_URL")
        .context("QUANT_CORE_DATABASE_URL must point to quant_core")?;
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("failed to connect to quant_core")
}

/// 从受限白名单分表加载已确认 K 线，避免动态表名扩大 SQL 注入面。
async fn load_confirmed_candles(
    pool: &PgPool,
    symbol: &str,
    limit: usize,
) -> Result<Vec<CandleItem>> {
    let table = match symbol {
        "BTC-USDT-SWAP" => r#""btc-usdt-swap_candles_4h""#,
        "ETH-USDT-SWAP" => r#""eth-usdt-swap_candles_4h""#,
        "SOL-USDT-SWAP" => r#""sol-usdt-swap_candles_4h""#,
        _ => return Err(anyhow!("unsupported real-data smoke symbol: {symbol}")),
    };
    let query = format!(
        "SELECT ts, o, h, l, c, vol, confirm \
         FROM {table} WHERE confirm = '1' ORDER BY ts DESC LIMIT $1"
    );
    let rows = sqlx::query(&query)
        .bind(limit as i64)
        .fetch_all(pool)
        .await
        .with_context(|| format!("failed to load {symbol} 4H candles"))?;

    rows.iter()
        .rev()
        .map(|row| {
            Ok(CandleItem {
                ts: row.try_get("ts")?,
                o: parse_decimal(row, "o")?,
                h: parse_decimal(row, "h")?,
                l: parse_decimal(row, "l")?,
                c: parse_decimal(row, "c")?,
                v: parse_decimal(row, "vol")?,
                confirm: row
                    .try_get::<String, _>("confirm")?
                    .parse()
                    .context("invalid confirm value")?,
            })
        })
        .collect()
}

/// 把历史 VARCHAR 行情字段转换为回测使用的浮点数。
fn parse_decimal(row: &sqlx::postgres::PgRow, column: &str) -> Result<f64> {
    row.try_get::<String, _>(column)?
        .parse()
        .with_context(|| format!("invalid numeric value in {column}"))
}
