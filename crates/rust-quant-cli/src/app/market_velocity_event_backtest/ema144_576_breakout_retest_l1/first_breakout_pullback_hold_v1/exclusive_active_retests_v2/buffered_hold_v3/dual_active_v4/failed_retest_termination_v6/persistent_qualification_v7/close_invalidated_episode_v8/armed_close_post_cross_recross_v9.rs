//! 用武装回踩收盘失败或交叉后反向 EMA576 两收盘确认终止 episode 的 L1 扫描。
//!
//! V9 保留 V8 的历史资格、候选触碰和影线消费规则，只收窄 episode 中断状态。

pub mod pre_cross_breakout_v10;
mod signal_cross_deadline;
#[cfg(test)]
mod tests;

use super::*;
use signal_cross_deadline::{V9SignalCrossDeadline, V9SignalCrossDeadlineEvent};

/// V9 独立候选身份，不能覆盖 V8 或任何旧 episode 研究。
pub const V9_CANDIDATE_KEY: &str =
    "market_momentum_ema576_persistent_qualification_armed_close_post_cross_recross_15m_v9";
/// V9 精确规则身份；episode 中断状态变化必须新建版本。
pub const V9_RULE_VERSION: &str =
    "l1_v8_armed_close_failure_or_post_cross_opposite_break2_invalidates_v9";

const V9_MIN_CANDIDATES: usize = 5_000;
const V9_MAX_CANDIDATES: usize = 80_000;

#[derive(Debug, Clone, Copy)]
struct V9ActiveDirection {
    /// V7/V8 冻结的资格、突破与再武装上下文。
    core: ActiveDirection,
    /// 当前 episode 是否曾完成 EMA144/576 的方向交叉；一旦成立不回退。
    post_cross_seen: bool,
}

#[derive(Debug, Clone, Copy)]
struct V9ActivityCore {
    /// 事件发生时的 episode 快照。
    active: V9ActiveDirection,
    /// 事件完成 K 时间，Unix 毫秒。
    ts: i64,
}

#[derive(Debug, Clone, Copy)]
struct V9InvalidationCore {
    /// 被终止的 episode 快照。
    active: V9ActiveDirection,
    /// 中断完成 K 时间，Unix 毫秒。
    invalidated_ts: i64,
    /// 武装收盘失败或交叉后反向 EMA576 两收盘确认。
    reason: &'static str,
    /// 中断收盘相对 EMA144 的方向归一化 ATR。
    close_to_ema144_directional_atr: f64,
    /// 反向 EMA576 确认方向；武装收盘失败时为空。
    confirmation_direction: Option<Direction>,
}

#[derive(Debug, Default)]
struct V9StepResult {
    /// 本根新锁存的历史资格数。
    qualification_latches: usize,
    /// 本根由新同方向 EMA576 突破建立或替换的 episode。
    episode_started: Option<V9ActiveDirection>,
    /// 本根首次记录完成 EMA144/576 方向交叉的 episode。
    post_cross_latches: Vec<V9ActivityCore>,
    /// 本根重新离开 EMA144 后的武装事件。
    retest_rearms: Vec<V9ActivityCore>,
    /// 本根满足冻结极值与收盘守稳的候选。
    candidates: Vec<V2CandidateCore>,
    /// 本根成功回踩事件，用于输出再武装到消费的日期链。
    held_retests: Vec<V9ActivityCore>,
    /// 本根终止的 episode。
    invalidations: Vec<V9InvalidationCore>,
    /// 本根只有影线越界、收盘守回并消费武装的事件。
    wick_only_failed_retests: Vec<V9ActivityCore>,
    /// 本根首次由交叉前交易信号启动的 EMA144/576 确认计时。
    signal_cross_deadline_starts: Vec<V9SignalCrossDeadlineEvent>,
    /// 本根在固定期限内完成的 EMA144/576 方向交叉确认。
    signal_cross_deadline_confirmations: Vec<V9SignalCrossDeadlineEvent>,
    /// 本根到期仍未完成方向交叉并清空旧资格的事件。
    signal_cross_deadline_timeouts: Vec<V9SignalCrossDeadlineEvent>,
}

#[derive(Debug)]
struct V9Machine {
    /// 多头历史资格积累与锁存状态。
    long_qualification: V7QualificationTracker,
    /// 空头历史资格积累与锁存状态。
    short_qualification: V7QualificationTracker,
    /// 当前独立多头 episode。
    long_active: Option<V9ActiveDirection>,
    /// 当前独立空头 episode。
    short_active: Option<V9ActiveDirection>,
    /// true 时只允许价格在 EMA144/576 方向交叉前建立新 episode。
    require_pre_cross_activation: bool,
    /// 首个交叉前信号后的固定确认根数；None 保持 V9/V10 原行为。
    post_signal_cross_timeout_bars: Option<usize>,
    /// 多头旧资格链首次交叉前信号启动的单次计时。
    long_signal_cross_deadline: Option<V9SignalCrossDeadline>,
    /// 空头旧资格链首次交叉前信号启动的单次计时。
    short_signal_cross_deadline: Option<V9SignalCrossDeadline>,
}

impl V9Machine {
    /// 供 V10 复用同一生命周期，只收紧新 episode 的建立时序。
    fn with_pre_cross_activation(require_pre_cross_activation: bool) -> Self {
        Self::with_lifecycle_policy(require_pre_cross_activation, None)
    }

    /// 供后续研究版本复用冻结生命周期，并可单独启用首信号交叉确认超时。
    fn with_lifecycle_policy(
        require_pre_cross_activation: bool,
        post_signal_cross_timeout_bars: Option<usize>,
    ) -> Self {
        Self {
            long_qualification: V7QualificationTracker::new(Direction::Long),
            short_qualification: V7QualificationTracker::new(Direction::Short),
            long_active: None,
            short_active: None,
            require_pre_cross_activation,
            post_signal_cross_timeout_bars,
            long_signal_cross_deadline: None,
            short_signal_cross_deadline: None,
        }
    }

    /// 先锁存交叉并处理中断，再建立新 episode，最后才处理普通再武装与回踩。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V9StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V9StepResult::default();
        };
        let mut result = V9StepResult {
            qualification_latches: usize::from(self.long_qualification.step(bar))
                + usize::from(self.short_qualification.step(bar)),
            ..V9StepResult::default()
        };

        for direction in [Direction::Long, Direction::Short] {
            let Some(mut active) = *self.active_mut(direction) else {
                continue;
            };
            if !active.post_cross_seen && post_cross_holds(direction, bar) {
                active.post_cross_seen = true;
                result
                    .post_cross_latches
                    .push(V9ActivityCore { active, ts: bar.ts });
                *self.active_mut(direction) = Some(active);
            }
        }

        // 首个信号计时属于旧资格链，不属于可被同方向新突破替换的 active episode。
        // 当前到期 K 先获得一次完成均线交叉的机会，之后才执行超时失效。
        for direction in [Direction::Long, Direction::Short] {
            if post_cross_holds(direction, bar) {
                if let Some(deadline) = self.signal_cross_deadline_mut(direction).take() {
                    result
                        .signal_cross_deadline_confirmations
                        .push(V9SignalCrossDeadlineEvent {
                            deadline,
                            ts: bar.ts,
                        });
                }
                continue;
            }
            let Some(timeout_bars) = self.post_signal_cross_timeout_bars else {
                continue;
            };
            let expired = self
                .signal_cross_deadline(direction)
                .is_some_and(|deadline| {
                    idx >= deadline.first_signal_idx.saturating_add(timeout_bars)
                });
            if !expired {
                continue;
            }
            let deadline = self
                .signal_cross_deadline_mut(direction)
                .take()
                .expect("V9 signal cross deadline expiry checked");
            *self.active_mut(direction) = None;
            self.qualification_mut(direction).reset_all();
            result
                .signal_cross_deadline_timeouts
                .push(V9SignalCrossDeadlineEvent {
                    deadline,
                    ts: bar.ts,
                });
        }

        let breakout_direction = [Direction::Long, Direction::Short]
            .into_iter()
            .find(|direction| effective_breakout_at(bars, idx, *direction));
        if let Some(direction) = breakout_direction {
            let opposite = opposite_direction(direction);
            if self
                .active_mut(opposite)
                .is_some_and(|active| active.post_cross_seen)
            {
                let active = self
                    .active_mut(opposite)
                    .take()
                    .expect("V9 opposite active checked");
                result.invalidations.push(V9InvalidationCore {
                    active,
                    invalidated_ts: bar.ts,
                    reason: "post_cross_opposite_two_close_ema576_breakout",
                    close_to_ema144_directional_atr: close_hold_atr(opposite, bar),
                    confirmation_direction: Some(direction),
                });
            }
            let activation_allowed =
                !self.require_pre_cross_activation || !post_cross_holds(direction, bar);
            if let Some(qualification) =
                self.qualification(direction).filter(|_| activation_allowed)
            {
                let rearmed = departed_from_ema144(direction, bar);
                let core = ActiveDirection {
                    direction,
                    qualified_ts: qualification.qualified_ts,
                    breakout_idx: idx,
                    breakout_ts: bar.ts,
                    relation_age_bars: qualification.relation_age_bars,
                    price_side_bars: qualification.price_side_bars,
                    rearmed_idx: rearmed.then_some(idx),
                    rearmed_ts: rearmed.then_some(bar.ts),
                };
                let active = V9ActiveDirection {
                    core,
                    post_cross_seen: post_cross_holds(direction, bar),
                };
                *self.active_mut(direction) = Some(active);
                result.episode_started = Some(active);
                if rearmed {
                    result
                        .retest_rearms
                        .push(V9ActivityCore { active, ts: bar.ts });
                }
                // 突破确认 K 只更新上下文，不能同时解释成 EMA144 回踩。
                return result;
            }
            if !result.invalidations.is_empty() {
                return result;
            }
        }

        for direction in [Direction::Long, Direction::Short] {
            let Some(mut active) = *self.active_mut(direction) else {
                continue;
            };
            if active.core.rearmed_idx.is_none() {
                if departed_from_ema144(direction, bar) {
                    active.core.rearmed_idx = Some(idx);
                    active.core.rearmed_ts = Some(bar.ts);
                    result
                        .retest_rearms
                        .push(V9ActivityCore { active, ts: bar.ts });
                }
                *self.active_mut(direction) = Some(active);
                continue;
            }
            if !retest_zone_reached(direction, bar) {
                *self.active_mut(direction) = Some(active);
                continue;
            }

            let close_atr = close_hold_atr(direction, bar);
            if close_atr < -V3_CLOSE_HOLD_BUFFER_ATR {
                result.invalidations.push(V9InvalidationCore {
                    active,
                    invalidated_ts: bar.ts,
                    reason: "armed_retest_close_beyond_ema144",
                    close_to_ema144_directional_atr: close_atr,
                    confirmation_direction: None,
                });
                *self.active_mut(direction) = None;
                continue;
            }

            let activity = V9ActivityCore { active, ts: bar.ts };
            if retest_holds_with_close_buffer(direction, bar, V3_CLOSE_HOLD_BUFFER_ATR) {
                result.candidates.push(V2CandidateCore {
                    active: active.core,
                    signal_idx: idx,
                    signal_bar: bar,
                });
                result.held_retests.push(activity);
                if !active.post_cross_seen
                    && self.post_signal_cross_timeout_bars.is_some()
                    && self.signal_cross_deadline(direction).is_none()
                {
                    let deadline = V9SignalCrossDeadline {
                        origin_active: active,
                        first_signal_idx: idx,
                        first_signal_ts: bar.ts,
                    };
                    *self.signal_cross_deadline_mut(direction) = Some(deadline);
                    result
                        .signal_cross_deadline_starts
                        .push(V9SignalCrossDeadlineEvent {
                            deadline,
                            ts: bar.ts,
                        });
                }
            } else {
                // 收盘仍守稳时只可能是影线失败；消费武装但保留 episode。
                result.wick_only_failed_retests.push(activity);
            }
            active.core.rearmed_idx = None;
            active.core.rearmed_ts = None;
            *self.active_mut(direction) = Some(active);
        }
        result
    }

    fn qualification(&self, direction: Direction) -> Option<V7QualificationSnapshot> {
        match direction {
            Direction::Long => self.long_qualification.latched,
            Direction::Short => self.short_qualification.latched,
        }
    }

    fn active_mut(&mut self, direction: Direction) -> &mut Option<V9ActiveDirection> {
        match direction {
            Direction::Long => &mut self.long_active,
            Direction::Short => &mut self.short_active,
        }
    }

    /// 获取方向资格积累器；超时时必须同时清空锁存快照和当前积累段。
    fn qualification_mut(&mut self, direction: Direction) -> &mut V7QualificationTracker {
        match direction {
            Direction::Long => &mut self.long_qualification,
            Direction::Short => &mut self.short_qualification,
        }
    }

    /// 读取方向级计时快照；计时不挂在可替换 active 上，防止新突破续期。
    fn signal_cross_deadline(&self, direction: Direction) -> Option<V9SignalCrossDeadline> {
        match direction {
            Direction::Long => self.long_signal_cross_deadline,
            Direction::Short => self.short_signal_cross_deadline,
        }
    }

    /// 更新方向级计时快照，供首次信号启动、期限内确认和超时消费共用。
    fn signal_cross_deadline_mut(
        &mut self,
        direction: Direction,
    ) -> &mut Option<V9SignalCrossDeadline> {
        match direction {
            Direction::Long => &mut self.long_signal_cross_deadline,
            Direction::Short => &mut self.short_signal_cross_deadline,
        }
    }

    fn reset_all(&mut self) {
        self.long_qualification.reset_all();
        self.short_qualification.reset_all();
        self.long_active = None;
        self.short_active = None;
        self.long_signal_cross_deadline = None;
        self.short_signal_cross_deadline = None;
    }
}

fn effective_breakout_at(bars: &[PatternBar], idx: usize, direction: Direction) -> bool {
    super::super::super::super::super::super::super::super::breakout_at(bars, idx, direction)
}

fn opposite_direction(direction: Direction) -> Direction {
    match direction {
        Direction::Long => Direction::Short,
        Direction::Short => Direction::Long,
    }
}

fn post_cross_holds(direction: Direction, bar: ReadyBar) -> bool {
    match direction {
        Direction::Long => bar.ema144 > bar.ema576,
        Direction::Short => bar.ema144 < bar.ema576,
    }
}

/// 连接本机 quant_core 并输出 V9 无成交后标签候选与完整 BTC 日期链。
pub async fn run_v9_l1_scan(output: &Path) -> Result<V9Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V9 armed-close/post-cross-recross L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v9_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V9 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V9 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v9_report(data: &BacktestDataSet) -> Result<V9Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(
            REQUIRED_PRE_EVALUATION_BARS as i64
                * super::super::super::super::super::super::super::super::MS_15M,
        )
        .context("V9 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut lifecycle_events = Vec::new();
    let mut stages = V9StageCounts::default();
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
            .context("V9 evaluation start index overflow")?;
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
        scan_symbol_v9(
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
    let btc_wrong_short_lifecycle_audit = audit_btc_v9_lifecycle(&lifecycle_events);
    let btc_interrupted_by_july18 = btc_wrong_short_lifecycle_audit
        .invalidated_ts_ms
        .is_some_and(|ts| ts <= BTC_JULY18_END_MS)
        && btc_wrong_short_lifecycle_audit
            .new_short_episode_start_timestamps_ms
            .is_empty();
    let summary = summarize_v9(&candidates, stages);
    let decision = decide_v9(
        &summary,
        &target_audits,
        &target_inputs,
        &btc_wrong_short_lifecycle_audit,
        btc_interrupted_by_july18,
    );

    Ok(V9Report {
        schema_version: "market_momentum_ema576_persistent_qualification_armed_close_post_cross_recross_l1_v9",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V9Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V9_CANDIDATE_KEY,
            rule_version: V9_RULE_VERSION,
            only_variable: "replace V8 any-bar EMA144 close invalidation with armed-retest close failure or, after EMA144/576 has crossed in episode direction, an opposite two-close EMA576 breakout",
            unchanged_entry_policy: "V8 persistent historical qualification, two-close same-direction EMA576 breakout, independent directional episodes, rearming, wick-only arm consumption, and candidate extreme plus close hold rules remain frozen",
            invalidation_policy: "an armed retest close beyond directional EMA144 0.30 ATR14 terminates; an unarmed EMA144 close does not; after post-cross has ever been seen, the opposite two-close EMA576 breakout terminates regardless of arm",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V9; not registered in paper, readonly shadow, live worker, compose, or production presets",
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
        btc_interrupted_by_july18,
        btc_lifecycle_events: lifecycle_events,
        decision,
        candidates,
    })
}

fn scan_symbol_v9(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V9LifecycleEvent>,
    stages: &mut V9StageCounts,
) -> Result<()> {
    scan_symbol_with_v9_machine(
        symbol,
        bars,
        start_idx,
        end_idx,
        candidates,
        lifecycle_events,
        stages,
        false,
    )
}

/// 让 V9/V10 共用完全相同的候选和中断实现，唯一差异由建立时序开关表达。
fn scan_symbol_with_v9_machine(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V9LifecycleEvent>,
    stages: &mut V9StageCounts,
    require_pre_cross_activation: bool,
) -> Result<()> {
    let mut machine = V9Machine::with_pre_cross_activation(require_pre_cross_activation);
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualification_latches += step.qualification_latches;
        stages.episode_starts += usize::from(step.episode_started.is_some());
        stages.post_cross_latches += step.post_cross_latches.len();
        stages.retest_rearms += step.retest_rearms.len();
        stages.held_retests += step.held_retests.len();
        stages.armed_close_invalidations += step
            .invalidations
            .iter()
            .filter(|event| event.reason == "armed_retest_close_beyond_ema144")
            .count();
        stages.post_cross_recross_invalidations += step
            .invalidations
            .iter()
            .filter(|event| event.reason == "post_cross_opposite_two_close_ema576_breakout")
            .count();
        stages.wick_only_failed_retests += step.wick_only_failed_retests.len();
        if symbol == "BTC-USDT-SWAP" && (BTC_JULY_AUDIT_START_MS..=EVALUATION_END_MS).contains(&ts)
        {
            append_btc_v9_events(symbol, &step, lifecycle_events);
        }
        for core in step.candidates {
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

fn append_btc_v9_events(symbol: &str, step: &V9StepResult, events: &mut Vec<V9LifecycleEvent>) {
    if let Some(active) = step.episode_started {
        events.push(lifecycle_event(
            symbol,
            active,
            active.core.breakout_ts,
            "episode_started",
            None,
            None,
        ));
    }
    for activity in &step.post_cross_latches {
        events.push(lifecycle_event(
            symbol,
            activity.active,
            activity.ts,
            "post_cross_seen",
            None,
            None,
        ));
    }
    for activity in &step.retest_rearms {
        events.push(lifecycle_event(
            symbol,
            activity.active,
            activity.ts,
            "retest_rearmed",
            None,
            None,
        ));
    }
    for activity in &step.held_retests {
        events.push(lifecycle_event(
            symbol,
            activity.active,
            activity.ts,
            "held_retest_arm_consumed",
            None,
            None,
        ));
    }
    for activity in &step.wick_only_failed_retests {
        events.push(lifecycle_event(
            symbol,
            activity.active,
            activity.ts,
            "wick_only_failed_retest_arm_consumed",
            None,
            None,
        ));
    }
    for invalidation in &step.invalidations {
        events.push(lifecycle_event(
            symbol,
            invalidation.active,
            invalidation.invalidated_ts,
            invalidation.reason,
            Some(invalidation.close_to_ema144_directional_atr),
            invalidation.confirmation_direction,
        ));
    }
}

fn lifecycle_event(
    symbol: &str,
    active: V9ActiveDirection,
    ts_ms: i64,
    event: &'static str,
    close_to_ema144_directional_atr: Option<f64>,
    confirmation_direction: Option<Direction>,
) -> V9LifecycleEvent {
    V9LifecycleEvent {
        symbol: symbol.to_owned(),
        ts_ms,
        event,
        direction: active.core.direction.label(),
        qualification_ts_ms: active.core.qualified_ts,
        episode_breakout_ts_ms: active.core.breakout_ts,
        rearmed_ts_ms: active.core.rearmed_ts,
        post_cross_seen: active.post_cross_seen,
        close_to_ema144_directional_atr,
        confirmation_direction: confirmation_direction.map(Direction::label),
    }
}

fn audit_btc_v9_lifecycle(events: &[V9LifecycleEvent]) -> V9BtcLifecycleAudit {
    let last_breakout = events.iter().rev().find(|event| {
        event.event == "episode_started"
            && event.direction == "short"
            && event.ts_ms < BTC_WRONG_SHORT_SIGNAL_MS
    });
    let last_breakout_ts = last_breakout.map(|event| event.episode_breakout_ts_ms);
    let episode_events = events
        .iter()
        .filter(|event| {
            event.direction == "short"
                && Some(event.episode_breakout_ts_ms) == last_breakout_ts
                && event.ts_ms <= BTC_WRONG_SHORT_SIGNAL_MS
        })
        .collect::<Vec<_>>();
    let invalidation = episode_events
        .iter()
        .find(|event| event.event == "post_cross_opposite_two_close_ema576_breakout");
    let invalidated_ts = invalidation.map(|event| event.ts_ms);
    let new_short_episode_start_timestamps_ms = invalidated_ts
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
    V9BtcLifecycleAudit {
        historical_short_qualification_ts_ms: last_breakout.map(|event| event.qualification_ts_ms),
        last_short_episode_breakout_before_old_signal_ts_ms: last_breakout_ts,
        post_cross_seen_ts_ms: episode_events
            .iter()
            .find(|event| event.event == "post_cross_seen")
            .map(|event| event.ts_ms)
            .or_else(|| {
                last_breakout
                    .filter(|event| event.post_cross_seen)
                    .map(|event| event.ts_ms)
            }),
        rearm_timestamps_ms: episode_events
            .iter()
            .filter(|event| event.event == "retest_rearmed")
            .map(|event| event.ts_ms)
            .collect(),
        held_retest_timestamps_ms: episode_events
            .iter()
            .filter(|event| event.event == "held_retest_arm_consumed")
            .map(|event| event.ts_ms)
            .collect(),
        wick_only_failed_retest_timestamps_ms: episode_events
            .iter()
            .filter(|event| event.event == "wick_only_failed_retest_arm_consumed")
            .map(|event| event.ts_ms)
            .collect(),
        invalidated_ts_ms: invalidated_ts,
        invalidation_reason: invalidation.map(|event| event.event),
        invalidation_confirmation_direction: invalidation
            .and_then(|event| event.confirmation_direction),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: new_short_episode_start_timestamps_ms.clone(),
        passed: invalidated_ts.is_some() && new_short_episode_start_timestamps_ms.is_empty(),
    }
}

fn summarize_v9(candidates: &[V2Candidate], stages: V9StageCounts) -> V9Summary {
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
    V9Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v6_event_count(candidates),
        stages,
    }
}

fn decide_v9(
    summary: &V9Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
    btc_lifecycle: &V9BtcLifecycleAudit,
    btc_interrupted_by_july18: bool,
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
        "btc_post_cross_recross_invalidation_exists",
        btc_lifecycle.passed,
    );
    gates.insert(
        "btc_short_interrupted_by_july18_end",
        btc_interrupted_by_july18,
    );
    gates.insert("all_six_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "candidate_count_between_5000_and_80000",
        (V9_MIN_CANDIDATES..=V9_MAX_CANDIDATES).contains(&summary.candidate_count),
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
    let definition_matches = positive_targets_match
        && negative_targets_clear
        && btc_lifecycle.passed
        && btc_interrupted_by_july18;
    let (status, reason) = if all_pass {
        (
            "coverage_pass_ready_for_l2_prereg",
            "V9 六张目标图、BTC 交叉后反向 EMA576 中断日期链和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !definition_matches {
        (
            "rejected_definition_mismatch",
            "V9 至少一张正反样本不符合，或 BTC 空头没有在 7 月 18 日结束前被交叉后反向 EMA576 两收盘中断；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V9 定义样本通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V9 冻结身份，明确局部回踩失败与趋势反向确认的不同中断职责。
#[derive(Debug, Clone, Serialize)]
pub struct V9Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V9 独立候选键。
    pub candidate_key: &'static str,
    /// V9 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V8 唯一改变的 episode 中断状态变量。
    pub only_variable: &'static str,
    /// V8 中保持冻结的资格、突破、武装和候选规则。
    pub unchanged_entry_policy: &'static str,
    /// 武装收盘失败与交叉后反向 EMA576 确认合同。
    pub invalidation_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V9 无标签生命周期阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V9StageCounts {
    /// 新的连续长期关系首次锁存资格的次数。
    pub qualification_latches: usize,
    /// 有历史资格的新 EMA576 突破建立 episode 的次数。
    pub episode_starts: usize,
    /// episode 首次完成 EMA144/576 方向交叉的次数。
    pub post_cross_latches: usize,
    /// episode 内重新离开 EMA144 并武装的次数。
    pub retest_rearms: usize,
    /// 回踩极值与收盘都满足冻结守稳条件的次数。
    pub held_retests: usize,
    /// 已武装回踩因完成 K 收盘越界而终止 episode 的次数。
    pub armed_close_invalidations: usize,
    /// 交叉后因反向两收盘 EMA576 确认而终止 episode 的次数。
    pub post_cross_recross_invalidations: usize,
    /// 只有影线越界、收盘守回并消费武装的次数。
    pub wick_only_failed_retests: usize,
}

/// V9 无标签覆盖与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V9Summary {
    /// 全部 V9 候选数。
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
    /// V9 资格、episode、交叉、武装、候选和中断阶段计数。
    pub stages: V9StageCounts,
}

/// BTC 七月窗口内的 V9 episode 日期链事件。
#[derive(Debug, Clone, Serialize)]
pub struct V9LifecycleEvent {
    /// 事件所属交易对。
    pub symbol: String,
    /// 事件完成 K 的 Unix 毫秒时间戳。
    pub ts_ms: i64,
    /// episode 建立、交叉、再武装、消费或中断事件名。
    pub event: &'static str,
    /// 事件所属多头或空头方向。
    pub direction: &'static str,
    /// 当前 episode 复用的历史资格时间。
    pub qualification_ts_ms: i64,
    /// 当前 episode 的同方向两收盘 EMA576 突破时间。
    pub episode_breakout_ts_ms: i64,
    /// 当前事件快照中的最近再武装时间。
    pub rearmed_ts_ms: Option<i64>,
    /// 当前 episode 是否已经完成方向 EMA144/576 交叉。
    pub post_cross_seen: bool,
    /// 中断收盘相对 EMA144 的方向归一化 ATR。
    pub close_to_ema144_directional_atr: Option<f64>,
    /// 交叉后反向 EMA576 两收盘确认方向。
    pub confirmation_direction: Option<&'static str>,
}

/// BTC 旧空头从历史资格到中断、再到禁止延续的完整无标签审计。
#[derive(Debug, Clone, Serialize)]
pub struct V9BtcLifecycleAudit {
    /// 最近空头 episode 复用的历史长期资格时间。
    pub historical_short_qualification_ts_ms: Option<i64>,
    /// 旧错误信号前最后一次空头 episode 的突破时间。
    pub last_short_episode_breakout_before_old_signal_ts_ms: Option<i64>,
    /// 该 episode 首次记录完成 EMA144/576 空头方向交叉的时间。
    pub post_cross_seen_ts_ms: Option<i64>,
    /// 该 episode 的全部再武装时间。
    pub rearm_timestamps_ms: Vec<i64>,
    /// 该 episode 的全部成功回踩并消费武装时间。
    pub held_retest_timestamps_ms: Vec<i64>,
    /// 该 episode 的影线失败并消费武装时间。
    pub wick_only_failed_retest_timestamps_ms: Vec<i64>,
    /// 交叉后反向 EMA576 两收盘中断时间。
    pub invalidated_ts_ms: Option<i64>,
    /// 中断原因；通过时必须是交叉后反向 EMA576 两收盘确认。
    pub invalidation_reason: Option<&'static str>,
    /// 中断确认的反向方向。
    pub invalidation_confirmation_direction: Option<&'static str>,
    /// V4/V7 曾保留的 7 月 19 日旧空头信号时间。
    pub old_wrong_signal_ts_ms: i64,
    /// 中断后到旧信号前重新建立的空头 episode；通过时必须为空。
    pub new_short_episode_start_timestamps_ms: Vec<i64>,
    /// true 表示存在正确中断且之后没有新空头 episode。
    pub passed: bool,
}

/// V9 完整 L1 机器产物；不包含成交、退出或收益结果。
#[derive(Debug, Clone, Serialize)]
pub struct V9Report {
    /// V9 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V9 规则、标签与运行隔离身份。
    pub identity: V9Identity,
    /// 冻结行情、成员与六个目标窗口输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和生命周期阶段摘要。
    pub summary: V9Summary,
    /// 三张正样本与三张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// BTC 旧空头完整日期链门禁。
    pub btc_wrong_short_lifecycle_audit: V9BtcLifecycleAudit,
    /// true 表示 BTC 旧空头最迟在北京时间 7 月 18 日结束前已中断。
    pub btc_interrupted_by_july18: bool,
    /// BTC 七月窗口的 episode 生命周期事件。
    pub btc_lifecycle_events: Vec<V9LifecycleEvent>,
    /// V9 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
