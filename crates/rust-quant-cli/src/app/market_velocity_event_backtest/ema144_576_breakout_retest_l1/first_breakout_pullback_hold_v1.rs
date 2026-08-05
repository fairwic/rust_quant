//! 长期慢线压制后首次突破 EMA576，再首次回踩 EMA144 守稳的 15m L1 扫描。
//!
//! 本模块是独立 Research 身份。它不继承旧 V6/V12 的永久资格、重复重扩张或运行态注册。

pub mod exclusive_active_retests_v2;
#[cfg(test)]
mod tests;

use super::super::{config_from_env_and_args, load_backtest_data, BacktestDataSet};
use super::{
    complete_window_bounds, ema_close_series, excluded_symbol, frozen_l1_args, hash_symbol_window,
    inclusive_candle_count, pattern_bars, Direction, ExcludedSymbol, PatternBar, ReadyBar,
    EMA_SLOW_PERIOD, EVALUATION_END_MS, EVALUATION_START_MS, REQUIRED_PRE_EVALUATION_BARS,
};
use anyhow::{Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

/// 用户确认后的独立策略候选身份，不覆盖此前任何 EMA144/576 研究版本。
pub const CANDIDATE_KEY: &str = "market_momentum_ema576_first_breakout_ema144_pullback_hold_15m_v1";
/// L1 冻结规则身份；阈值变化必须创建新版本，不能原地改写。
pub const RULE_VERSION: &str = "l1_regime144_price80_break2_retest030_wait576_first_only_v1";

const REGIME_WINDOW_BARS: usize = 144;
const MIN_PRICE_SIDE_PERCENT: usize = 80;
const RETEST_ZONE_ATR: f64 = 0.30;
const RETEST_WAIT_BARS: usize = 576;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

const TARGETS: [TargetDefinition; 5] = [
    TargetDefinition {
        name: "nmr_2026_07_01_user_chart",
        symbol: "NMR-USDT-SWAP",
        direction: Direction::Long,
        start_ms: 1_782_835_200_000,
        end_ms: 1_782_878_400_000,
        expectation: TargetExpectation::MustMatch,
    },
    TargetDefinition {
        name: "btc_2026_07_02_user_chart",
        symbol: "BTC-USDT-SWAP",
        direction: Direction::Long,
        start_ms: 1_782_943_200_000,
        end_ms: 1_782_964_800_000,
        expectation: TargetExpectation::MustMatch,
    },
    TargetDefinition {
        name: "btc_2026_07_12_user_chart",
        symbol: "BTC-USDT-SWAP",
        direction: Direction::Long,
        start_ms: 1_783_828_800_000,
        end_ms: 1_783_850_400_000,
        expectation: TargetExpectation::MustMatch,
    },
    TargetDefinition {
        name: "algo_2026_07_19_wrong_short",
        symbol: "ALGO-USDT-SWAP",
        direction: Direction::Short,
        start_ms: 1_784_453_400_000,
        end_ms: 1_784_453_400_000,
        expectation: TargetExpectation::MustNotMatch,
    },
    TargetDefinition {
        name: "merl_2026_07_19_wrong_short",
        symbol: "MERL-USDT-SWAP",
        direction: Direction::Short,
        start_ms: 1_784_457_900_000,
        end_ms: 1_784_457_900_000,
        expectation: TargetExpectation::MustNotMatch,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetExpectation {
    MustMatch,
    MustNotMatch,
}

impl TargetExpectation {
    fn label(self) -> &'static str {
        match self {
            Self::MustMatch => "must_match",
            Self::MustNotMatch => "must_not_match",
        }
    }

    fn passes(self, matched: bool) -> bool {
        match self {
            Self::MustMatch => matched,
            Self::MustNotMatch => !matched,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TargetDefinition {
    name: &'static str,
    symbol: &'static str,
    direction: Direction,
    start_ms: i64,
    end_ms: i64,
    expectation: TargetExpectation,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Building,
    Armed {
        qualified_ts: i64,
    },
    AwaitRetest {
        qualified_ts: i64,
        breakout_idx: usize,
        breakout_ts: i64,
        relation_age_bars: usize,
        price_side_bars: usize,
    },
}

#[derive(Debug, Clone, Copy)]
struct CandidateCore {
    direction: Direction,
    qualified_ts: i64,
    breakout_idx: usize,
    breakout_ts: i64,
    relation_age_bars: usize,
    price_side_bars: usize,
    signal_idx: usize,
    signal_bar: ReadyBar,
}

#[derive(Debug, Default)]
struct StepResult {
    regime_qualified: bool,
    confirmed_breakout: bool,
    failed_first_retest: bool,
    retest_timeout: bool,
    candidate: Option<CandidateCore>,
}

#[derive(Debug)]
struct DirectionMachine {
    direction: Direction,
    phase: Phase,
    relation_age_bars: usize,
    price_side_window: VecDeque<bool>,
    price_side_bars: usize,
}

impl DirectionMachine {
    fn new(direction: Direction) -> Self {
        Self {
            direction,
            phase: Phase::Building,
            relation_age_bars: 0,
            price_side_window: VecDeque::with_capacity(REGIME_WINDOW_BARS),
            price_side_bars: 0,
        }
    }

    /// 每根完成 K 只能推进一个事件阶段，避免同一根同时消费旧机会并重建新资格。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_for_new_regime();
            return StepResult::default();
        };
        match self.phase {
            Phase::AwaitRetest {
                qualified_ts,
                breakout_idx,
                breakout_ts,
                relation_age_bars,
                price_side_bars,
            } => self.advance_retest(
                idx,
                bar,
                qualified_ts,
                breakout_idx,
                breakout_ts,
                relation_age_bars,
                price_side_bars,
            ),
            Phase::Building | Phase::Armed { .. } => self.advance_regime(bars, idx, bar),
        }
    }

    fn advance_regime(&mut self, bars: &[PatternBar], idx: usize, bar: ReadyBar) -> StepResult {
        let mut result = StepResult::default();
        if !self.direction.regime_holds(bar) {
            self.reset_for_new_regime();
            return result;
        }

        self.relation_age_bars = self.relation_age_bars.saturating_add(1);
        let price_on_side = price_on_regime_side(self.direction, bar);
        self.price_side_window.push_back(price_on_side);
        self.price_side_bars += usize::from(price_on_side);
        if self.price_side_window.len() > REGIME_WINDOW_BARS {
            self.price_side_bars -= usize::from(
                self.price_side_window
                    .pop_front()
                    .expect("window length checked"),
            );
        }

        if !self.regime_is_qualified() {
            self.phase = Phase::Building;
            return result;
        }

        if matches!(self.phase, Phase::Building) {
            self.phase = Phase::Armed {
                qualified_ts: bar.ts,
            };
            result.regime_qualified = true;
        }

        let Phase::Armed { qualified_ts } = self.phase else {
            return result;
        };
        if !super::breakout_at(bars, idx, self.direction) {
            return result;
        }

        result.confirmed_breakout = true;
        self.phase = Phase::AwaitRetest {
            qualified_ts,
            breakout_idx: idx,
            breakout_ts: bar.ts,
            // 报告保存突破确认 K 已完成后的最新状态，不能沿用上一根 Armed 快照。
            relation_age_bars: self.relation_age_bars,
            price_side_bars: self.price_side_bars,
        };
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_retest(
        &mut self,
        idx: usize,
        bar: ReadyBar,
        qualified_ts: i64,
        breakout_idx: usize,
        breakout_ts: i64,
        relation_age_bars: usize,
        price_side_bars: usize,
    ) -> StepResult {
        let mut result = StepResult::default();
        let elapsed = idx.saturating_sub(breakout_idx);
        if elapsed > RETEST_WAIT_BARS {
            result.retest_timeout = true;
            self.reset_for_new_regime();
            return result;
        }
        if elapsed == 0 || !retest_zone_reached(self.direction, bar) {
            return result;
        }

        if retest_holds(self.direction, bar) {
            result.candidate = Some(CandidateCore {
                direction: self.direction,
                qualified_ts,
                breakout_idx,
                breakout_ts,
                relation_age_bars,
                price_side_bars,
                signal_idx: idx,
                signal_bar: bar,
            });
        } else {
            result.failed_first_retest = true;
        }
        // 成功或失败都消费本次首次突破；下一次必须重新积累 144 根新状态。
        self.reset_for_new_regime();
        result
    }

    fn regime_is_qualified(&self) -> bool {
        self.relation_age_bars >= REGIME_WINDOW_BARS
            && self.price_side_window.len() == REGIME_WINDOW_BARS
            && self.price_side_bars * 100
                >= REGIME_WINDOW_BARS.saturating_mul(MIN_PRICE_SIDE_PERCENT)
    }

    fn reset_for_new_regime(&mut self) {
        self.phase = Phase::Building;
        self.relation_age_bars = 0;
        self.price_side_window.clear();
        self.price_side_bars = 0;
    }
}

fn price_on_regime_side(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => bar.close < bar.ema576,
        Direction::Short => bar.close > bar.ema576,
    }
}

fn retest_zone_reached(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => bar.low <= bar.ema144 + RETEST_ZONE_ATR * bar.atr14,
        Direction::Short => bar.high >= bar.ema144 - RETEST_ZONE_ATR * bar.atr14,
    }
}

fn retest_holds(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => {
            bar.low >= bar.ema144 - RETEST_ZONE_ATR * bar.atr14 && bar.close >= bar.ema144
        }
        Direction::Short => {
            bar.high <= bar.ema144 + RETEST_ZONE_ATR * bar.atr14 && bar.close <= bar.ema144
        }
    }
}

fn retest_extreme_atr(direction: Direction, bar: ReadyBar) -> f64 {
    match direction {
        Direction::Long => (bar.low - bar.ema144) / bar.atr14,
        Direction::Short => (bar.ema144 - bar.high) / bar.atr14,
    }
}

fn close_hold_atr(direction: Direction, bar: ReadyBar) -> f64 {
    match direction {
        Direction::Long => (bar.close - bar.ema144) / bar.atr14,
        Direction::Short => (bar.ema144 - bar.close) / bar.atr14,
    }
}

/// 连接本地 quant_core 并输出不含成交后标签的 L1 机器账本。
pub async fn run_l1_scan(output: &Path) -> Result<L1Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for first-breakout pullback-hold L1 scan")?;
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

fn build_l1_report(data: &BacktestDataSet) -> Result<L1Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * super::MS_15M)
        .context("L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut stages = L1StageCounts::default();
    let mut eligible_symbols = BTreeSet::new();
    let mut hasher = Sha256::new();
    let mut target_inputs = target_input_template();

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
    let target_audits = audit_targets(&candidates);
    let summary = summarize(&candidates, stages);
    let decision = decide(&summary, &target_audits, &target_inputs);

    Ok(L1Report {
        schema_version: "market_momentum_ema576_first_breakout_ema144_pullback_hold_l1_v1",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: CANDIDATE_KEY,
            rule_version: RULE_VERSION,
            only_variable: "replace persistent reusable qualification with one finite long-regime, first EMA576 breakout, first EMA144 pullback-hold event",
            causal_policy: "144 completed EMA relation bars plus 80% price-side closes; two completed EMA576 breakout closes; first EMA144 +/-0.30 ATR touch within 576 bars",
            execution_policy: "signal exists after the hold candle closes; earliest executable price is the next candle open and is not evaluated in L1",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
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
            stages.qualified_regimes += usize::from(step.regime_qualified);
            stages.confirmed_first_breakouts += usize::from(step.confirmed_breakout);
            stages.failed_first_retests += usize::from(step.failed_first_retest);
            stages.retest_timeouts += usize::from(step.retest_timeout);
            if let Some(core) = step.candidate {
                stages.held_first_retests += 1;
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
        setup_ts_ms: core.qualified_ts,
        breakout_ts_ms: core.breakout_ts,
        signal_ts_ms: bar.ts,
        signal_month_utc,
        prior_relation_age_bars: core.relation_age_bars,
        prior_price_side_bars: core.price_side_bars,
        prior_price_side_ratio: core.price_side_bars as f64 / REGIME_WINDOW_BARS as f64,
        bars_since_breakout: core.signal_idx.saturating_sub(core.breakout_idx),
        cross_phase: core.direction.cross_phase(bar),
        ema144: bar.ema144,
        ema576: bar.ema576,
        atr14: bar.atr14,
        retest_extreme_to_ema144_atr: retest_extreme_atr(core.direction, bar),
        close_to_ema144_directional_atr: close_hold_atr(core.direction, bar),
        execution_status: "signal_confirmed_next_bar_open_not_evaluated_l1",
    })
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
            let matched = !matched_signal_timestamps_ms.is_empty();
            TargetAudit {
                name: target.name,
                symbol: target.symbol,
                direction: target.direction.label(),
                start_ms: target.start_ms,
                end_ms: target.end_ms,
                expectation: target.expectation.label(),
                passed: target.expectation.passes(matched),
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
    let positive_targets_match = audits
        .iter()
        .filter(|audit| audit.expectation == "must_match")
        .all(|audit| audit.passed);
    let negative_targets_clear = audits
        .iter()
        .filter(|audit| audit.expectation == "must_not_match")
        .all(|audit| audit.passed);
    let target_inputs_ready = target_inputs.iter().all(|coverage| coverage.ready);
    let mut gates = BTreeMap::new();
    gates.insert("all_three_positive_targets_match", positive_targets_match);
    gates.insert("both_negative_targets_clear", negative_targets_clear);
    gates.insert("all_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "candidate_count_between_30_and_5000",
        (30..=5_000).contains(&summary.candidate_count),
    );
    gates.insert(
        "both_directions_at_least_10",
        summary
            .by_direction
            .get("long")
            .copied()
            .unwrap_or_default()
            >= 10
            && summary
                .by_direction
                .get("short")
                .copied()
                .unwrap_or_default()
                >= 10,
    );
    gates.insert("symbols_at_least_8", summary.by_symbol.len() >= 8);
    gates.insert("utc_months_at_least_6", summary.by_month_utc.len() >= 6);
    gates.insert(
        "effective_events_at_least_15",
        summary.effective_market_events >= 15,
    );
    let all_pass = gates.values().all(|passed| *passed);
    let (status, reason) = if all_pass {
        (
            "coverage_pass_ready_for_l2_prereg",
            "五张目标图和无标签分散性门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !positive_targets_match || !negative_targets_clear {
        (
            "rejected_definition_mismatch",
            "至少一张正样本未命中或反样本仍触发；按预注册停止，禁止读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "目标图通过但候选覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
        )
    };
    L1Decision {
        status,
        gates,
        reason: reason.to_owned(),
        outcome_evaluation_performed: false,
        target_chart_audit_completed: target_inputs_ready,
    }
}

fn target_input_template() -> Vec<TargetInputCoverage> {
    TARGETS
        .iter()
        .map(|target| TargetInputCoverage {
            name: target.name,
            symbol: target.symbol,
            expected_candles: inclusive_candle_count(target.start_ms, target.end_ms)
                .expect("frozen target boundaries must align to 15m"),
            ready_candles: 0,
            ready: false,
        })
        .collect()
}

fn update_target_input_coverage(
    symbol: &str,
    bars: &[PatternBar],
    coverage: &mut [TargetInputCoverage],
) {
    for (target, target_coverage) in TARGETS.iter().zip(coverage.iter_mut()) {
        if target.symbol != symbol {
            continue;
        }
        target_coverage.ready_candles = bars
            .iter()
            .filter(|bar| (target.start_ms..=target.end_ms).contains(&bar.ts))
            .filter(|bar| bar.ready().is_some())
            .count();
        target_coverage.ready = target_coverage.ready_candles == target_coverage.expected_candles;
    }
}

/// L1 报告身份，明确该机器产物只有形态覆盖意义。
#[derive(Debug, Clone, Serialize)]
pub struct L1Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// 独立候选键。
    pub candidate_key: &'static str,
    /// 冻结规则版本。
    pub rule_version: &'static str,
    /// 相对旧错误家族唯一改变的事件语义。
    pub only_variable: &'static str,
    /// 只使用已完成 K 的完整因果顺序。
    pub causal_policy: &'static str,
    /// L1 不成交，最早执行只能发生在信号后的下一根。
    pub execution_policy: &'static str,
    /// 明确禁止 L1 读取的成交后字段。
    pub label_boundary: &'static str,
    /// 与 Paper、ReadOnly、Live 和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// 一条目标窗口的数据完整性证据。
#[derive(Debug, Clone, Serialize)]
pub struct TargetInputCoverage {
    /// 预注册目标名。
    pub name: &'static str,
    /// OKX 永续合约标识。
    pub symbol: &'static str,
    /// 目标窗口应有的 15m K 根数。
    pub expected_candles: usize,
    /// OHLC 与 EMA/ATR 均可判定的实际根数。
    pub ready_candles: usize,
    /// true 表示目标窗口完整，false 表示不能据此判断命中与否。
    pub ready: bool,
}

/// 冻结行情窗口和存活成员的 L1 数据身份。
#[derive(Debug, Clone, Serialize)]
pub struct L1Coverage {
    /// 预注册抽样的成员上限。
    pub expected_symbol_count: usize,
    /// 加载器实际返回的成员数。
    pub returned_symbol_count: usize,
    /// 具备完整预热与评价窗口的成员数。
    pub eligible_symbol_count: usize,
    /// 因缺 K 或指标不可用而排除的成员证据。
    pub excluded_symbols: Vec<ExcludedSymbol>,
    /// 评价起点，Unix 毫秒。
    pub evaluation_start_ms: i64,
    /// 评价终点，Unix 毫秒。
    pub evaluation_end_ms: i64,
    /// 评价前用于 EMA576 与长期状态预热的 15m K 根数。
    pub required_pre_evaluation_bars: usize,
    /// 五张正反样本窗口的数据完整性。
    pub target_inputs: Vec<TargetInputCoverage>,
    /// 成员、OHLC、EMA144、EMA576 与 ATR14 的 SHA-256 指纹。
    pub dataset_fingerprint_sha256: String,
    /// 当前存活成员抽样的幸存者偏差限制。
    pub universe_limitation: &'static str,
}

/// 一条仅含信号收盘时可见特征的候选账本记录。
#[derive(Debug, Clone, Serialize)]
pub struct L1Candidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 144 根长期状态首次完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 连续两根 EMA576 突破确认时间，Unix 毫秒。
    pub breakout_ts_ms: i64,
    /// 首次 EMA144 回踩守稳确认时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 信号所在 UTC 月份，用于无标签分散性诊断。
    pub signal_month_utc: String,
    /// 突破时 EMA144 与 EMA576 保持规定关系的连续根数。
    pub prior_relation_age_bars: usize,
    /// 突破前最近 144 根中收盘位于 EMA576 规定一侧的根数。
    pub prior_price_side_bars: usize,
    /// `prior_price_side_bars / 144`，范围 0～1。
    pub prior_price_side_ratio: f64,
    /// 突破确认到回踩确认之间的 15m K 根数。
    pub bars_since_breakout: usize,
    /// `pre_cross_retest` 表示回踩时 EMA144 尚未穿越 EMA576，另一个值表示已经穿越。
    pub cross_phase: &'static str,
    /// 信号 K 完成后的 EMA144。
    pub ema144: f64,
    /// 信号 K 完成后的 EMA576。
    pub ema576: f64,
    /// 信号 K 完成后的 ATR14。
    pub atr14: f64,
    /// 回踩极值相对 EMA144 的方向归一化 ATR；负数表示向不利方向刺穿。
    pub retest_extreme_to_ema144_atr: f64,
    /// 收盘相对 EMA144 的方向归一化 ATR；有效候选不小于零。
    pub close_to_ema144_directional_atr: f64,
    /// L1 只确认信号，明确不伪造成交。
    pub execution_status: &'static str,
}

/// 状态机在评价窗口内的无标签阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct L1StageCounts {
    /// 新鲜 144 根长期状态完成次数。
    pub qualified_regimes: usize,
    /// 已资格化状态后的首次两收盘 EMA576 突破次数。
    pub confirmed_first_breakouts: usize,
    /// 首次 EMA144 回踩守稳并形成候选的次数。
    pub held_first_retests: usize,
    /// 首次触碰深度或收盘不满足守稳条件的次数。
    pub failed_first_retests: usize,
    /// 突破后 576 根内没有首次触碰的次数。
    pub retest_timeouts: usize,
}

/// L1 候选覆盖与分散性摘要，不包含收益指标。
#[derive(Debug, Clone, Serialize)]
pub struct L1Summary {
    /// 全部无标签候选数。
    pub candidate_count: usize,
    /// 多空候选分布。
    pub by_direction: BTreeMap<&'static str, usize>,
    /// EMA144/576 交叉前后回踩分布。
    pub by_cross_phase: BTreeMap<&'static str, usize>,
    /// 各币种候选数。
    pub by_symbol: BTreeMap<String, usize>,
    /// 各 UTC 月份候选数。
    pub by_month_utc: BTreeMap<String, usize>,
    /// 按方向和 60 分钟窗口归并后的有效市场事件数。
    pub effective_market_events: usize,
    /// 状态机资格、突破、回踩和超时计数。
    pub stages: L1StageCounts,
}

/// 一张用户正样本或错误开单反样本的定义审计。
#[derive(Debug, Clone, Serialize)]
pub struct TargetAudit {
    /// 预注册目标名。
    pub name: &'static str,
    /// OKX 永续合约标识。
    pub symbol: &'static str,
    /// 预期候选方向。
    pub direction: &'static str,
    /// 审计窗口起点，Unix 毫秒。
    pub start_ms: i64,
    /// 审计窗口终点，Unix 毫秒。
    pub end_ms: i64,
    /// `must_match` 为正样本，`must_not_match` 为反样本。
    pub expectation: &'static str,
    /// true 表示实际命中状态符合预注册期望。
    pub passed: bool,
    /// 窗口内实际候选时间，Unix 毫秒；反样本应为空。
    pub matched_signal_timestamps_ms: Vec<i64>,
}

/// L1 门禁结论；通过也只允许新建 L2 预注册。
#[derive(Debug, Clone, Serialize)]
pub struct L1Decision {
    /// `coverage_pass_ready_for_l2_prereg` 或明确停止状态。
    pub status: &'static str,
    /// 每项预注册门禁及其布尔结果。
    pub gates: BTreeMap<&'static str, bool>,
    /// 人类可读的停止或升级边界。
    pub reason: String,
    /// L1 必须恒为 false，防止用结果标签反向调定义。
    pub outcome_evaluation_performed: bool,
    /// true 表示五个目标窗口输入完整并已审计。
    pub target_chart_audit_completed: bool,
}

/// 新策略身份的完整 L1 机器产物。
#[derive(Debug, Clone, Serialize)]
pub struct L1Report {
    /// 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339；不参与数据指纹。
    pub generated_at_utc: String,
    /// 策略、因果、标签和运行隔离身份。
    pub identity: L1Identity,
    /// 冻结窗口、成员与目标输入证据。
    pub coverage: L1Coverage,
    /// 不含 outcome 的覆盖率与分散性摘要。
    pub summary: L1Summary,
    /// 三张正样本和两张反样本的定义门禁。
    pub target_audits: Vec<TargetAudit>,
    /// L1 停止或准备进入 L2 预注册的结论。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<L1Candidate>,
}
