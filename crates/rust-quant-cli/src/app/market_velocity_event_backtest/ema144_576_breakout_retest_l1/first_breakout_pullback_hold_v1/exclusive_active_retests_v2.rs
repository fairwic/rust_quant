//! 最新合格 EMA576 突破方向独占生效，并允许 EMA144 回踩重复再武装的 L1 扫描。
//!
//! V2 只改变 V1 的生命周期；长期状态、突破、回踩区和同 K 守稳条件全部保持冻结。

pub mod buffered_hold_v3;
#[cfg(test)]
mod tests;

use super::*;

/// V2 独立候选身份，不能覆盖单次回踩 V1 或旧永久双向资格版本。
pub const V2_CANDIDATE_KEY: &str = "market_momentum_ema576_exclusive_active_ema144_retests_15m_v2";
/// V2 只改变互斥活动方向和回踩再武装生命周期。
pub const V2_RULE_VERSION: &str = "l1_v1_exclusive_active_rearm_same030_hold_v2";

#[derive(Debug, Clone, Copy)]
enum TrackerPhase {
    Building,
    Armed { qualified_ts: i64 },
}

#[derive(Debug, Clone, Copy)]
struct Activation {
    direction: Direction,
    qualified_ts: i64,
    breakout_idx: usize,
    breakout_ts: i64,
    relation_age_bars: usize,
    price_side_bars: usize,
}

#[derive(Debug, Default)]
struct TrackerStep {
    qualified: bool,
    activation: Option<Activation>,
}

#[derive(Debug)]
struct RegimeTracker {
    direction: Direction,
    phase: TrackerPhase,
    relation_age_bars: usize,
    price_side_window: VecDeque<bool>,
    price_side_bars: usize,
}

impl RegimeTracker {
    fn new(direction: Direction) -> Self {
        Self {
            direction,
            phase: TrackerPhase::Building,
            relation_age_bars: 0,
            price_side_window: VecDeque::with_capacity(REGIME_WINDOW_BARS),
            price_side_bars: 0,
        }
    }

    fn step(&mut self, bars: &[PatternBar], idx: usize, bar: ReadyBar) -> TrackerStep {
        let mut result = TrackerStep::default();
        if !self.direction.regime_holds(bar) {
            self.reset();
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

        if !self.qualified() {
            self.phase = TrackerPhase::Building;
            return result;
        }
        if matches!(self.phase, TrackerPhase::Building) {
            self.phase = TrackerPhase::Armed {
                qualified_ts: bar.ts,
            };
            result.qualified = true;
        }

        let TrackerPhase::Armed { qualified_ts } = self.phase else {
            return result;
        };
        if !super::super::breakout_at(bars, idx, self.direction) {
            return result;
        }
        result.activation = Some(Activation {
            direction: self.direction,
            qualified_ts,
            breakout_idx: idx,
            breakout_ts: bar.ts,
            relation_age_bars: self.relation_age_bars,
            price_side_bars: self.price_side_bars,
        });
        self.reset();
        result
    }

    fn qualified(&self) -> bool {
        self.relation_age_bars >= REGIME_WINDOW_BARS
            && self.price_side_window.len() == REGIME_WINDOW_BARS
            && self.price_side_bars * 100
                >= REGIME_WINDOW_BARS.saturating_mul(MIN_PRICE_SIDE_PERCENT)
    }

    fn reset(&mut self) {
        self.phase = TrackerPhase::Building;
        self.relation_age_bars = 0;
        self.price_side_window.clear();
        self.price_side_bars = 0;
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveDirection {
    direction: Direction,
    qualified_ts: i64,
    breakout_idx: usize,
    breakout_ts: i64,
    relation_age_bars: usize,
    price_side_bars: usize,
    rearmed_idx: Option<usize>,
    rearmed_ts: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct V2CandidateCore {
    active: ActiveDirection,
    signal_idx: usize,
    signal_bar: ReadyBar,
}

#[derive(Debug, Default)]
struct V2StepResult {
    qualified_regimes: usize,
    activated_direction: bool,
    replaced_opposite_direction: bool,
    retest_rearmed: bool,
    failed_retest: bool,
    candidate: Option<V2CandidateCore>,
}

#[derive(Debug)]
struct ExclusiveActiveMachine {
    long_tracker: RegimeTracker,
    short_tracker: RegimeTracker,
    active: Option<ActiveDirection>,
    close_hold_buffer_atr: f64,
}

impl ExclusiveActiveMachine {
    fn new() -> Self {
        Self::with_close_hold_buffer(0.0)
    }

    /// 仅供后续单变量研究复用 V2 生命周期；V2 自身始终传入零缓冲。
    fn with_close_hold_buffer(close_hold_buffer_atr: f64) -> Self {
        debug_assert!((0.0..=RETEST_ZONE_ATR).contains(&close_hold_buffer_atr));
        Self {
            long_tracker: RegimeTracker::new(Direction::Long),
            short_tracker: RegimeTracker::new(Direction::Short),
            active: None,
            close_hold_buffer_atr,
        }
    }

    /// 先处理最新合格的 EMA576 突破，再处理回踩，保证新方向当根不会继承旧方向订单。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V2StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V2StepResult::default();
        };
        let long = self.long_tracker.step(bars, idx, bar);
        let short = self.short_tracker.step(bars, idx, bar);
        let mut result = V2StepResult {
            qualified_regimes: usize::from(long.qualified) + usize::from(short.qualified),
            ..V2StepResult::default()
        };

        let activation = long.activation.or(short.activation);
        if let Some(activation) = activation {
            result.activated_direction = true;
            result.replaced_opposite_direction = self
                .active
                .is_some_and(|active| active.direction != activation.direction);
            // 新方向必须从此刻重新积累对侧资格，杜绝旧多空资格并存。
            self.long_tracker.reset();
            self.short_tracker.reset();
            let rearmed = departed_from_ema144(activation.direction, bar);
            self.active = Some(ActiveDirection {
                direction: activation.direction,
                qualified_ts: activation.qualified_ts,
                breakout_idx: activation.breakout_idx,
                breakout_ts: activation.breakout_ts,
                relation_age_bars: activation.relation_age_bars,
                price_side_bars: activation.price_side_bars,
                rearmed_idx: rearmed.then_some(idx),
                rearmed_ts: rearmed.then_some(bar.ts),
            });
            result.retest_rearmed = rearmed;
            return result;
        }

        let Some(mut active) = self.active else {
            return result;
        };
        if active.rearmed_idx.is_none() {
            if departed_from_ema144(active.direction, bar) {
                active.rearmed_idx = Some(idx);
                active.rearmed_ts = Some(bar.ts);
                result.retest_rearmed = true;
            }
            self.active = Some(active);
            return result;
        }
        if !retest_zone_reached(active.direction, bar) {
            self.active = Some(active);
            return result;
        }

        if retest_holds_with_close_buffer(active.direction, bar, self.close_hold_buffer_atr) {
            result.candidate = Some(V2CandidateCore {
                active,
                signal_idx: idx,
                signal_bar: bar,
            });
        } else {
            result.failed_retest = true;
        }
        // 只消费本次回踩武装；活动方向保留，等待下一次真正离开 EMA144 后再武装。
        active.rearmed_idx = None;
        active.rearmed_ts = None;
        self.active = Some(active);
        result
    }

    fn reset_all(&mut self) {
        self.long_tracker.reset();
        self.short_tracker.reset();
        self.active = None;
    }
}

fn departed_from_ema144(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => bar.close > bar.ema144 + RETEST_ZONE_ATR * bar.atr14,
        Direction::Short => bar.close < bar.ema144 - RETEST_ZONE_ATR * bar.atr14,
    }
}

fn retest_holds_with_close_buffer(
    direction: Direction,
    bar: ReadyBar,
    close_hold_buffer_atr: f64,
) -> bool {
    retest_extreme_atr(direction, bar) >= -RETEST_ZONE_ATR
        && close_hold_atr(direction, bar) >= -close_hold_buffer_atr
}

/// 连接本地 quant_core 并输出 V2 无标签候选账本。
pub async fn run_v2_l1_scan(output: &Path) -> Result<V2Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for exclusive-active EMA144 retest L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v2_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V2 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V2 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v2_report(data: &BacktestDataSet) -> Result<V2Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * super::super::MS_15M)
        .context("V2 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut stages = V2StageCounts::default();
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
            .context("V2 evaluation start index overflow")?;
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
        scan_symbol_v2(
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
    let target_audits = audit_v2_targets(&candidates);
    let summary = summarize_v2(&candidates, stages);
    let decision = decide_v2(&summary, &target_audits, &target_inputs);

    Ok(V2Report {
        schema_version: "market_momentum_ema576_exclusive_active_ema144_retests_l1_v2",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V2Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V2_CANDIDATE_KEY,
            rule_version: V2_RULE_VERSION,
            only_variable: "replace V1 one-shot episode consumption with one mutually exclusive active breakout direction and repeatable EMA144 retest rearming",
            unchanged_entry_policy: "V1 regime144, price-side80%, two-close EMA576 breakout, EMA144 +/-0.30 ATR touch, and same-candle EMA144 close hold remain unchanged",
            lifecycle_policy: "latest qualified breakout direction clears both qualification trackers; active direction persists and rearms only after a completed favorable close beyond the same 0.30 ATR zone",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V2; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn scan_symbol_v2(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    stages: &mut V2StageCounts,
) -> Result<()> {
    let mut machine = ExclusiveActiveMachine::new();
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualified_regimes += step.qualified_regimes;
        stages.activated_directions += usize::from(step.activated_direction);
        stages.opposite_direction_replacements += usize::from(step.replaced_opposite_direction);
        stages.retest_rearms += usize::from(step.retest_rearmed);
        stages.failed_retests += usize::from(step.failed_retest);
        if let Some(core) = step.candidate {
            stages.held_retests += 1;
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

fn candidate_from_v2_core(symbol: &str, core: V2CandidateCore) -> Result<V2Candidate> {
    let bar = core.signal_bar;
    let rearmed_idx = core
        .active
        .rearmed_idx
        .context("V2 candidate missing causal rearm index")?;
    let rearmed_ts = core
        .active
        .rearmed_ts
        .context("V2 candidate missing causal rearm timestamp")?;
    let signal_month_utc = Utc
        .timestamp_millis_opt(bar.ts)
        .single()
        .context("invalid V2 signal timestamp")?
        .format("%Y-%m")
        .to_string();
    Ok(V2Candidate {
        symbol: symbol.to_owned(),
        direction: core.active.direction.label(),
        setup_ts_ms: core.active.qualified_ts,
        breakout_ts_ms: core.active.breakout_ts,
        rearmed_ts_ms: rearmed_ts,
        signal_ts_ms: bar.ts,
        signal_month_utc,
        prior_relation_age_bars: core.active.relation_age_bars,
        prior_price_side_bars: core.active.price_side_bars,
        prior_price_side_ratio: core.active.price_side_bars as f64 / REGIME_WINDOW_BARS as f64,
        bars_since_breakout: core.signal_idx.saturating_sub(core.active.breakout_idx),
        bars_since_rearm: core.signal_idx.saturating_sub(rearmed_idx),
        cross_phase: core.active.direction.cross_phase(bar),
        ema144: bar.ema144,
        ema576: bar.ema576,
        atr14: bar.atr14,
        retest_extreme_to_ema144_atr: retest_extreme_atr(core.active.direction, bar),
        close_to_ema144_directional_atr: close_hold_atr(core.active.direction, bar),
        execution_status: "signal_confirmed_next_bar_open_not_evaluated_l1",
    })
}

fn summarize_v2(candidates: &[V2Candidate], stages: V2StageCounts) -> V2Summary {
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
    V2Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v2_event_count(candidates),
        stages,
    }
}

fn effective_v2_event_count(candidates: &[V2Candidate]) -> usize {
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

fn audit_v2_targets(candidates: &[V2Candidate]) -> Vec<TargetAudit> {
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

fn decide_v2(
    summary: &V2Summary,
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
        "candidate_count_between_2000_and_60000",
        (2_000..=60_000).contains(&summary.candidate_count),
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
    let (status, reason) = if all_pass {
        (
            "coverage_pass_ready_for_l2_prereg",
            "V2 五张目标图和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !positive_targets_match || !negative_targets_clear {
        (
            "rejected_definition_mismatch",
            "V2 至少一张正样本未命中或反样本仍触发；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V2 目标图通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V2 冻结身份，明确只改变活动方向生命周期。
#[derive(Debug, Clone, Serialize)]
pub struct V2Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V2 独立候选键。
    pub candidate_key: &'static str,
    /// V2 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V1 唯一改变的生命周期变量。
    pub only_variable: &'static str,
    /// V1 中保持冻结的入场形态规则。
    pub unchanged_entry_policy: &'static str,
    /// 互斥活动方向和重复回踩再武装合同。
    pub lifecycle_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// 一条 V2 信号时可见候选，额外保存本次回踩再武装时间。
#[derive(Debug, Clone, Serialize)]
pub struct V2Candidate {
    /// OKX 永续合约标识。
    pub symbol: String,
    /// `long` 或 `short`。
    pub direction: &'static str,
    /// 长期状态资格完成时间，Unix 毫秒。
    pub setup_ts_ms: i64,
    /// 当前互斥活动方向的 EMA576 突破时间，Unix 毫秒。
    pub breakout_ts_ms: i64,
    /// 价格重新离开 EMA144 区域并武装本次回踩的时间，Unix 毫秒。
    pub rearmed_ts_ms: i64,
    /// 回踩守稳确认时间，Unix 毫秒。
    pub signal_ts_ms: i64,
    /// 信号所在 UTC 月份。
    pub signal_month_utc: String,
    /// 活动方向突破时 EMA144/576 关系的连续根数。
    pub prior_relation_age_bars: usize,
    /// 突破前最近 144 根中价格位于规定一侧的根数。
    pub prior_price_side_bars: usize,
    /// `prior_price_side_bars / 144`，范围 0～1。
    pub prior_price_side_ratio: f64,
    /// 活动方向突破到当前信号的 15m K 根数。
    pub bars_since_breakout: usize,
    /// 本次重新武装到回踩信号的 15m K 根数。
    pub bars_since_rearm: usize,
    /// EMA144/576 交叉前或交叉后回踩分组。
    pub cross_phase: &'static str,
    /// 信号 K 完成后的 EMA144。
    pub ema144: f64,
    /// 信号 K 完成后的 EMA576。
    pub ema576: f64,
    /// 信号 K 完成后的 ATR14。
    pub atr14: f64,
    /// 回踩极值相对 EMA144 的方向归一化 ATR。
    pub retest_extreme_to_ema144_atr: f64,
    /// 收盘相对 EMA144 的方向归一化 ATR。
    pub close_to_ema144_directional_atr: f64,
    /// L1 只确认信号，不伪造成交。
    pub execution_status: &'static str,
}

/// V2 生命周期阶段计数，不包含 outcome。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V2StageCounts {
    /// 新鲜长期状态资格完成次数。
    pub qualified_regimes: usize,
    /// 成为唯一活动方向的合格 EMA576 突破次数。
    pub activated_directions: usize,
    /// 新方向替换相反活动方向的次数。
    pub opposite_direction_replacements: usize,
    /// 活动方向内价格重新离开 EMA144 并武装回踩的次数。
    pub retest_rearms: usize,
    /// 回踩当根满足冻结守稳条件的次数。
    pub held_retests: usize,
    /// 回踩当根未满足冻结守稳条件的次数。
    pub failed_retests: usize,
}

/// V2 无标签覆盖和分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V2Summary {
    /// 全部 V2 候选数。
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
    /// V2 资格、方向替换、再武装和回踩阶段计数。
    pub stages: V2StageCounts,
}

/// V2 的完整 L1 机器产物。
#[derive(Debug, Clone, Serialize)]
pub struct V2Report {
    /// V2 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V2 规则、标签和运行隔离身份。
    pub identity: V2Identity,
    /// 冻结行情与目标输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和生命周期阶段摘要。
    pub summary: V2Summary,
    /// 三张正样本与两张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// V2 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
