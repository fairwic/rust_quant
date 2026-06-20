impl VegasFactorResearchService {
    pub fn align_latest_snapshot(
        event_time: i64,
        snapshots: &[ExternalMarketSnapshot],
    ) -> Option<&ExternalMarketSnapshot> {
        snapshots
            .iter()
            .filter(|row| {
                row.metric_time <= event_time && event_time - row.metric_time <= FOUR_HOURS_MS
            })
            .max_by_key(|row| row.metric_time)
    }

    pub fn classify_price_oi_state(
        price_change: Option<f64>,
        oi_change: Option<f64>,
    ) -> PriceOiState {
        match (price_change, oi_change) {
            (Some(price), Some(oi)) if price > 0.0 && oi > 0.0 => PriceOiState::LongBuildup,
            (Some(price), Some(oi)) if price < 0.0 && oi > 0.0 => PriceOiState::ShortBuildup,
            (Some(price), Some(oi)) if price > 0.0 && oi < 0.0 => PriceOiState::ShortCovering,
            (Some(price), Some(oi)) if price < 0.0 && oi < 0.0 => PriceOiState::LongUnwinding,
            _ => PriceOiState::Flat,
        }
    }

    pub fn classify_funding_signal_contexts(
        funding_bucket: Option<&str>,
        side: &str,
        signal_value: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let Some(funding_bucket) = funding_bucket else {
            return Vec::new();
        };
        let side = Self::normalize_side(side);
        let mut contexts = vec![(
            "funding_direction_context",
            format!("{funding_bucket}_{side}"),
        )];

        let Some(signal_json) = signal_value.and_then(Self::parse_signal_json) else {
            return contexts;
        };

        if let Some(trend_bucket) = Self::trend_bucket(&signal_json) {
            contexts.push((
                "funding_trend_context",
                format!("{funding_bucket}_{side}_{trend_bucket}"),
            ));
        }
        if let Some(macd_bucket) = Self::macd_bucket(&signal_json) {
            contexts.push((
                "funding_macd_context",
                format!("{funding_bucket}_{side}_{macd_bucket}"),
            ));
        }
        if let Some(volume_bucket) = Self::volume_bucket(&signal_json) {
            contexts.push((
                "funding_volume_context",
                format!("{funding_bucket}_{side}_{volume_bucket}"),
            ));
        }

        contexts
    }

    pub fn classify_funding_filter_contexts(
        funding_bucket: Option<&str>,
        direction: &str,
        filter_reasons: Option<&str>,
        signal_value: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let Some(funding_bucket) = funding_bucket else {
            return Vec::new();
        };
        let Some(primary_reason) = filter_reasons.and_then(Self::primary_filter_reason) else {
            return Vec::new();
        };
        let side = Self::normalize_side(direction);
        let Some(signal_json) = signal_value.and_then(Self::parse_signal_json) else {
            return vec![(
                "funding_filter_context",
                format!("{funding_bucket}_{side}_{primary_reason}"),
            )];
        };

        let distance = Self::distance_bucket(&signal_json).unwrap_or("distance_unknown");
        let leg = Self::leg_bucket(&signal_json).unwrap_or("leg_unknown");
        vec![(
            "funding_filter_context",
            format!("{funding_bucket}_{side}_{primary_reason}_{distance}_{leg}"),
        )]
    }

    pub fn classify_internal_exit_contexts(
        side: &str,
        close_type: Option<&str>,
        _stop_loss_source: Option<&str>,
        signal_value: Option<&str>,
    ) -> Vec<(&'static str, String)> {
        let Some(exit_bucket) = close_type.and_then(Self::exit_bucket) else {
            return Vec::new();
        };
        let Some(signal_json) = signal_value.and_then(Self::parse_signal_json) else {
            return Vec::new();
        };
        let Some(trend_alignment) = Self::trend_alignment_bucket(&signal_json, side) else {
            return Vec::new();
        };
        let Some(macd_alignment) = Self::macd_alignment_bucket(&signal_json, side) else {
            return Vec::new();
        };
        let distance = Self::distance_bucket(&signal_json).unwrap_or("distance_unknown");
        let volume = Self::volume_bucket(&signal_json).unwrap_or("volume_unknown");

        vec![(
            "exit_environment_context",
            format!("{exit_bucket}_{trend_alignment}_{macd_alignment}_{distance}_{volume}"),
        )]
    }

    pub fn evaluate_factor_conclusion(reports: &[FactorBucketReport]) -> FactorConclusion {
        let traded = reports
            .iter()
            .find(|row| row.sample_kind == ResearchSampleKind::Traded);
        let filtered = reports
            .iter()
            .find(|row| row.sample_kind == ResearchSampleKind::Filtered);

        match (traded, filtered) {
            (Some(traded), Some(filtered))
                if traded.volatility_tier == VolatilityTier::Eth
                    && traded.sample_count >= 5
                    && traded.avg_pnl > 0.0
                    && traded.sharpe_proxy >= 0.5
                    && traded.avg_pnl > filtered.avg_pnl
                    && traded.sharpe_proxy > filtered.sharpe_proxy + 0.3 =>
            {
                FactorConclusion::Candidate
            }
            (Some(traded), _)
                if traded.volatility_tier == VolatilityTier::Eth
                    && traded.sample_count >= 3
                    && (traded.avg_pnl > 0.0 || traded.sharpe_proxy > 0.0) =>
            {
                FactorConclusion::Observe
            }
            (Some(traded), Some(filtered))
                if traded.sample_count >= 3
                    && traded.avg_pnl > filtered.avg_pnl
                    && traded.sharpe_proxy >= filtered.sharpe_proxy =>
            {
                FactorConclusion::Observe
            }
            (None, Some(filtered))
                if filtered.volatility_tier == VolatilityTier::Eth
                    && filtered.sample_count >= 4
                    && filtered.win_rate >= 0.7
                    && filtered.avg_pnl > 0.02
                    && filtered.sharpe_proxy >= 0.5 =>
            {
                FactorConclusion::Candidate
            }
            _ => FactorConclusion::Reject,
        }
    }

    fn parse_signal_json(signal_value: &str) -> Option<serde_json::Value> {
        serde_json::from_str(signal_value).ok()
    }

    fn normalize_side(side: &str) -> &'static str {
        match side.to_ascii_lowercase().as_str() {
            "long" => "long",
            "short" => "short",
            "buy" => "long",
            "sell" => "short",
            value if value.contains("long") => "long",
            value if value.contains("short") => "short",
            _ => "unknown",
        }
    }

    fn trend_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        let ema = signal.get("ema_values")?;
        match (
            ema.get("is_long_trend").and_then(|value| value.as_bool()),
            ema.get("is_short_trend").and_then(|value| value.as_bool()),
        ) {
            (Some(true), _) => Some("long_trend"),
            (_, Some(true)) => Some("short_trend"),
            (Some(false), Some(false)) => Some("mixed_trend"),
            _ => None,
        }
    }

    fn macd_bucket(signal: &serde_json::Value) -> Option<String> {
        let macd = signal.get("macd_value")?;
        let histogram = macd.get("histogram").and_then(|value| value.as_f64());
        let zone = match histogram {
            Some(value) if value >= 0.0 => "macd_above_zero",
            Some(_) => "macd_below_zero",
            None => match macd.get("above_zero").and_then(|value| value.as_bool()) {
                Some(true) => "macd_above_zero",
                Some(false) => "macd_below_zero",
                None => return None,
            },
        };
        let momentum = if macd
            .get("histogram_improving")
            .or_else(|| macd.get("histogram_increasing"))
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            "hist_improving"
        } else if macd
            .get("histogram_decreasing")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            "hist_decreasing"
        } else {
            "hist_flat"
        };

        Some(format!("{zone}_{momentum}"))
    }

    fn volume_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        let ratio = signal
            .get("volume_value")
            .and_then(|value| value.get("volume_ratio"))
            .and_then(|value| value.as_f64())?;
        if ratio >= 2.5 {
            Some("volume_extreme")
        } else if ratio >= 1.5 {
            Some("volume_expansion")
        } else if ratio < 0.8 {
            Some("volume_contract")
        } else {
            Some("volume_normal")
        }
    }

    fn exit_bucket(close_type: &str) -> Option<&'static str> {
        let lower = close_type.to_ascii_lowercase();
        if close_type.contains("Signal_Kline_Stop_Loss") || lower.contains("signal_kline_stop_loss")
        {
            Some("signal_stop")
        } else if close_type.contains("最大亏损止损") || lower.contains("max_loss") {
            Some("max_loss_stop")
        } else if close_type.contains("反向信号") || lower.contains("opposite") {
            Some("opposite_signal_close")
        } else if close_type.contains("止盈")
            || lower.contains("take_profit")
            || lower.contains("atr")
        {
            Some("take_profit")
        } else {
            None
        }
    }

    fn trend_alignment_bucket(signal: &serde_json::Value, side: &str) -> Option<&'static str> {
        match (Self::normalize_side(side), Self::trend_bucket(signal)?) {
            (_, "mixed_trend") => Some("mixed_trend"),
            ("long", "long_trend") | ("short", "short_trend") => Some("with_trend"),
            ("long", "short_trend") | ("short", "long_trend") => Some("counter_trend"),
            _ => Some("trend_unknown"),
        }
    }

    fn macd_alignment_bucket(signal: &serde_json::Value, side: &str) -> Option<&'static str> {
        let histogram = signal
            .get("macd_value")?
            .get("histogram")
            .and_then(|value| value.as_f64())?;
        match Self::normalize_side(side) {
            "long" if histogram >= 0.0 => Some("macd_align"),
            "long" => Some("macd_against"),
            "short" if histogram < 0.0 => Some("macd_align"),
            "short" => Some("macd_against"),
            _ => Some("macd_unknown"),
        }
    }

    fn distance_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        match signal
            .get("ema_distance_filter")?
            .get("state")?
            .as_str()?
            .to_ascii_lowercase()
            .as_str()
        {
            "toofar" => Some("distance_too_far"),
            "normal" => Some("distance_normal"),
            "tangled" => Some("distance_tangled"),
            _ => Some("distance_other"),
        }
    }

    fn leg_bucket(signal: &serde_json::Value) -> Option<&'static str> {
        let leg = signal.get("leg_detection_value")?;
        match (
            leg.get("is_bullish_leg").and_then(|value| value.as_bool()),
            leg.get("is_bearish_leg").and_then(|value| value.as_bool()),
        ) {
            (Some(true), _) => Some("bullish_leg"),
            (_, Some(true)) => Some("bearish_leg"),
            (Some(false), Some(false)) => Some("mixed_leg"),
            _ => None,
        }
    }

    fn primary_filter_reason(filter_reasons: &str) -> Option<String> {
        serde_json::from_str::<Vec<String>>(filter_reasons)
            .ok()
            .and_then(|values| values.into_iter().next())
            .map(|value| value.to_ascii_lowercase())
    }

    fn extract_flow_value(payload: &serde_json::Value) -> Option<f64> {
        [
            "netflow_usd",
            "transfer_value_usd",
            "amount_usd",
            "value_usd",
        ]
        .iter()
        .find_map(|key| payload.get(*key).and_then(|value| value.as_f64()))
    }

}
