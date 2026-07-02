use rust_quant_domain::SignalDirection;
use rust_quant_strategies::framework::backtest::types::{
    BackTestResult, BasicRiskStrategyConfig, SignalResult,
};
use rust_quant_strategies::framework::backtest::{
    run_indicator_strategy_backtest, IndicatorStrategyBacktest,
};
use rust_quant_strategies::CandleItem;
use serde_json::json;

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const UTC_AFTER_ONE_START_MINUTE: i64 = 60;
const UTC_AFTER_ONE_END_MINUTE: i64 = 8 * 60;
const US_PREMARKET_START_MINUTE: i64 = 12 * 60 + 30;
const US_PREMARKET_END_MINUTE: i64 = 14 * 60;
const BEIJING_MIDNIGHT_UTC_MINUTE: i64 = 16 * 60;
const STRATEGY_TAG: &str = "eth_volume_reversal_5m_v1_research";
const DUAL_STRATEGY_TAG: &str = "eth_volume_reversal_dual_5m_v1_research";
const BTC_HYBRID_STRATEGY_TAG: &str = "btc_volume_reversal_hybrid_5m_v1_research";
const STOP_SOURCE: &str = "ETH_VOLUME_REVERSAL_5M_TRIGGER_LOW";
const SHORT_STOP_SOURCE: &str = "ETH_VOLUME_REVERSAL_DUAL_5M_TRIGGER_HIGH";
const BTC_FAILED_REBOUND_SHORT_STOP_SOURCE: &str =
    "BTC_VOLUME_REVERSAL_HYBRID_5M_FAILED_REBOUND_HIGH";

/// Runs the ETH 5m volume-reversal research preset through the shared backtest pipeline.
///
/// This is intentionally scoped to the CLI research runner. It does not register a production
/// strategy, does not create execution tasks, and does not authorize live order mutation.
pub fn run_eth_volume_reversal_5m(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
) -> BackTestResult {
    run_eth_volume_reversal_5m_with_tuning(
        inst_id,
        candles,
        risk,
        EthVolumeReversal5mTuning::default(),
    )
}

/// Runs the dual-direction research version without changing the long-only candidate.
pub fn run_eth_volume_reversal_dual_5m(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
) -> BackTestResult {
    run_indicator_strategy_backtest(
        inst_id,
        EthVolumeReversalDual5m::default(),
        candles,
        volume_reversal_risk_config(risk),
    )
}

/// Runs the BTC-specific dual research preset using only lower-volatility parameter adjustments.
pub fn run_btc_volume_reversal_dual_5m(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
) -> BackTestResult {
    let tuning = btc_volume_reversal_5m_tuning();
    run_btc_volume_reversal_dual_5m_with_tuning(inst_id, candles, risk, tuning)
}

/// Runs the BTC hybrid research preset with long reversals plus failed weak-rebound shorts.
pub fn run_btc_volume_reversal_hybrid_5m(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
) -> BackTestResult {
    let tuning = btc_volume_reversal_5m_tuning();
    run_indicator_strategy_backtest(
        inst_id,
        BtcVolumeReversalHybrid5m {
            long: EthVolumeReversal5m {
                tuning,
                cooldown_remaining: 0,
            },
            failed_short_tuning: BtcFailedWeakReboundShortTuning::default(),
            pending_failed_short: None,
            short_cooldown_remaining: 0,
        },
        candles,
        volume_reversal_risk_config_for_tuning(risk, tuning),
    )
}

/// Runs the BTC dual research preset with an explicit low-dimensional scan tuning.
pub(super) fn run_btc_volume_reversal_dual_5m_with_tuning(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
    tuning: EthVolumeReversal5mTuning,
) -> BackTestResult {
    run_indicator_strategy_backtest(
        inst_id,
        EthVolumeReversalDual5m {
            long: EthVolumeReversal5m {
                tuning,
                cooldown_remaining: 0,
            },
            short_tuning: EthInvertedVShortTuning::default(),
            pending_short: None,
            short_cooldown_remaining: 0,
        },
        candles,
        volume_reversal_risk_config_for_tuning(risk, tuning),
    )
}

/// Runs the same research preset with an explicit parameter set from the scan grid.
pub(super) fn run_eth_volume_reversal_5m_with_tuning(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
    tuning: EthVolumeReversal5mTuning,
) -> BackTestResult {
    run_indicator_strategy_backtest(
        inst_id,
        EthVolumeReversal5m {
            tuning,
            cooldown_remaining: 0,
        },
        candles,
        volume_reversal_risk_config_for_tuning(risk, tuning),
    )
}

/// Applies the user-specified risk contract for this research preset: trigger-low stop and 10x.
pub(super) fn volume_reversal_risk_config(
    mut risk: BasicRiskStrategyConfig,
) -> BasicRiskStrategyConfig {
    risk.position_leverage = Some(10.0);
    risk.is_used_signal_k_line_stop_loss = Some(true);
    risk.max_loss_percent = 100.0;
    risk.dynamic_max_loss = Some(false);
    risk.atr_take_profit_ratio = None;
    risk.fixed_signal_kline_take_profit_ratio = None;
    risk
}

/// Applies optional partial-take-profit ratios for tiered research runs.
pub(super) fn volume_reversal_risk_config_for_tuning(
    risk: BasicRiskStrategyConfig,
    tuning: EthVolumeReversal5mTuning,
) -> BasicRiskStrategyConfig {
    let mut risk = volume_reversal_risk_config(risk);
    if tuning.tiered_take_profit {
        risk.tiered_take_profit_level_1_close_ratio =
            Some(tuning.partial_take_profit_level_1_close_ratio);
        risk.tiered_take_profit_level_2_close_ratio =
            Some(tuning.partial_take_profit_level_2_close_ratio);
    }
    risk
}

/// Tunable thresholds for the ETH 5m volume-reversal research preset.
#[derive(Debug, Clone, Copy)]
pub(super) struct EthVolumeReversal5mTuning {
    /// Volume baseline window, in 5m candles.
    pub(super) volume_window: usize,
    /// Current-volume multiple required versus the previous window average.
    pub(super) volume_spike_mult: f64,
    /// EMA window used as the left-side full take-profit anchor.
    pub(super) ema_window: usize,
    /// Recent low lookback used to require an actual downside sweep in left-side windows.
    pub(super) sweep_lookback: usize,
    /// Fibonacci lookback used by the pre-market right-side retracement entry.
    pub(super) fib_lookback: usize,
    /// Minimum intrabar downside excursion from open/previous close.
    pub(super) min_downside_excursion_pct: f64,
    /// Minimum close position after the washout, where 0 is low and 1 is high.
    pub(super) min_rebound_close_pos: f64,
    /// Reject low-conviction long rebounds when both body and range are too small.
    pub(super) weak_rebound_body_pct: Option<f64>,
    /// Maximum candle range for the weak-rebound body filter, in percent of entry price.
    pub(super) weak_rebound_range_pct: Option<f64>,
    /// Reject entries whose trigger-low stop is wider than this fraction of entry price.
    pub(super) max_stop_pct: Option<f64>,
    /// Extra stop buffer below the trigger low; this is only for research scans.
    pub(super) stop_buffer_pct: f64,
    /// Minimum room from entry to EMA696, in percent.
    pub(super) min_ema_distance_pct: Option<f64>,
    /// Reject targets whose reward/risk is below this floor.
    pub(super) min_target_r: f64,
    /// Optional fixed-R target override; None keeps the session anchor target.
    pub(super) target_r_override: Option<f64>,
    /// Use current UTC day's high/low as the pre-market Fibonacci anchor.
    pub(super) use_utc_day_fib: bool,
    /// Enable staged protection levels before the final take-profit.
    pub(super) tiered_take_profit: bool,
    /// First staged protection level in R; moves stop to breakeven in the shared risk engine.
    pub(super) tier_1_r: f64,
    /// Second staged protection level in R; moves stop to level 1 in the shared risk engine.
    pub(super) tier_2_r: f64,
    /// First partial-take-profit close ratio, relative to current remaining position.
    pub(super) partial_take_profit_level_1_close_ratio: f64,
    /// Second partial-take-profit close ratio, relative to current remaining position.
    pub(super) partial_take_profit_level_2_close_ratio: f64,
    /// Whether UTC-after-01:00 left-side entries are allowed.
    pub(super) allow_utc_after_one: bool,
    /// Whether U.S. pre-market Fibonacci entries are allowed.
    pub(super) allow_us_premarket_fib: bool,
    /// Whether Beijing-after-midnight entries are allowed.
    pub(super) allow_beijing_midnight: bool,
    /// Cooldown after an accepted signal, in 5m candles.
    pub(super) cooldown_candles: usize,
}

impl Default for EthVolumeReversal5mTuning {
    fn default() -> Self {
        Self {
            volume_window: 20,
            volume_spike_mult: 3.0,
            ema_window: 696,
            sweep_lookback: 48,
            fib_lookback: 144,
            min_downside_excursion_pct: 0.004,
            min_rebound_close_pos: 0.50,
            weak_rebound_body_pct: Some(0.12),
            weak_rebound_range_pct: Some(0.80),
            max_stop_pct: Some(0.012),
            stop_buffer_pct: 0.0,
            min_ema_distance_pct: Some(1.5),
            min_target_r: 1.5,
            target_r_override: None,
            use_utc_day_fib: true,
            tiered_take_profit: false,
            tier_1_r: 1.0,
            tier_2_r: 2.0,
            partial_take_profit_level_1_close_ratio: 0.40,
            partial_take_profit_level_2_close_ratio: 0.50,
            allow_utc_after_one: true,
            allow_us_premarket_fib: true,
            allow_beijing_midnight: false,
            cooldown_candles: 12,
        }
    }
}

impl EthVolumeReversal5mTuning {
    fn allows_mode(self, mode: EntryMode) -> bool {
        match mode {
            EntryMode::LeftUtcAfterOne => self.allow_utc_after_one,
            EntryMode::RightUsPremarketFib => self.allow_us_premarket_fib,
            EntryMode::LeftBeijingAfterMidnight => self.allow_beijing_midnight,
        }
    }
}

/// BTC has lower short-cycle volatility than ETH, so the research preset keeps the same mechanics
/// but uses a stricter volume event and a fixed 3R target instead of assuming full mean reversion.
pub(super) fn btc_volume_reversal_5m_tuning() -> EthVolumeReversal5mTuning {
    EthVolumeReversal5mTuning {
        volume_spike_mult: 4.0,
        min_downside_excursion_pct: 0.004,
        min_rebound_close_pos: 0.50,
        weak_rebound_body_pct: Some(0.20),
        weak_rebound_range_pct: Some(1.00),
        target_r_override: Some(3.0),
        min_target_r: 3.0,
        ..EthVolumeReversal5mTuning::default()
    }
}

/// Research-only ETH 5m volume reversal state.
#[derive(Debug, Clone)]
struct EthVolumeReversal5m {
    tuning: EthVolumeReversal5mTuning,
    cooldown_remaining: usize,
}

impl Default for EthVolumeReversal5m {
    fn default() -> Self {
        Self {
            tuning: EthVolumeReversal5mTuning::default(),
            cooldown_remaining: 0,
        }
    }
}

/// Research-only state for mixing the validated long reversal with a BJ inverted-V short.
#[derive(Debug, Clone)]
struct EthVolumeReversalDual5m {
    long: EthVolumeReversal5m,
    short_tuning: EthInvertedVShortTuning,
    pending_short: Option<PendingInvertedVShort>,
    short_cooldown_remaining: usize,
}

/// BTC hybrid state that keeps accepted long reversals and tracks failed weak-rebound shorts.
#[derive(Debug, Clone)]
struct BtcVolumeReversalHybrid5m {
    long: EthVolumeReversal5m,
    failed_short_tuning: BtcFailedWeakReboundShortTuning,
    pending_failed_short: Option<PendingFailedWeakReboundShort>,
    short_cooldown_remaining: usize,
}

impl Default for EthVolumeReversalDual5m {
    fn default() -> Self {
        Self {
            long: EthVolumeReversal5m::default(),
            short_tuning: EthInvertedVShortTuning::default(),
            pending_short: None,
            short_cooldown_remaining: 0,
        }
    }
}

/// Short-side confirmation for weak BTC rebounds that the long preset intentionally rejects.
#[derive(Debug, Clone, Copy)]
pub(super) struct BtcFailedWeakReboundShortTuning {
    /// Reject short entries whose trigger-high stop is wider than this fraction of entry.
    max_stop_pct: Option<f64>,
    /// Require the failed rebound trigger to have recovered into the upper part of its range.
    min_trigger_rebound_close_pos: f64,
    /// Require the failed rebound trigger body to remain compact.
    max_trigger_body_pct: f64,
    /// Fixed-R target for failed weak-rebound shorts.
    target_r: f64,
    /// Cooldown after an accepted failed-rebound short, in 5m candles.
    cooldown_candles: usize,
}

impl Default for BtcFailedWeakReboundShortTuning {
    fn default() -> Self {
        Self {
            max_stop_pct: Some(0.018),
            min_trigger_rebound_close_pos: 0.65,
            max_trigger_body_pct: 0.12,
            target_r: 1.5,
            cooldown_candles: 12,
        }
    }
}

/// Weak-rebound trigger waiting for one continuation candle before opening a short.
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingFailedWeakReboundShort {
    trigger_low: f64,
    trigger_high: f64,
    trigger_close: f64,
    trigger_ts: i64,
    volume: f64,
    volume_avg: f64,
    volume_multiple: f64,
    downside_excursion_pct: f64,
    rebound_close_pos: f64,
    candle_range_pct: f64,
    body_pct: f64,
}

/// Short-side thresholds are intentionally separate from long-side filters.
///
/// The first research question is whether the Beijing-midnight long drag becomes useful only when
/// treated as a right-side inverted-V failure, so this default does not enable short entries in
/// every time bucket. Shorts also require one continuation candle because the failed samples were
/// liquidation-like selloffs that snapped back before reaching a fixed 2R target. The relaxed
/// impulse/body thresholds are guarded by a confirmation-volume contraction filter so that active
/// liquidation flow does not get shorted into a snapback.
#[derive(Debug, Clone, Copy)]
struct EthInvertedVShortTuning {
    /// Volume baseline window, in 5m candles.
    volume_window: usize,
    /// Recent lookback used to measure the left leg of the inverted V.
    impulse_lookback: usize,
    /// Minimum rally size before the failure, expressed as a fraction of the leg low.
    min_impulse_pct: f64,
    /// Required retracement fraction from the V top back toward the leg low.
    min_break_retrace: f64,
    /// Minimum bearish body size on the confirmation candle.
    min_breakdown_body_pct: f64,
    /// Current-volume multiple required versus the previous window average.
    volume_spike_mult: f64,
    /// Reject entries whose confirmation-candle-high stop is wider than this fraction of entry.
    max_stop_pct: Option<f64>,
    /// Reject soft inverted-V entries when the continuation candle still has active liquidation flow.
    max_confirmation_volume_ratio: Option<f64>,
    /// Fixed-R target for confirmed short entries.
    target_r: f64,
    /// Cooldown after an accepted short signal, in 5m candles.
    cooldown_candles: usize,
    /// Whether Beijing-midnight inverted-V shorts are allowed.
    allow_beijing_midnight: bool,
}

impl Default for EthInvertedVShortTuning {
    fn default() -> Self {
        Self {
            volume_window: 20,
            impulse_lookback: 6,
            min_impulse_pct: 0.010,
            min_break_retrace: 0.382,
            min_breakdown_body_pct: 0.006,
            volume_spike_mult: 2.0,
            max_stop_pct: Some(0.025),
            max_confirmation_volume_ratio: Some(0.35),
            target_r: 1.5,
            cooldown_candles: 12,
            allow_beijing_midnight: true,
        }
    }
}

/// Candidate short setup captured on the volume-spike breakdown candle.
#[derive(Debug, Clone, Copy)]
struct PendingInvertedVShort {
    /// Trigger candle close; the next candle must close below this price to prove continuation.
    trigger_close: f64,
    /// Stop anchor from the original breakdown candle high.
    trigger_stop: f64,
    /// Trigger candle timestamp, Unix milliseconds.
    trigger_ts: i64,
    /// Trigger candle volume, in exchange candle volume units.
    trigger_volume: f64,
    /// Previous-window average volume used for the spike multiple.
    volume_avg: f64,
    /// Trigger volume divided by the previous-window average volume.
    volume_multiple: f64,
    /// Low of the pre-failure inverted-V leg.
    impulse_low: f64,
    /// High of the pre-failure inverted-V leg.
    impulse_high: f64,
    /// Rally size before failure, as a fraction of the leg low.
    impulse_pct: f64,
    /// Required retracement line used by the initial breakdown candle.
    retrace_line: f64,
    /// Bearish body percentage of the initial breakdown candle.
    breakdown_body_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryMode {
    LeftUtcAfterOne,
    RightUsPremarketFib,
    LeftBeijingAfterMidnight,
}

impl EntryMode {
    fn as_str(self) -> &'static str {
        match self {
            EntryMode::LeftUtcAfterOne => "left_utc_after_one",
            EntryMode::RightUsPremarketFib => "right_us_premarket_fib",
            EntryMode::LeftBeijingAfterMidnight => "left_beijing_after_midnight",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FibContext {
    fib_0236: f64,
    swing_low: f64,
    swing_high: f64,
}

impl IndicatorStrategyBacktest for EthVolumeReversal5m {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.tuning.ema_window + 1
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        _candle: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    /// Generates an immediate long signal on the current 5m volume-spike candle.
    ///
    /// The existing candle backtest only has completed 5m bars, so "immediate" is represented by
    /// entering on the signal candle close instead of waiting for a later support reclaim candle.
    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        _values: &mut Self::IndicatorValues,
        _risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        let Some(last) = candles.last() else {
            return SignalResult::default();
        };
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return SignalResult::default();
        }
        let Some(previous) = candles.get(candles.len().saturating_sub(2)) else {
            return SignalResult::default();
        };
        let Some(mode) = entry_mode_for_ts(last.ts) else {
            return SignalResult::default();
        };
        if !self.tuning.allows_mode(mode) {
            return SignalResult::default();
        }
        let Some(volume_avg) = previous_volume_average(candles, self.tuning.volume_window) else {
            return SignalResult::default();
        };
        if volume_avg <= 0.0 || last.v < volume_avg * self.tuning.volume_spike_mult {
            return SignalResult::default();
        }
        if !has_sharp_downside_reversal(last, previous, self.tuning) {
            return SignalResult::default();
        }

        let fib = fib_context_for_tuning(candles, self.tuning);
        let mode_confirmed = match mode {
            EntryMode::RightUsPremarketFib => fib
                .as_ref()
                .is_some_and(|ctx| candle_touches_price(last, ctx.fib_0236)),
            EntryMode::LeftUtcAfterOne | EntryMode::LeftBeijingAfterMidnight => {
                sweeps_recent_low(candles, self.tuning.sweep_lookback)
            }
        };
        if !mode_confirmed {
            return SignalResult::default();
        }

        let entry_price = round_price(last.c);
        let stop_price = round_price(last.l - entry_price * self.tuning.stop_buffer_pct);
        if stop_price >= entry_price {
            return SignalResult::default();
        }
        let risk = entry_price - stop_price;
        let stop_pct = risk / entry_price.max(1e-9);
        if self
            .tuning
            .max_stop_pct
            .is_some_and(|max_stop_pct| stop_pct > max_stop_pct)
        {
            return SignalResult::default();
        }

        let Some((target_price, target_source, ema696)) =
            target_for_mode(mode, candles, entry_price, stop_price, self.tuning)
        else {
            return SignalResult::default();
        };
        let ema_distance_pct = (ema696 - entry_price) / entry_price.max(1e-9) * 100.0;
        if self
            .tuning
            .min_ema_distance_pct
            .is_some_and(|min_distance| ema_distance_pct < min_distance)
        {
            return SignalResult::default();
        }
        let target_r = (target_price - entry_price) / risk.max(1e-9);
        if target_r < self.tuning.min_target_r {
            return SignalResult::default();
        }
        let (target_level_1, target_level_2, target_level_3) =
            tiered_targets(entry_price, stop_price, target_price, self.tuning);
        let volume_multiple = last.v / volume_avg;
        let candle_range = (last.h - last.l).max(0.0);
        let candle_range_pct = candle_range / entry_price.max(1e-9) * 100.0;
        let body_pct = (last.c - last.o).abs() / entry_price.max(1e-9) * 100.0;
        if is_weak_compact_rebound(body_pct, candle_range_pct, self.tuning) {
            return SignalResult::default();
        }
        let downside_excursion_pct =
            downside_excursion(last.o, last.l).max(downside_excursion(previous.c, last.l)) * 100.0;
        let rebound_close_pos = if candle_range > 0.0 {
            (last.c - last.l) / candle_range
        } else {
            0.0
        };
        let lower_wick_pct = (last.o.min(last.c) - last.l).max(0.0) / entry_price.max(1e-9) * 100.0;
        let upper_wick_pct = (last.h - last.o.max(last.c)).max(0.0) / entry_price.max(1e-9) * 100.0;
        let mut reasons = vec![
            "ETH_VOLUME_REVERSAL_5M_SPIKE".to_string(),
            format!("ENTRY_MODE:{}", mode.as_str()),
            format!("VOLUME_MULT:{:.2}", volume_multiple),
            format!("STOP_SOURCE:{STOP_SOURCE}"),
            format!("TARGET_SOURCE:{target_source}"),
            format!("TARGET_R:{:.2}", target_r),
        ];
        if let Some(ctx) = fib {
            reasons.push(format!("FIB_0236:{:.2}", ctx.fib_0236));
        }
        let snapshot = json!({
            "strategy": STRATEGY_TAG,
            "entry_mode": mode.as_str(),
            "price": entry_price,
            "stop_price": stop_price,
            "target_price": target_price,
            "target_source": target_source,
            "target_r": target_r,
            "tiered_take_profit": self.tuning.tiered_take_profit,
            "target_level_1": target_level_1,
            "target_level_2": target_level_2,
            "target_level_3": target_level_3,
            "partial_take_profit_level_1_close_ratio": if self.tuning.tiered_take_profit { Some(self.tuning.partial_take_profit_level_1_close_ratio) } else { None },
            "partial_take_profit_level_2_close_ratio": if self.tuning.tiered_take_profit { Some(self.tuning.partial_take_profit_level_2_close_ratio) } else { None },
            "ema696": ema696,
            "volume": last.v,
            "volume_avg_20": volume_avg,
            "volume_multiple": volume_multiple,
            "downside_excursion_pct": downside_excursion_pct,
            "rebound_close_pos": rebound_close_pos,
            "candle_range_pct": candle_range_pct,
            "body_pct": body_pct,
            "lower_wick_pct": lower_wick_pct,
            "upper_wick_pct": upper_wick_pct,
            "leverage": 10.0,
            "fib_0236": fib.map(|ctx| ctx.fib_0236),
            "fib_swing_low": fib.map(|ctx| ctx.swing_low),
            "fib_swing_high": fib.map(|ctx| ctx.swing_high),
            "reasons": reasons,
        });

        self.cooldown_remaining = self.tuning.cooldown_candles;
        SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: entry_price,
            signal_kline_stop_loss_price: Some(stop_price),
            stop_loss_source: Some(STOP_SOURCE.to_string()),
            atr_stop_loss_price: Some(stop_price),
            atr_take_profit_level_1: Some(target_level_1),
            atr_take_profit_level_2: Some(target_level_2),
            atr_take_profit_level_3: Some(target_level_3),
            ts: last.ts,
            single_value: Some(snapshot.to_string()),
            single_result: Some(json!({ "reasons": snapshot["reasons"] }).to_string()),
            filter_reasons: vec![format!("{STRATEGY_TAG}_CONFIRMED")],
            direction: SignalDirection::Long,
            ..SignalResult::default()
        }
    }
}

impl IndicatorStrategyBacktest for EthVolumeReversalDual5m {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.long.min_data_length()
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        _candle: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        if self.short_cooldown_remaining > 0 {
            self.short_cooldown_remaining -= 1;
        } else {
            if let Some(pending) = self.pending_short.take() {
                if let Some(signal) =
                    confirmed_inverted_v_short_signal(candles, pending, self.short_tuning)
                {
                    self.short_cooldown_remaining = self.short_tuning.cooldown_candles;
                    return signal;
                }
                if let Some(setup) = inverted_v_short_setup(candles, self.short_tuning) {
                    self.pending_short = Some(setup);
                    return SignalResult::default();
                }
            } else if let Some(setup) = inverted_v_short_setup(candles, self.short_tuning) {
                self.pending_short = Some(setup);
                return SignalResult::default();
            }
        }
        self.long.generate_signal(candles, values, risk_config)
    }
}

impl IndicatorStrategyBacktest for BtcVolumeReversalHybrid5m {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.long.min_data_length()
    }

    fn init_indicator_combine(&self) -> Self::IndicatorCombine {}

    fn build_indicator_values(
        _indicator_combine: &mut Self::IndicatorCombine,
        _candle: &CandleItem,
    ) -> Self::IndicatorValues {
    }

    fn generate_signal(
        &mut self,
        candles: &[CandleItem],
        values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        if self.short_cooldown_remaining > 0 {
            self.short_cooldown_remaining -= 1;
        } else if let Some(pending) = self.pending_failed_short.take() {
            if let Some(signal) = confirmed_failed_weak_rebound_short_signal(
                candles,
                pending,
                self.failed_short_tuning,
            ) {
                self.short_cooldown_remaining = self.failed_short_tuning.cooldown_candles;
                return signal;
            }
        }

        if self.short_cooldown_remaining == 0 {
            if let Some(setup) =
                failed_weak_rebound_short_setup(candles, self.long.tuning, self.failed_short_tuning)
            {
                self.pending_failed_short = Some(setup);
                return SignalResult::default();
            }
        }

        self.long.generate_signal(candles, values, risk_config)
    }
}

/// Prints win/loss candle-shape diagnostics for fixed high-R volume-reversal candidates.
pub(super) fn print_volume_reversal_diagnostics(
    loaded_cases: &[super::LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = super::strategy_family_risk_config(risk_percent, trade_fee_rate);
    let volume_cases = loaded_cases
        .iter()
        .filter(|loaded| {
            matches!(
                loaded.case.family,
                super::StrategyFamily::EthVolumeReversal5m
            )
        })
        .collect::<Vec<_>>();
    if volume_cases.is_empty() {
        println!("no_volume_reversal_cases source=quant_core_sharded");
        return;
    }

    for (label, tuning) in volume_reversal_diagnostic_tunings() {
        for loaded in &volume_cases {
            let result = run_eth_volume_reversal_5m_with_tuning(
                loaded.case.symbol,
                &loaded.candles,
                risk,
                tuning,
            );
            let report = super::build_report(label, &loaded.candles, &result);
            println!(
                "volume_reversal_diagnostic label={} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% volume_mult={:.2} target_r={:?} modes=utc:{},premarket:{},bj:{}",
                label,
                report.entries,
                report.wins,
                report.losses,
                report.win_rate_pct,
                report.pnl,
                report.max_drawdown_pct,
                tuning.volume_spike_mult,
                tuning.target_r_override,
                tuning.allow_utc_after_one,
                tuning.allow_us_premarket_fib,
                tuning.allow_beijing_midnight
            );
            print_shape_summary(
                label,
                "win",
                report.trades.iter().filter(|trade| trade.pnl > 0.0),
            );
            print_shape_summary(
                label,
                "loss",
                report.trades.iter().filter(|trade| trade.pnl < 0.0),
            );
        }
    }
}

/// Returns fixed candidate pairs for shape diagnostics and UTC-vs-BJ attribution.
pub(super) fn volume_reversal_diagnostic_tunings() -> Vec<(&'static str, EthVolumeReversal5mTuning)>
{
    vec![
        (
            "utc_only_3r",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 3.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_target_r: 3.0,
                target_r_override: Some(3.0),
                tiered_take_profit: false,
                allow_utc_after_one: true,
                allow_us_premarket_fib: false,
                allow_beijing_midnight: false,
                ..Default::default()
            },
        ),
        (
            "utc_bj_3r",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 3.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_target_r: 3.0,
                target_r_override: Some(3.0),
                tiered_take_profit: false,
                allow_utc_after_one: true,
                allow_us_premarket_fib: false,
                allow_beijing_midnight: true,
                ..Default::default()
            },
        ),
        (
            "bj_only_3r",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 3.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_target_r: 3.0,
                target_r_override: Some(3.0),
                tiered_take_profit: false,
                allow_utc_after_one: false,
                allow_us_premarket_fib: false,
                allow_beijing_midnight: true,
                ..Default::default()
            },
        ),
        (
            "utc_only_2r_volume5",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 5.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_target_r: 2.0,
                target_r_override: Some(2.0),
                tiered_take_profit: false,
                allow_utc_after_one: true,
                allow_us_premarket_fib: false,
                allow_beijing_midnight: false,
                ..Default::default()
            },
        ),
        (
            "utc_only_ema696_room15_volume2",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 2.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_ema_distance_pct: Some(1.5),
                min_target_r: 1.5,
                target_r_override: None,
                tiered_take_profit: false,
                allow_utc_after_one: true,
                allow_us_premarket_fib: false,
                allow_beijing_midnight: false,
                ..Default::default()
            },
        ),
        (
            "utc_only_ema696_room15_volume3",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 3.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_ema_distance_pct: Some(1.5),
                min_target_r: 1.5,
                target_r_override: None,
                tiered_take_profit: false,
                allow_utc_after_one: true,
                allow_us_premarket_fib: false,
                allow_beijing_midnight: false,
                ..Default::default()
            },
        ),
        (
            "premarket_ema696_room15_volume3_dayfib",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 3.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_ema_distance_pct: Some(1.5),
                min_target_r: 1.5,
                target_r_override: None,
                use_utc_day_fib: true,
                tiered_take_profit: false,
                allow_utc_after_one: false,
                allow_us_premarket_fib: true,
                allow_beijing_midnight: false,
                ..Default::default()
            },
        ),
        (
            "utc_premarket_ema696_room15_volume3_dayfib",
            EthVolumeReversal5mTuning {
                volume_spike_mult: 3.0,
                min_downside_excursion_pct: 0.004,
                min_rebound_close_pos: 0.50,
                max_stop_pct: Some(0.012),
                min_ema_distance_pct: Some(1.5),
                min_target_r: 1.5,
                target_r_override: None,
                use_utc_day_fib: true,
                tiered_take_profit: false,
                allow_utc_after_one: true,
                allow_us_premarket_fib: true,
                allow_beijing_midnight: false,
                ..Default::default()
            },
        ),
    ]
}

fn print_shape_summary<'a>(
    label: &str,
    outcome: &str,
    trades: impl Iterator<Item = &'a super::ClosedTradeDebug>,
) {
    let mut count = 0usize;
    let mut pnl = 0.0;
    let mut volume_multiple = 0.0;
    let mut downside = 0.0;
    let mut rebound = 0.0;
    let mut range = 0.0;
    let mut body = 0.0;
    let mut lower_wick = 0.0;
    let mut upper_wick = 0.0;
    let mut stop_distance = 0.0;
    let mut target_r = 0.0;
    let mut ema_distance = 0.0;

    for trade in trades {
        let Some(snapshot) = trade.entry_snapshot else {
            continue;
        };
        count += 1;
        pnl += trade.pnl;
        volume_multiple += snapshot.volume_multiple;
        downside += snapshot.downside_excursion_pct;
        rebound += snapshot.rebound_close_pos;
        range += snapshot.candle_range_pct;
        body += snapshot.body_pct;
        lower_wick += snapshot.lower_wick_pct;
        upper_wick += snapshot.upper_wick_pct;
        stop_distance += snapshot.stop_distance_pct;
        target_r += snapshot.target_r;
        ema_distance += snapshot.ema_distance_pct;
    }
    let divisor = count.max(1) as f64;
    println!(
        "volume_reversal_shape label={} outcome={} count={} avg_pnl={:.4} avg_volume_mult={:.2} avg_downside={:.4}% avg_rebound={:.2} avg_range={:.4}% avg_body={:.4}% avg_lower_wick={:.4}% avg_upper_wick={:.4}% avg_stop_dist={:.4}% avg_target_r={:.2} avg_ema_dist={:.4}%",
        label,
        outcome,
        count,
        pnl / divisor,
        volume_multiple / divisor,
        downside / divisor,
        rebound / divisor,
        range / divisor,
        body / divisor,
        lower_wick / divisor,
        upper_wick / divisor,
        stop_distance / divisor,
        target_r / divisor,
        ema_distance / divisor
    );
}

/// Prints candidate rows for the ETH 5m volume-reversal research scan.
pub(super) fn print_volume_reversal_scan(
    loaded_cases: &[super::LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = super::strategy_family_risk_config(risk_percent, trade_fee_rate);
    let volume_cases = loaded_cases
        .iter()
        .filter(|loaded| {
            matches!(
                loaded.case.family,
                super::StrategyFamily::EthVolumeReversal5m
            )
        })
        .collect::<Vec<_>>();
    if volume_cases.is_empty() {
        println!("no_volume_reversal_cases source=quant_core_sharded");
        return;
    }

    let mut raw_candidates = Vec::new();
    let mut candidates = Vec::new();
    for tuning in volume_reversal_scan_tunings() {
        let reports = volume_cases
            .iter()
            .map(|loaded| {
                let result = run_eth_volume_reversal_5m_with_tuning(
                    loaded.case.symbol,
                    &loaded.candles,
                    risk,
                    tuning,
                );
                super::build_report(loaded.case.label, &loaded.candles, &result)
            })
            .collect::<Vec<_>>();
        let summary = summarize_volume_reversal_reports(&reports, tuning);
        if volume_reversal_candidate_meets_constraints(&summary) {
            candidates.push(summary);
        }
        raw_candidates.push(summary);
    }

    sort_volume_reversal_candidates(&mut raw_candidates);
    for (index, candidate) in raw_candidates.iter().take(12).enumerate() {
        print_volume_reversal_candidate("volume_reversal_raw_top", index + 1, candidate);
    }
    let mut sampled_candidates = raw_candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.entries >= 3)
        .collect::<Vec<_>>();
    sort_volume_reversal_candidates(&mut sampled_candidates);
    for (index, candidate) in sampled_candidates.iter().take(12).enumerate() {
        print_volume_reversal_candidate("volume_reversal_sample_top", index + 1, candidate);
    }
    sampled_candidates.sort_by(|left, right| {
        right
            .pnl
            .total_cmp(&left.pnl)
            .then_with(|| right.win_rate_pct.total_cmp(&left.win_rate_pct))
            .then_with(|| left.max_drawdown_pct.total_cmp(&right.max_drawdown_pct))
    });
    for (index, candidate) in sampled_candidates.iter().take(12).enumerate() {
        print_volume_reversal_candidate("volume_reversal_sample_pnl_top", index + 1, candidate);
    }

    sort_volume_reversal_candidates(&mut candidates);
    if candidates.is_empty() {
        println!(
            "no_volume_reversal_candidates source=quant_core_sharded constraints=entries>=3,win_rate>=55,pnl>0,max_dd<15,profit_factor>=1.5"
        );
        return;
    }
    for (index, candidate) in candidates.iter().take(20).enumerate() {
        print_volume_reversal_candidate("volume_reversal_candidate", index + 1, candidate);
    }
}

/// Prints a BTC-specific frequency scan without changing the accepted research preset.
pub(super) fn print_btc_volume_reversal_frequency_scan(
    loaded_cases: &[super::LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = super::strategy_family_risk_config(risk_percent, trade_fee_rate);
    let volume_cases = loaded_cases
        .iter()
        .filter(|loaded| {
            matches!(
                loaded.case.family,
                super::StrategyFamily::BtcVolumeReversalDual5m
            )
        })
        .collect::<Vec<_>>();
    if volume_cases.is_empty() {
        println!("no_btc_volume_reversal_cases source=quant_core_sharded");
        return;
    }

    let mut raw_candidates = Vec::new();
    let mut goal_candidates = Vec::new();
    for tuning in btc_volume_reversal_frequency_scan_tunings() {
        let reports = volume_cases
            .iter()
            .map(|loaded| {
                let result = run_btc_volume_reversal_dual_5m_with_tuning(
                    loaded.case.symbol,
                    &loaded.candles,
                    risk,
                    tuning,
                );
                super::build_report(loaded.case.label, &loaded.candles, &result)
            })
            .collect::<Vec<_>>();
        let summary = summarize_volume_reversal_reports(&reports, tuning);
        if btc_volume_reversal_frequency_candidate_meets_goal(&summary) {
            goal_candidates.push(summary);
        }
        raw_candidates.push(summary);
    }

    sort_btc_volume_reversal_frequency_candidates(&mut raw_candidates);
    for (index, candidate) in raw_candidates.iter().take(16).enumerate() {
        print_volume_reversal_candidate(
            "btc_volume_reversal_frequency_raw_top",
            index + 1,
            candidate,
        );
    }
    raw_candidates.sort_by(|left, right| {
        right
            .pnl
            .total_cmp(&left.pnl)
            .then_with(|| right.entries.cmp(&left.entries))
            .then_with(|| left.max_drawdown_pct.total_cmp(&right.max_drawdown_pct))
    });
    for (index, candidate) in raw_candidates.iter().take(16).enumerate() {
        print_volume_reversal_candidate(
            "btc_volume_reversal_frequency_pnl_top",
            index + 1,
            candidate,
        );
    }

    sort_btc_volume_reversal_frequency_candidates(&mut goal_candidates);
    if goal_candidates.is_empty() {
        println!(
            "no_btc_volume_reversal_frequency_goal source=quant_core_sharded constraints=entries>5,pnl>71.3992,win_rate>=50,max_dd<15"
        );
        return;
    }
    for (index, candidate) in goal_candidates.iter().take(20).enumerate() {
        print_volume_reversal_candidate("btc_volume_reversal_frequency_goal", index + 1, candidate);
    }
}

/// Builds the bounded scan grid for the research-only ETH volume-reversal strategy.
pub(super) fn volume_reversal_scan_tunings() -> Vec<EthVolumeReversal5mTuning> {
    let mut tunings = Vec::new();
    for (allow_utc_after_one, allow_us_premarket_fib, allow_beijing_midnight) in [
        (false, true, false),
        (true, false, false),
        (false, false, true),
        (true, false, true),
    ] {
        for volume_spike_mult in [3.0, 5.0] {
            for min_downside_excursion_pct in [0.004, 0.010] {
                for min_rebound_close_pos in [0.50] {
                    for max_stop_pct in [Some(0.006), Some(0.012)] {
                        for target_r_override in
                            [None, Some(1.0), Some(1.2), Some(1.5), Some(2.0), Some(3.0)]
                        {
                            for min_ema_distance_pct in [None, Some(1.0), Some(1.5)] {
                                for use_utc_day_fib in [false, true] {
                                    tunings.push(EthVolumeReversal5mTuning {
                                        volume_spike_mult,
                                        min_downside_excursion_pct,
                                        min_rebound_close_pos,
                                        max_stop_pct,
                                        min_ema_distance_pct,
                                        min_target_r: target_r_override.unwrap_or(1.5),
                                        target_r_override,
                                        use_utc_day_fib,
                                        tiered_take_profit: false,
                                        allow_utc_after_one,
                                        allow_us_premarket_fib,
                                        allow_beijing_midnight,
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    tunings
}

/// Builds a BTC-only neighborhood scan that targets more entries without adding new mechanics.
pub(super) fn btc_volume_reversal_frequency_scan_tunings() -> Vec<EthVolumeReversal5mTuning> {
    let mut tunings = Vec::new();
    for volume_spike_mult in [3.5, 4.0] {
        for min_downside_excursion_pct in [0.003, 0.004] {
            for max_stop_pct in [Some(0.012)] {
                for min_ema_distance_pct in [Some(1.0), Some(1.5)] {
                    for cooldown_candles in [4, 8, 12] {
                        for target_r_override in [Some(2.5), Some(3.0), Some(4.0)] {
                            for (weak_rebound_body_pct, weak_rebound_range_pct) in [
                                (None, None),
                                (Some(0.18), Some(0.90)),
                                (Some(0.20), Some(1.00)),
                            ] {
                                tunings.push(EthVolumeReversal5mTuning {
                                    volume_spike_mult,
                                    min_downside_excursion_pct,
                                    min_rebound_close_pos: 0.50,
                                    weak_rebound_body_pct,
                                    weak_rebound_range_pct,
                                    max_stop_pct,
                                    min_ema_distance_pct,
                                    min_target_r: target_r_override.unwrap_or(1.5),
                                    target_r_override,
                                    use_utc_day_fib: true,
                                    tiered_take_profit: false,
                                    allow_utc_after_one: true,
                                    allow_us_premarket_fib: true,
                                    allow_beijing_midnight: false,
                                    cooldown_candles,
                                    ..btc_volume_reversal_5m_tuning()
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    tunings
}

#[derive(Debug, Clone, Copy)]
struct VolumeReversalScanCandidate {
    tuning: EthVolumeReversal5mTuning,
    entries: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    max_drawdown_pct: f64,
    trades_per_day: f64,
    profit_factor: f64,
    avg_win_loss_ratio: f64,
}

fn summarize_volume_reversal_reports(
    reports: &[super::CaseReport],
    tuning: EthVolumeReversal5mTuning,
) -> VolumeReversalScanCandidate {
    let wins = reports.iter().map(|report| report.wins).sum::<usize>();
    let losses = reports.iter().map(|report| report.losses).sum::<usize>();
    let pnl = reports.iter().map(|report| report.pnl).sum::<f64>();
    let entries = reports.iter().map(|report| report.entries).sum::<usize>();
    let max_drawdown_pct = reports
        .iter()
        .map(|report| report.max_drawdown_pct)
        .fold(0.0, f64::max);
    let days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    let trades = reports
        .iter()
        .flat_map(|report| report.trades.iter())
        .collect::<Vec<_>>();
    let gross_win = trades
        .iter()
        .filter(|trade| trade.pnl > 0.0)
        .map(|trade| trade.pnl)
        .sum::<f64>();
    let gross_loss = trades
        .iter()
        .filter(|trade| trade.pnl < 0.0)
        .map(|trade| trade.pnl.abs())
        .sum::<f64>();
    let avg_win = if wins > 0 {
        gross_win / wins as f64
    } else {
        0.0
    };
    let avg_loss = if losses > 0 {
        gross_loss / losses as f64
    } else {
        0.0
    };
    VolumeReversalScanCandidate {
        tuning,
        entries,
        wins,
        losses,
        win_rate_pct: super::ratio_pct(wins, wins + losses),
        pnl,
        max_drawdown_pct,
        trades_per_day: if days > 0.0 {
            entries as f64 / days
        } else {
            0.0
        },
        profit_factor: if gross_loss > 0.0 {
            gross_win / gross_loss
        } else if gross_win > 0.0 {
            f64::INFINITY
        } else {
            0.0
        },
        avg_win_loss_ratio: if avg_loss > 0.0 {
            avg_win / avg_loss
        } else {
            0.0
        },
    }
}

fn volume_reversal_candidate_meets_constraints(candidate: &VolumeReversalScanCandidate) -> bool {
    candidate.entries >= 3
        && candidate.win_rate_pct >= 55.0
        && candidate.pnl > 0.0
        && candidate.max_drawdown_pct < 15.0
        && candidate.profit_factor >= 1.5
}

fn btc_volume_reversal_frequency_candidate_meets_goal(
    candidate: &VolumeReversalScanCandidate,
) -> bool {
    candidate.entries > 5
        && candidate.pnl > 71.3992
        && candidate.win_rate_pct >= 50.0
        && candidate.max_drawdown_pct < 15.0
}

fn sort_btc_volume_reversal_frequency_candidates(candidates: &mut [VolumeReversalScanCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .entries
            .cmp(&left.entries)
            .then_with(|| right.pnl.total_cmp(&left.pnl))
            .then_with(|| left.max_drawdown_pct.total_cmp(&right.max_drawdown_pct))
            .then_with(|| right.win_rate_pct.total_cmp(&left.win_rate_pct))
    });
}

fn sort_volume_reversal_candidates(candidates: &mut [VolumeReversalScanCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .win_rate_pct
            .total_cmp(&left.win_rate_pct)
            .then_with(|| right.pnl.total_cmp(&left.pnl))
            .then_with(|| right.profit_factor.total_cmp(&left.profit_factor))
            .then_with(|| left.max_drawdown_pct.total_cmp(&right.max_drawdown_pct))
    });
}

fn print_volume_reversal_candidate(
    prefix: &str,
    rank: usize,
    candidate: &VolumeReversalScanCandidate,
) {
    let tuning = candidate.tuning;
    println!(
        "{prefix} rank={} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} profit_factor={:.2} avg_win_loss={:.2} volume_mult={:.2} min_drop={:.4} rebound_pos={:.2} max_stop={:?} min_ema_dist={:?} min_target_r={:.2} target_r={:?} cooldown={} day_fib={} tiered_tp={} modes=utc:{},premarket:{},bj:{}",
        rank,
        candidate.entries,
        candidate.wins,
        candidate.losses,
        candidate.win_rate_pct,
        candidate.pnl,
        candidate.max_drawdown_pct,
        candidate.trades_per_day,
        candidate.profit_factor,
        candidate.avg_win_loss_ratio,
        tuning.volume_spike_mult,
        tuning.min_downside_excursion_pct,
        tuning.min_rebound_close_pos,
        tuning.max_stop_pct,
        tuning.min_ema_distance_pct,
        tuning.min_target_r,
        tuning.target_r_override,
        tuning.cooldown_candles,
        tuning.use_utc_day_fib,
        tuning.tiered_take_profit,
        tuning.allow_utc_after_one,
        tuning.allow_us_premarket_fib,
        tuning.allow_beijing_midnight
    );
}

fn entry_mode_for_ts(ts: i64) -> Option<EntryMode> {
    let minute = utc_minute_of_day(ts);
    if (US_PREMARKET_START_MINUTE..=US_PREMARKET_END_MINUTE).contains(&minute) {
        return Some(EntryMode::RightUsPremarketFib);
    }
    if (UTC_AFTER_ONE_START_MINUTE..UTC_AFTER_ONE_END_MINUTE).contains(&minute) {
        return Some(EntryMode::LeftUtcAfterOne);
    }
    if minute >= BEIJING_MIDNIGHT_UTC_MINUTE {
        return Some(EntryMode::LeftBeijingAfterMidnight);
    }
    None
}

fn utc_minute_of_day(ts: i64) -> i64 {
    ts.rem_euclid(DAY_MS) / (60 * 1_000)
}

fn previous_volume_average(candles: &[CandleItem], window: usize) -> Option<f64> {
    if candles.len() <= window {
        return None;
    }
    let previous = &candles[candles.len() - window - 1..candles.len() - 1];
    Some(previous.iter().map(|candle| candle.v).sum::<f64>() / window as f64)
}

fn has_sharp_downside_reversal(
    last: &CandleItem,
    previous: &CandleItem,
    tuning: EthVolumeReversal5mTuning,
) -> bool {
    let open_excursion = downside_excursion(last.o, last.l);
    let previous_excursion = downside_excursion(previous.c, last.l);
    let range = (last.h - last.l).abs();
    let close_pos = if range > 0.0 {
        (last.c - last.l) / range
    } else {
        0.0
    };
    open_excursion.max(previous_excursion) >= tuning.min_downside_excursion_pct
        && close_pos >= tuning.min_rebound_close_pos
}

fn is_weak_compact_rebound(
    body_pct: f64,
    candle_range_pct: f64,
    tuning: EthVolumeReversal5mTuning,
) -> bool {
    matches!(
        (tuning.weak_rebound_body_pct, tuning.weak_rebound_range_pct),
        (Some(max_body), Some(max_range))
            if body_pct < max_body && candle_range_pct < max_range
    )
}

pub(super) fn failed_weak_rebound_short_setup(
    candles: &[CandleItem],
    long_tuning: EthVolumeReversal5mTuning,
    short_tuning: BtcFailedWeakReboundShortTuning,
) -> Option<PendingFailedWeakReboundShort> {
    let last = candles.last()?;
    let previous = candles.get(candles.len().saturating_sub(2))?;
    let mode = entry_mode_for_ts(last.ts)?;
    if !long_tuning.allows_mode(mode) {
        return None;
    }
    let volume_avg = previous_volume_average(candles, long_tuning.volume_window)?;
    if volume_avg <= 0.0 || last.v < volume_avg * long_tuning.volume_spike_mult {
        return None;
    }
    if !has_sharp_downside_reversal(last, previous, long_tuning) {
        return None;
    }
    let fib = fib_context_for_tuning(candles, long_tuning);
    let mode_confirmed = match mode {
        EntryMode::RightUsPremarketFib => fib
            .as_ref()
            .is_some_and(|ctx| candle_touches_price(last, ctx.fib_0236)),
        EntryMode::LeftUtcAfterOne | EntryMode::LeftBeijingAfterMidnight => {
            sweeps_recent_low(candles, long_tuning.sweep_lookback)
        }
    };
    if !mode_confirmed {
        return None;
    }

    let entry_price = round_price(last.c);
    let candle_range = (last.h - last.l).max(0.0);
    let candle_range_pct = candle_range / entry_price.max(1e-9) * 100.0;
    let body_pct = (last.c - last.o).abs() / entry_price.max(1e-9) * 100.0;
    if !is_weak_compact_rebound(body_pct, candle_range_pct, long_tuning) {
        return None;
    }
    let downside_excursion_pct =
        downside_excursion(last.o, last.l).max(downside_excursion(previous.c, last.l)) * 100.0;
    let rebound_close_pos = if candle_range > 0.0 {
        (last.c - last.l) / candle_range
    } else {
        0.0
    };
    if rebound_close_pos < short_tuning.min_trigger_rebound_close_pos
        || body_pct > short_tuning.max_trigger_body_pct
    {
        return None;
    }
    Some(PendingFailedWeakReboundShort {
        trigger_low: round_price(last.l),
        trigger_high: round_price(last.h),
        trigger_close: entry_price,
        trigger_ts: last.ts,
        volume: last.v,
        volume_avg,
        volume_multiple: last.v / volume_avg,
        downside_excursion_pct,
        rebound_close_pos,
        candle_range_pct,
        body_pct,
    })
}

pub(super) fn confirmed_failed_weak_rebound_short_signal(
    candles: &[CandleItem],
    pending: PendingFailedWeakReboundShort,
    tuning: BtcFailedWeakReboundShortTuning,
) -> Option<SignalResult> {
    let last = candles.last()?;
    if last.ts <= pending.trigger_ts || last.c >= pending.trigger_low {
        return None;
    }
    let entry_price = round_price(last.c);
    let stop_price = round_price(pending.trigger_high.max(last.h));
    if stop_price <= entry_price {
        return None;
    }
    let risk = stop_price - entry_price;
    let stop_pct = risk / entry_price.max(1e-9);
    if tuning
        .max_stop_pct
        .is_some_and(|max_stop_pct| stop_pct > max_stop_pct)
    {
        return None;
    }
    let target_price = round_price(entry_price - risk * tuning.target_r);
    if target_price >= entry_price {
        return None;
    }

    let reasons = vec![
        "BTC_VOLUME_REVERSAL_HYBRID_5M_FAILED_WEAK_REBOUND_SHORT".to_string(),
        "ENTRY_MODE:short_failed_weak_rebound_confirmed".to_string(),
        format!("VOLUME_MULT:{:.2}", pending.volume_multiple),
        format!("STOP_SOURCE:{BTC_FAILED_REBOUND_SHORT_STOP_SOURCE}"),
        format!("TARGET_R:{:.2}", tuning.target_r),
    ];
    let snapshot = json!({
        "strategy": BTC_HYBRID_STRATEGY_TAG,
        "entry_mode": "short_failed_weak_rebound_confirmed",
        "price": entry_price,
        "stop_price": stop_price,
        "target_price": target_price,
        "target_source": "failed_weak_rebound_fixed_r",
        "target_r": tuning.target_r,
        "volume": pending.volume,
        "volume_avg_20": pending.volume_avg,
        "volume_multiple": pending.volume_multiple,
        "downside_excursion_pct": pending.downside_excursion_pct,
        "rebound_close_pos": pending.rebound_close_pos,
        "candle_range_pct": pending.candle_range_pct,
        "body_pct": pending.body_pct,
        "trigger_low": pending.trigger_low,
        "trigger_close": pending.trigger_close,
        "trigger_ts": pending.trigger_ts,
        "confirmation_ts": last.ts,
        "leverage": 10.0,
        "reasons": reasons,
    });

    Some(SignalResult {
        should_buy: false,
        should_sell: true,
        open_price: entry_price,
        signal_kline_stop_loss_price: Some(stop_price),
        stop_loss_source: Some(BTC_FAILED_REBOUND_SHORT_STOP_SOURCE.to_string()),
        atr_stop_loss_price: Some(stop_price),
        atr_take_profit_level_1: Some(target_price),
        atr_take_profit_level_2: Some(target_price),
        atr_take_profit_level_3: Some(target_price),
        short_signal_take_profit_price: Some(target_price),
        ts: last.ts,
        single_value: Some(snapshot.to_string()),
        single_result: Some(json!({ "reasons": snapshot["reasons"] }).to_string()),
        filter_reasons: vec![format!("{BTC_HYBRID_STRATEGY_TAG}_SHORT_CONFIRMED")],
        direction: SignalDirection::Short,
        ..SignalResult::default()
    })
}

fn inverted_v_short_setup(
    candles: &[CandleItem],
    tuning: EthInvertedVShortTuning,
) -> Option<PendingInvertedVShort> {
    let last = candles.last()?;
    if !tuning.allow_beijing_midnight || utc_minute_of_day(last.ts) < BEIJING_MIDNIGHT_UTC_MINUTE {
        return None;
    }
    let volume_avg = previous_volume_average(candles, tuning.volume_window)?;
    if volume_avg <= 0.0 || last.v < volume_avg * tuning.volume_spike_mult {
        return None;
    }
    let bearish_body_pct = downside_excursion(last.o, last.c);
    if bearish_body_pct < tuning.min_breakdown_body_pct {
        return None;
    }
    let impulse = inverted_v_impulse(candles, tuning.impulse_lookback)?;
    if impulse.impulse_pct < tuning.min_impulse_pct {
        return None;
    }
    let retrace_line =
        impulse.high - (impulse.high - impulse.low).max(0.0) * tuning.min_break_retrace;
    if last.c > retrace_line {
        return None;
    }

    let entry_price = round_price(last.c);
    let stop_price = round_price(last.h);
    if stop_price <= entry_price {
        return None;
    }
    let risk = stop_price - entry_price;
    let stop_pct = risk / entry_price.max(1e-9);
    if tuning
        .max_stop_pct
        .is_some_and(|max_stop_pct| stop_pct > max_stop_pct)
    {
        return None;
    }
    let volume_multiple = last.v / volume_avg;
    Some(PendingInvertedVShort {
        trigger_close: entry_price,
        trigger_stop: stop_price,
        trigger_ts: last.ts,
        trigger_volume: last.v,
        volume_avg,
        volume_multiple,
        impulse_low: impulse.low,
        impulse_high: impulse.high,
        impulse_pct: impulse.impulse_pct,
        retrace_line,
        breakdown_body_pct: bearish_body_pct,
    })
}

fn confirmed_inverted_v_short_signal(
    candles: &[CandleItem],
    pending: PendingInvertedVShort,
    tuning: EthInvertedVShortTuning,
) -> Option<SignalResult> {
    let last = candles.last()?;
    if last.ts <= pending.trigger_ts || last.c > pending.trigger_close {
        return None;
    }
    let entry_price = round_price(last.c);
    let stop_price = round_price(pending.trigger_stop.max(last.h));
    let confirmation_volume_ratio = last.v / pending.trigger_volume.max(1e-9);
    if tuning
        .max_confirmation_volume_ratio
        .is_some_and(|max_ratio| confirmation_volume_ratio > max_ratio)
    {
        return None;
    }
    if stop_price <= entry_price {
        return None;
    }
    let risk = stop_price - entry_price;
    let stop_pct = risk / entry_price.max(1e-9);
    if tuning
        .max_stop_pct
        .is_some_and(|max_stop_pct| stop_pct > max_stop_pct)
    {
        return None;
    }
    let target_price = round_price(entry_price - risk * tuning.target_r);
    if target_price >= entry_price {
        return None;
    }
    let reasons = vec![
        "ETH_VOLUME_REVERSAL_DUAL_5M_INVERTED_V_SHORT".to_string(),
        "ENTRY_MODE:short_beijing_inverted_v_confirmed".to_string(),
        format!("VOLUME_MULT:{:.2}", pending.volume_multiple),
        format!("CONFIRM_VOLUME_RATIO:{confirmation_volume_ratio:.2}"),
        format!("STOP_SOURCE:{SHORT_STOP_SOURCE}"),
        format!("TARGET_R:{:.2}", tuning.target_r),
    ];
    let snapshot = json!({
        "strategy": DUAL_STRATEGY_TAG,
        "entry_mode": "short_beijing_inverted_v_confirmed",
        "price": entry_price,
        "stop_price": stop_price,
        "target_price": target_price,
        "target_source": "confirmed_fixed_r",
        "target_r": tuning.target_r,
        "volume": pending.trigger_volume,
        "confirmation_volume": last.v,
        "confirmation_volume_ratio": confirmation_volume_ratio,
        "volume_avg_20": pending.volume_avg,
        "volume_multiple": pending.volume_multiple,
        "inverted_v_low": pending.impulse_low,
        "inverted_v_high": pending.impulse_high,
        "inverted_v_impulse_pct": pending.impulse_pct * 100.0,
        "break_retrace_line": pending.retrace_line,
        "breakdown_body_pct": pending.breakdown_body_pct * 100.0,
        "trigger_price": pending.trigger_close,
        "trigger_ts": pending.trigger_ts,
        "confirmation_ts": last.ts,
        "leverage": 10.0,
        "reasons": reasons,
    });

    Some(SignalResult {
        should_buy: false,
        should_sell: true,
        open_price: entry_price,
        signal_kline_stop_loss_price: Some(stop_price),
        stop_loss_source: Some(SHORT_STOP_SOURCE.to_string()),
        atr_stop_loss_price: Some(stop_price),
        atr_take_profit_level_1: Some(target_price),
        atr_take_profit_level_2: Some(target_price),
        atr_take_profit_level_3: Some(target_price),
        short_signal_take_profit_price: Some(target_price),
        ts: last.ts,
        single_value: Some(snapshot.to_string()),
        single_result: Some(json!({ "reasons": snapshot["reasons"] }).to_string()),
        filter_reasons: vec![format!("{DUAL_STRATEGY_TAG}_SHORT_CONFIRMED")],
        direction: SignalDirection::Short,
        ..SignalResult::default()
    })
}

#[derive(Debug, Clone, Copy)]
struct InvertedVImpulse {
    low: f64,
    high: f64,
    impulse_pct: f64,
}

fn inverted_v_impulse(candles: &[CandleItem], lookback: usize) -> Option<InvertedVImpulse> {
    if candles.len() <= lookback {
        return None;
    }
    let window = &candles[candles.len() - lookback - 1..candles.len() - 1];
    let low = window
        .iter()
        .map(|candle| candle.l)
        .fold(f64::INFINITY, f64::min);
    let high = window
        .iter()
        .map(|candle| candle.h)
        .fold(f64::NEG_INFINITY, f64::max);
    if !low.is_finite() || !high.is_finite() || high <= low || low <= 0.0 {
        return None;
    }
    Some(InvertedVImpulse {
        low,
        high,
        impulse_pct: (high - low) / low,
    })
}

fn downside_excursion(anchor: f64, low: f64) -> f64 {
    if anchor <= 0.0 {
        return 0.0;
    }
    ((anchor - low) / anchor).max(0.0)
}

fn sweeps_recent_low(candles: &[CandleItem], lookback: usize) -> bool {
    if candles.len() <= lookback {
        return false;
    }
    let last = candles.last().expect("last candle exists");
    let start = candles.len() - lookback - 1;
    let recent_low = candles[start..candles.len() - 1]
        .iter()
        .map(|candle| candle.l)
        .fold(f64::INFINITY, f64::min);
    last.l <= recent_low
}

fn fib_context(candles: &[CandleItem], lookback: usize) -> Option<FibContext> {
    if candles.len() <= lookback {
        return None;
    }
    let window = &candles[candles.len() - lookback - 1..candles.len() - 1];
    let swing_low = window
        .iter()
        .map(|candle| candle.l)
        .fold(f64::INFINITY, f64::min);
    let swing_high = window
        .iter()
        .map(|candle| candle.h)
        .fold(f64::NEG_INFINITY, f64::max);
    if !swing_low.is_finite() || !swing_high.is_finite() || swing_high <= swing_low {
        return None;
    }
    Some(FibContext {
        fib_0236: round_price(swing_low + (swing_high - swing_low) * 0.236),
        swing_low,
        swing_high,
    })
}

fn fib_context_for_tuning(
    candles: &[CandleItem],
    tuning: EthVolumeReversal5mTuning,
) -> Option<FibContext> {
    if tuning.use_utc_day_fib {
        return utc_day_fib_context(candles).or_else(|| fib_context(candles, tuning.fib_lookback));
    }
    fib_context(candles, tuning.fib_lookback)
}

fn utc_day_fib_context(candles: &[CandleItem]) -> Option<FibContext> {
    let current_ts = candles.last()?.ts;
    let day_start = current_ts - current_ts.rem_euclid(DAY_MS);
    let window = candles
        .iter()
        .filter(|candle| candle.ts >= day_start && candle.ts < current_ts)
        .collect::<Vec<_>>();
    if window.len() < 2 {
        return None;
    }
    let swing_low = window
        .iter()
        .map(|candle| candle.l)
        .fold(f64::INFINITY, f64::min);
    let swing_high = window
        .iter()
        .map(|candle| candle.h)
        .fold(f64::NEG_INFINITY, f64::max);
    if !swing_low.is_finite() || !swing_high.is_finite() || swing_high <= swing_low {
        return None;
    }
    Some(FibContext {
        fib_0236: round_price(swing_low + (swing_high - swing_low) * 0.236),
        swing_low,
        swing_high,
    })
}

fn candle_touches_price(candle: &CandleItem, price: f64) -> bool {
    candle.l <= price && price <= candle.h
}

fn target_for_mode(
    mode: EntryMode,
    candles: &[CandleItem],
    entry_price: f64,
    stop_price: f64,
    tuning: EthVolumeReversal5mTuning,
) -> Option<(f64, &'static str, f64)> {
    let ema696 = round_price(ema(candles, tuning.ema_window)?);
    if let Some(target_r) = tuning.target_r_override {
        let target = round_price(entry_price + (entry_price - stop_price) * target_r);
        return (target > entry_price).then_some((target, "fixed_r", ema696));
    }
    match mode {
        EntryMode::LeftUtcAfterOne | EntryMode::LeftBeijingAfterMidnight => {
            (ema696 > entry_price).then_some((ema696, "ema696", ema696))
        }
        EntryMode::RightUsPremarketFib => {
            let impulse = prior_utc_morning_impulse_move(candles)?;
            let target = round_price(entry_price + impulse);
            (target > entry_price).then_some((target, "prior_utc_morning_impulse", ema696))
        }
    }
}

fn tiered_targets(
    entry_price: f64,
    stop_price: f64,
    final_target: f64,
    tuning: EthVolumeReversal5mTuning,
) -> (f64, f64, f64) {
    if !tuning.tiered_take_profit {
        return (final_target, final_target, final_target);
    }
    let risk = entry_price - stop_price;
    let level_1 = round_price(entry_price + risk * tuning.tier_1_r);
    let level_2 = round_price(entry_price + risk * tuning.tier_2_r);
    if level_1 < final_target && level_2 < final_target {
        (level_1, level_2, final_target)
    } else {
        (final_target, final_target, final_target)
    }
}

fn ema(candles: &[CandleItem], window: usize) -> Option<f64> {
    if candles.len() < window {
        return None;
    }
    let alpha = 2.0 / (window as f64 + 1.0);
    let mut iter = candles[candles.len() - window..].iter();
    let mut value = iter.next()?.c;
    for candle in iter {
        value = candle.c * alpha + value * (1.0 - alpha);
    }
    Some(value)
}

fn prior_utc_morning_impulse_move(candles: &[CandleItem]) -> Option<f64> {
    let current_ts = candles.last()?.ts;
    let day_start = current_ts - current_ts.rem_euclid(DAY_MS);
    let window_start = day_start + UTC_AFTER_ONE_START_MINUTE * 60 * 1_000;
    let window_end = (day_start + UTC_AFTER_ONE_END_MINUTE * 60 * 1_000).min(current_ts);
    let morning = candles
        .iter()
        .filter(|candle| candle.ts >= window_start && candle.ts < window_end)
        .collect::<Vec<_>>();
    if morning.len() < 2 {
        return None;
    }
    let mut low = f64::INFINITY;
    let mut high_after_low = f64::NEG_INFINITY;
    for candle in morning {
        if candle.l < low {
            low = candle.l;
            high_after_low = candle.h;
        } else if candle.h > high_after_low {
            high_after_low = candle.h;
        }
    }
    (low.is_finite() && high_after_low.is_finite() && high_after_low > low)
        .then_some(high_after_low - low)
}

fn round_price(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
