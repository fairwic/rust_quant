use super::types::{
    round_price, BtcEthLiquidityScalperAction, BtcEthLiquidityScalperBacktestMarketContext,
    BtcEthLiquidityScalperBacktestTuning, BtcEthLiquidityScalperConfig,
    BtcEthLiquidityScalperDecision, BtcEthLiquidityScalperSignalSnapshot,
    BtcEthLiquidityScalperThresholds,
};
use crate::framework::backtest::{run_indicator_strategy_backtest, IndicatorStrategyBacktest};
use crate::strategy_common::{BackTestResult, BasicRiskStrategyConfig, SignalResult};
use crate::CandleItem;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub struct BtcEthLiquidityScalperStrategy;

impl BtcEthLiquidityScalperStrategy {
    pub fn evaluate(
        config: &BtcEthLiquidityScalperConfig,
        snapshot: &BtcEthLiquidityScalperSignalSnapshot,
    ) -> BtcEthLiquidityScalperDecision {
        let thresholds = &config.thresholds;
        let blockers = Self::blockers(snapshot, thresholds);
        if !blockers.is_empty() {
            return Self::decision(BtcEthLiquidityScalperAction::Flat, blockers);
        }

        let action = match Self::bias(snapshot).as_str() {
            "long" => BtcEthLiquidityScalperAction::Long,
            "short" => BtcEthLiquidityScalperAction::Short,
            _ => BtcEthLiquidityScalperAction::Flat,
        };
        let mut reasons = vec![
            "BTC_ETH_LIQUIDITY_SCALP_CONFIRMED".to_string(),
            format!(
                "STOP_PRICE:{}",
                Self::stop_price(snapshot, thresholds, action)
            ),
        ];
        if snapshot.oi_expansion_pct < thresholds.min_oi_expansion_pct {
            reasons.push("OI_NOT_CONFIRMED_REDUCE_SIZE".to_string());
        }
        Self::decision(action, reasons)
    }

    pub fn flat_missing_snapshot(price: f64, ts: i64) -> SignalResult {
        Self::decision(
            BtcEthLiquidityScalperAction::Flat,
            vec!["MISSING_MARKET_SNAPSHOT".to_string()],
        )
        .to_signal(price, ts)
    }

    pub fn run_test(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
    ) -> BackTestResult {
        self.run_test_with_tuning(
            inst_id,
            candles,
            risk,
            BtcEthLiquidityScalperBacktestTuning::default(),
        )
    }

    pub fn run_test_with_tuning(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: BtcEthLiquidityScalperBacktestTuning,
    ) -> BackTestResult {
        // 复用通用 pipeline，保证回测的持仓、止损、审计轨迹和现有策略保持同一口径。
        run_indicator_strategy_backtest(
            inst_id,
            BtcEthLiquidityScalperBacktestAdapter {
                symbol: inst_id.to_string(),
                cooldown_remaining: 0,
                tuning,
                market_context: None,
            },
            candles,
            risk,
        )
    }

    pub fn run_test_with_tuning_and_context(
        self,
        inst_id: &str,
        candles: &[CandleItem],
        risk: BasicRiskStrategyConfig,
        tuning: BtcEthLiquidityScalperBacktestTuning,
        mut market_context: Vec<BtcEthLiquidityScalperBacktestMarketContext>,
    ) -> BackTestResult {
        market_context.sort_unstable_by_key(|row| row.ts);
        run_indicator_strategy_backtest(
            inst_id,
            BtcEthLiquidityScalperBacktestAdapter {
                symbol: inst_id.to_string(),
                cooldown_remaining: 0,
                tuning,
                market_context: Some(market_context),
            },
            candles,
            risk,
        )
    }

    fn blockers(
        snapshot: &BtcEthLiquidityScalperSignalSnapshot,
        t: &BtcEthLiquidityScalperThresholds,
    ) -> Vec<String> {
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
        Self::push_if(
            !Self::trend_aligned(snapshot),
            "TIMEFRAME_TREND_CONFLICT",
            &mut reasons,
        );
        Self::push_if(
            !snapshot.volume_impulse_confirmed,
            "VOLUME_IMPULSE_MISSING",
            &mut reasons,
        );
        Self::push_if(
            !snapshot.pullback_to_anchor,
            "PULLBACK_TO_ANCHOR_MISSING",
            &mut reasons,
        );
        Self::push_if(
            !Self::anchor_distance_ok(snapshot, t),
            "ANCHOR_DISTANCE_TOO_WIDE",
            &mut reasons,
        );
        Self::push_if(
            !Self::microstructure_ok(snapshot, t),
            "MICROSTRUCTURE_CONFIRMATION_MISSING",
            &mut reasons,
        );
        Self::push_if(
            Self::funding_too_hot(snapshot, t),
            "FUNDING_TOO_CROWDED",
            &mut reasons,
        );
        Self::push_if(
            snapshot.spread_bps > t.max_spread_bps,
            "SPREAD_TOO_WIDE",
            &mut reasons,
        );
        Self::push_if(
            snapshot.depth_usd < t.min_depth_usd,
            "DEPTH_TOO_THIN",
            &mut reasons,
        );
        Self::push_if(
            snapshot.breakout_candle_atr > t.max_breakout_candle_atr,
            "BREAKOUT_CANDLE_TOO_EXTENDED",
            &mut reasons,
        );
        reasons
    }

    fn trend_aligned(snapshot: &BtcEthLiquidityScalperSignalSnapshot) -> bool {
        let bias = Self::bias(snapshot);
        if bias != "long" && bias != "short" {
            return false;
        }
        Self::normalize(&snapshot.trend_1h) == bias
            && Self::normalize(&snapshot.trend_4h) != Self::opposite(&bias)
    }

    fn anchor_distance_ok(
        snapshot: &BtcEthLiquidityScalperSignalSnapshot,
        t: &BtcEthLiquidityScalperThresholds,
    ) -> bool {
        snapshot.atr_5m > 0.0
            && ((snapshot.price - snapshot.anchor_price).abs() / snapshot.atr_5m)
                <= t.max_anchor_distance_atr
    }

    fn microstructure_ok(
        snapshot: &BtcEthLiquidityScalperSignalSnapshot,
        t: &BtcEthLiquidityScalperThresholds,
    ) -> bool {
        snapshot.taker_aggression >= t.min_taker_aggression
            || snapshot.orderbook_imbalance >= t.min_orderbook_imbalance
    }

    fn funding_too_hot(
        snapshot: &BtcEthLiquidityScalperSignalSnapshot,
        t: &BtcEthLiquidityScalperThresholds,
    ) -> bool {
        match Self::bias(snapshot).as_str() {
            "long" => snapshot.funding_rate > t.max_abs_funding_rate,
            "short" => snapshot.funding_rate < -t.max_abs_funding_rate,
            _ => true,
        }
    }

    fn stop_price(
        snapshot: &BtcEthLiquidityScalperSignalSnapshot,
        t: &BtcEthLiquidityScalperThresholds,
        action: BtcEthLiquidityScalperAction,
    ) -> f64 {
        match action {
            BtcEthLiquidityScalperAction::Long => {
                round_price(snapshot.anchor_price - snapshot.atr_5m * t.max_anchor_distance_atr)
            }
            BtcEthLiquidityScalperAction::Short => {
                round_price(snapshot.anchor_price + snapshot.atr_5m * t.max_anchor_distance_atr)
            }
            BtcEthLiquidityScalperAction::Flat => snapshot.price,
        }
    }

    fn is_allowed_exchange(exchange: &str) -> bool {
        matches!(Self::normalize(exchange).as_str(), "binance" | "okx")
    }

    fn is_btc_or_eth(symbol: &str) -> bool {
        let upper = symbol.to_ascii_uppercase();
        upper.starts_with("BTC") || upper.starts_with("ETH")
    }

    fn bias(snapshot: &BtcEthLiquidityScalperSignalSnapshot) -> String {
        Self::normalize(&snapshot.execution_bias)
    }

    fn opposite(value: &str) -> &'static str {
        match value {
            "long" => "short",
            "short" => "long",
            _ => "",
        }
    }

    fn normalize(value: &str) -> String {
        value.trim().to_ascii_lowercase()
    }

    fn decision(
        action: BtcEthLiquidityScalperAction,
        reasons: Vec<String>,
    ) -> BtcEthLiquidityScalperDecision {
        BtcEthLiquidityScalperDecision { action, reasons }
    }

    fn push_if(condition: bool, reason: &str, reasons: &mut Vec<String>) {
        if condition {
            reasons.push(reason.to_string());
        }
    }
}

// 回测适配器只把 candle 序列转成策略所需 snapshot；live 执行仍要求上游提供真实订单流、OI 和 funding。
#[derive(Debug, Clone)]
struct BtcEthLiquidityScalperBacktestAdapter {
    symbol: String,
    cooldown_remaining: usize,
    tuning: BtcEthLiquidityScalperBacktestTuning,
    market_context: Option<Vec<BtcEthLiquidityScalperBacktestMarketContext>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct BtcEthLiquidityScalperBacktestValues {
    atr_5m: f64,
    breakout_candle_atr: f64,
    volume_impulse_confirmed: bool,
}

impl IndicatorStrategyBacktest for BtcEthLiquidityScalperBacktestAdapter {
    type IndicatorCombine = ();
    type IndicatorValues = BtcEthLiquidityScalperBacktestValues;

    fn min_data_length(&self) -> usize {
        self.tuning
            .trend_slow_window
            .max(self.tuning.trend_fast_window)
            .max(12)
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        candle: &CandleItem,
    ) -> Self::IndicatorValues {
        let range = (candle.h - candle.l).abs().max(candle.c.abs() * 0.0005);
        let body = (candle.c - candle.o).abs();
        let atr_5m = range.max(0.0001);
        Self::IndicatorValues {
            atr_5m,
            breakout_candle_atr: body / atr_5m,
            volume_impulse_confirmed: candle.v > 0.0,
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
        let Some(setup) = Self::setup(candles, values, &self.tuning) else {
            return SignalResult::default();
        };
        let Some(snapshot) = self.snapshot(last, values, setup) else {
            return BtcEthLiquidityScalperStrategy::flat_missing_snapshot(last.c, last.ts);
        };
        // 订单流、盘口、OI 和 funding 在默认回测中是 pipeline 验证用占位证据；
        // context-aware 回测会用调用方传入的历史市场上下文替代这些值。
        let config = BtcEthLiquidityScalperConfig::default();
        if self.tuning.require_oi_confirmation
            && snapshot.oi_expansion_pct < config.thresholds.min_oi_expansion_pct
        {
            let mut signal = BtcEthLiquidityScalperStrategy::decision(
                BtcEthLiquidityScalperAction::Flat,
                vec!["OI_NOT_CONFIRMED_BLOCKED_BY_BACKTEST_TUNING".to_string()],
            )
            .to_signal(snapshot.price, last.ts);
            signal.single_value = Some(json!(snapshot).to_string());
            return signal;
        }
        let decision = BtcEthLiquidityScalperStrategy::evaluate(&config, &snapshot);
        let mut signal = decision.to_signal(snapshot.price, last.ts);
        if signal.should_buy || signal.should_sell {
            apply_backtest_exit_tuning(&mut signal, self.tuning);
            self.cooldown_remaining = self.tuning.cooldown_candles;
        }
        signal.single_value = Some(json!(snapshot).to_string());
        signal
    }
}

impl BtcEthLiquidityScalperBacktestAdapter {
    fn snapshot(
        &self,
        last: &CandleItem,
        values: &BtcEthLiquidityScalperBacktestValues,
        setup: ScalperBacktestSetup,
    ) -> Option<BtcEthLiquidityScalperSignalSnapshot> {
        let context = self.context_at(last.ts);
        if context.is_none()
            && (self.market_context.is_some() || !self.tuning.allow_synthetic_market_context)
        {
            return None;
        }
        let mut snapshot = BtcEthLiquidityScalperSignalSnapshot {
            exchange: "binance".to_string(),
            symbol: self.symbol.clone(),
            price: last.c,
            anchor_price: setup.anchor_price,
            atr_5m: values.atr_5m,
            trend_4h: setup.bias.to_string(),
            trend_1h: setup.bias.to_string(),
            execution_bias: setup.bias.to_string(),
            volume_impulse_confirmed: true,
            pullback_to_anchor: true,
            taker_aggression: 0.62,
            orderbook_imbalance: 0.58,
            oi_expansion_pct: 1.2,
            funding_rate: 0.00008,
            spread_bps: 1.0,
            depth_usd: 25_000_000.0,
            breakout_candle_atr: values.breakout_candle_atr,
        };
        if let Some(context) = context {
            snapshot.funding_rate = context.funding_rate;
            snapshot.oi_expansion_pct = context.oi_expansion_pct;
            snapshot.taker_aggression = taker_aggression_for_bias(context, setup.bias);
            snapshot.orderbook_imbalance = context.orderbook_imbalance;
            snapshot.spread_bps = context.spread_bps;
            snapshot.depth_usd = context.depth_usd;
        }
        Some(snapshot)
    }

    fn context_at(&self, ts: i64) -> Option<&BtcEthLiquidityScalperBacktestMarketContext> {
        self.market_context
            .as_ref()?
            .iter()
            .rev()
            .find(|row| row.ts <= ts)
    }
}

#[derive(Debug, Clone, Copy)]
struct ScalperBacktestSetup {
    bias: &'static str,
    anchor_price: f64,
}

impl BtcEthLiquidityScalperBacktestAdapter {
    fn setup(
        candles: &[CandleItem],
        values: &BtcEthLiquidityScalperBacktestValues,
        tuning: &BtcEthLiquidityScalperBacktestTuning,
    ) -> Option<ScalperBacktestSetup> {
        let trend_fast_window = tuning.trend_fast_window.max(1).min(candles.len());
        let trend_slow_window = tuning.trend_slow_window.max(1).min(candles.len());
        if candles.len() < trend_fast_window.max(trend_slow_window) {
            return None;
        }
        let last = candles.last()?;
        let fast_trend = sma_close(&candles[candles.len() - trend_fast_window..]);
        let slow_trend = sma_close(&candles[candles.len() - trend_slow_window..]);
        let bias = if last.c > fast_trend && fast_trend > slow_trend {
            "long"
        } else if last.c < fast_trend && fast_trend < slow_trend {
            "short"
        } else {
            return None;
        };
        if bias == "short" && !tuning.allow_short {
            return None;
        }
        if directional_move_ratio(candles, 48, bias) < tuning.min_directional_ratio_48
            || directional_move_ratio(candles, 24, bias) < tuning.min_directional_ratio_24
        {
            return None;
        }
        let impulse_index = recent_impulse_index(candles, bias, tuning)?;
        if !has_pullback_and_resume(candles, impulse_index, bias, tuning)
            || (tuning.require_previous_extreme_break && !breaks_previous_candle(candles, bias))
        {
            return None;
        }
        let anchor_price = if bias == "long" {
            last.c - values.atr_5m * 0.3
        } else {
            last.c + values.atr_5m * 0.3
        };
        Some(ScalperBacktestSetup { bias, anchor_price })
    }
}

fn recent_impulse_index(
    candles: &[CandleItem],
    bias: &str,
    tuning: &BtcEthLiquidityScalperBacktestTuning,
) -> Option<usize> {
    let start = candles.len().saturating_sub(12).max(1);
    let end = candles.len().saturating_sub(1);
    let avg_range = average_range(&candles[start - 1..end]).max(0.0001);
    let avg_volume = average_volume(&candles[start - 1..end]).max(0.0001);
    (start..end).rev().find(|index| {
        let current = &candles[*index];
        let previous = &candles[*index - 1];
        let move_size = current.c - previous.c;
        let range = (current.h - current.l).abs().max(0.0001);
        let body_ratio = (current.c - current.o).abs() / range;
        let volume_ok = current.v >= avg_volume * tuning.impulse_min_volume_mult;
        match bias {
            "long" => {
                move_size >= avg_range * tuning.impulse_move_range_mult
                    && current.c > current.o
                    && body_ratio >= tuning.impulse_min_body_ratio
                    && volume_ok
            }
            "short" => {
                move_size <= -avg_range * tuning.impulse_move_range_mult
                    && current.c < current.o
                    && body_ratio >= tuning.impulse_min_body_ratio
                    && volume_ok
            }
            _ => false,
        }
    })
}

fn taker_aggression_for_bias(
    context: &BtcEthLiquidityScalperBacktestMarketContext,
    bias: &str,
) -> f64 {
    let total = context.taker_buy_volume + context.taker_sell_volume;
    if total <= 0.0 {
        return 0.0;
    }
    match bias {
        "long" => context.taker_buy_volume / total,
        "short" => context.taker_sell_volume / total,
        _ => 0.0,
    }
}

fn breaks_previous_candle(candles: &[CandleItem], bias: &str) -> bool {
    if candles.len() < 2 {
        return false;
    }
    let last = &candles[candles.len() - 1];
    let previous = &candles[candles.len() - 2];
    match bias {
        "long" => last.c > previous.h,
        "short" => last.c < previous.l,
        _ => false,
    }
}

fn has_pullback_and_resume(
    candles: &[CandleItem],
    impulse_index: usize,
    bias: &str,
    tuning: &BtcEthLiquidityScalperBacktestTuning,
) -> bool {
    let Some(last) = candles.last() else {
        return false;
    };
    let impulse = &candles[impulse_index];
    let after_impulse = &candles[impulse_index + 1..];
    if after_impulse.len() < 2 {
        return false;
    }
    let body = (impulse.c - impulse.o).abs().max(0.0001);
    match bias {
        "long" => {
            let pullback_low = after_impulse
                .iter()
                .map(|candle| candle.l)
                .fold(f64::INFINITY, f64::min);
            let depth = (impulse.c - pullback_low) / body;
            (tuning.pullback_min_depth..=tuning.pullback_max_depth).contains(&depth)
                && last.c > last.o
                && last.c <= impulse.h + body * tuning.resume_extension_body_mult
        }
        "short" => {
            let pullback_high = after_impulse
                .iter()
                .map(|candle| candle.h)
                .fold(f64::NEG_INFINITY, f64::max);
            let depth = (pullback_high - impulse.c) / body;
            (tuning.pullback_min_depth..=tuning.pullback_max_depth).contains(&depth)
                && last.c < last.o
                && last.c >= impulse.l - body * tuning.resume_extension_body_mult
        }
        _ => false,
    }
}

fn directional_move_ratio(candles: &[CandleItem], lookback: usize, bias: &str) -> f64 {
    if candles.len() < 2 {
        return 0.0;
    }
    let lookback = lookback.min(candles.len() - 1);
    let start = candles.len() - lookback - 1;
    let window = &candles[start..];
    let Some(first) = window.first() else {
        return 0.0;
    };
    let Some(last) = window.last() else {
        return 0.0;
    };
    let directional_move = match bias {
        "long" => last.c - first.c,
        "short" => first.c - last.c,
        _ => return 0.0,
    };
    if directional_move <= 0.0 {
        return 0.0;
    }
    let total_move = window
        .windows(2)
        .map(|pair| (pair[1].c - pair[0].c).abs())
        .sum::<f64>();
    directional_move / total_move.max(0.0001)
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

fn sma_close(candles: &[CandleItem]) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    candles.iter().map(|candle| candle.c).sum::<f64>() / candles.len() as f64
}

fn apply_backtest_exit_tuning(
    signal: &mut SignalResult,
    tuning: BtcEthLiquidityScalperBacktestTuning,
) {
    let Some(stop) = signal.signal_kline_stop_loss_price else {
        return;
    };
    let risk = (signal.open_price - stop).abs();
    if risk <= 0.0 {
        return;
    }
    if signal.should_buy {
        signal.atr_take_profit_level_1 =
            Some(round_price(signal.open_price + risk * tuning.target_r_1));
        signal.atr_take_profit_level_2 =
            Some(round_price(signal.open_price + risk * tuning.target_r_2));
        signal.atr_take_profit_level_3 = signal.atr_take_profit_level_2;
    } else if signal.should_sell {
        signal.atr_take_profit_level_1 =
            Some(round_price(signal.open_price - risk * tuning.target_r_1));
        signal.atr_take_profit_level_2 =
            Some(round_price(signal.open_price - risk * tuning.target_r_2));
        signal.atr_take_profit_level_3 = signal.atr_take_profit_level_2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::SignalDirection;

    #[test]
    fn backtest_exit_tuning_rewrites_scalper_take_profit_levels() {
        let mut signal = SignalResult {
            should_buy: true,
            direction: SignalDirection::Long,
            open_price: 100.0,
            signal_kline_stop_loss_price: Some(99.0),
            atr_take_profit_level_1: Some(100.8),
            atr_take_profit_level_2: Some(101.6),
            atr_take_profit_level_3: Some(101.6),
            ..Default::default()
        };

        apply_backtest_exit_tuning(
            &mut signal,
            BtcEthLiquidityScalperBacktestTuning {
                target_r_1: 0.4,
                target_r_2: 0.8,
                ..Default::default()
            },
        );

        assert_eq!(signal.atr_take_profit_level_1, Some(100.4));
        assert_eq!(signal.atr_take_profit_level_2, Some(100.8));
        assert_eq!(signal.atr_take_profit_level_3, Some(100.8));
    }
}
