use anyhow::{anyhow, Result};
use rust_quant_core::database::{get_db_pool, init_db_pool};
use rust_quant_indicators::trend::vegas::ema_filter::EmaDistanceState;
use rust_quant_indicators::trend::vegas::{VegasIndicatorSignalValue, VegasStrategy};
use rust_quant_market::quote_legacy_table_name;
use rust_quant_strategies::framework::backtest::types::BasicRiskStrategyConfig;
use rust_quant_strategies::implementations::vegas_backtest::VegasBacktestAdapter;
use rust_quant_strategies::{get_multi_indicator_values, CandleItem, IndicatorStrategyBacktest};
use serde_json::json;
use sqlx::Row;
use std::env;

fn parse_f64(row: &sqlx::postgres::PgRow, col: &str) -> Result<f64> {
    let raw: String = row.get(col);
    raw.parse::<f64>()
        .map_err(|e| anyhow!("failed to parse {}='{}': {}", col, raw, e))
}

fn parse_i32(row: &sqlx::postgres::PgRow, col: &str) -> Result<i32> {
    let raw: String = row.get(col);
    raw.parse::<i32>()
        .map_err(|e| anyhow!("failed to parse {}='{}': {}", col, raw, e))
}

#[allow(clippy::type_complexity)]
fn parse_args() -> Result<(i64, Option<String>, Option<String>, Option<usize>)> {
    let mut back_test_id: Option<i64> = None;
    let mut inst_id: Option<String> = None;
    let mut period: Option<String> = None;
    let mut limit: Option<usize> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--back-test-id" => {
                back_test_id = args.next().and_then(|v| v.parse::<i64>().ok());
            }
            "--inst-id" => inst_id = args.next(),
            "--period" => period = args.next(),
            "--limit" => {
                limit = args.next().and_then(|v| v.parse::<usize>().ok());
            }
            _ => {}
        }
    }

    let back_test_id = back_test_id.ok_or_else(|| anyhow!("missing --back-test-id"))?;
    Ok((back_test_id, inst_id, period, limit))
}

fn period_to_table_suffix(period: &str) -> String {
    match period.trim() {
        "1M" => "1M".to_string(),
        other => other.to_ascii_lowercase(),
    }
}

fn is_above_zero_death_cross_range_break_short_candidate(
    data_items: &[CandleItem],
    values: &VegasIndicatorSignalValue,
) -> bool {
    if data_items.len() < 7 {
        return false;
    }

    let current = data_items.last().expect("data items cannot be empty");
    let prior_window = &data_items[data_items.len() - 6..data_items.len() - 1];
    let prior_range_high = prior_window
        .iter()
        .map(|item| item.h)
        .fold(f64::MIN, f64::max);
    let prior_range_low = prior_window
        .iter()
        .map(|item| item.l)
        .fold(f64::MAX, f64::min);
    let prior_range_width = (prior_range_high - prior_range_low) / current.c.max(1e-9);
    let close_break_pct = (prior_range_low - current.c).max(0.0) / current.c.max(1e-9);
    let volume_ratio = values.volume_value.volume_ratio;
    let macd_val = &values.macd_value;
    let ema_values = &values.ema_values;
    let ema_distance = &values.ema_distance_filter;
    let structure = &values.market_structure_value;

    current.c < current.o
        && current.body_ratio() >= 0.6
        && volume_ratio >= 1.5
        && !ema_values.is_long_trend
        && !ema_values.is_short_trend
        && ema_distance.state == EmaDistanceState::TooFar
        && macd_val.above_zero
        && macd_val.is_death_cross
        && macd_val.histogram < 0.0
        && structure.swing_trend == 1
        && !structure.internal_bearish_bos
        && !structure.swing_bearish_bos
        && prior_range_width <= 0.03
        && close_break_pct >= 0.01
}

#[tokio::main]
async fn main() -> Result<()> {
    let (back_test_id, inst_id_override, period_override, limit_override) = parse_args()?;

    init_db_pool().await?;
    let pool = get_db_pool();

    let row = sqlx::query(
        "SELECT inst_type, time, strategy_detail, risk_config_detail FROM back_test_log WHERE id=$1",
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
    strategy.emit_debug = false;

    let risk_config: BasicRiskStrategyConfig = serde_json::from_str(&risk_config_detail)
        .map_err(|e| anyhow!("failed to parse risk_config_detail: {}", e))?;

    let suffix = period_to_table_suffix(&period);
    let table_name = quote_legacy_table_name(&format!(
        "{}_candles_{}",
        inst_id.to_ascii_lowercase(),
        suffix
    ))?;

    let limit = limit_override.unwrap_or(50_000);
    let query = format!(
        "SELECT ts, o, h, l, c, vol, confirm FROM {} ORDER BY ts ASC LIMIT $1",
        table_name
    );
    let rows = sqlx::query(&query)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;

    let mut candles: Vec<CandleItem> = Vec::with_capacity(rows.len());
    for r in rows {
        candles.push(CandleItem {
            ts: r.get::<i64, _>("ts"),
            o: parse_f64(&r, "o")?,
            h: parse_f64(&r, "h")?,
            l: parse_f64(&r, "l")?,
            c: parse_f64(&r, "c")?,
            v: parse_f64(&r, "vol")?,
            confirm: parse_i32(&r, "confirm")?,
        });
    }

    let min_len = strategy.min_k_line_num.max(1);
    let mut adapter = VegasBacktestAdapter::new(strategy);
    let mut indicator_combine = adapter.init_indicator_combine();
    let mut buffer: Vec<CandleItem> = Vec::with_capacity(4096);
    let mut matches = Vec::new();

    for candle in candles.iter() {
        let values: VegasIndicatorSignalValue =
            get_multi_indicator_values(&mut indicator_combine, candle);
        buffer.push(candle.clone());
        if buffer.len() < min_len {
            continue;
        }

        let window = &buffer[buffer.len() - min_len..];
        let signal = adapter.generate_signal(window, &mut values.clone(), &risk_config);

        if is_above_zero_death_cross_range_break_short_candidate(window, &values) {
            let signal_time = chrono::DateTime::from_timestamp_millis(candle.ts)
                .map(|dt| {
                    dt.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_else(|| candle.ts.to_string());
            let prior_window = &window[window.len() - 6..window.len() - 1];
            let prior_high = prior_window
                .iter()
                .map(|item| item.h)
                .fold(f64::MIN, f64::max);
            let prior_low = prior_window
                .iter()
                .map(|item| item.l)
                .fold(f64::MAX, f64::min);
            let prior_range_width = (prior_high - prior_low) / candle.c.max(1e-9);
            let close_break_pct = (prior_low - candle.c).max(0.0) / candle.c.max(1e-9);

            matches.push(json!({
                "time_ms": candle.ts,
                "signal_time": signal_time,
                "price": candle.c,
                "volume_ratio": values.volume_value.volume_ratio,
                "body_ratio": candle.body_ratio(),
                "prior_range_width": prior_range_width,
                "close_break_pct": close_break_pct,
                "macd_line": values.macd_value.macd_line,
                "signal_line": values.macd_value.signal_line,
                "histogram": values.macd_value.histogram,
                "is_death_cross": values.macd_value.is_death_cross,
                "ema_state": format!("{:?}", values.ema_distance_filter.state),
                "is_long_trend": values.ema_values.is_long_trend,
                "is_short_trend": values.ema_values.is_short_trend,
                "baseline_direction": signal.direction,
                "baseline_should_sell": signal.should_sell,
                "baseline_filter_reasons": signal.filter_reasons,
            }));
        }

        if buffer.len() > min_len * 2 {
            let drain = buffer.len() - min_len;
            buffer.drain(0..drain);
        }
    }

    let output = json!({
        "back_test_id": back_test_id,
        "inst_id": inst_id,
        "period": period,
        "match_count": matches.len(),
        "matches": matches,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}
