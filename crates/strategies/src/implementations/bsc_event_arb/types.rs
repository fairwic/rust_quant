use rust_quant_domain::SignalDirection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::strategy_common::SignalResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BscEventArbAction {
    Long,
    Flat,
    ForceExit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BscEventArbThresholds {
    pub min_volume_24h_usd: f64,
    pub min_volume_1h_vs_24h_avg: f64,
    pub min_depth_2pct_usd: f64,
    pub max_tax_pct: f64,
    pub min_cex_volume_share: f64,
    pub min_price_change_15m_pct: f64,
    pub min_price_change_1h_pct: f64,
    pub min_volume_zscore: f64,
    pub min_oi_growth_1h_pct: f64,
    pub min_oi_growth_4h_pct: f64,
    pub min_short_crowding_score: f64,
    pub large_cex_flow_usd: f64,
    pub hard_stop_loss_pct: f64,
    pub first_take_profit_pct: f64,
    pub second_take_profit_pct: f64,
    pub trailing_stop_pct: f64,
    pub max_event_hold_minutes: i64,
    pub time_stop_minutes: i64,
    pub min_time_stop_profit_pct: f64,
    pub max_oi_drop_1h_pct: f64,
}

impl Default for BscEventArbThresholds {
    fn default() -> Self {
        Self {
            min_volume_24h_usd: 5_000_000.0,
            min_volume_1h_vs_24h_avg: 5.0,
            min_depth_2pct_usd: 50_000.0,
            max_tax_pct: 5.0,
            min_cex_volume_share: 0.40,
            min_price_change_15m_pct: 8.0,
            min_price_change_1h_pct: 20.0,
            min_volume_zscore: 3.0,
            min_oi_growth_1h_pct: 30.0,
            min_oi_growth_4h_pct: 80.0,
            min_short_crowding_score: 0.65,
            large_cex_flow_usd: 250_000.0,
            hard_stop_loss_pct: -10.0,
            first_take_profit_pct: 25.0,
            second_take_profit_pct: 60.0,
            trailing_stop_pct: 20.0,
            max_event_hold_minutes: 24 * 60,
            time_stop_minutes: 30,
            min_time_stop_profit_pct: 8.0,
            max_oi_drop_1h_pct: 25.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BscEventArbStrategyConfig {
    pub strategy_name: Option<String>,
    pub thresholds: BscEventArbThresholds,
    pub snapshot: Option<BscEventArbSignalSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BscEventArbSignalSnapshot {
    pub chain_id: String,
    pub event_tags: Vec<String>,
    pub price_usd: f64,
    pub volume_24h_usd: f64,
    pub volume_1h_vs_24h_avg: f64,
    pub depth_2pct_usd: f64,
    pub is_dex_only: bool,
    pub sell_simulation_passed: bool,
    pub buy_tax_pct: f64,
    pub sell_tax_pct: f64,
    pub has_blacklist_risk: bool,
    pub has_pause_risk: bool,
    pub has_mint_risk: bool,
    pub cex_volume_share: f64,
    pub price_change_15m_pct: f64,
    pub price_change_1h_pct: f64,
    pub price_above_15m_vwap: bool,
    pub volume_zscore_5m: f64,
    pub volume_zscore_15m: f64,
    pub oi_growth_1h_pct: f64,
    pub oi_growth_4h_pct: f64,
    pub funding_rate: f64,
    pub short_crowding_score: f64,
    pub price_up_with_oi: bool,
    pub cex_net_inflow_usd: f64,
    pub price_resilient_after_inflow: bool,
    pub cex_outflow_after_inflow: bool,
    pub spot_absorption: bool,
    pub price_below_15m_vwap: bool,
    pub oi_drop_1h_pct: f64,
    pub funding_flipped_positive: bool,
    pub price_making_new_high: bool,
    pub top_holder_or_lp_abnormal_outflow: bool,
    pub cex_withdrawal_or_trading_restriction: bool,
    pub minutes_since_entry: i64,
    pub max_unrealized_profit_pct: f64,
    pub trailing_drawdown_pct: f64,
    pub price_change_from_entry_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BscEventArbDecision {
    pub action: BscEventArbAction,
    pub reasons: Vec<String>,
}

impl BscEventArbDecision {
    pub fn has_reason(&self, reason: &str) -> bool {
        self.reasons.iter().any(|item| item == reason)
    }

    pub fn to_signal(&self, price: f64, ts: i64) -> SignalResult {
        let mut signal = SignalResult {
            open_price: price,
            ts,
            filter_reasons: self.reasons.clone(),
            single_result: Some(self.result_payload().to_string()),
            ..Default::default()
        };

        match self.action {
            BscEventArbAction::Long => self.apply_long_signal(&mut signal, price),
            BscEventArbAction::ForceExit => self.apply_force_exit_signal(&mut signal),
            BscEventArbAction::Flat => {}
        }

        signal
    }

    fn result_payload(&self) -> Value {
        json!({
            "strategy": "bsc_event_arb",
            "action": self.action_name(),
            "reasons": self.reasons,
        })
    }

    fn action_name(&self) -> &'static str {
        match self.action {
            BscEventArbAction::Long => "long",
            BscEventArbAction::Flat => "flat",
            BscEventArbAction::ForceExit => "force_exit",
        }
    }

    fn apply_long_signal(&self, signal: &mut SignalResult, price: f64) {
        signal.should_buy = true;
        signal.direction = SignalDirection::Long;
        signal.signal_kline_stop_loss_price = Some(price * 0.90);
        signal.long_signal_take_profit_price = Some(price * 1.25);
        signal.atr_take_profit_level_1 = Some(price * 1.25);
        signal.atr_take_profit_level_2 = Some(price * 1.60);
        signal.dynamic_adjustments = vec!["BSC_EVENT_ARB_EVENT_LONG".to_string()];
    }

    fn apply_force_exit_signal(&self, signal: &mut SignalResult) {
        signal.direction = SignalDirection::Close;
        signal.dynamic_adjustments = vec!["BSC_EVENT_ARB_FORCE_EXIT".to_string()];
    }
}
