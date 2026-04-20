use super::types::{
    BscEventArbAction, BscEventArbDecision, BscEventArbSignalSnapshot, BscEventArbStrategyConfig,
    BscEventArbThresholds,
};
use crate::strategy_common::SignalResult;

pub struct BscEventArbStrategy;

impl BscEventArbStrategy {
    pub fn evaluate(
        config: &BscEventArbStrategyConfig,
        snapshot: &BscEventArbSignalSnapshot,
    ) -> BscEventArbDecision {
        let thresholds = &config.thresholds;
        let exit_reasons = Self::exit_reasons(snapshot, thresholds);
        if !exit_reasons.is_empty() {
            return Self::decision(BscEventArbAction::ForceExit, exit_reasons);
        }

        let blockers = Self::entry_blockers(snapshot, thresholds);
        if !blockers.is_empty() {
            return Self::decision(BscEventArbAction::Flat, blockers);
        }

        let confirmations = Self::entry_confirmations(snapshot, thresholds);
        if confirmations.len() == 4 {
            return Self::decision(BscEventArbAction::Long, confirmations);
        }

        let mut reasons = confirmations;
        reasons.push("ENTRY_CONFIRMATION_INCOMPLETE".to_string());
        Self::decision(BscEventArbAction::Flat, reasons)
    }

    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            BscEventArbAction::Flat,
            vec!["MISSING_EVENT_SNAPSHOT".to_string()],
        )
        .to_signal(price, ts)
    }

    fn decision(action: BscEventArbAction, reasons: Vec<String>) -> BscEventArbDecision {
        BscEventArbDecision { action, reasons }
    }

    fn entry_blockers(
        snapshot: &BscEventArbSignalSnapshot,
        t: &BscEventArbThresholds,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        Self::push_candidate_blockers(snapshot, t, &mut reasons);
        Self::push_security_blockers(snapshot, t, &mut reasons);
        reasons
    }

    fn push_candidate_blockers(
        snapshot: &BscEventArbSignalSnapshot,
        t: &BscEventArbThresholds,
        reasons: &mut Vec<String>,
    ) {
        if !Self::is_bsc_chain(&snapshot.chain_id) {
            reasons.push("CHAIN_NOT_BSC".to_string());
        }
        if !Self::has_event_tag(snapshot) {
            reasons.push("EVENT_TAG_MISSING".to_string());
        }
        if !Self::has_volume_surge(snapshot, t) {
            reasons.push("VOLUME_SURGE_MISSING".to_string());
        }
        if snapshot.depth_2pct_usd < t.min_depth_2pct_usd {
            reasons.push("DEX_DEPTH_TOO_THIN".to_string());
        }
    }

    fn push_security_blockers(
        snapshot: &BscEventArbSignalSnapshot,
        t: &BscEventArbThresholds,
        reasons: &mut Vec<String>,
    ) {
        if !snapshot.sell_simulation_passed {
            reasons.push("CONTRACT_SECURITY_BLOCK".to_string());
        }
        if snapshot.buy_tax_pct > t.max_tax_pct {
            reasons.push("BUY_TAX_TOO_HIGH".to_string());
        }
        if snapshot.sell_tax_pct > t.max_tax_pct {
            reasons.push("SELL_TAX_TOO_HIGH".to_string());
        }
        if snapshot.has_blacklist_risk || snapshot.has_pause_risk || snapshot.has_mint_risk {
            reasons.push("CONTRACT_PRIVILEGE_RISK".to_string());
        }
    }

    fn entry_confirmations(
        snapshot: &BscEventArbSignalSnapshot,
        t: &BscEventArbThresholds,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        Self::push_if(
            Self::has_cex_event(snapshot, t),
            "EVENT_CONFIRMED",
            &mut reasons,
        );
        Self::push_if(
            Self::has_momentum(snapshot, t),
            "MOMENTUM_CONFIRMED",
            &mut reasons,
        );
        Self::push_if(
            Self::has_squeeze(snapshot, t),
            "SQUEEZE_CONFIRMED",
            &mut reasons,
        );
        Self::push_if(
            Self::has_whale_flow_ok(snapshot, t),
            "WHALE_FLOW_CONFIRMED",
            &mut reasons,
        );
        reasons
    }

    fn exit_reasons(
        snapshot: &BscEventArbSignalSnapshot,
        t: &BscEventArbThresholds,
    ) -> Vec<String> {
        let mut reasons = Vec::new();
        let oi_unwind = snapshot.oi_drop_1h_pct > t.max_oi_drop_1h_pct;
        Self::push_if(
            oi_unwind && snapshot.price_below_15m_vwap,
            "OI_UNWIND_VWAP_BREAK",
            &mut reasons,
        );
        Self::push_if(
            snapshot.funding_flipped_positive && !snapshot.price_making_new_high,
            "FUNDING_FLIP_NO_HIGH",
            &mut reasons,
        );
        Self::push_if(
            snapshot.top_holder_or_lp_abnormal_outflow,
            "HOLDER_OR_LP_OUTFLOW",
            &mut reasons,
        );
        Self::push_if(
            snapshot.cex_withdrawal_or_trading_restriction,
            "CEX_TRADING_RESTRICTION",
            &mut reasons,
        );
        Self::push_if(
            snapshot.price_change_from_entry_pct <= t.hard_stop_loss_pct,
            "HARD_STOP_LOSS",
            &mut reasons,
        );
        Self::push_if(
            snapshot.trailing_drawdown_pct >= t.trailing_stop_pct,
            "TRAILING_STOP",
            &mut reasons,
        );
        Self::push_if(
            Self::time_stop(snapshot, t),
            "TIME_STOP_NO_PROFIT",
            &mut reasons,
        );
        Self::push_if(
            snapshot.minutes_since_entry >= t.max_event_hold_minutes,
            "EVENT_HOLD_TIMEOUT",
            &mut reasons,
        );
        reasons
    }

    fn has_event_tag(snapshot: &BscEventArbSignalSnapshot) -> bool {
        snapshot.event_tags.iter().any(|tag| {
            matches!(
                tag.as_str(),
                "binance_alpha"
                    | "cex_listing"
                    | "four_meme"
                    | "meme_rush"
                    | "top_gainer"
                    | "volume_surge"
            )
        })
    }

    fn has_cex_event(snapshot: &BscEventArbSignalSnapshot, t: &BscEventArbThresholds) -> bool {
        !snapshot.is_dex_only && snapshot.cex_volume_share >= t.min_cex_volume_share
    }

    fn has_momentum(snapshot: &BscEventArbSignalSnapshot, t: &BscEventArbThresholds) -> bool {
        let zscore = snapshot.volume_zscore_5m.max(snapshot.volume_zscore_15m);
        snapshot.price_change_15m_pct >= t.min_price_change_15m_pct
            && snapshot.price_change_1h_pct >= t.min_price_change_1h_pct
            && snapshot.price_above_15m_vwap
            && zscore >= t.min_volume_zscore
    }

    fn has_squeeze(snapshot: &BscEventArbSignalSnapshot, t: &BscEventArbThresholds) -> bool {
        let oi_growth = snapshot.oi_growth_1h_pct >= t.min_oi_growth_1h_pct
            || snapshot.oi_growth_4h_pct >= t.min_oi_growth_4h_pct;
        let short_crowded = snapshot.funding_rate < 0.0
            || snapshot.short_crowding_score >= t.min_short_crowding_score;
        oi_growth && short_crowded && snapshot.price_up_with_oi
    }

    fn has_whale_flow_ok(snapshot: &BscEventArbSignalSnapshot, t: &BscEventArbThresholds) -> bool {
        if snapshot.cex_net_inflow_usd < t.large_cex_flow_usd {
            return true;
        }
        snapshot.price_resilient_after_inflow
            && (snapshot.cex_outflow_after_inflow || snapshot.spot_absorption)
    }

    fn has_volume_surge(snapshot: &BscEventArbSignalSnapshot, t: &BscEventArbThresholds) -> bool {
        snapshot.volume_24h_usd >= t.min_volume_24h_usd
            || snapshot.volume_1h_vs_24h_avg >= t.min_volume_1h_vs_24h_avg
    }

    fn is_bsc_chain(chain_id: &str) -> bool {
        matches!(
            chain_id.to_ascii_lowercase().as_str(),
            "bsc" | "bnb" | "bnb_chain" | "56"
        )
    }

    fn time_stop(snapshot: &BscEventArbSignalSnapshot, t: &BscEventArbThresholds) -> bool {
        snapshot.minutes_since_entry >= t.time_stop_minutes
            && snapshot.max_unrealized_profit_pct < t.min_time_stop_profit_pct
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}
