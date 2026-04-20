use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_strategies::implementations::BscEventArbSignalSnapshot;
use serde_json::Value;
use std::collections::HashMap;

pub struct BscEventArbSnapshotBuilder;

impl BscEventArbSnapshotBuilder {
    pub fn build(
        symbol: &str,
        rows: &[ExternalMarketSnapshot],
    ) -> Option<BscEventArbSignalSnapshot> {
        let latest = Self::latest_by_metric_type(symbol, rows);
        if latest.is_empty() {
            return None;
        }

        let mut snapshot = BscEventArbSignalSnapshot::default();
        Self::apply_pair(latest.get("bsc_pair").copied(), &mut snapshot);
        Self::apply_security(latest.get("bsc_security").copied(), &mut snapshot);
        Self::apply_cex_market(latest.get("cex_market").copied(), &mut snapshot);
        Self::apply_derivatives(latest.get("derivatives").copied(), &mut snapshot);
        Self::apply_cex_flow(latest.get("cex_flow").copied(), &mut snapshot);
        Self::apply_holder(latest.get("holder_concentration").copied(), &mut snapshot);
        Some(snapshot)
    }

    fn latest_by_metric_type<'a>(
        symbol: &str,
        rows: &'a [ExternalMarketSnapshot],
    ) -> HashMap<String, &'a ExternalMarketSnapshot> {
        let mut latest = HashMap::new();
        for row in rows
            .iter()
            .filter(|row| row.symbol.eq_ignore_ascii_case(symbol))
        {
            latest
                .entry(row.metric_type.clone())
                .and_modify(|current: &mut &ExternalMarketSnapshot| {
                    if row.metric_time > current.metric_time {
                        *current = row;
                    }
                })
                .or_insert(row);
        }
        latest
    }

    fn apply_pair(row: Option<&ExternalMarketSnapshot>, snapshot: &mut BscEventArbSignalSnapshot) {
        let Some(payload) = row.and_then(Self::payload) else {
            return;
        };
        snapshot.chain_id = Self::str_value(payload, "chain_id");
        snapshot.event_tags = Self::string_array(payload, "event_tags");
        snapshot.price_usd = Self::f64_value(payload, "price_usd");
        snapshot.volume_24h_usd = Self::f64_value(payload, "volume_24h_usd");
        snapshot.volume_1h_vs_24h_avg = Self::f64_value(payload, "volume_1h_vs_24h_avg");
        snapshot.depth_2pct_usd = Self::f64_value(payload, "depth_2pct_usd");
        snapshot.is_dex_only = Self::bool_value(payload, "is_dex_only");
    }

    fn apply_security(
        row: Option<&ExternalMarketSnapshot>,
        snapshot: &mut BscEventArbSignalSnapshot,
    ) {
        let Some(payload) = row.and_then(Self::payload) else {
            return;
        };
        snapshot.sell_simulation_passed = Self::bool_value(payload, "sell_simulation_passed");
        snapshot.buy_tax_pct = Self::f64_value(payload, "buy_tax_pct");
        snapshot.sell_tax_pct = Self::f64_value(payload, "sell_tax_pct");
        snapshot.has_blacklist_risk = Self::bool_value(payload, "has_blacklist_risk");
        snapshot.has_pause_risk = Self::bool_value(payload, "has_pause_risk");
        snapshot.has_mint_risk = Self::bool_value(payload, "has_mint_risk");
    }

    fn apply_cex_market(
        row: Option<&ExternalMarketSnapshot>,
        snapshot: &mut BscEventArbSignalSnapshot,
    ) {
        let Some(payload) = row.and_then(Self::payload) else {
            return;
        };
        snapshot.cex_volume_share = Self::f64_value(payload, "cex_volume_share");
        snapshot.price_change_15m_pct = Self::f64_value(payload, "price_change_15m_pct");
        snapshot.price_change_1h_pct = Self::f64_value(payload, "price_change_1h_pct");
        snapshot.price_above_15m_vwap = Self::bool_value(payload, "price_above_15m_vwap");
        snapshot.price_below_15m_vwap = Self::bool_value(payload, "price_below_15m_vwap");
        snapshot.volume_zscore_5m = Self::f64_value(payload, "volume_zscore_5m");
        snapshot.volume_zscore_15m = Self::f64_value(payload, "volume_zscore_15m");
        snapshot.minutes_since_entry = Self::i64_value(payload, "minutes_since_entry");
        snapshot.max_unrealized_profit_pct = Self::f64_value(payload, "max_unrealized_profit_pct");
        snapshot.trailing_drawdown_pct = Self::f64_value(payload, "trailing_drawdown_pct");
        snapshot.price_change_from_entry_pct =
            Self::f64_value(payload, "price_change_from_entry_pct");
    }

    fn apply_derivatives(
        row: Option<&ExternalMarketSnapshot>,
        snapshot: &mut BscEventArbSignalSnapshot,
    ) {
        let Some(row) = row else {
            return;
        };
        let payload = row.raw_payload.as_ref();
        snapshot.oi_growth_1h_pct = Self::payload_f64(payload, "oi_growth_1h_pct");
        snapshot.oi_growth_4h_pct = Self::payload_f64(payload, "oi_growth_4h_pct");
        snapshot.funding_rate = row
            .funding_rate
            .unwrap_or_else(|| Self::payload_f64(payload, "funding_rate"));
        snapshot.short_crowding_score = Self::payload_f64(payload, "short_crowding_score");
        snapshot.price_up_with_oi = Self::payload_bool(payload, "price_up_with_oi");
        snapshot.oi_drop_1h_pct = Self::payload_f64(payload, "oi_drop_1h_pct");
        snapshot.funding_flipped_positive = Self::payload_bool(payload, "funding_flipped_positive");
        snapshot.price_making_new_high = Self::payload_bool(payload, "price_making_new_high");
    }

    fn apply_cex_flow(
        row: Option<&ExternalMarketSnapshot>,
        snapshot: &mut BscEventArbSignalSnapshot,
    ) {
        let Some(payload) = row.and_then(Self::payload) else {
            return;
        };
        snapshot.cex_net_inflow_usd = Self::f64_value(payload, "cex_net_inflow_usd");
        snapshot.price_resilient_after_inflow =
            Self::bool_value(payload, "price_resilient_after_inflow");
        snapshot.cex_outflow_after_inflow = Self::bool_value(payload, "cex_outflow_after_inflow");
        snapshot.spot_absorption = Self::bool_value(payload, "spot_absorption");
        snapshot.cex_withdrawal_or_trading_restriction =
            Self::bool_value(payload, "cex_withdrawal_or_trading_restriction");
    }

    fn apply_holder(
        row: Option<&ExternalMarketSnapshot>,
        snapshot: &mut BscEventArbSignalSnapshot,
    ) {
        let Some(payload) = row.and_then(Self::payload) else {
            return;
        };
        snapshot.top_holder_or_lp_abnormal_outflow =
            Self::bool_value(payload, "top_holder_or_lp_abnormal_outflow");
    }

    fn payload(row: &ExternalMarketSnapshot) -> Option<&Value> {
        row.raw_payload.as_ref()
    }

    fn f64_value(payload: &Value, key: &str) -> f64 {
        payload.get(key).and_then(Value::as_f64).unwrap_or_default()
    }

    fn bool_value(payload: &Value, key: &str) -> bool {
        payload
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or_default()
    }

    fn i64_value(payload: &Value, key: &str) -> i64 {
        payload.get(key).and_then(Value::as_i64).unwrap_or_default()
    }

    fn str_value(payload: &Value, key: &str) -> String {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn string_array(payload: &Value, key: &str) -> Vec<String> {
        payload
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect()
    }

    fn payload_f64(payload: Option<&Value>, key: &str) -> f64 {
        payload
            .map(|value| Self::f64_value(value, key))
            .unwrap_or_default()
    }

    fn payload_bool(payload: Option<&Value>, key: &str) -> bool {
        payload
            .map(|value| Self::bool_value(value, key))
            .unwrap_or_default()
    }
}
