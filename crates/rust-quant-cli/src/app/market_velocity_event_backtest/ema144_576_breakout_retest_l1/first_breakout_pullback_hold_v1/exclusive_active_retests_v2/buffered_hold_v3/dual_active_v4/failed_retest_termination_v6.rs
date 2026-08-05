//! V4 双活动方向下，EMA144 失败回踩立即终止该方向 episode 的 L1 扫描。
//!
//! V6 不把 EMA576 的正常反穿当成失效；只有已经武装的 EMA144 回踩越出冻结
//! `0.30 ATR14` 守稳边界，才删除该方向活动上下文，阻止后续日期再次武装。

pub mod persistent_qualification_v7;
#[cfg(test)]
mod tests;

use super::*;

/// V6 独立候选身份，不能覆盖 V4 或任何旧生命周期版本。
pub const V6_CANDIDATE_KEY: &str =
    "market_momentum_ema576_dual_active_ema144_failed_retest_termination_15m_v6";
/// V6 精确规则身份；失败回踩的后果变化必须新建版本。
pub const V6_RULE_VERSION: &str = "l1_v4_dual_active_same030_failed_retest_terminates_v6";

const V6_MIN_CANDIDATES: usize = 5_000;
const V6_MAX_CANDIDATES: usize = 78_000;
const BTC_JULY_AUDIT_START_MS: i64 = 1_782_835_200_000;
const BTC_WRONG_SHORT_SIGNAL_MS: i64 = 1_784_466_900_000;

const V6_TARGETS: [V6TargetDefinition; 6] = [
    V6TargetDefinition::must_match(
        "nmr_2026_07_01_user_chart",
        "NMR-USDT-SWAP",
        Direction::Long,
        1_782_835_200_000,
        1_782_878_400_000,
    ),
    V6TargetDefinition::must_match(
        "btc_2026_07_02_user_chart",
        "BTC-USDT-SWAP",
        Direction::Long,
        1_782_943_200_000,
        1_782_964_800_000,
    ),
    V6TargetDefinition::must_match(
        "btc_2026_07_12_user_chart",
        "BTC-USDT-SWAP",
        Direction::Long,
        1_783_828_800_000,
        1_783_850_400_000,
    ),
    V6TargetDefinition::must_not_match(
        "algo_2026_07_19_wrong_short",
        "ALGO-USDT-SWAP",
        Direction::Short,
        1_784_453_400_000,
    ),
    V6TargetDefinition::must_not_match(
        "merl_2026_07_19_wrong_short",
        "MERL-USDT-SWAP",
        Direction::Short,
        1_784_457_900_000,
    ),
    V6TargetDefinition::must_not_match(
        "btc_2026_07_19_interrupted_short",
        "BTC-USDT-SWAP",
        Direction::Short,
        BTC_WRONG_SHORT_SIGNAL_MS,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V6TargetExpectation {
    /// 目标窗口必须出现同方向候选。
    MustMatch,
    /// 目标时间不得出现同方向候选。
    MustNotMatch,
}

impl V6TargetExpectation {
    fn label(self) -> &'static str {
        match self {
            Self::MustMatch => "must_match",
            Self::MustNotMatch => "must_not_match",
        }
    }

    fn passes(self, matched: bool) -> bool {
        matches!(
            (self, matched),
            (Self::MustMatch, true) | (Self::MustNotMatch, false)
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct V6TargetDefinition {
    /// 稳定的目标样本名称。
    name: &'static str,
    /// OKX 永续合约标识。
    symbol: &'static str,
    /// 目标要求审计的多空方向。
    direction: Direction,
    /// 目标窗口起点，Unix 毫秒。
    start_ms: i64,
    /// 目标窗口终点，Unix 毫秒。
    end_ms: i64,
    /// 正样本或禁止触发的反样本。
    expectation: V6TargetExpectation,
}

impl V6TargetDefinition {
    const fn must_match(
        name: &'static str,
        symbol: &'static str,
        direction: Direction,
        start_ms: i64,
        end_ms: i64,
    ) -> Self {
        Self {
            name,
            symbol,
            direction,
            start_ms,
            end_ms,
            expectation: V6TargetExpectation::MustMatch,
        }
    }

    const fn must_not_match(
        name: &'static str,
        symbol: &'static str,
        direction: Direction,
        ts_ms: i64,
    ) -> Self {
        Self {
            name,
            symbol,
            direction,
            start_ms: ts_ms,
            end_ms: ts_ms,
            expectation: V6TargetExpectation::MustNotMatch,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FailedRetestInvalidationCore {
    /// 被失败回踩终止的活动方向快照。
    active: ActiveDirection,
    /// 失败回踩完成 K 时间，Unix 毫秒。
    invalidated_ts: i64,
    /// 回踩极值相对 EMA144 的方向归一化 ATR。
    extreme_to_ema144_atr: f64,
    /// 收盘相对 EMA144 的方向归一化 ATR。
    close_to_ema144_atr: f64,
}

#[derive(Debug, Default)]
struct V6StepResult {
    /// 本根完成的新长期资格数。
    qualified_regimes: usize,
    /// 本根建立或替换的活动 episode。
    episode_started: Option<ActiveDirection>,
    /// 本根各方向重新离开 EMA144 后的武装总数。
    retest_rearms: usize,
    /// 本根满足守稳条件的候选，可分别来自两个独立方向。
    candidates: Vec<V2CandidateCore>,
    /// 本根因 EMA144 失守而终止的方向，可分别来自两个独立 episode。
    failed_retest_invalidations: Vec<FailedRetestInvalidationCore>,
}

#[derive(Debug)]
struct FailedRetestTerminationMachine {
    /// V4 原样保留的多头长期资格积累器。
    long_tracker: RegimeTracker,
    /// V4 原样保留的空头长期资格积累器。
    short_tracker: RegimeTracker,
    /// 独立保存的多头活动 episode。
    long_active: Option<ActiveDirection>,
    /// 独立保存的空头活动 episode。
    short_active: Option<ActiveDirection>,
}

impl FailedRetestTerminationMachine {
    fn new() -> Self {
        Self {
            long_tracker: RegimeTracker::new(Direction::Long),
            short_tracker: RegimeTracker::new(Direction::Short),
            long_active: None,
            short_active: None,
        }
    }

    /// 新突破沿用 V4 优先级；没有新突破时才评估两个方向各自的回踩。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V6StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V6StepResult::default();
        };
        let long = self.long_tracker.step(bars, idx, bar);
        let short = self.short_tracker.step(bars, idx, bar);
        let mut result = V6StepResult {
            qualified_regimes: usize::from(long.qualified) + usize::from(short.qualified),
            ..V6StepResult::default()
        };

        if let Some(activation) = long.activation.or(short.activation) {
            let rearmed = departed_from_ema144(activation.direction, bar);
            let active = ActiveDirection {
                direction: activation.direction,
                qualified_ts: activation.qualified_ts,
                breakout_idx: activation.breakout_idx,
                breakout_ts: activation.breakout_ts,
                relation_age_bars: activation.relation_age_bars,
                price_side_bars: activation.price_side_bars,
                rearmed_idx: rearmed.then_some(idx),
                rearmed_ts: rearmed.then_some(bar.ts),
            };
            *self.active_mut(activation.direction) = Some(active);
            self.long_tracker.reset();
            self.short_tracker.reset();
            result.episode_started = Some(active);
            result.retest_rearms = usize::from(rearmed);
            return result;
        }

        for direction in [Direction::Long, Direction::Short] {
            let Some(mut active) = *self.active_mut(direction) else {
                continue;
            };
            if active.rearmed_idx.is_none() {
                if departed_from_ema144(active.direction, bar) {
                    active.rearmed_idx = Some(idx);
                    active.rearmed_ts = Some(bar.ts);
                    result.retest_rearms += 1;
                }
                *self.active_mut(direction) = Some(active);
                continue;
            }
            if !retest_zone_reached(active.direction, bar) {
                *self.active_mut(direction) = Some(active);
                continue;
            }

            if retest_holds_with_close_buffer(active.direction, bar, V3_CLOSE_HOLD_BUFFER_ATR) {
                result.candidates.push(V2CandidateCore {
                    active,
                    signal_idx: idx,
                    signal_bar: bar,
                });
                active.rearmed_idx = None;
                active.rearmed_ts = None;
                *self.active_mut(direction) = Some(active);
            } else {
                // 失败回踩已经否定这次突破，不能像 V4 一样保留到后续日期再次武装。
                result
                    .failed_retest_invalidations
                    .push(FailedRetestInvalidationCore {
                        active,
                        invalidated_ts: bar.ts,
                        extreme_to_ema144_atr: retest_extreme_atr(active.direction, bar),
                        close_to_ema144_atr: close_hold_atr(active.direction, bar),
                    });
                *self.active_mut(direction) = None;
            }
        }
        result
    }

    fn active_mut(&mut self, direction: Direction) -> &mut Option<ActiveDirection> {
        match direction {
            Direction::Long => &mut self.long_active,
            Direction::Short => &mut self.short_active,
        }
    }

    fn reset_all(&mut self) {
        self.long_tracker.reset();
        self.short_tracker.reset();
        self.long_active = None;
        self.short_active = None;
    }
}

/// 连接本机 quant_core 并输出 V6 无成交后标签候选与失败中断证据。
pub async fn run_v6_l1_scan(output: &Path) -> Result<V6Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V6 failed EMA144 retest L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v6_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V6 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V6 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v6_report(data: &BacktestDataSet) -> Result<V6Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(
            REQUIRED_PRE_EVALUATION_BARS as i64 * super::super::super::super::super::MS_15M,
        )
        .context("V6 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut lifecycle_events = Vec::new();
    let mut stages = V6StageCounts::default();
    let mut eligible_symbols = BTreeSet::new();
    let mut hasher = Sha256::new();
    let mut target_inputs = v6_target_input_template();

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
            .context("V6 evaluation start index overflow")?;
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
        update_v6_target_input_coverage(&pair.symbol, &bars, &mut target_inputs);
        scan_symbol_v6(
            &pair.symbol,
            &bars,
            start_idx,
            end_idx,
            &mut candidates,
            &mut lifecycle_events,
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
    lifecycle_events.sort_by_key(|event| event.ts_ms);
    let target_audits = audit_v6_targets(&candidates);
    let btc_wrong_short_lifecycle_audit = audit_btc_wrong_short_lifecycle(&lifecycle_events);
    let summary = summarize_v6(&candidates, stages);
    let decision = decide_v6(
        &summary,
        &target_audits,
        &target_inputs,
        &btc_wrong_short_lifecycle_audit,
    );

    Ok(V6Report {
        schema_version: "market_momentum_ema576_dual_active_ema144_failed_retest_termination_l1_v6",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V6Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V6_CANDIDATE_KEY,
            rule_version: V6_RULE_VERSION,
            only_variable: "change V4 failed EMA144 retest handling from consuming only the arm to terminating that direction's entire active episode",
            unchanged_entry_policy: "V4 regime144, price-side80%, two-close EMA576 breakout, independent directional ownership, repeated rearming after successful retests, and the same +/-0.30 ATR14 buffer remain frozen",
            failure_policy: "once an armed retest reaches EMA144 but its extreme or close crosses the directional 0.30 ATR14 hold boundary, the active direction and arm are deleted; a fresh qualified breakout is required",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V6; not registered in paper, readonly shadow, live worker, compose, or production presets",
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
        btc_wrong_short_lifecycle_audit,
        btc_lifecycle_events: lifecycle_events,
        decision,
        candidates,
    })
}

fn scan_symbol_v6(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V6LifecycleEvent>,
    stages: &mut V6StageCounts,
) -> Result<()> {
    let mut machine = FailedRetestTerminationMachine::new();
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualified_regimes += step.qualified_regimes;
        stages.episode_starts += usize::from(step.episode_started.is_some());
        stages.retest_rearms += step.retest_rearms;
        stages.held_retests += step.candidates.len();
        stages.failed_retest_invalidations += step.failed_retest_invalidations.len();
        if symbol == "BTC-USDT-SWAP" && (BTC_JULY_AUDIT_START_MS..=EVALUATION_END_MS).contains(&ts)
        {
            append_btc_lifecycle_events(symbol, &step, lifecycle_events);
        }
        for core in step.candidates {
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

fn append_btc_lifecycle_events(
    symbol: &str,
    step: &V6StepResult,
    events: &mut Vec<V6LifecycleEvent>,
) {
    if let Some(active) = step.episode_started {
        events.push(V6LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: active.breakout_ts,
            event: "episode_started",
            direction: active.direction.label(),
            qualification_ts_ms: active.qualified_ts,
            episode_breakout_ts_ms: active.breakout_ts,
            retest_extreme_to_ema144_atr: None,
            close_to_ema144_directional_atr: None,
        });
    }
    for invalidation in &step.failed_retest_invalidations {
        events.push(V6LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: invalidation.invalidated_ts,
            event: "episode_invalidated_failed_ema144_retest",
            direction: invalidation.active.direction.label(),
            qualification_ts_ms: invalidation.active.qualified_ts,
            episode_breakout_ts_ms: invalidation.active.breakout_ts,
            retest_extreme_to_ema144_atr: Some(invalidation.extreme_to_ema144_atr),
            close_to_ema144_directional_atr: Some(invalidation.close_to_ema144_atr),
        });
    }
}

fn v6_target_input_template() -> Vec<TargetInputCoverage> {
    V6_TARGETS
        .iter()
        .map(|target| TargetInputCoverage {
            name: target.name,
            symbol: target.symbol,
            expected_candles: inclusive_candle_count(target.start_ms, target.end_ms)
                .expect("V6 frozen target boundaries must align to 15m"),
            ready_candles: 0,
            ready: false,
        })
        .collect()
}

fn update_v6_target_input_coverage(
    symbol: &str,
    bars: &[PatternBar],
    coverage: &mut [TargetInputCoverage],
) {
    for (target, target_coverage) in V6_TARGETS.iter().zip(coverage.iter_mut()) {
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

fn audit_v6_targets(candidates: &[V2Candidate]) -> Vec<TargetAudit> {
    V6_TARGETS
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

fn audit_btc_wrong_short_lifecycle(events: &[V6LifecycleEvent]) -> V6BtcLifecycleAudit {
    let last_short_episode_breakout_ts_ms = events
        .iter()
        .rev()
        .find(|event| {
            event.event == "episode_started"
                && event.direction == "short"
                && event.ts_ms < BTC_WRONG_SHORT_SIGNAL_MS
        })
        .map(|event| event.ts_ms);
    let invalidation = events.iter().find(|event| {
        event.event == "episode_invalidated_failed_ema144_retest"
            && event.direction == "short"
            && Some(event.episode_breakout_ts_ms) == last_short_episode_breakout_ts_ms
            && event.ts_ms < BTC_WRONG_SHORT_SIGNAL_MS
    });
    let invalidated_ts_ms = invalidation.map(|event| event.ts_ms);
    let new_short_episode_start_timestamps_ms = invalidated_ts_ms
        .map(|invalidated_ts| {
            events
                .iter()
                .filter(|event| {
                    event.event == "episode_started"
                        && event.direction == "short"
                        && (invalidated_ts + 1..=BTC_WRONG_SHORT_SIGNAL_MS).contains(&event.ts_ms)
                })
                .map(|event| event.ts_ms)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    V6BtcLifecycleAudit {
        last_short_episode_breakout_before_old_signal_ts_ms: last_short_episode_breakout_ts_ms,
        invalidated_ts_ms,
        invalidation_retest_extreme_to_ema144_atr: invalidation
            .and_then(|event| event.retest_extreme_to_ema144_atr),
        invalidation_close_to_ema144_directional_atr: invalidation
            .and_then(|event| event.close_to_ema144_directional_atr),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: new_short_episode_start_timestamps_ms.clone(),
        passed: invalidated_ts_ms.is_some() && new_short_episode_start_timestamps_ms.is_empty(),
    }
}

fn summarize_v6(candidates: &[V2Candidate], stages: V6StageCounts) -> V6Summary {
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
    V6Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v6_event_count(candidates),
        stages,
    }
}

fn effective_v6_event_count(candidates: &[V2Candidate]) -> usize {
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

fn decide_v6(
    summary: &V6Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
    btc_lifecycle: &V6BtcLifecycleAudit,
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
    gates.insert("all_three_negative_targets_clear", negative_targets_clear);
    gates.insert(
        "btc_failed_retest_interrupted_old_short",
        btc_lifecycle.passed,
    );
    gates.insert("all_six_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "candidate_count_between_5000_and_78000",
        (V6_MIN_CANDIDATES..=V6_MAX_CANDIDATES).contains(&summary.candidate_count),
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
        "effective_events_at_least_100",
        summary.effective_market_events >= 100,
    );
    let all_pass = gates.values().all(|passed| *passed);
    let definition_matches =
        positive_targets_match && negative_targets_clear && btc_lifecycle.passed;
    let (status, reason) = if all_pass {
        (
            "coverage_pass_ready_for_l2_prereg",
            "V6 六张目标图、BTC 失败回踩中断证据和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !definition_matches {
        (
            "rejected_definition_mismatch",
            "V6 至少一张正反样本不符合，或 BTC 旧空头未留下失败 EMA144 回踩中断证据；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V6 定义样本通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V6 冻结身份，明确失败 EMA144 回踩对活动 episode 的终止语义。
#[derive(Debug, Clone, Serialize)]
pub struct V6Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V6 独立候选键。
    pub candidate_key: &'static str,
    /// V6 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V4 唯一改变的失败回踩生命周期变量。
    pub only_variable: &'static str,
    /// V4 中保持冻结的资格、活动方向、阈值和成功回踩规则。
    pub unchanged_entry_policy: &'static str,
    /// 失败回踩终止活动方向且要求重新资格化的合同。
    pub failure_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V6 无标签生命周期阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V6StageCounts {
    /// 新鲜长期资格完成次数。
    pub qualified_regimes: usize,
    /// 建立或替换同方向活动 episode 的次数。
    pub episode_starts: usize,
    /// 活动方向重新离开 EMA144 并武装的次数。
    pub retest_rearms: usize,
    /// 回踩满足冻结缓冲守稳条件的次数。
    pub held_retests: usize,
    /// 回踩失守并终止整个方向 episode 的次数。
    pub failed_retest_invalidations: usize,
}

/// V6 无标签覆盖与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V6Summary {
    /// 全部 V6 候选数。
    pub candidate_count: usize,
    /// 多空候选分布。
    pub by_direction: BTreeMap<&'static str, usize>,
    /// EMA144/576 交叉前后候选分布。
    pub by_cross_phase: BTreeMap<&'static str, usize>,
    /// 各币种候选数。
    pub by_symbol: BTreeMap<String, usize>,
    /// 各 UTC 月份候选数。
    pub by_month_utc: BTreeMap<String, usize>,
    /// 按方向和 60 分钟窗口归并的有效市场事件数。
    pub effective_market_events: usize,
    /// V6 资格、episode、武装、成功与失败中断阶段计数。
    pub stages: V6StageCounts,
}

/// BTC 七月目标窗口内的一条信号时可见生命周期事件。
#[derive(Debug, Clone, Serialize)]
pub struct V6LifecycleEvent {
    /// OKX 永续合约标识，本报告只保留 BTC 审计窗口。
    pub symbol: String,
    /// 事件完成 K 时间，Unix 毫秒。
    pub ts_ms: i64,
    /// `episode_started` 或 `episode_invalidated_failed_ema144_retest`。
    pub event: &'static str,
    /// 事件影响的 `long` 或 `short` 方向。
    pub direction: &'static str,
    /// 该 episode 使用的长期资格时间，Unix 毫秒。
    pub qualification_ts_ms: i64,
    /// 该 episode 的 EMA576 两收盘突破时间，Unix 毫秒。
    pub episode_breakout_ts_ms: i64,
    /// 失败回踩极值相对 EMA144 的方向 ATR；episode 建立事件为 `None`。
    pub retest_extreme_to_ema144_atr: Option<f64>,
    /// 失败回踩收盘相对 EMA144 的方向 ATR；episode 建立事件为 `None`。
    pub close_to_ema144_directional_atr: Option<f64>,
}

/// BTC 7 月 19 日旧空点的失败回踩中断门禁。
#[derive(Debug, Clone, Serialize)]
pub struct V6BtcLifecycleAudit {
    /// 旧信号前最后一次空头 episode 的突破时间；未找到时为 `None`。
    pub last_short_episode_breakout_before_old_signal_ts_ms: Option<i64>,
    /// 该空头 episode 因 EMA144 失守被终止的时间；未找到时为 `None`。
    pub invalidated_ts_ms: Option<i64>,
    /// 终止 K 极值相对 EMA144 的方向 ATR；未找到中断时为 `None`。
    pub invalidation_retest_extreme_to_ema144_atr: Option<f64>,
    /// 终止 K 收盘相对 EMA144 的方向 ATR；未找到中断时为 `None`。
    pub invalidation_close_to_ema144_directional_atr: Option<f64>,
    /// V4 曾错误触发的 BTC 空头信号时间，Unix 毫秒。
    pub old_wrong_signal_ts_ms: i64,
    /// 中断后、旧信号前重新建立的空头 episode；通过时必须为空。
    pub new_short_episode_start_timestamps_ms: Vec<i64>,
    /// true 表示旧空头已因失败回踩中断，且旧信号前没有重新取得资格。
    pub passed: bool,
}

/// V6 的完整 L1 机器产物；候选与中断均只使用当时已完成 K。
#[derive(Debug, Clone, Serialize)]
pub struct V6Report {
    /// V6 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V6 规则、标签与运行隔离身份。
    pub identity: V6Identity,
    /// 冻结行情、成员与六个目标窗口输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和失败回踩中断摘要。
    pub summary: V6Summary,
    /// 三张正样本与三张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// BTC 旧空头失败 EMA144 回踩中断门禁。
    pub btc_wrong_short_lifecycle_audit: V6BtcLifecycleAudit,
    /// BTC 七月目标窗口的 episode 建立与失败中断事件。
    pub btc_lifecycle_events: Vec<V6LifecycleEvent>,
    /// V6 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
