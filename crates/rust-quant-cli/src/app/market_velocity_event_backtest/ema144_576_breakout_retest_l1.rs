//! EMA144/576 趋势转换后首次回踩的 15m L1 无标签扫描。
//!
//! 这里只读取当前及此前已完成 K 线。候选账本不包含成交、未来价格或收益字段。

use super::args::market_momentum_exhaustion_reversal_v2_research_args;
use super::{
    config_from_env_and_args, load_backtest_data, BacktestDataSet, ComputedCandle,
    MarketVelocityEventBacktestArgs, MS_15M,
};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 新机会分支使用独立策略身份，不能覆盖现有 15m 动量生产版本。
pub const CANDIDATE_KEY: &str = "market_momentum_ema144_576_transition_first_retest_15m_v1";
/// L1 因果规则的完整冻结身份。
pub const RULE_VERSION: &str =
    "l1_ema144_576_age144_break2_expand075atr_first_retest_up025_down050_wait96_v1";
/// 冻结评价起点：2025-07-01 00:00:00 UTC。
pub const EVALUATION_START_MS: i64 = 1_751_328_000_000;
/// 冻结评价终点：2026-07-19 14:15:00 UTC。
pub const EVALUATION_END_MS: i64 = 1_784_470_500_000;
/// 与现有 15m 研究保持相同的冻结 Top60 抽样身份。
pub const SAMPLE_SEED: &str = "top60_v36_direct_kline_20260721";

const EMA_FAST_PERIOD: usize = 144;
const EMA_SLOW_PERIOD: usize = 576;
const REQUIRED_REGIME_BARS: usize = EMA_FAST_PERIOD;
const REQUIRED_PRE_EVALUATION_BARS: usize = EMA_SLOW_PERIOD + REQUIRED_REGIME_BARS;
const LOAD_EVENT_LEAD_BARS: usize = 192;
const IMPULSE_WINDOW_BARS: usize = 24;
const RETEST_WINDOW_BARS: usize = 96;
const IMPULSE_ATR: f64 = 0.75;
const RETEST_ZONE_ATR: f64 = 0.25;
const MAX_PIERCE_ATR: f64 = 0.50;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

const TARGETS: [TargetDefinition; 3] = [
    TargetDefinition {
        name: "nmr_2026_07_01_user_chart",
        symbol: "NMR-USDT-SWAP",
        start_ms: 1_782_835_200_000,
        end_ms: 1_782_878_400_000,
        direction: Direction::Long,
    },
    TargetDefinition {
        name: "btc_2026_07_02_user_chart",
        symbol: "BTC-USDT-SWAP",
        start_ms: 1_782_943_200_000,
        end_ms: 1_782_964_800_000,
        direction: Direction::Long,
    },
    TargetDefinition {
        name: "btc_2026_07_12_user_chart",
        symbol: "BTC-USDT-SWAP",
        start_ms: 1_783_828_800_000,
        end_ms: 1_783_850_400_000,
        direction: Direction::Long,
    },
];

pub mod first_breakout_pullback_hold_v1;
pub mod momentum_entry_shell_v10;
pub mod persistent_dynamic_retest_v2;
pub mod persistent_qualification_order_l2;
pub mod reexpansion_volume_rank_stable_panel_v12;
pub mod reexpansion_volume_rank_v11;
mod report;
pub mod structure_target_v13;
pub mod target2r_v9;
mod target_trace;
pub use report::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Long,
    Short,
}

impl Direction {
    fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    fn regime_holds(self, bar: ReadyBar) -> bool {
        match self {
            Self::Long => bar.ema144 < bar.ema576,
            Self::Short => bar.ema144 > bar.ema576,
        }
    }

    fn departed(self, bar: ReadyBar) -> bool {
        match self {
            Self::Long => bar.close - bar.ema576 >= IMPULSE_ATR * bar.atr14,
            Self::Short => bar.ema576 - bar.close >= IMPULSE_ATR * bar.atr14,
        }
    }

    fn touches_retest_zone(self, bar: ReadyBar) -> bool {
        match self {
            Self::Long => bar.low <= bar.ema144 + RETEST_ZONE_ATR * bar.atr14,
            Self::Short => bar.high >= bar.ema144 - RETEST_ZONE_ATR * bar.atr14,
        }
    }

    fn holds_retest(self, bar: ReadyBar) -> bool {
        match self {
            Self::Long => {
                bar.low >= bar.ema144 - MAX_PIERCE_ATR * bar.atr14 && bar.close >= bar.ema144
            }
            Self::Short => {
                bar.high <= bar.ema144 + MAX_PIERCE_ATR * bar.atr14 && bar.close <= bar.ema144
            }
        }
    }

    fn cross_phase(self, bar: ReadyBar) -> &'static str {
        match self {
            Self::Long if bar.ema144 <= bar.ema576 => "pre_cross_retest",
            Self::Short if bar.ema144 >= bar.ema576 => "pre_cross_retest",
            _ => "post_cross_retest",
        }
    }

    fn retest_extreme_atr(self, bar: ReadyBar) -> f64 {
        match self {
            Self::Long => (bar.low - bar.ema144) / bar.atr14,
            Self::Short => (bar.ema144 - bar.high) / bar.atr14,
        }
    }

    fn close_hold_atr(self, bar: ReadyBar) -> f64 {
        match self {
            Self::Long => (bar.close - bar.ema144) / bar.atr14,
            Self::Short => (bar.ema144 - bar.close) / bar.atr14,
        }
    }

    fn cross_progress_atr(self, bar: ReadyBar) -> f64 {
        match self {
            Self::Long => (bar.ema144 - bar.ema576) / bar.atr14,
            Self::Short => (bar.ema576 - bar.ema144) / bar.atr14,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TargetDefinition {
    name: &'static str,
    symbol: &'static str,
    start_ms: i64,
    end_ms: i64,
    direction: Direction,
}

#[derive(Debug, Clone, Copy)]
struct PatternBar {
    ts: i64,
    high: f64,
    low: f64,
    close: f64,
    ema144: Option<f64>,
    ema576: Option<f64>,
    atr14: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct ReadyBar {
    ts: i64,
    high: f64,
    low: f64,
    close: f64,
    ema144: f64,
    ema576: f64,
    atr14: f64,
}

impl PatternBar {
    fn ready(self) -> Option<ReadyBar> {
        let values = [self.high, self.low, self.close];
        if values.iter().any(|value| !positive(*value)) || self.high < self.low {
            return None;
        }
        Some(ReadyBar {
            ts: self.ts,
            high: self.high,
            low: self.low,
            close: self.close,
            ema144: self.ema144.filter(|value| positive(*value))?,
            ema576: self.ema576.filter(|value| positive(*value))?,
            atr14: self.atr14.filter(|value| positive(*value))?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Building,
    Armed {
        prior_regime_bars: usize,
    },
    AwaitImpulse {
        breakout_idx: usize,
        breakout_ts: i64,
        prior_regime_bars: usize,
    },
    AwaitRetest {
        breakout_idx: usize,
        breakout_ts: i64,
        impulse_idx: usize,
        impulse_ts: i64,
        prior_regime_bars: usize,
    },
    Consumed {
        reset_seen: bool,
    },
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Building => "building_regime_age",
            Self::Armed { .. } => "armed_waiting_breakout",
            Self::AwaitImpulse { .. } => "waiting_effective_departure",
            Self::AwaitRetest { .. } => "waiting_first_retest",
            Self::Consumed { .. } => "episode_consumed_waiting_reset",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CandidateCore {
    direction: Direction,
    signal_idx: usize,
    breakout_idx: usize,
    breakout_ts: i64,
    impulse_idx: usize,
    impulse_ts: i64,
    prior_regime_bars: usize,
    signal_bar: ReadyBar,
}

#[derive(Debug, Default)]
struct StepResult {
    armed: bool,
    breakout: bool,
    departure: bool,
    failed_first_retest: bool,
    retest_timeout: bool,
    candidate: Option<CandidateCore>,
}

#[derive(Debug)]
struct DirectionMachine {
    direction: Direction,
    relation_age: usize,
    phase: Phase,
}

impl DirectionMachine {
    fn new(direction: Direction) -> Self {
        Self {
            direction,
            relation_age: 0,
            phase: Phase::Building,
        }
    }

    /// 每根已完成 K 只推进一次；失败首次回踩会立即进入 Consumed。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.relation_age = 0;
            self.phase = Phase::Building;
            return StepResult::default();
        };
        let relation_holds = self.direction.regime_holds(bar);
        self.relation_age = if relation_holds {
            self.relation_age.saturating_add(1)
        } else {
            0
        };

        let mut result = StepResult::default();
        match self.phase {
            Phase::Building => {
                if self.relation_age >= REQUIRED_REGIME_BARS {
                    self.phase = Phase::Armed {
                        prior_regime_bars: self.relation_age,
                    };
                    result.armed = true;
                    self.try_breakout(bars, idx, bar, &mut result);
                }
            }
            Phase::Armed { prior_regime_bars } => {
                let prior_regime_bars = prior_regime_bars.max(self.relation_age);
                self.phase = Phase::Armed { prior_regime_bars };
                self.try_breakout(bars, idx, bar, &mut result);
            }
            Phase::AwaitImpulse {
                breakout_idx,
                breakout_ts,
                prior_regime_bars,
            } => {
                let elapsed = idx.saturating_sub(breakout_idx);
                if elapsed >= IMPULSE_WINDOW_BARS {
                    self.phase = Phase::Armed { prior_regime_bars };
                    self.try_breakout(bars, idx, bar, &mut result);
                } else if self.direction.departed(bar) {
                    result.departure = true;
                    self.phase = Phase::AwaitRetest {
                        breakout_idx,
                        breakout_ts,
                        impulse_idx: idx,
                        impulse_ts: bar.ts,
                        prior_regime_bars,
                    };
                }
            }
            Phase::AwaitRetest {
                breakout_idx,
                breakout_ts,
                impulse_idx,
                impulse_ts,
                prior_regime_bars,
            } => {
                let elapsed = idx.saturating_sub(impulse_idx);
                if elapsed > RETEST_WINDOW_BARS {
                    result.retest_timeout = true;
                    self.phase = Phase::Consumed {
                        reset_seen: !relation_holds,
                    };
                } else if elapsed > 0 && self.direction.touches_retest_zone(bar) {
                    if self.direction.holds_retest(bar) {
                        result.candidate = Some(CandidateCore {
                            direction: self.direction,
                            signal_idx: idx,
                            breakout_idx,
                            breakout_ts,
                            impulse_idx,
                            impulse_ts,
                            prior_regime_bars,
                            signal_bar: bar,
                        });
                    } else {
                        result.failed_first_retest = true;
                    }
                    self.phase = Phase::Consumed {
                        reset_seen: !relation_holds,
                    };
                }
            }
            Phase::Consumed { mut reset_seen } => {
                if !relation_holds {
                    reset_seen = true;
                    self.relation_age = 0;
                } else if reset_seen {
                    self.relation_age = 1;
                    self.phase = Phase::Building;
                    return result;
                }
                self.phase = Phase::Consumed { reset_seen };
            }
        }
        result
    }

    fn try_breakout(
        &mut self,
        bars: &[PatternBar],
        idx: usize,
        bar: ReadyBar,
        result: &mut StepResult,
    ) {
        let Phase::Armed { prior_regime_bars } = self.phase else {
            return;
        };
        if !breakout_at(bars, idx, self.direction) {
            return;
        }
        result.breakout = true;
        if self.direction.departed(bar) {
            result.departure = true;
            self.phase = Phase::AwaitRetest {
                breakout_idx: idx,
                breakout_ts: bar.ts,
                impulse_idx: idx,
                impulse_ts: bar.ts,
                prior_regime_bars,
            };
        } else {
            self.phase = Phase::AwaitImpulse {
                breakout_idx: idx,
                breakout_ts: bar.ts,
                prior_regime_bars,
            };
        }
    }
}

/// 连接 quant_core 并生成只含信号时点字段的 L1 报告。
pub async fn run_l1_scan(output: &Path) -> Result<L1Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 first-retest L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_l1_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 构造固定输入；提前 192 根加载事件只为确保 EMA576 与 144 根状态均已预热。
fn frozen_l1_args() -> Result<MarketVelocityEventBacktestArgs> {
    let mut args = market_momentum_exhaustion_reversal_v2_research_args()?;
    args.sample_limit = 60;
    args.sample_seed = SAMPLE_SEED.to_owned();
    args.event_start_ms = Some(
        EVALUATION_START_MS
            .checked_sub(LOAD_EVENT_LEAD_BARS as i64 * MS_15M)
            .context("L1 load start overflow")?,
    );
    args.event_end_ms = Some(EVALUATION_END_MS);
    args.save_backtest_detail = false;
    if !args.entry_filtered_volume_rsi_ema_macd {
        bail!("frozen loader no longer guarantees the 699-bar EMA warmup");
    }
    Ok(args)
}

/// 从本地冻结行情构造覆盖、候选、目标审计和停止边界。
fn build_l1_report(data: &BacktestDataSet) -> Result<L1Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * MS_15M)
        .context("L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut stages = L1StageCounts::default();
    let mut eligible_symbols = BTreeSet::new();
    let mut hasher = Sha256::new();
    let mut target_inputs = target_input_template();
    let mut target_traces = Vec::new();

    let mut pairs = data.pairs.iter().collect::<Vec<_>>();
    pairs.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    for pair in pairs {
        let candles = data
            .candles_15m_computed
            .get(&pair.symbol)
            .with_context(|| format!("missing computed candles for {}", pair.symbol))?;
        let Some((start_idx, end_idx)) =
            complete_window_bounds(candles, warmup_start_ms, EVALUATION_END_MS)
        else {
            excluded_symbols.push(excluded_symbol(
                &pair.symbol,
                candles,
                warmup_start_ms,
                EVALUATION_END_MS,
                expected_window_candles,
            ));
            continue;
        };
        let ema576 = ema_close_series(candles, EMA_SLOW_PERIOD);
        let bars = pattern_bars(candles, &ema576);
        let evaluation_start_idx = start_idx
            .checked_add(REQUIRED_PRE_EVALUATION_BARS)
            .context("evaluation start index overflow")?;
        if candles
            .get(evaluation_start_idx)
            .is_none_or(|candle| candle.candle.ts != EVALUATION_START_MS)
        {
            bail!("eligible window lost evaluation start for {}", pair.symbol);
        }
        if bars[evaluation_start_idx..=end_idx]
            .iter()
            .any(|bar| bar.ready().is_none())
        {
            excluded_symbols.push(ExcludedSymbol {
                symbol: pair.symbol.clone(),
                expected_candles: expected_window_candles,
                loaded_candles: expected_window_candles,
                missing_candles: 0,
                reason: "ema144_ema576_or_atr14_not_ready_in_required_window",
            });
            continue;
        }
        eligible_symbols.insert(pair.symbol.clone());
        hash_symbol_window(&mut hasher, &pair.symbol, &bars[start_idx..=end_idx]);
        update_target_input_coverage(&pair.symbol, &bars, &mut target_inputs);
        target_traces.extend(target_trace::build_target_traces(
            &pair.symbol,
            &bars,
            start_idx,
        ));
        scan_symbol(
            &pair.symbol,
            &bars,
            start_idx,
            end_idx,
            &mut candidates,
            &mut stages,
        )?;
    }
    excluded_symbols.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.signal_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    target_traces.sort_by_key(|trace| trace.name);
    let target_audits = audit_targets(&candidates);
    let summary = summarize(&candidates, stages);
    let decision = decide(&summary, &target_audits, &target_inputs);

    Ok(L1Report {
        schema_version: "market_momentum_ema144_576_first_retest_l1_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: CANDIDATE_KEY,
            rule_version: RULE_VERSION,
            only_variable: "144 completed EMA144-vs-EMA576 regime bars, two-close EMA576 break, 0.75 ATR departure, then the first EMA144 retest that holds",
            ema_policy: "EMA144 and EMA576 use the first full-period close SMA seed, then alpha=2/(period+1) recursive updates on completed 15m candles",
            signal_time_policy: "the retest candidate exists only after its completed candle closes; earliest execution is the next candle and is not evaluated in L1",
            label_boundary: "candidate construction reads no fill, future candle, MFE, MAE, exit, R, win, loss, or PnL fields",
            runtime_boundary: "research-only independent candidate; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        coverage: L1Coverage {
            expected_symbol_count: 60,
            returned_symbol_count: data.pairs.len(),
            eligible_symbol_count: eligible_symbols.len(),
            excluded_symbols,
            evaluation_start_ms: EVALUATION_START_MS,
            evaluation_end_ms: EVALUATION_END_MS,
            required_pre_evaluation_bars: REQUIRED_PRE_EVALUATION_BARS,
            target_inputs,
            dataset_fingerprint_sha256: hex::encode(hasher.finalize()),
            universe_limitation: "current-live Top60 is a survivorship-biased L1 diagnostic; missing local members are skipped and no candles are backfilled",
        },
        summary,
        target_audits,
        target_traces,
        decision,
        candidates,
    })
}

fn scan_symbol(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<L1Candidate>,
    stages: &mut L1StageCounts,
) -> Result<()> {
    let mut long = DirectionMachine::new(Direction::Long);
    let mut short = DirectionMachine::new(Direction::Short);
    for idx in start_idx..=end_idx {
        for machine in [&mut long, &mut short] {
            let step = machine.step(bars, idx);
            let ts = bars[idx].ts;
            if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
                continue;
            }
            stages.armed_episodes += usize::from(step.armed);
            stages.confirmed_breakouts += usize::from(step.breakout);
            stages.effective_departures += usize::from(step.departure);
            stages.failed_first_retests += usize::from(step.failed_first_retest);
            stages.retest_timeouts += usize::from(step.retest_timeout);
            if let Some(core) = step.candidate {
                candidates.push(candidate_from_core(symbol, core)?);
            }
        }
    }
    Ok(())
}

fn candidate_from_core(symbol: &str, core: CandidateCore) -> Result<L1Candidate> {
    let bar = core.signal_bar;
    let signal_month_utc = Utc
        .timestamp_millis_opt(bar.ts)
        .single()
        .context("invalid signal timestamp")?
        .format("%Y-%m")
        .to_string();
    Ok(L1Candidate {
        symbol: symbol.to_owned(),
        direction: core.direction.label(),
        signal_ts_ms: bar.ts,
        signal_month_utc,
        breakout_ts_ms: core.breakout_ts,
        impulse_ts_ms: core.impulse_ts,
        prior_regime_bars: core.prior_regime_bars,
        bars_since_breakout: core.signal_idx.saturating_sub(core.breakout_idx),
        bars_since_impulse: core.signal_idx.saturating_sub(core.impulse_idx),
        cross_phase: core.direction.cross_phase(bar),
        ema144: bar.ema144,
        ema576: bar.ema576,
        atr14: bar.atr14,
        retest_extreme_to_ema144_atr: core.direction.retest_extreme_atr(bar),
        close_to_ema144_directional_atr: core.direction.close_hold_atr(bar),
        ema_cross_progress_atr: core.direction.cross_progress_atr(bar),
    })
}

fn breakout_at(bars: &[PatternBar], idx: usize, direction: Direction) -> bool {
    let Some(two_back_idx) = idx.checked_sub(2) else {
        return false;
    };
    let Some(two_back) = bars[two_back_idx].ready() else {
        return false;
    };
    let Some(previous) = bars[idx - 1].ready() else {
        return false;
    };
    let Some(current) = bars[idx].ready() else {
        return false;
    };
    match direction {
        Direction::Long => {
            two_back.close <= two_back.ema576
                && previous.close > previous.ema576
                && current.close > current.ema576
        }
        Direction::Short => {
            two_back.close >= two_back.ema576
                && previous.close < previous.ema576
                && current.close < current.ema576
        }
    }
}

fn pattern_bars(candles: &[ComputedCandle], ema576: &[Option<f64>]) -> Vec<PatternBar> {
    candles
        .iter()
        .zip(ema576)
        .map(|(candle, ema576)| PatternBar {
            ts: candle.candle.ts,
            high: candle.candle.high,
            low: candle.candle.low,
            close: candle.candle.close,
            ema144: candle.ema144,
            ema576: *ema576,
            atr14: candle.atr14,
        })
        .collect()
}

/// EMA 从第一个完整周期 SMA 起步，保持与 ComputedCandle 的现有语义一致。
fn ema_close_series(candles: &[ComputedCandle], period: usize) -> Vec<Option<f64>> {
    let mut values = vec![None; candles.len()];
    if period == 0 || candles.len() < period {
        return values;
    }
    let mut seed_sum = 0.0;
    for candle in &candles[..period] {
        if !positive(candle.candle.close) {
            return values;
        }
        seed_sum += candle.candle.close;
    }
    let seed_idx = period - 1;
    let mut previous = seed_sum / period as f64;
    values[seed_idx] = Some(previous);
    let alpha = 2.0 / (period as f64 + 1.0);
    for idx in period..candles.len() {
        let close = candles[idx].candle.close;
        if !positive(close) {
            break;
        }
        previous = (close - previous) * alpha + previous;
        values[idx] = Some(previous);
    }
    values
}

fn summarize(candidates: &[L1Candidate], stages: L1StageCounts) -> L1Summary {
    let mut by_direction = BTreeMap::new();
    let mut by_cross_phase = BTreeMap::new();
    let mut by_symbol = BTreeMap::new();
    let mut by_month_utc = BTreeMap::new();
    for candidate in candidates {
        *by_direction.entry(candidate.direction).or_default() += 1;
        *by_cross_phase.entry(candidate.cross_phase).or_default() += 1;
        *by_symbol.entry(candidate.symbol.clone()).or_default() += 1;
        *by_month_utc
            .entry(candidate.signal_month_utc.clone())
            .or_default() += 1;
    }
    L1Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_market_event_count(candidates),
        stages,
    }
}

fn effective_market_event_count(candidates: &[L1Candidate]) -> usize {
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for candidate in candidates {
        let starts_new = last_by_direction
            .get(candidate.direction)
            .is_none_or(|previous| candidate.signal_ts_ms - *previous > EVENT_CLUSTER_WINDOW_MS);
        if starts_new {
            count += 1;
        }
        last_by_direction.insert(candidate.direction, candidate.signal_ts_ms);
    }
    count
}

fn audit_targets(candidates: &[L1Candidate]) -> Vec<TargetAudit> {
    TARGETS
        .iter()
        .map(|target| {
            let matched_signal_timestamps_ms = candidates
                .iter()
                .filter(|candidate| {
                    candidate.symbol == target.symbol
                        && candidate.direction == target.direction.label()
                        && (target.start_ms..=target.end_ms).contains(&candidate.signal_ts_ms)
                })
                .map(|candidate| candidate.signal_ts_ms)
                .collect::<Vec<_>>();
            TargetAudit {
                name: target.name,
                symbol: target.symbol,
                direction: target.direction.label(),
                start_ms: target.start_ms,
                end_ms: target.end_ms,
                matched: !matched_signal_timestamps_ms.is_empty(),
                matched_signal_timestamps_ms,
            }
        })
        .collect()
}

fn decide(
    summary: &L1Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
) -> L1Decision {
    let long_count = summary
        .by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let targets_match = audits.iter().all(|audit| audit.matched);
    let targets_ready = target_inputs.iter().all(|coverage| coverage.ready);
    let mut gates = BTreeMap::new();
    gates.insert("all_three_user_targets_match", targets_match);
    gates.insert("candidates_at_least_10", summary.candidate_count >= 10);
    gates.insert(
        "effective_events_at_least_5",
        summary.effective_market_events >= 5,
    );
    gates.insert("symbols_at_least_4", summary.by_symbol.len() >= 4);
    gates.insert("utc_months_at_least_3", summary.by_month_utc.len() >= 3);
    gates.insert(
        "both_directions_at_least_2",
        long_count >= 2 && short_count >= 2,
    );
    gates.insert("btc_and_nmr_inputs_ready", targets_ready);
    let all_pass = gates.values().all(|passed| *passed);
    let (status, reason) = if !targets_match {
        (
            "rejected_definition_mismatch",
            "至少一张用户目标图未被冻结定义命中；按预注册停止，不读取任何收益或放宽阈值。",
        )
    } else if !all_pass {
        (
            "stop_coverage_gate_failed",
            "目标定义已匹配，但至少一项无标签覆盖或分散性门槛未通过；不进入 L2。",
        )
    } else {
        (
            "coverage_pass_ready_for_l2_prereg",
            "目标定义、覆盖和分散性全部通过；下一步只能先冻结 L2 成本回放清单。",
        )
    };
    L1Decision {
        status,
        gates,
        reason: reason.to_owned(),
        outcome_evaluation_performed: false,
        target_chart_audit_completed: true,
    }
}

fn target_input_template() -> Vec<TargetInputCoverage> {
    TARGETS
        .iter()
        .map(|target| TargetInputCoverage {
            symbol: target.symbol,
            ready: false,
            expected_candles: inclusive_candle_count(target.start_ms, target.end_ms)
                .expect("target windows are statically aligned"),
            ready_candles: 0,
        })
        .collect()
}

fn update_target_input_coverage(
    symbol: &str,
    bars: &[PatternBar],
    target_inputs: &mut [TargetInputCoverage],
) {
    for (target, coverage) in TARGETS.iter().zip(target_inputs.iter_mut()) {
        if target.symbol != symbol {
            continue;
        }
        let ready_candles = bars
            .iter()
            .filter(|bar| (target.start_ms..=target.end_ms).contains(&bar.ts))
            .filter(|bar| bar.ready().is_some())
            .count();
        coverage.ready_candles = ready_candles;
        coverage.ready = ready_candles == coverage.expected_candles;
    }
}

fn complete_window_bounds(
    candles: &[ComputedCandle],
    start_ms: i64,
    end_ms: i64,
) -> Option<(usize, usize)> {
    let start_idx = candles
        .binary_search_by_key(&start_ms, |candle| candle.candle.ts)
        .ok()?;
    let end_idx = candles
        .binary_search_by_key(&end_ms, |candle| candle.candle.ts)
        .ok()?;
    let window = candles.get(start_idx..=end_idx)?;
    let expected = inclusive_candle_count(start_ms, end_ms).ok()?;
    if window.len() != expected
        || window
            .iter()
            .enumerate()
            .any(|(offset, candle)| candle.candle.ts != start_ms + offset as i64 * MS_15M)
    {
        return None;
    }
    Some((start_idx, end_idx))
}

fn excluded_symbol(
    symbol: &str,
    candles: &[ComputedCandle],
    start_ms: i64,
    end_ms: i64,
    expected_candles: usize,
) -> ExcludedSymbol {
    let loaded_candles = candles
        .iter()
        .filter(|candle| (start_ms..=end_ms).contains(&candle.candle.ts))
        .count();
    ExcludedSymbol {
        symbol: symbol.to_owned(),
        expected_candles,
        loaded_candles,
        missing_candles: expected_candles.saturating_sub(loaded_candles),
        reason: "incomplete_or_non_contiguous_15m_warmup_and_evaluation_window",
    }
}

fn inclusive_candle_count(start_ms: i64, end_ms: i64) -> Result<usize> {
    let span = end_ms
        .checked_sub(start_ms)
        .context("candle window end precedes start")?;
    if span < 0 || span % MS_15M != 0 {
        bail!("candle window is not aligned to 15m boundaries");
    }
    usize::try_from(span / MS_15M + 1).context("candle count overflows usize")
}

fn hash_symbol_window(hasher: &mut Sha256, symbol: &str, bars: &[PatternBar]) {
    hash_bytes(hasher, symbol.as_bytes());
    for bar in bars {
        hasher.update(bar.ts.to_le_bytes());
        hasher.update(bar.high.to_bits().to_le_bytes());
        hasher.update(bar.low.to_bits().to_le_bytes());
        hasher.update(bar.close.to_bits().to_le_bytes());
        hash_optional_f64(hasher, bar.ema144);
        hash_optional_f64(hasher, bar.ema576);
        hash_optional_f64(hasher, bar.atr14);
    }
}

fn hash_bytes(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn hash_optional_f64(hasher: &mut Sha256, value: Option<f64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_bits().to_le_bytes());
        }
        None => hasher.update([0]),
    }
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests;
