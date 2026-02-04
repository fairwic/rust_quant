use anyhow::{anyhow, Result};
use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use rust_quant_core::database::{get_db_pool, init_db_pool};
use rust_quant_indicators::trend::vegas::{VegasIndicatorSignalValue, VegasStrategy};
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::vegas_backtest::VegasBacktestAdapter;
use rust_quant_strategies::{get_multi_indicator_values, CandleItem, IndicatorStrategyBacktest};
use serde_json::json;
use sqlx::Row;
use std::env;

fn parse_f64(row: &sqlx::mysql::MySqlRow, col: &str) -> Result<f64> {
    let raw: String = row.get(col);
    raw.parse::<f64>()
        .map_err(|e| anyhow!("failed to parse {}='{}': {}", col, raw, e))
}

fn parse_i32(row: &sqlx::mysql::MySqlRow, col: &str) -> Result<i32> {
    let raw: String = row.get(col);
    raw.parse::<i32>()
        .map_err(|e| anyhow!("failed to parse {}='{}': {}", col, raw, e))
}

fn parse_args() -> Result<(i64, String, Option<String>, Option<String>, Option<usize>)> {
    let mut back_test_id: Option<i64> = None;
    let mut time_str: Option<String> = None;
    let mut inst_id: Option<String> = None;
    let mut period: Option<String> = None;
    let mut limit: Option<usize> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--back-test-id" => {
                back_test_id = args.next().and_then(|v| v.parse::<i64>().ok());
            }
            "--time" => {
                time_str = args.next();
            }
            "--inst-id" => {
                inst_id = args.next();
            }
            "--period" => {
                period = args.next();
            }
            "--limit" => {
                limit = args.next().and_then(|v| v.parse::<usize>().ok());
            }
            _ => {}
        }
    }

    let back_test_id = back_test_id.ok_or_else(|| anyhow!("missing --back-test-id"))?;
    let time_str = time_str.ok_or_else(|| anyhow!("missing --time"))?;
    Ok((back_test_id, time_str, inst_id, period, limit))
}

fn period_to_table_suffix(period: &str) -> Result<String> {
    let p = period.trim();
    let suffix = match p {
        "1M" => "1M".to_string(),
        _ => p.to_ascii_lowercase(),
    };
    Ok(suffix)
}

fn parse_shanghai_time_to_ms(time_str: &str) -> Result<i64> {
    let naive = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S")
        .map_err(|e| anyhow!("invalid time format: {}", e))?;
    let offset = FixedOffset::east_opt(8 * 3600).ok_or_else(|| anyhow!("invalid offset"))?;
    let dt = offset
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| anyhow!("ambiguous local datetime"))?;
    Ok(dt.timestamp_millis())
}

#[tokio::main]
async fn main() -> Result<()> {
    let (back_test_id, time_str, inst_id_override, period_override, limit_override) = parse_args()?;

    init_db_pool().await?;
    let pool = get_db_pool();

    let row = sqlx::query(
        "SELECT inst_type, time, strategy_detail, risk_config_detail FROM back_test_log WHERE id=?",
    )
    .bind(back_test_id)
    .fetch_one(pool)
    .await?;

    let inst_id: String = inst_id_override.unwrap_or_else(|| row.get::<String, _>("inst_type"));
    let period: String = period_override.unwrap_or_else(|| row.get::<String, _>("time"));
    let strategy_detail: String = row.get::<String, _>("strategy_detail");
    let risk_config_detail: String = row.get::<String, _>("risk_config_detail");

    let mut strategy: VegasStrategy = serde_json::from_str(&strategy_detail)
        .map_err(|e| anyhow!("failed to parse strategy_detail: {}", e))?;
    strategy.emit_debug = true;

    let risk_config: BasicRiskStrategyConfig = serde_json::from_str(&risk_config_detail)
        .map_err(|e| anyhow!("failed to parse risk_config_detail: {}", e))?;

    let target_ts = parse_shanghai_time_to_ms(&time_str)?;

    let suffix = period_to_table_suffix(&period)?;
    let table_name = format!("{}_candles_{}", inst_id.to_ascii_lowercase(), suffix);

    let min_len = strategy.min_k_line_num.max(1);
    let limit = limit_override.unwrap_or(min_len + 200);

    let query = format!(
        "SELECT ts, o, h, l, c, vol, confirm FROM `{}` WHERE ts <= ? ORDER BY ts DESC LIMIT ?",
        table_name
    );

    let rows = sqlx::query(&query)
        .bind(target_ts)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Err(anyhow!("no candles found before target time"));
    }

    let mut candles: Vec<CandleItem> = Vec::with_capacity(rows.len());
    for r in rows {
        let candle = CandleItem {
            ts: r.get::<i64, _>("ts"),
            o: parse_f64(&r, "o")?,
            h: parse_f64(&r, "h")?,
            l: parse_f64(&r, "l")?,
            c: parse_f64(&r, "c")?,
            v: parse_f64(&r, "vol")?,
            confirm: parse_i32(&r, "confirm")?,
        };
        candles.push(candle);
    }

    candles.sort_by_key(|c| c.ts);

    let mut adapter = VegasBacktestAdapter::new(strategy);
    let mut indicator_combine = adapter.init_indicator_combine();
    let weights = adapter
        .strategy()
        .signal_weights
        .clone()
        .unwrap_or_default();

    let mut buffer: Vec<CandleItem> = Vec::with_capacity(limit.max(1024));
    let mut found = false;

    for candle in candles.iter() {
        let mut values: VegasIndicatorSignalValue =
            get_multi_indicator_values(&mut indicator_combine, candle);
        buffer.push(candle.clone());

        if buffer.len() < min_len {
            continue;
        }

        let window = &buffer[buffer.len() - min_len..];
        let mut signal = adapter.generate_signal(window, &mut values, &risk_config);

        if signal.single_value.is_none() {
            signal.single_value = Some(serde_json::to_string(&values).unwrap_or_default());
        }

        if candle.ts == target_ts {
            found = true;
            let output = json!({
                "back_test_id": back_test_id,
                "inst_id": inst_id,
                "period": period,
                "target_time": time_str,
                "target_ts": target_ts,
                "candle": {
                    "ts": candle.ts,
                    "o": candle.o,
                    "h": candle.h,
                    "l": candle.l,
                    "c": candle.c,
                    "v": candle.v,
                    "confirm": candle.confirm,
                },
                "signal": signal,
                "indicator_values": values,
                "signal_weights": weights,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            break;
        }

        if buffer.len() > min_len * 2 {
            let drain = buffer.len() - min_len;
            buffer.drain(0..drain);
        }
    }

    if !found {
        let nearest = candles
            .iter()
            .min_by_key(|c| (c.ts - target_ts).abs())
            .map(|c| (c.ts, c.o, c.h, c.l, c.c))
            .unwrap();
        return Err(anyhow!(
            "target candle not found; nearest ts={} (o/h/l/c={}/{}/{}/{})",
            nearest.0,
            nearest.1,
            nearest.2,
            nearest.3,
            nearest.4
        ));
    }

    Ok(())
}
