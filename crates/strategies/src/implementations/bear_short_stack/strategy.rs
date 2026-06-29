use super::types::{
    round_price, BearShortAction, BearShortDecision, BearShortPreset, BearShortSignalSnapshot,
    BearShortStackBacktestMarketContext, BearShortStackBacktestTuning, BearShortStackConfig,
    BearShortStackThresholds,
};
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct BearShortStackStrategy;

/// 回测用 preset 选择器；live 执行仍通过配置里的 `strategy_key` 或 `preset` 显式选择子策略。
#[derive(Debug, Clone)]
pub struct BearShortStackBacktestPreset {
    preset: BearShortPreset,
    tuning: BearShortStackBacktestTuning,
    market_context: Option<Vec<BearShortStackBacktestMarketContext>>,
}

impl BearShortStackStrategy {
    pub fn evaluate(
        config: &BearShortStackConfig,
        snapshot: &BearShortSignalSnapshot,
    ) -> BearShortDecision {
        let preset = Self::effective_preset(config, snapshot);
        let thresholds = &config.thresholds;
        let blockers = match preset {
            BearShortPreset::BearBreakdown => Self::breakdown_blockers(snapshot, thresholds),
            BearShortPreset::ExhaustionFade => Self::exhaustion_blockers(snapshot, thresholds),
        };
        if !blockers.is_empty() {
            return Self::decision(BearShortAction::Flat, preset, blockers);
        }
        Self::decision(
            BearShortAction::Short,
            preset,
            Self::short_reasons(snapshot, thresholds, preset),
        )
    }

    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            BearShortAction::Flat,
            BearShortPreset::BearBreakdown,
            vec!["MISSING_MARKET_SNAPSHOT".to_string()],
        )
        .to_signal(price, ts)
    }

    pub fn for_preset(preset: BearShortPreset) -> BearShortStackBacktestPreset {
        BearShortStackBacktestPreset {
            preset,
            tuning: BearShortStackBacktestTuning::default(),
            market_context: None,
        }
    }

    pub fn for_preset_with_tuning(
        preset: BearShortPreset,
        tuning: BearShortStackBacktestTuning,
    ) -> BearShortStackBacktestPreset {
        BearShortStackBacktestPreset {
            preset,
            tuning,
            market_context: None,
        }
    }

    pub fn for_preset_with_context(
        preset: BearShortPreset,
        mut market_context: Vec<BearShortStackBacktestMarketContext>,
    ) -> BearShortStackBacktestPreset {
        market_context.sort_unstable_by_key(|row| row.ts);
        BearShortStackBacktestPreset {
            preset,
            tuning: BearShortStackBacktestTuning::default(),
            market_context: Some(market_context),
        }
    }

    pub fn for_preset_with_tuning_and_context(
        preset: BearShortPreset,
        tuning: BearShortStackBacktestTuning,
        mut market_context: Vec<BearShortStackBacktestMarketContext>,
    ) -> BearShortStackBacktestPreset {
        market_context.sort_unstable_by_key(|row| row.ts);
        BearShortStackBacktestPreset {
            preset,
            tuning,
            market_context: Some(market_context),
        }
    }

    fn breakdown_blockers(
        snapshot: &BearShortSignalSnapshot,
        t: &BearShortStackThresholds,
    ) -> Vec<String> {
        let mut reasons = Self::shared_blockers(snapshot);
        Self::push_if(
            Self::normalize(&snapshot.trend_4h) != "short",
            "FOUR_HOUR_NOT_BEARISH",
            &mut reasons,
        );
        Self::push_if(
            !Self::one_hour_bearish(&snapshot.trend_1h),
            "ONE_HOUR_LOWER_HIGH_MISSING",
            &mut reasons,
        );
        Self::push_if(
            !snapshot.breakdown_confirmed,
            "BREAKDOWN_NOT_CONFIRMED",
            &mut reasons,
        );
        Self::push_if(
            !snapshot.failed_reclaim_confirmed,
            "FAILED_RECLAIM_MISSING",
            &mut reasons,
        );
        Self::push_if(
            !snapshot.price_down_with_oi_up,
            "PRICE_DOWN_WITH_OI_UP_MISSING",
            &mut reasons,
        );
        Self::push_if(
            snapshot.oi_growth_pct < t.min_oi_growth_pct,
            "OI_GROWTH_MISSING",
            &mut reasons,
        );
        Self::push_if(
            snapshot.funding_rate <= t.deeply_negative_funding_rate,
            "FUNDING_ALREADY_DEEPLY_NEGATIVE",
            &mut reasons,
        );
        Self::push_if(
            snapshot.long_short_ratio < t.min_long_short_ratio,
            "LONG_CROWDING_MISSING",
            &mut reasons,
        );
        Self::push_if(
            snapshot.downside_extension_atr > t.max_downside_extension_atr,
            "DOWNSIDE_ALREADY_EXTENDED",
            &mut reasons,
        );
        reasons
    }

    fn exhaustion_blockers(
        snapshot: &BearShortSignalSnapshot,
        t: &BearShortStackThresholds,
    ) -> Vec<String> {
        let mut reasons = Self::shared_blockers(snapshot);
        Self::push_if(
            !snapshot.new_high_failed,
            "NEW_HIGH_FAILURE_MISSING",
            &mut reasons,
        );
        Self::push_if(
            !Self::flow_diverged(snapshot),
            "FLOW_DIVERGENCE_MISSING",
            &mut reasons,
        );
        Self::push_if(
            snapshot.oi_growth_pct < t.exhaustion_min_oi_growth_pct,
            "EXHAUSTION_OI_SPIKE_MISSING",
            &mut reasons,
        );
        Self::push_if(
            snapshot.funding_rate < t.exhaustion_hot_funding_rate,
            "FUNDING_NOT_HOT_ENOUGH",
            &mut reasons,
        );
        Self::push_if(
            !snapshot.pullback_failed_below_vwap,
            "PULLBACK_FAILURE_MISSING",
            &mut reasons,
        );
        reasons
    }

    fn shared_blockers(snapshot: &BearShortSignalSnapshot) -> Vec<String> {
        let mut reasons = Vec::new();
        Self::push_if(
            !Self::is_allowed_exchange(&snapshot.exchange),
            "EXCHANGE_NOT_LIVE_READY_V1",
            &mut reasons,
        );
        Self::push_if(
            !Self::is_btc_or_eth(&snapshot.symbol),
            "SYMBOL_NOT_BTC_ETH",
            &mut reasons,
        );
        Self::push_if(snapshot.price <= 0.0, "PRICE_MISSING", &mut reasons);
        Self::push_if(
            snapshot.failed_reclaim_high <= snapshot.price,
            "FAILED_RECLAIM_HIGH_INVALID",
            &mut reasons,
        );
        Self::push_if(snapshot.atr_15m <= 0.0, "ATR_15M_MISSING", &mut reasons);
        reasons
    }

    fn short_reasons(
        snapshot: &BearShortSignalSnapshot,
        t: &BearShortStackThresholds,
        preset: BearShortPreset,
    ) -> Vec<String> {
        let (buffer, target_r_1, target_r_2) = match preset {
            BearShortPreset::BearBreakdown => (
                t.breakdown_stop_atr_buffer,
                t.breakdown_target_r_1,
                t.breakdown_target_r_2,
            ),
            BearShortPreset::ExhaustionFade => (
                t.exhaustion_stop_atr_buffer,
                t.exhaustion_target_r_1,
                t.exhaustion_target_r_2,
            ),
        };
        vec![
            format!("{}_CONFIRMED", preset.strategy_key().to_ascii_uppercase()),
            format!(
                "STOP_PRICE:{}",
                round_price(snapshot.failed_reclaim_high + snapshot.atr_15m * buffer)
            ),
            format!("TARGET_R_1:{target_r_1}"),
            format!("TARGET_R_2:{target_r_2}"),
        ]
    }

    fn effective_preset(
        config: &BearShortStackConfig,
        snapshot: &BearShortSignalSnapshot,
    ) -> BearShortPreset {
        if snapshot.preset == BearShortPreset::ExhaustionFade {
            return BearShortPreset::ExhaustionFade;
        }
        config.preset
    }

    fn one_hour_bearish(value: &str) -> bool {
        matches!(
            Self::normalize(value).as_str(),
            "lower_high" | "short" | "breakdown" | "bearish"
        )
    }

    fn flow_diverged(snapshot: &BearShortSignalSnapshot) -> bool {
        snapshot.taker_flow_diverged || snapshot.orderbook_imbalance_diverged
    }

    fn is_allowed_exchange(exchange: &str) -> bool {
        matches!(Self::normalize(exchange).as_str(), "binance" | "okx")
    }

    fn is_btc_or_eth(symbol: &str) -> bool {
        let upper = symbol.to_ascii_uppercase();
        upper.starts_with("BTC") || upper.starts_with("ETH")
    }

    fn normalize(value: &str) -> String {
        value.trim().to_ascii_lowercase()
    }

    fn decision(
        action: BearShortAction,
        preset: BearShortPreset,
        reasons: Vec<String>,
    ) -> BearShortDecision {
        BearShortDecision {
            action,
            preset,
            reasons,
        }
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}

impl BearShortStackBacktestPreset {
    pub fn run_test(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        // 复用通用 pipeline，避免做空策略拥有一套独立的持仓和风控统计口径。
        run_indicator_strategy_backtest(
            inst_id,
            BearShortStackBacktestAdapter {
                preset: self.preset,
                symbol: inst_id.to_string(),
                cooldown_remaining: 0,
                tuning: self.tuning,
                market_context: self.market_context,
                exhaustion_entry_index: None,
            },
            candles,
            risk,
        )
    }
}

// 回测适配器只验证策略接入链路；真实 OI、funding 和多空比后续应由 market snapshot 填充。
#[derive(Debug, Clone)]
struct BearShortStackBacktestAdapter {
    preset: BearShortPreset,
    symbol: String,
    cooldown_remaining: usize,
    tuning: BearShortStackBacktestTuning,
    market_context: Option<Vec<BearShortStackBacktestMarketContext>>,
    exhaustion_entry_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct BearShortStackBacktestValues {
    atr_15m: f64,
    oi_growth_pct: f64,
}

impl IndicatorStrategyBacktest for BearShortStackBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = BearShortStackBacktestValues;

    fn min_data_length(&self) -> usize {
        96
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues {
        let atr_15m = (candle.h - candle.l).abs().max(candle.c.abs() * 0.0008);
        Self::IndicatorValues {
            atr_15m: atr_15m.max(0.0001),
            oi_growth_pct: 2.1,
        }
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        _risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return SignalResult::default();
        }
        let Some(last) = candles.last() else {
            return SignalResult::default();
        };
        if let Some(signal) = self.exhaustion_time_stop_signal(candles, last) {
            return signal;
        }
        let snapshot = match self.preset {
            BearShortPreset::BearBreakdown => {
                let Some(setup) = self.breakdown_setup(candles, values) else {
                    return SignalResult::default();
                };
                let Some(snapshot) = self.breakdown_snapshot(last, values, setup) else {
                    return BearShortStackStrategy::flat_missing_snapshot(last.c, last.ts);
                };
                snapshot
            }
            BearShortPreset::ExhaustionFade => {
                let Some(setup) = self.exhaustion_setup(candles) else {
                    return SignalResult::default();
                };
                let Some(snapshot) = self.exhaustion_snapshot(last, values, setup) else {
                    return BearShortStackStrategy::flat_missing_snapshot(last.c, last.ts);
                };
                snapshot
            }
        };
        let config = BearShortStackConfig {
            preset: self.preset,
            ..Default::default()
        };
        let decision = BearShortStackStrategy::evaluate(&config, &snapshot);
        let mut signal = decision.to_signal(snapshot.price, last.ts);
        if signal.should_sell {
            self.cooldown_remaining = self.tuning.cooldown_candles;
            if matches!(self.preset, BearShortPreset::ExhaustionFade) {
                self.exhaustion_entry_index = candles.len().checked_sub(1);
            }
        }
        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

impl BearShortStackBacktestAdapter {
    fn exhaustion_time_stop_signal(
        &mut self,
        candles: &[CandleItem],
        last: &CandleItem,
    ) -> Option<SignalResult> {
        if !matches!(self.preset, BearShortPreset::ExhaustionFade)
            || self.tuning.exhaustion_max_holding_candles == 0
        {
            return None;
        }
        let entry_index = self.exhaustion_entry_index?;
        let current_index = candles.len().checked_sub(1)?;
        if current_index.saturating_sub(entry_index) < self.tuning.exhaustion_max_holding_candles {
            return None;
        }
        self.exhaustion_entry_index = None;
        Some(SignalResult {
            should_buy: true,
            open_price: last.c,
            ts: last.ts,
            direction: rust_quant_domain::SignalDirection::Long,
            filter_reasons: vec!["FIB_STRICT_MAJOR_BEAR_BLOCK_LONG".to_string()],
            single_value: Some("EXHAUSTION_TIME_STOP".to_string()),
            single_result: Some("EXHAUSTION_TIME_STOP".to_string()),
            ..SignalResult::default()
        })
    }

    fn breakdown_setup(
        &self,
        candles: &[CandleItem],
        values: &BearShortStackBacktestValues,
    ) -> Option<BearBreakdownBacktestSetup> {
        if candles.len() < 96 {
            return None;
        }
        let last = candles.last()?;
        let sma24 = average_close(&candles[candles.len() - 24..]);
        let sma96 = average_close(candles);
        if !(last.c < sma24 && sma24 < sma96) {
            return None;
        }
        let breakdown_index = recent_breakdown_index(candles, self.tuning)?;
        let reclaim_high = failed_reclaim_high(candles, breakdown_index)?;
        let reclaim_distance_atr = (reclaim_high - last.c) / values.atr_15m.max(0.0001);
        if !(self.tuning.breakdown_min_reclaim_distance_atr
            ..=self.tuning.breakdown_max_reclaim_distance_atr)
            .contains(&reclaim_distance_atr)
        {
            return None;
        }
        if !secondary_breakdown_confirmed(candles, breakdown_index, self.tuning) {
            return None;
        }
        Some(BearBreakdownBacktestSetup {
            failed_reclaim_high: reclaim_high,
            downside_extension_atr: downside_extension_atr(candles, values.atr_15m),
        })
    }

    fn breakdown_snapshot(
        &self,
        last: &CandleItem,
        values: &BearShortStackBacktestValues,
        setup: BearBreakdownBacktestSetup,
    ) -> Option<BearShortSignalSnapshot> {
        let mut snapshot = BearShortSignalSnapshot {
            exchange: "okx".to_string(),
            symbol: self.symbol.clone(),
            price: last.c,
            failed_reclaim_high: setup.failed_reclaim_high,
            atr_15m: values.atr_15m,
            preset: BearShortPreset::BearBreakdown,
            trend_4h: "short".to_string(),
            trend_1h: "lower_high".to_string(),
            breakdown_confirmed: true,
            failed_reclaim_confirmed: true,
            price_down_with_oi_up: true,
            oi_growth_pct: values.oi_growth_pct,
            funding_rate: 0.00003,
            long_short_ratio: 1.18,
            downside_extension_atr: setup.downside_extension_atr,
            ..Default::default()
        };
        if let Some(context) = self.context_at(last.ts) {
            snapshot.price_down_with_oi_up = context.oi_growth_pct > 0.0;
            snapshot.oi_growth_pct = context.oi_growth_pct;
            snapshot.funding_rate = context.funding_rate;
            snapshot.long_short_ratio = context.long_short_ratio;
        } else if self.market_context.is_some() {
            return None;
        }
        Some(snapshot)
    }

    fn exhaustion_setup(&self, candles: &[CandleItem]) -> Option<ExhaustionBacktestSetup> {
        if candles.len() < 48 {
            return None;
        }
        let last = candles.last()?;
        if !(last.c < last.o) {
            return None;
        }
        let start = candles.len().saturating_sub(12).max(1);
        let recent = &candles[start..candles.len() - 1];
        let spike = recent
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.h.total_cmp(&right.h))?;
        let spike_index = start + spike.0;
        let previous_high = candles[start - 1..spike_index]
            .iter()
            .map(|candle| candle.h)
            .fold(f64::NEG_INFINITY, f64::max);
        let previous_range = average_range(&candles[start - 1..spike_index]).max(0.0001);
        if !previous_high.is_finite()
            || spike.1.h
                <= previous_high + previous_range * self.tuning.exhaustion_new_high_range_mult
        {
            return None;
        }
        if last.h >= spike.1.h || last.c >= spike.1.c {
            return None;
        }
        let recent_volume = average_volume(recent).max(0.0001);
        let last_body_ratio = candle_body_ratio(last);
        if last_body_ratio < self.tuning.exhaustion_min_body_ratio
            || last.v < recent_volume * self.tuning.exhaustion_min_volume_mult
        {
            return None;
        }
        Some(ExhaustionBacktestSetup {
            failed_reclaim_high: spike.1.h,
        })
    }

    fn exhaustion_snapshot(
        &self,
        last: &CandleItem,
        values: &BearShortStackBacktestValues,
        setup: ExhaustionBacktestSetup,
    ) -> Option<BearShortSignalSnapshot> {
        // 反转狙击风险高于主跌顺势，固定为 ExhaustionFade 后信号层会携带 HALF_RISK。
        let mut snapshot = BearShortSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: self.symbol.clone(),
            price: last.c,
            failed_reclaim_high: setup.failed_reclaim_high,
            atr_15m: values.atr_15m,
            preset: BearShortPreset::ExhaustionFade,
            oi_growth_pct: 4.6,
            funding_rate: 0.00045,
            new_high_failed: true,
            taker_flow_diverged: true,
            orderbook_imbalance_diverged: true,
            pullback_failed_below_vwap: true,
            ..Default::default()
        };
        if let Some(context) = self.context_at(last.ts) {
            snapshot.oi_growth_pct = context.oi_growth_pct.abs();
            snapshot.funding_rate = context.funding_rate;
            snapshot.taker_flow_diverged = context.taker_sell_volume >= context.taker_buy_volume;
        } else if self.market_context.is_some() {
            return None;
        }
        Some(snapshot)
    }

    fn context_at(&self, ts: i64) -> Option<&BearShortStackBacktestMarketContext> {
        self.market_context
            .as_ref()?
            .iter()
            .rev()
            .find(|row| row.ts <= ts)
    }
}

#[derive(Debug, Clone, Copy)]
struct BearBreakdownBacktestSetup {
    failed_reclaim_high: f64,
    downside_extension_atr: f64,
}

#[derive(Debug, Clone, Copy)]
struct ExhaustionBacktestSetup {
    failed_reclaim_high: f64,
}

fn recent_breakdown_index(
    candles: &[CandleItem],
    tuning: BearShortStackBacktestTuning,
) -> Option<usize> {
    let start = candles.len().saturating_sub(12).max(1);
    let end = candles.len().saturating_sub(3);
    let avg_range = average_range(&candles[start - 1..end]).max(0.0001);
    let avg_volume = average_volume(&candles[start - 1..end]).max(0.0001);
    (start..end).rev().find(|index| {
        let current = &candles[*index];
        let previous = &candles[*index - 1];
        previous.c - current.c >= avg_range * tuning.breakdown_initial_move_range_mult
            && current.c < current.o
            && current.v >= avg_volume * tuning.breakdown_initial_volume_mult
    })
}

fn failed_reclaim_high(candles: &[CandleItem], breakdown_index: usize) -> Option<f64> {
    let after_breakdown = &candles[breakdown_index + 1..];
    if after_breakdown.len() < 2 {
        return None;
    }
    let breakdown = &candles[breakdown_index];
    let high = after_breakdown
        .iter()
        .map(|candle| candle.h)
        .fold(f64::NEG_INFINITY, f64::max);
    let last = candles.last()?;
    if high > breakdown.c && last.c < high && last.c < last.o {
        Some(high)
    } else {
        None
    }
}

fn secondary_breakdown_confirmed(
    candles: &[CandleItem],
    breakdown_index: usize,
    tuning: BearShortStackBacktestTuning,
) -> bool {
    if candles.len() <= breakdown_index + 3 {
        return false;
    }
    let last = match candles.last() {
        Some(value) => value,
        None => return false,
    };
    let recent_volume =
        average_volume(&candles[candles.len().saturating_sub(12)..candles.len() - 1]).max(0.0001);
    let previous_support = candles[breakdown_index + 1..candles.len() - 1]
        .iter()
        .map(|candle| candle.c)
        .fold(f64::INFINITY, f64::min);
    let support_break = previous_support - last.c;
    previous_support.is_finite()
        && support_break
            >= average_range(&candles[breakdown_index + 1..]).max(0.0001)
                * tuning.breakdown_min_support_break_range
        && last.c < last.o
        && candle_body_ratio(last) >= tuning.breakdown_min_body_ratio
        && last.v >= recent_volume * tuning.breakdown_min_volume_mult
}

fn downside_extension_atr(candles: &[CandleItem], atr: f64) -> f64 {
    let recent = &candles[candles.len().saturating_sub(12)..];
    let Some(first) = recent.first() else {
        return 0.0;
    };
    let Some(last) = recent.last() else {
        return 0.0;
    };
    (first.c - last.c).max(0.0) / atr.max(0.0001)
}

fn average_close(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles.iter().map(|candle| candle.c).sum::<f64>() / candles.len() as f64
}

fn average_range(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles
        .iter()
        .map(|candle| (candle.h - candle.l).abs())
        .sum::<f64>()
        / candles.len() as f64
}

fn average_volume(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles.iter().map(|candle| candle.v).sum::<f64>() / candles.len() as f64
}

fn candle_body_ratio(candle: &CandleItem) -> f64 {
    let range = (candle.h - candle.l).abs().max(0.0001);
    (candle.c - candle.o).abs() / range
}
