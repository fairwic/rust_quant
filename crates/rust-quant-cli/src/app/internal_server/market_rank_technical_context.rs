use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub(super) struct MarketRankTechnicalSource<'a> {
    pub timeframe: Option<&'a str>,
    pub period: Option<i32>,
    pub close_price: Option<f64>,
    pub ma_value: Option<f64>,
    pub ema_value: Option<f64>,
    pub ma_distance_pct: Option<f64>,
    pub ema_distance_pct: Option<f64>,
    pub ma_state: Option<&'a str>,
    pub ema_state: Option<&'a str>,
    pub candle_count: Option<i32>,
    pub snapshot_at: Option<DateTime<Utc>>,
    pub snapshot_status: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MarketRankTechnicalContext {
    pub ma_4h: Option<MarketRankMovingAverageContext>,
    pub ema_4h: Option<MarketRankMovingAverageContext>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct MarketRankMovingAverageContext {
    pub timeframe: String,
    pub period: i32,
    pub ma_value: f64,
    pub latest_close: f64,
    pub distance_pct: f64,
    pub state: String,
    pub label: String,
    pub detail: String,
    pub tone: String,
}

pub(super) fn build_market_rank_technical_context(
    source: MarketRankTechnicalSource<'_>,
) -> Option<MarketRankTechnicalContext> {
    let captured_or_attempted = source
        .snapshot_status
        .is_some_and(|status| status != "not_requested");
    if source.timeframe != Some("4h") {
        return captured_or_attempted.then_some(MarketRankTechnicalContext {
            ma_4h: None,
            ema_4h: None,
        });
    }

    let period = source.period?;
    let latest_close = source.close_price?;
    let ma_4h = build_market_rank_moving_average_context(
        "MA",
        source.timeframe?,
        period,
        latest_close,
        source.ma_value,
        source.ma_distance_pct,
        source.ma_state,
        source.candle_count,
        source.snapshot_at,
    );
    let ema_4h = build_market_rank_moving_average_context(
        "EMA",
        source.timeframe?,
        period,
        latest_close,
        source.ema_value,
        source.ema_distance_pct,
        source.ema_state,
        source.candle_count,
        source.snapshot_at,
    );

    if ma_4h.is_none() && ema_4h.is_none() {
        return captured_or_attempted.then_some(MarketRankTechnicalContext {
            ma_4h: None,
            ema_4h: None,
        });
    }

    Some(MarketRankTechnicalContext { ma_4h, ema_4h })
}

fn build_market_rank_moving_average_context(
    average_label: &str,
    timeframe: &str,
    period: i32,
    latest_close: f64,
    value: Option<f64>,
    distance_pct: Option<f64>,
    state: Option<&str>,
    candle_count: Option<i32>,
    snapshot_at: Option<DateTime<Utc>>,
) -> Option<MarketRankMovingAverageContext> {
    let value = value?;
    let distance_pct = distance_pct?;
    let state = state?.to_string();
    let state_label = market_rank_moving_average_state_label(&state);
    let tone = market_rank_moving_average_tone(&state);
    let label = format!("{timeframe} {average_label}{period} {state_label}");
    let detail = format!(
        "收盘 {}，{average_label}{period} {}，偏离 {}{}",
        format_compact_float(latest_close),
        format_compact_float(value),
        format_signed_pct_text(distance_pct),
        format_snapshot_suffix(candle_count, snapshot_at)
    );

    Some(MarketRankMovingAverageContext {
        timeframe: timeframe.to_string(),
        period,
        ma_value: value,
        latest_close,
        distance_pct,
        state,
        label,
        detail,
        tone: tone.to_string(),
    })
}

fn market_rank_moving_average_state_label(state: &str) -> &'static str {
    match state {
        "breakout_up" => "突破",
        "breakdown_down" => "跌破",
        "above" => "站上",
        "below" => "低于",
        "touching" => "贴近",
        _ => "待确认",
    }
}

fn market_rank_moving_average_tone(state: &str) -> &'static str {
    match state {
        "breakout_up" | "above" => "positive",
        "breakdown_down" | "below" => "negative",
        _ => "neutral",
    }
}

fn format_compact_float(value: f64) -> String {
    if value.abs() >= 1.0 {
        format!("{value:.4}")
    } else {
        format!("{value:.8}")
    }
}

fn format_signed_pct_text(value: f64) -> String {
    format!("{value:+.2}%")
}

fn format_snapshot_suffix(candle_count: Option<i32>, snapshot_at: Option<DateTime<Utc>>) -> String {
    match (candle_count, snapshot_at) {
        (Some(candle_count), Some(snapshot_at)) => {
            format!(
                "，样本 {candle_count} 根，快照 {}",
                snapshot_at.format("%m-%d %H:%M")
            )
        }
        (Some(candle_count), None) => format!("，样本 {candle_count} 根"),
        (None, Some(snapshot_at)) => format!("，快照 {}", snapshot_at.format("%m-%d %H:%M")),
        (None, None) => String::new(),
    }
}
