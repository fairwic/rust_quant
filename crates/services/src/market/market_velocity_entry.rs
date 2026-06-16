use chrono::{DateTime, Utc};
use rust_quant_domain::Candle;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq)]
pub struct MarketVelocityEntryConfirmationConfig {
    pub period: usize,
    pub max_average_distance_pct: f64,
    pub min_volume_ratio: f64,
}

impl Default for MarketVelocityEntryConfirmationConfig {
    fn default() -> Self {
        Self {
            period: 20,
            max_average_distance_pct: 1.5,
            min_volume_ratio: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MarketVelocityEntryConfirmation {
    pub timeframe: String,
    pub period: usize,
    pub trigger: String,
    pub latest_close: f64,
    pub previous_close: Option<f64>,
    pub previous_high: Option<f64>,
    pub ma_value: f64,
    pub ema_value: f64,
    pub ma_distance_pct: f64,
    pub ema_distance_pct: f64,
    pub volume_ratio: Option<f64>,
    pub candle_count: usize,
    pub snapshot_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketVelocityEntryConfirmationBlocker {
    InsufficientCandles,
    InvalidAverages,
    PriceBelowAverages,
    OverextendedFromAverages,
    VolumeNotConfirmed,
    TimingTriggerNotConfirmed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarketVelocityEntryConfirmationDecision {
    Confirmed(MarketVelocityEntryConfirmation),
    Blocked(MarketVelocityEntryConfirmationBlocker),
}

pub fn build_market_velocity_entry_confirmation_from_candles(
    timeframe: &str,
    candles: &[Candle],
    config: &MarketVelocityEntryConfirmationConfig,
) -> MarketVelocityEntryConfirmationDecision {
    if config.period < 2 || candles.len() <= config.period {
        return MarketVelocityEntryConfirmationDecision::Blocked(
            MarketVelocityEntryConfirmationBlocker::InsufficientCandles,
        );
    }

    let mut candles = candles.to_vec();
    candles.sort_by_key(|candle| candle.timestamp);

    let closes = candles
        .iter()
        .map(|candle| candle.close.value())
        .collect::<Vec<_>>();
    if closes
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return MarketVelocityEntryConfirmationDecision::Blocked(
            MarketVelocityEntryConfirmationBlocker::InvalidAverages,
        );
    }

    let period = config.period;
    let candle_count = closes.len();
    let latest = match candles.last() {
        Some(candle) => candle,
        None => {
            return MarketVelocityEntryConfirmationDecision::Blocked(
                MarketVelocityEntryConfirmationBlocker::InsufficientCandles,
            )
        }
    };
    let previous = candles.get(candle_count - 2);
    let latest_close = latest.close.value();
    let previous_close = previous.map(|candle| candle.close.value());
    let previous_high = previous.map(|candle| candle.high.value());
    let ma_value = match simple_moving_average(&closes[candle_count - period..]) {
        Some(value) => value,
        None => {
            return MarketVelocityEntryConfirmationDecision::Blocked(
                MarketVelocityEntryConfirmationBlocker::InvalidAverages,
            )
        }
    };
    let ema_value = match exponential_moving_average(&closes, period) {
        Some(value) => value,
        None => {
            return MarketVelocityEntryConfirmationDecision::Blocked(
                MarketVelocityEntryConfirmationBlocker::InvalidAverages,
            )
        }
    };
    let previous_ma = simple_moving_average(&closes[candle_count - period - 1..candle_count - 1]);
    let previous_ema = exponential_moving_average(&closes[..candle_count - 1], period);
    let ma_distance_pct = match moving_average_distance_pct(latest_close, ma_value) {
        Some(value) => value,
        None => {
            return MarketVelocityEntryConfirmationDecision::Blocked(
                MarketVelocityEntryConfirmationBlocker::InvalidAverages,
            )
        }
    };
    let ema_distance_pct = match moving_average_distance_pct(latest_close, ema_value) {
        Some(value) => value,
        None => {
            return MarketVelocityEntryConfirmationDecision::Blocked(
                MarketVelocityEntryConfirmationBlocker::InvalidAverages,
            )
        }
    };

    if latest_close <= ma_value || latest_close <= ema_value {
        return MarketVelocityEntryConfirmationDecision::Blocked(
            MarketVelocityEntryConfirmationBlocker::PriceBelowAverages,
        );
    }

    if config.max_average_distance_pct > 0.0
        && (ma_distance_pct > config.max_average_distance_pct
            || ema_distance_pct > config.max_average_distance_pct)
    {
        return MarketVelocityEntryConfirmationDecision::Blocked(
            MarketVelocityEntryConfirmationBlocker::OverextendedFromAverages,
        );
    }

    let volume_ratio = latest_volume_ratio(&candles, period);
    if config.min_volume_ratio > 0.0 {
        match volume_ratio {
            Some(ratio) if ratio >= config.min_volume_ratio => {}
            _ => {
                return MarketVelocityEntryConfirmationDecision::Blocked(
                    MarketVelocityEntryConfirmationBlocker::VolumeNotConfirmed,
                )
            }
        }
    }

    let trigger = entry_trigger(
        latest,
        previous,
        ma_value,
        ema_value,
        previous_ma,
        previous_ema,
    );
    let Some(trigger) = trigger else {
        return MarketVelocityEntryConfirmationDecision::Blocked(
            MarketVelocityEntryConfirmationBlocker::TimingTriggerNotConfirmed,
        );
    };

    MarketVelocityEntryConfirmationDecision::Confirmed(MarketVelocityEntryConfirmation {
        timeframe: timeframe.to_string(),
        period,
        trigger: trigger.to_string(),
        latest_close: round_metric(latest_close),
        previous_close: previous_close.map(round_metric),
        previous_high: previous_high.map(round_metric),
        ma_value: round_metric(ma_value),
        ema_value: round_metric(ema_value),
        ma_distance_pct: round_metric(ma_distance_pct),
        ema_distance_pct: round_metric(ema_distance_pct),
        volume_ratio: volume_ratio.map(round_metric),
        candle_count,
        snapshot_at: latest.datetime,
    })
}

fn entry_trigger<'a>(
    latest: &Candle,
    previous: Option<&Candle>,
    ma_value: f64,
    ema_value: f64,
    previous_ma: Option<f64>,
    previous_ema: Option<f64>,
) -> Option<&'a str> {
    let previous_close = previous.map(|candle| candle.close.value())?;
    let previous_high = previous.map(|candle| candle.high.value())?;
    let latest_close = latest.close.value();

    if previous_ema.is_some_and(|value| previous_close <= value) && latest_close > ema_value {
        return Some("reclaim_ema");
    }
    if previous_ma.is_some_and(|value| previous_close <= value) && latest_close > ma_value {
        return Some("reclaim_ma");
    }
    if latest_close > previous_high {
        return Some("breakout_previous_high");
    }
    if latest.low.value() <= ema_value && latest.is_bullish() && latest_close > ema_value {
        return Some("pullback_hold_ema");
    }

    None
}

fn latest_volume_ratio(candles: &[Candle], period: usize) -> Option<f64> {
    let latest_volume = candles.last()?.volume.value();
    if !latest_volume.is_finite() {
        return None;
    }
    let end = candles.len().checked_sub(1)?;
    let start = end.checked_sub(period)?;
    let average_volume = simple_moving_average(
        &candles[start..end]
            .iter()
            .map(|candle| candle.volume.value())
            .collect::<Vec<_>>(),
    )?;
    if average_volume <= 0.0 || !average_volume.is_finite() {
        return None;
    }
    Some(latest_volume / average_volume)
}

fn simple_moving_average(values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn exponential_moving_average(values: &[f64], period: usize) -> Option<f64> {
    if period == 0 || values.len() < period || values.iter().any(|value| !value.is_finite()) {
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

fn round_metric(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::{Price, Timeframe, Volume};

    fn candle(timestamp: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle::new(
            "ETH-USDT-SWAP".to_string(),
            Timeframe::M15,
            timestamp,
            Price::new(open).expect("valid open"),
            Price::new(high).expect("valid high"),
            Price::new(low).expect("valid low"),
            Price::new(close).expect("valid close"),
            Volume::new(volume).expect("valid volume"),
        )
    }

    #[test]
    fn confirms_15m_reclaim_near_ema_with_volume() {
        let mut candles = (0..20)
            .map(|index| {
                candle(
                    1_700_000_000_000 + index * 900_000,
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    100.0,
                )
            })
            .collect::<Vec<_>>();
        candles.push(candle(1_700_018_000_000, 100.2, 101.8, 99.8, 101.4, 130.0));

        let decision = build_market_velocity_entry_confirmation_from_candles(
            "15m",
            &candles,
            &MarketVelocityEntryConfirmationConfig::default(),
        );

        let MarketVelocityEntryConfirmationDecision::Confirmed(confirmation) = decision else {
            panic!("15m reclaim should be confirmed");
        };
        assert_eq!(confirmation.timeframe, "15m");
        assert_eq!(confirmation.trigger, "reclaim_ema");
        assert_eq!(confirmation.period, 20);
        assert_eq!(confirmation.candle_count, 21);
        assert!(confirmation.volume_ratio.expect("volume ratio") > 1.0);
    }

    #[test]
    fn blocks_15m_when_price_is_too_far_from_averages() {
        let mut candles = (0..20)
            .map(|index| {
                candle(
                    1_700_000_000_000 + index * 900_000,
                    100.0,
                    101.0,
                    99.0,
                    100.0,
                    100.0,
                )
            })
            .collect::<Vec<_>>();
        candles.push(candle(1_700_018_000_000, 100.0, 120.0, 99.0, 115.0, 150.0));

        assert_eq!(
            build_market_velocity_entry_confirmation_from_candles(
                "15m",
                &candles,
                &MarketVelocityEntryConfirmationConfig::default(),
            ),
            MarketVelocityEntryConfirmationDecision::Blocked(
                MarketVelocityEntryConfirmationBlocker::OverextendedFromAverages
            )
        );
    }
}
