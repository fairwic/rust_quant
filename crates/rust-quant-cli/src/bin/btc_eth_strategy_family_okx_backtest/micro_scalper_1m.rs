use rust_quant_domain::SignalDirection;
use rust_quant_strategies::framework::backtest::types::{
    BackTestResult, BasicRiskStrategyConfig, SignalResult,
};
use rust_quant_strategies::framework::backtest::{
    run_indicator_strategy_backtest, IndicatorStrategyBacktest,
};
use rust_quant_strategies::CandleItem;
use serde_json::json;

/// Research-only 1m scalper runner used by the OKX sharded-data backtest CLI.
///
/// This deliberately lives outside the production strategy registry: 1m behavior needs higher
/// signal frequency and separate acceptance evidence before it can be promoted to live execution.
pub fn run_micro_scalper_1m(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
) -> BackTestResult {
    run_indicator_strategy_backtest(inst_id, MicroScalper1m::default(), candles, risk)
}

/// Runs the same 1m backtest with an explicit research tuning from the scan grid.
fn run_micro_scalper_1m_with_tuning(
    inst_id: &str,
    candles: &[CandleItem],
    risk: BasicRiskStrategyConfig,
    tuning: MicroScalper1mTuning,
) -> BackTestResult {
    run_indicator_strategy_backtest(
        inst_id,
        MicroScalper1m {
            tuning,
            cooldown_remaining: 0,
        },
        candles,
        risk,
    )
}

/// Tunable thresholds for the 1m micro scalper research preset.
#[derive(Debug, Clone, Copy)]
pub(super) struct MicroScalper1mTuning {
    /// Fast moving-average window, in 1m candles.
    pub(super) fast_window: usize,
    /// Slow moving-average window, in 1m candles.
    pub(super) slow_window: usize,
    /// Volume baseline window, in 1m candles.
    pub(super) volume_window: usize,
    /// ATR-like range baseline window, in 1m candles.
    pub(super) atr_window: usize,
    /// Number of recent candles allowed to form the pullback before continuation.
    pub(super) pullback_window: usize,
    /// Number of recent candles used to anchor the structural stop.
    pub(super) swing_window: usize,
    /// Bars to skip after a confirmed entry signal to avoid repeated same-leg entries.
    pub(super) cooldown_candles: usize,
    /// Minimum candle body/range ratio required for the resume candle.
    pub(super) min_body_ratio: f64,
    /// Minimum volume multiple versus the recent baseline.
    pub(super) min_volume_mult: f64,
    /// Maximum distance from the fast average, measured in ATR-like ranges.
    pub(super) max_extension_atr: f64,
    /// Maximum distance from the fast average for the recent pullback touch.
    pub(super) touch_atr: f64,
    /// Maximum entry-to-stop risk as a fraction of entry price.
    pub(super) max_risk_pct: f64,
    /// Stop buffer beyond the recent swing, measured in ATR-like ranges.
    pub(super) stop_buffer_atr: f64,
    /// First take-profit distance in R.
    pub(super) target_r_1: f64,
    /// Final take-profit distance in R.
    pub(super) target_r_2: f64,
    /// Whether the research preset may take short continuation entries.
    pub(super) allow_short: bool,
}

impl Default for MicroScalper1mTuning {
    fn default() -> Self {
        Self {
            fast_window: 8,
            slow_window: 34,
            volume_window: 30,
            atr_window: 20,
            pullback_window: 6,
            swing_window: 8,
            cooldown_candles: 3,
            min_body_ratio: 0.35,
            min_volume_mult: 1.0,
            max_extension_atr: 2.8,
            touch_atr: 0.4,
            max_risk_pct: 0.009,
            stop_buffer_atr: 0.2,
            target_r_1: 0.8,
            target_r_2: 1.35,
            allow_short: true,
        }
    }
}

/// Prints fee-aware scan results for the research-only 1m micro scalper.
pub(super) fn print_micro_scalper_scan(
    loaded_cases: &[super::LoadedCase],
    risk_percent: f64,
    trade_fee_rate: Option<f64>,
) {
    let risk = super::strategy_family_risk_config(risk_percent, trade_fee_rate);
    let micro_cases = loaded_cases
        .iter()
        .filter(|loaded| matches!(loaded.case.family, super::StrategyFamily::MicroScalper1m))
        .collect::<Vec<_>>();
    if micro_cases.is_empty() {
        println!("no_micro_scalper_cases source=quant_core_sharded");
        return;
    }

    let mut candidates = Vec::new();
    let mut raw_candidates = Vec::new();
    for tuning in micro_scalper_scan_tunings() {
        let mut reports = Vec::with_capacity(micro_cases.len());
        for loaded in &micro_cases {
            let result =
                run_micro_scalper_1m_with_tuning(loaded.case.symbol, &loaded.candles, risk, tuning);
            reports.push(super::build_report(
                loaded.case.label,
                &loaded.candles,
                &result,
            ));
        }
        let summary = summarize_micro_reports(&reports);
        raw_candidates.push(MicroScalperScanCandidateReport { tuning, ..summary });
        if summary.win_rate_pct >= 60.0
            && summary.max_drawdown_pct < 15.0
            && summary.pnl > 0.0
            && summary.remove_top5_pnl > 0.0
            && summary.trades_per_day >= 8.0
        {
            candidates.push(MicroScalperScanCandidateReport { tuning, ..summary });
        }
    }

    raw_candidates.sort_by(|left, right| {
        right
            .pnl
            .total_cmp(&left.pnl)
            .then_with(|| right.win_rate_pct.total_cmp(&left.win_rate_pct))
            .then_with(|| right.trades_per_day.total_cmp(&left.trades_per_day))
    });
    for candidate in raw_candidates.iter().take(8) {
        print_micro_candidate("micro_raw_top", candidate);
    }

    candidates.sort_by(|left, right| {
        right
            .pnl
            .total_cmp(&left.pnl)
            .then_with(|| right.trades_per_day.total_cmp(&left.trades_per_day))
    });
    if candidates.is_empty() {
        println!(
            "no_micro_scalper_candidates source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,remove_top5_pnl>0,min_trades_per_day>=8"
        );
        return;
    }
    for candidate in candidates.iter().take(20) {
        print_micro_candidate("micro_candidate", candidate);
    }
}

/// Builds the bounded 1m scan grid; it has a high-frequency floor, not a 2-5/day cap.
pub(super) fn micro_scalper_scan_tunings() -> Vec<MicroScalper1mTuning> {
    let mut tunings = Vec::new();
    for allow_short in [false, true] {
        for fast_window in [8_usize, 13] {
            for slow_window in [34_usize] {
                for cooldown_candles in [3_usize, 8] {
                    for min_body_ratio in [0.45, 0.60] {
                        for min_volume_mult in [1.0, 1.4] {
                            for max_extension_atr in [1.2] {
                                for touch_atr in [0.15, 0.35] {
                                    for swing_window in [8_usize] {
                                        for (target_r_1, target_r_2) in [(1.2, 2.8), (1.5, 3.5)] {
                                            tunings.push(MicroScalper1mTuning {
                                                fast_window,
                                                slow_window,
                                                swing_window,
                                                cooldown_candles,
                                                min_body_ratio,
                                                min_volume_mult,
                                                max_extension_atr,
                                                touch_atr,
                                                target_r_1,
                                                target_r_2,
                                                allow_short,
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
        }
    }
    tunings
}

/// Compact scan output record for comparing micro-scalper tuning variants.
#[derive(Debug, Clone, Copy)]
struct MicroScalperScanCandidateReport {
    tuning: MicroScalper1mTuning,
    entries: usize,
    wins: usize,
    losses: usize,
    win_rate_pct: f64,
    pnl: f64,
    max_drawdown_pct: f64,
    trades_per_day: f64,
    early_win_rate_pct: f64,
    early_pnl: f64,
    late_win_rate_pct: f64,
    late_pnl: f64,
    remove_top5_pnl: f64,
}

/// Summarizes per-symbol case reports into one combined scan row.
fn summarize_micro_reports(reports: &[super::CaseReport]) -> MicroScalperScanCandidateReport {
    let wins = reports.iter().map(|report| report.wins).sum::<usize>();
    let losses = reports.iter().map(|report| report.losses).sum::<usize>();
    let pnl = reports.iter().map(|report| report.pnl).sum::<f64>();
    let entries = reports.iter().map(|report| report.entries).sum::<usize>();
    let max_drawdown_pct = reports
        .iter()
        .map(|report| report.max_drawdown_pct)
        .fold(0.0, f64::max);
    let combo_days = reports.iter().map(|report| report.days).fold(0.0, f64::max);
    let mut trades = reports
        .iter()
        .flat_map(|report| report.trades.iter().cloned())
        .collect::<Vec<_>>();
    trades.sort_unstable_by(|left, right| left.open_time.cmp(&right.open_time));
    let mid = trades.len() / 2;
    let (early_win_rate_pct, early_pnl) = summarize_micro_trades(&trades[..mid]);
    let (late_win_rate_pct, late_pnl) = summarize_micro_trades(&trades[mid..]);
    let mut without_top5 = trades.clone();
    without_top5.sort_unstable_by(|left, right| right.pnl.total_cmp(&left.pnl));
    let remove_top5_pnl = without_top5
        .iter()
        .skip(5)
        .map(|trade| trade.pnl)
        .sum::<f64>();
    MicroScalperScanCandidateReport {
        tuning: MicroScalper1mTuning::default(),
        entries,
        wins,
        losses,
        win_rate_pct: super::ratio_pct(wins, wins + losses),
        pnl,
        max_drawdown_pct,
        trades_per_day: if combo_days > 0.0 {
            entries as f64 / combo_days
        } else {
            0.0
        },
        early_win_rate_pct,
        early_pnl,
        late_win_rate_pct,
        late_pnl,
        remove_top5_pnl,
    }
}

/// Summarizes one time split so scan rows expose early/late robustness.
fn summarize_micro_trades(trades: &[super::ClosedTradeDebug]) -> (f64, f64) {
    let wins = trades.iter().filter(|trade| trade.pnl > 0.0).count();
    let losses = trades.iter().filter(|trade| trade.pnl < 0.0).count();
    let pnl = trades.iter().map(|trade| trade.pnl).sum::<f64>();
    (super::ratio_pct(wins, wins + losses), pnl)
}

/// Emits a single scan row in the same plain-text style as the existing backtest scans.
fn print_micro_candidate(prefix: &str, candidate: &MicroScalperScanCandidateReport) {
    println!(
        "{prefix} allow_short={} fast={} slow={} cooldown={} body={:.2} volume={:.2} max_ext={:.2} touch={:.2} swing={} r1={:.2} r2={:.2} entries={} wins={} losses={} win_rate={:.2}% pnl={:.4} max_dd={:.2}% trades_per_day={:.2} early_wr={:.2}% early_pnl={:.4} late_wr={:.2}% late_pnl={:.4} remove_top5_pnl={:.4}",
        candidate.tuning.allow_short,
        candidate.tuning.fast_window,
        candidate.tuning.slow_window,
        candidate.tuning.cooldown_candles,
        candidate.tuning.min_body_ratio,
        candidate.tuning.min_volume_mult,
        candidate.tuning.max_extension_atr,
        candidate.tuning.touch_atr,
        candidate.tuning.swing_window,
        candidate.tuning.target_r_1,
        candidate.tuning.target_r_2,
        candidate.entries,
        candidate.wins,
        candidate.losses,
        candidate.win_rate_pct,
        candidate.pnl,
        candidate.max_drawdown_pct,
        candidate.trades_per_day,
        candidate.early_win_rate_pct,
        candidate.early_pnl,
        candidate.late_win_rate_pct,
        candidate.late_pnl,
        candidate.remove_top5_pnl
    );
}

/// Stateful adapter for the common indicator backtest pipeline.
#[derive(Debug, Clone)]
struct MicroScalper1m {
    tuning: MicroScalper1mTuning,
    cooldown_remaining: usize,
}

impl Default for MicroScalper1m {
    fn default() -> Self {
        Self {
            tuning: MicroScalper1mTuning::default(),
            cooldown_remaining: 0,
        }
    }
}

impl IndicatorStrategyBacktest for MicroScalper1m {
    type IndicatorCombine = ();
    type IndicatorValues = ();

    fn min_data_length(&self) -> usize {
        self.tuning
            .slow_window
            .max(self.tuning.volume_window)
            .max(self.tuning.atr_window)
            + self.tuning.pullback_window
            + 2
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
        _values: &mut Self::IndicatorValues,
        risk_config: &BasicRiskStrategyConfig,
    ) -> SignalResult {
        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            return SignalResult::default();
        }
        let Some(setup) = micro_setup(candles, self.tuning, normalized_max_risk(risk_config))
        else {
            return SignalResult::default();
        };
        self.cooldown_remaining = self.tuning.cooldown_candles;
        setup.to_signal()
    }
}

/// Directional setup emitted by the micro scalper before conversion to a backtest signal.
#[derive(Debug, Clone, Copy)]
struct MicroSetup {
    direction: SignalDirection,
    entry: f64,
    stop: f64,
    target_1: f64,
    target_2: f64,
    ts: i64,
    fast: f64,
    slow: f64,
    atr: f64,
}

impl MicroSetup {
    /// Converts a confirmed setup into the shared backtest signal contract.
    fn to_signal(self) -> SignalResult {
        let should_buy = self.direction == SignalDirection::Long;
        let should_sell = self.direction == SignalDirection::Short;
        SignalResult {
            should_buy,
            should_sell,
            open_price: self.entry,
            signal_kline_stop_loss_price: Some(self.stop),
            stop_loss_source: Some("MICRO_SCALPER_1M_SWING".to_string()),
            atr_stop_loss_price: Some(self.stop),
            atr_take_profit_level_1: Some(self.target_1),
            atr_take_profit_level_2: Some(self.target_2),
            atr_take_profit_level_3: Some(self.target_2),
            ts: self.ts,
            direction: self.direction,
            filter_reasons: vec!["MICRO_SCALPER_1M_CONFIRMED".to_string()],
            single_value: Some(
                json!({
                    "fast": self.fast,
                    "slow": self.slow,
                    "atr": self.atr,
                    "entry": self.entry,
                    "stop": self.stop,
                    "target_1": self.target_1,
                    "target_2": self.target_2,
                })
                .to_string(),
            ),
            ..Default::default()
        }
    }
}

/// Builds a setup from 1m continuation structure: trend, pullback, resume, risk.
fn micro_setup(
    candles: &[CandleItem],
    tuning: MicroScalper1mTuning,
    max_risk_pct: f64,
) -> Option<MicroSetup> {
    let last = candles.last()?;
    let previous = candles.get(candles.len().checked_sub(2)?)?;
    let fast = sma_close(tail(candles, tuning.fast_window)?);
    let slow = sma_close(tail(candles, tuning.slow_window)?);
    let atr = average_range(tail(candles, tuning.atr_window)?).max(last.c.abs() * 0.0002);
    let avg_volume = average_volume(tail(candles, tuning.volume_window)?).max(0.0001);

    if long_resume_ok(candles, last, previous, fast, slow, atr, avg_volume, tuning) {
        return build_setup(
            candles,
            SignalDirection::Long,
            last.c,
            atr,
            fast,
            slow,
            tuning,
        )
        .filter(|setup| setup_risk_ok(setup, max_risk_pct.min(tuning.max_risk_pct)));
    }
    if tuning.allow_short
        && short_resume_ok(candles, last, previous, fast, slow, atr, avg_volume, tuning)
    {
        return build_setup(
            candles,
            SignalDirection::Short,
            last.c,
            atr,
            fast,
            slow,
            tuning,
        )
        .filter(|setup| setup_risk_ok(setup, max_risk_pct.min(tuning.max_risk_pct)));
    }
    None
}

/// Checks whether the latest candle resumes a long 1m trend after a shallow pullback.
fn long_resume_ok(
    candles: &[CandleItem],
    last: &CandleItem,
    previous: &CandleItem,
    fast: f64,
    slow: f64,
    atr: f64,
    avg_volume: f64,
    tuning: MicroScalper1mTuning,
) -> bool {
    fast > slow
        && last.c > fast
        && last.c > previous.h
        && last.c >= last.o
        && body_ratio(last) >= tuning.min_body_ratio
        && last.v >= avg_volume * tuning.min_volume_mult
        && (last.c - fast).abs() <= atr * tuning.max_extension_atr
        && recent_pullback_touched_fast(candles, fast, atr, tuning, SignalDirection::Long)
}

/// Checks whether the latest candle resumes a short 1m trend after a shallow pullback.
fn short_resume_ok(
    candles: &[CandleItem],
    last: &CandleItem,
    previous: &CandleItem,
    fast: f64,
    slow: f64,
    atr: f64,
    avg_volume: f64,
    tuning: MicroScalper1mTuning,
) -> bool {
    fast < slow
        && last.c < fast
        && last.c < previous.l
        && last.c <= last.o
        && body_ratio(last) >= tuning.min_body_ratio
        && last.v >= avg_volume * tuning.min_volume_mult
        && (last.c - fast).abs() <= atr * tuning.max_extension_atr
        && recent_pullback_touched_fast(candles, fast, atr, tuning, SignalDirection::Short)
}

/// Converts a directional entry into stop and target prices for the shared risk engine.
fn build_setup(
    candles: &[CandleItem],
    direction: SignalDirection,
    entry: f64,
    atr: f64,
    fast: f64,
    slow: f64,
    tuning: MicroScalper1mTuning,
) -> Option<MicroSetup> {
    let recent = tail(candles, tuning.swing_window)?;
    let risk = match direction {
        SignalDirection::Long => {
            let stop = recent
                .iter()
                .map(|candle| candle.l)
                .fold(f64::INFINITY, f64::min)
                - atr * tuning.stop_buffer_atr;
            entry - stop
        }
        SignalDirection::Short => {
            let stop = recent
                .iter()
                .map(|candle| candle.h)
                .fold(f64::NEG_INFINITY, f64::max)
                + atr * tuning.stop_buffer_atr;
            stop - entry
        }
        _ => return None,
    };
    if risk <= 0.0 {
        return None;
    }
    let stop = if direction == SignalDirection::Long {
        entry - risk
    } else {
        entry + risk
    };
    let target_1 = if direction == SignalDirection::Long {
        entry + risk * tuning.target_r_1
    } else {
        entry - risk * tuning.target_r_1
    };
    let target_2 = if direction == SignalDirection::Long {
        entry + risk * tuning.target_r_2
    } else {
        entry - risk * tuning.target_r_2
    };
    Some(MicroSetup {
        direction,
        entry,
        stop,
        target_1,
        target_2,
        ts: candles.last()?.ts,
        fast,
        slow,
        atr,
    })
}

/// Rejects entries whose structural stop is too wide for a 1m scalping preset.
fn setup_risk_ok(setup: &MicroSetup, max_risk_pct: f64) -> bool {
    if setup.entry <= 0.0 {
        return false;
    }
    (setup.entry - setup.stop).abs() / setup.entry <= max_risk_pct
}

/// Normalizes legacy percent-style configs and fraction-style defaults to one fraction value.
fn normalized_max_risk(risk_config: &BasicRiskStrategyConfig) -> f64 {
    if risk_config.max_loss_percent > 1.0 {
        risk_config.max_loss_percent / 100.0
    } else {
        risk_config.max_loss_percent
    }
}

/// Checks whether the recent pullback touched the fast average without requiring a full reversal.
fn recent_pullback_touched_fast(
    candles: &[CandleItem],
    fast: f64,
    atr: f64,
    tuning: MicroScalper1mTuning,
    direction: SignalDirection,
) -> bool {
    let Some(window) = tail(candles, tuning.pullback_window) else {
        return false;
    };
    match direction {
        SignalDirection::Long => window
            .iter()
            .take(window.len().saturating_sub(1))
            .any(|candle| candle.l <= fast + atr * tuning.touch_atr),
        SignalDirection::Short => window
            .iter()
            .take(window.len().saturating_sub(1))
            .any(|candle| candle.h >= fast - atr * tuning.touch_atr),
        _ => false,
    }
}

/// Returns a trailing window without allocating.
fn tail<T>(items: &[T], len: usize) -> Option<&[T]> {
    if len == 0 || items.len() < len {
        return None;
    }
    Some(&items[items.len() - len..])
}

/// Computes a simple close-price moving average for short backtest windows.
fn sma_close(candles: &[CandleItem]) -> f64 {
    candles.iter().map(|candle| candle.c).sum::<f64>() / candles.len() as f64
}

/// Computes an ATR-like average range; the 1m preset uses this only as a local scale.
fn average_range(candles: &[CandleItem]) -> f64 {
    candles
        .iter()
        .map(|candle| (candle.h - candle.l).abs())
        .sum::<f64>()
        / candles.len() as f64
}

/// Computes the recent volume baseline used to avoid completely dry 1m candles.
fn average_volume(candles: &[CandleItem]) -> f64 {
    candles.iter().map(|candle| candle.v).sum::<f64>() / candles.len() as f64
}

/// Measures how directional the latest resume candle is relative to its full range.
fn body_ratio(candle: &CandleItem) -> f64 {
    let range = (candle.h - candle.l).abs().max(0.0001);
    (candle.c - candle.o).abs() / range
}
