use anyhow::Result;
use rust_quant_common::CandleItem;
use sqlx::Row;
use std::env;

/// 直接分析K线数据，不通过策略框架
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:example@localhost:5432/quant_core".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

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

    let mut candles: Vec<CandleItem> = Vec::new();
    for row in rows.iter().rev() {
        candles.push(CandleItem {
            ts: row.try_get("ts")?,
            o: row.try_get::<String, _>("o")?.parse()?,
            h: row.try_get::<String, _>("h")?.parse()?,
            l: row.try_get::<String, _>("l")?.parse()?,
            c: row.try_get::<String, _>("c")?.parse()?,
            v: row.try_get::<String, _>("vol")?.parse()?,
            confirm: row.try_get::<String, _>("confirm")?.parse()?,
        });
    }

    println!("========== 原始数据分析 ==========\n");
    println!("总K线数: {}\n", candles.len());

    // 从第520根开始分析（跳过warmup）
    let lookback = 20;
    let mut breakout_count = 0;
    let mut close_breakout_count = 0;
    let mut wick_breakout_count = 0;
    let mut total_analyzed = 0;

    for i in (lookback + 500)..candles.len() {
        let range_start = i - lookback;
        let range_candles = &candles[range_start..i];
        let last = &candles[i];

        // 计算震荡区间
        let range_high = range_candles
            .iter()
            .map(|c| c.h)
            .fold(f64::NEG_INFINITY, f64::max);
        let range_low = range_candles
            .iter()
            .map(|c| c.l)
            .fold(f64::INFINITY, f64::min);

        // 检查突破
        let close_breakout = last.c < range_low;
        let is_bearish = last.c < last.o;
        let low_touched = last.l < range_low;
        let body_size = (last.o - last.c).max(0.0);
        let wick_breakout = low_touched && is_bearish && body_size > 0.0;

        total_analyzed += 1;

        if close_breakout {
            close_breakout_count += 1;
            if breakout_count < 10 {
                println!(
                    "收盘价突破 #{}: range_low={:.2}, close={:.2}, 差值={:.2}",
                    breakout_count + 1,
                    range_low,
                    last.c,
                    range_low - last.c
                );
            }
            breakout_count += 1;
        } else if wick_breakout {
            wick_breakout_count += 1;
            if breakout_count < 10 {
                println!(
                    "最低价触及 #{}: range_low={:.2}, low={:.2}, close={:.2}, 阴线实体={:.2}",
                    breakout_count + 1,
                    range_low,
                    last.l,
                    last.c,
                    body_size
                );
            }
            breakout_count += 1;
        }
    }

    println!("\n========== 统计结果 ==========");
    println!("分析的K线数: {}", total_analyzed);
    println!(
        "收盘价突破次数: {} ({:.1}%)",
        close_breakout_count,
        (close_breakout_count as f64 / total_analyzed as f64) * 100.0
    );
    println!(
        "最低价触及次数: {} ({:.1}%)",
        wick_breakout_count,
        (wick_breakout_count as f64 / total_analyzed as f64) * 100.0
    );
    println!(
        "任一突破次数: {} ({:.1}%)",
        breakout_count,
        (breakout_count as f64 / total_analyzed as f64) * 100.0
    );
    println!(
        "未突破次数: {} ({:.1}%)",
        total_analyzed - breakout_count,
        ((total_analyzed - breakout_count) as f64 / total_analyzed as f64) * 100.0
    );

    println!("\n================================");

    Ok(())
}
