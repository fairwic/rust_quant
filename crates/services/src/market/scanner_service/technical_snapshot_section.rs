fn build_market_rank_technical_snapshot_from_candles(
    timeframe: &str,
    period: usize,
    candles: &[Candle],
) -> Option<MarketRankTechnicalSnapshot> {
    let mut candles = candles.to_vec();
    candles.sort_by_key(|candle| candle.timestamp);

    let snapshot_at = candles.last()?.datetime;
    let closes = candles
        .iter()
        .map(|candle| candle.close.value())
        .collect::<Vec<_>>();
    build_market_rank_technical_snapshot_from_closes(timeframe, period, &closes, snapshot_at)
}

fn build_market_rank_technical_snapshot_from_closes(
    timeframe: &str,
    period: usize,
    closes: &[f64],
    snapshot_at: DateTime<Utc>,
) -> Option<MarketRankTechnicalSnapshot> {
    if period == 0 || closes.len() < period || closes.iter().any(|value| !value.is_finite()) {
        return None;
    }

    let latest_close = *closes.last()?;
    let ma_value = simple_moving_average(&closes[closes.len() - period..])?;
    let ema_value = exponential_moving_average(closes, period)?;
    let previous_close = closes.get(closes.len().checked_sub(2)?).copied();
    let previous_ma = if closes.len() > period {
        simple_moving_average(&closes[closes.len() - period - 1..closes.len() - 1])
    } else {
        None
    };
    let previous_ema = if closes.len() > period {
        exponential_moving_average(&closes[..closes.len() - 1], period)
    } else {
        None
    };

    Some(MarketRankTechnicalSnapshot {
        timeframe: timeframe.to_string(),
        period: period as i32,
        close_price: decimal_from_f64(latest_close)?,
        ma_value: decimal_from_f64(ma_value)?,
        ema_value: decimal_from_f64(ema_value)?,
        ma_distance_pct: decimal_from_f64(moving_average_distance_pct(latest_close, ma_value)?)?,
        ema_distance_pct: decimal_from_f64(moving_average_distance_pct(latest_close, ema_value)?)?,
        ma_state: moving_average_state(latest_close, ma_value, previous_close, previous_ma),
        ema_state: moving_average_state(latest_close, ema_value, previous_close, previous_ema),
        candle_count: closes.len() as i32,
        snapshot_at,
    })
}

fn simple_moving_average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn exponential_moving_average(values: &[f64], period: usize) -> Option<f64> {
    if values.len() < period {
        return None;
    }

    let mut ema = simple_moving_average(&values[..period])?;
    let multiplier = 2.0 / (period as f64 + 1.0);
    for value in &values[period..] {
        ema = (*value - ema) * multiplier + ema;
    }
    Some(ema)
}

fn moving_average_distance_pct(close: f64, average: f64) -> Option<f64> {
    if average <= 0.0 || !average.is_finite() || !close.is_finite() {
        return None;
    }
    Some((close - average) / average * 100.0)
}

fn moving_average_state(
    close: f64,
    average: f64,
    previous_close: Option<f64>,
    previous_average: Option<f64>,
) -> String {
    if let (Some(previous_close), Some(previous_average)) = (previous_close, previous_average) {
        if close > average && previous_close <= previous_average {
            return "breakout_up".to_string();
        }
        if close < average && previous_close >= previous_average {
            return "breakdown_down".to_string();
        }
    }

    let distance_pct = moving_average_distance_pct(close, average).unwrap_or(0.0);
    if distance_pct.abs() <= MARKET_RANK_TECHNICAL_TOUCH_THRESHOLD_PCT {
        "touching".to_string()
    } else if close > average {
        "above".to_string()
    } else {
        "below".to_string()
    }
}

fn decimal_from_f64(value: f64) -> Option<Decimal> {
    Decimal::from_f64(value).map(|value| value.round_dp(12))
}
