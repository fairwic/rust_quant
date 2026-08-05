//! 将历史长期资格与当前 EMA576 价格 episode 分层的 L1 无结果标签扫描。
//!
//! V5 只改变生命周期：历史资格可以跨反向穿越保留，但反向两收盘确认会立即
//! 清空旧方向的活动 episode 和待回踩武装，不能把已中断突破延续到后续日期。

#[cfg(test)]
mod tests;

use super::*;

/// V5 独立候选身份，不能覆盖 V1～V4 或旧的永久活动方向研究。
pub const V5_CANDIDATE_KEY: &str =
    "market_momentum_ema576_persistent_qualification_finite_episode_ema144_buffered_hold_15m_v5";
/// V5 精确规则身份；生命周期语义变化必须新建版本。
pub const V5_RULE_VERSION: &str =
    "l1_v4_persistent_qualification_finite_episode_break2_cancel_arm_same030_hold_v5";

const V5_MIN_CANDIDATES: usize = 9_000;
const V5_MAX_CANDIDATES: usize = 74_000;
const BTC_LIFECYCLE_AUDIT_START_MS: i64 = 1_782_835_200_000;
const BTC_WRONG_SHORT_BREAKOUT_MS: i64 = 1_784_242_800_000;
const BTC_WRONG_SHORT_SIGNAL_MS: i64 = 1_784_466_900_000;

const V5_TARGETS: [V5TargetDefinition; 6] = [
    V5TargetDefinition {
        name: "nmr_2026_07_01_user_chart",
        symbol: "NMR-USDT-SWAP",
        direction: Direction::Long,
        start_ms: 1_782_835_200_000,
        end_ms: 1_782_878_400_000,
        expectation: V5TargetExpectation::MustMatch,
    },
    V5TargetDefinition {
        name: "btc_2026_07_02_user_chart",
        symbol: "BTC-USDT-SWAP",
        direction: Direction::Long,
        start_ms: 1_782_943_200_000,
        end_ms: 1_782_964_800_000,
        expectation: V5TargetExpectation::MustMatch,
    },
    V5TargetDefinition {
        name: "btc_2026_07_12_user_chart",
        symbol: "BTC-USDT-SWAP",
        direction: Direction::Long,
        start_ms: 1_783_828_800_000,
        end_ms: 1_783_850_400_000,
        expectation: V5TargetExpectation::MustMatch,
    },
    V5TargetDefinition {
        name: "algo_2026_07_19_wrong_short",
        symbol: "ALGO-USDT-SWAP",
        direction: Direction::Short,
        start_ms: 1_784_453_400_000,
        end_ms: 1_784_453_400_000,
        expectation: V5TargetExpectation::MustNotMatch,
    },
    V5TargetDefinition {
        name: "merl_2026_07_19_wrong_short",
        symbol: "MERL-USDT-SWAP",
        direction: Direction::Short,
        start_ms: 1_784_457_900_000,
        end_ms: 1_784_457_900_000,
        expectation: V5TargetExpectation::MustNotMatch,
    },
    V5TargetDefinition {
        name: "btc_2026_07_19_interrupted_short",
        symbol: "BTC-USDT-SWAP",
        direction: Direction::Short,
        start_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        end_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        expectation: V5TargetExpectation::MustNotMatch,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V5TargetExpectation {
    /// 目标窗口必须至少产生一条同方向候选。
    MustMatch,
    /// 目标时间不得产生同方向候选。
    MustNotMatch,
}

impl V5TargetExpectation {
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
struct V5TargetDefinition {
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
    expectation: V5TargetExpectation,
}

#[derive(Debug, Clone, Copy)]
struct QualificationSnapshot {
    /// 当前连续长期关系首次满足完整资格的 Unix 毫秒时间戳。
    qualified_ts: i64,
    /// 建立资格时 EMA144/576 关系已连续保持的 15m K 根数。
    relation_age_bars: usize,
    /// 建立资格时最近 144 根收盘位于 EMA576 规定一侧的根数。
    price_side_bars: usize,
}

#[derive(Debug)]
struct PersistentQualificationTracker {
    /// 该积累器负责的多头或空头长期资格。
    direction: Direction,
    /// 当前连续 EMA144/576 关系已保持的 15m K 根数。
    relation_age_bars: usize,
    /// 当前关系内最近 144 根的价格侧布尔窗口。
    price_side_window: VecDeque<bool>,
    /// `price_side_window` 中满足规定价格侧的根数。
    price_side_bars: usize,
    /// true 表示当前连续关系已经产生过资格，不再逐根改写资格日期。
    qualified_in_current_run: bool,
    /// 最近一次完成的长期资格；关系中断不会删除，数据断档才删除。
    latched: Option<QualificationSnapshot>,
}

impl PersistentQualificationTracker {
    fn new(direction: Direction) -> Self {
        Self {
            direction,
            relation_age_bars: 0,
            price_side_window: VecDeque::with_capacity(REGIME_WINDOW_BARS),
            price_side_bars: 0,
            qualified_in_current_run: false,
            latched: None,
        }
    }

    /// 更新当前连续关系；新关系可替换更旧资格，但关系反转只重置积累过程。
    fn step(&mut self, bar: ReadyBar) -> Option<QualificationSnapshot> {
        if !self.direction.regime_holds(bar) {
            self.reset_run();
            return None;
        }

        self.relation_age_bars = self.relation_age_bars.saturating_add(1);
        let on_side = price_on_regime_side(self.direction, bar);
        self.price_side_window.push_back(on_side);
        self.price_side_bars += usize::from(on_side);
        if self.price_side_window.len() > REGIME_WINDOW_BARS {
            self.price_side_bars -= usize::from(
                self.price_side_window
                    .pop_front()
                    .expect("V5 price-side window length checked"),
            );
        }

        let qualified = self.relation_age_bars >= REGIME_WINDOW_BARS
            && self.price_side_window.len() == REGIME_WINDOW_BARS
            && self.price_side_bars * 100
                >= REGIME_WINDOW_BARS.saturating_mul(MIN_PRICE_SIDE_PERCENT);
        if self.qualified_in_current_run || !qualified {
            return None;
        }

        let snapshot = QualificationSnapshot {
            qualified_ts: bar.ts,
            relation_age_bars: self.relation_age_bars,
            price_side_bars: self.price_side_bars,
        };
        self.qualified_in_current_run = true;
        self.latched = Some(snapshot);
        Some(snapshot)
    }

    fn reset_run(&mut self) {
        self.relation_age_bars = 0;
        self.price_side_window.clear();
        self.price_side_bars = 0;
        self.qualified_in_current_run = false;
    }

    fn reset_all(&mut self) {
        self.reset_run();
        self.latched = None;
    }
}

#[derive(Debug, Clone, Copy)]
struct EpisodeInvalidationCore {
    /// 被终止的旧活动方向完整信号时快照。
    invalidated_active: ActiveDirection,
    /// 触发终止的反向两收盘方向。
    confirming_direction: Direction,
    /// 反向确认完成 K 的 Unix 毫秒时间戳。
    invalidated_ts: i64,
}

#[derive(Debug, Default)]
struct V5StepResult {
    /// 本根新锁存的长期资格；同一根最多只可能有一个方向。
    qualification_latched: Option<(Direction, QualificationSnapshot)>,
    /// 本根由合格两收盘突破开启的新价格 episode。
    episode_started: Option<ActiveDirection>,
    /// 本根反向两收盘确认终止的旧 episode。
    invalidation: Option<EpisodeInvalidationCore>,
    /// 本根出现有效穿越，但对应历史资格尚不存在的方向。
    breakout_without_qualification: Option<Direction>,
    /// 本根重新离开 EMA144 后形成的待回踩武装。
    retest_rearmed: Option<ActiveDirection>,
    /// 本根触及 EMA144 并满足冻结缓冲守稳的候选。
    candidate: Option<V2CandidateCore>,
    /// 本根触及 EMA144 但越出冻结缓冲边界的方向。
    failed_retest: Option<Direction>,
}

#[derive(Debug)]
struct PersistentQualificationFiniteEpisodeMachine {
    /// 多头历史资格积累与锁存状态。
    long_qualification: PersistentQualificationTracker,
    /// 空头历史资格积累与锁存状态。
    short_qualification: PersistentQualificationTracker,
    /// 当前唯一价格 episode；历史资格不放在这里，避免旧 episode 无限续命。
    active: Option<ActiveDirection>,
}

impl PersistentQualificationFiniteEpisodeMachine {
    fn new() -> Self {
        Self {
            long_qualification: PersistentQualificationTracker::new(Direction::Long),
            short_qualification: PersistentQualificationTracker::new(Direction::Short),
            active: None,
        }
    }

    /// 先处理两收盘穿越，再处理回踩，确保反向确认 K 不会触发旧方向挂单。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V5StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V5StepResult::default();
        };
        let mut result = V5StepResult::default();
        let long_qualification = self.long_qualification.step(bar);
        let short_qualification = self.short_qualification.step(bar);
        result.qualification_latched = long_qualification
            .map(|snapshot| (Direction::Long, snapshot))
            .or_else(|| short_qualification.map(|snapshot| (Direction::Short, snapshot)));

        let breakout_direction = [Direction::Long, Direction::Short]
            .into_iter()
            .find(|direction| super::super::super::super::breakout_at(bars, idx, *direction));
        if let Some(direction) = breakout_direction {
            if let Some(active) = self.active.take() {
                if active.direction != direction {
                    result.invalidation = Some(EpisodeInvalidationCore {
                        invalidated_active: active,
                        confirming_direction: direction,
                        invalidated_ts: bar.ts,
                    });
                } else {
                    self.active = Some(active);
                }
            }

            let qualification = self.qualification(direction);
            if let Some(qualification) = qualification {
                let rearmed = departed_from_ema144(direction, bar);
                let active = ActiveDirection {
                    direction,
                    qualified_ts: qualification.qualified_ts,
                    breakout_idx: idx,
                    breakout_ts: bar.ts,
                    relation_age_bars: qualification.relation_age_bars,
                    price_side_bars: qualification.price_side_bars,
                    rearmed_idx: rearmed.then_some(idx),
                    rearmed_ts: rearmed.then_some(bar.ts),
                };
                self.active = Some(active);
                result.episode_started = Some(active);
                result.retest_rearmed = rearmed.then_some(active);
            } else {
                // 即使反方向没有资格，穿越也已经终止旧 episode，不能恢复旧活动状态。
                self.active = None;
                result.breakout_without_qualification = Some(direction);
            }
            return result;
        }

        let Some(mut active) = self.active else {
            return result;
        };
        if active.rearmed_idx.is_none() {
            if departed_from_ema144(active.direction, bar) {
                active.rearmed_idx = Some(idx);
                active.rearmed_ts = Some(bar.ts);
                result.retest_rearmed = Some(active);
            }
            self.active = Some(active);
            return result;
        }
        if !retest_zone_reached(active.direction, bar) {
            self.active = Some(active);
            return result;
        }

        if retest_holds_with_close_buffer(active.direction, bar, V3_CLOSE_HOLD_BUFFER_ATR) {
            result.candidate = Some(V2CandidateCore {
                active,
                signal_idx: idx,
                signal_bar: bar,
            });
        } else {
            result.failed_retest = Some(active.direction);
        }
        active.rearmed_idx = None;
        active.rearmed_ts = None;
        self.active = Some(active);
        result
    }

    fn qualification(&self, direction: Direction) -> Option<QualificationSnapshot> {
        match direction {
            Direction::Long => self.long_qualification.latched,
            Direction::Short => self.short_qualification.latched,
        }
    }

    fn reset_all(&mut self) {
        self.long_qualification.reset_all();
        self.short_qualification.reset_all();
        self.active = None;
    }
}

/// 连接本机 quant_core 并输出 V5 无成交后标签候选与生命周期证据。
pub async fn run_v5_l1_scan(output: &Path) -> Result<V5Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V5 finite EMA576 episode L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v5_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V5 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V5 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v5_report(data: &BacktestDataSet) -> Result<V5Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * super::super::super::super::MS_15M)
        .context("V5 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut lifecycle_events = Vec::new();
    let mut stages = V5StageCounts::default();
    let mut eligible_symbols = BTreeSet::new();
    let mut hasher = Sha256::new();
    let mut target_inputs = v5_target_input_template();

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
            .context("V5 evaluation start index overflow")?;
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
        update_v5_target_input_coverage(&pair.symbol, &bars, &mut target_inputs);
        scan_symbol_v5(
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
    let target_audits = audit_v5_targets(&candidates);
    let btc_wrong_short_lifecycle_audit = audit_btc_wrong_short_lifecycle(&lifecycle_events);
    let summary = summarize_v5(&candidates, stages);
    let decision = decide_v5(
        &summary,
        &target_audits,
        &target_inputs,
        &btc_wrong_short_lifecycle_audit,
    );

    Ok(V5Report {
        schema_version: "market_momentum_ema576_persistent_qualification_finite_episode_ema144_hold_l1_v5",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V5Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V5_CANDIDATE_KEY,
            rule_version: V5_RULE_VERSION,
            only_variable: "decompose V4 permanent directional ownership into persistent historical qualification plus one finite EMA576 price episode; an opposite two-close break immediately cancels the old episode and its arm",
            unchanged_entry_policy: "regime144, price-side80%, two-close EMA576 break, repeated EMA144 rearming, and the same +/-0.30 ATR14 touch and close-hold buffer remain frozen",
            qualification_policy: "each completed continuous 144-bar qualification is latched across later EMA relation changes; a newer completed run replaces its timestamp, while only an input or indicator gap clears it",
            episode_invalidation_policy: "the opposite two-close EMA576 break clears active context and any resting retest arm before the confirmation candle can be evaluated as an old-direction touch; no opposite qualification is required to invalidate",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V5; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn scan_symbol_v5(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V5LifecycleEvent>,
    stages: &mut V5StageCounts,
) -> Result<()> {
    let mut machine = PersistentQualificationFiniteEpisodeMachine::new();
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualification_latches += usize::from(step.qualification_latched.is_some());
        stages.episode_starts += usize::from(step.episode_started.is_some());
        stages.opposite_breakout_invalidations += usize::from(step.invalidation.is_some());
        stages.breakouts_without_qualification +=
            usize::from(step.breakout_without_qualification.is_some());
        stages.retest_rearms += usize::from(step.retest_rearmed.is_some());
        stages.failed_retests += usize::from(step.failed_retest.is_some());
        if step.candidate.is_some() {
            stages.held_retests += 1;
        }

        if symbol == "BTC-USDT-SWAP"
            && (BTC_LIFECYCLE_AUDIT_START_MS..=EVALUATION_END_MS).contains(&ts)
        {
            append_btc_lifecycle_events(symbol, ts, &step, lifecycle_events);
        }
        if let Some(core) = step.candidate {
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

fn append_btc_lifecycle_events(
    symbol: &str,
    ts: i64,
    step: &V5StepResult,
    events: &mut Vec<V5LifecycleEvent>,
) {
    if let Some((direction, qualification)) = step.qualification_latched {
        events.push(V5LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: qualification.qualified_ts,
            event: "qualification_latched",
            direction: direction.label(),
            qualification_ts_ms: Some(qualification.qualified_ts),
            episode_breakout_ts_ms: None,
            related_direction: None,
        });
    }
    if let Some(invalidation) = step.invalidation {
        events.push(V5LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: invalidation.invalidated_ts,
            event: "episode_invalidated_opposite_two_close_break",
            direction: invalidation.invalidated_active.direction.label(),
            qualification_ts_ms: Some(invalidation.invalidated_active.qualified_ts),
            episode_breakout_ts_ms: Some(invalidation.invalidated_active.breakout_ts),
            related_direction: Some(invalidation.confirming_direction.label()),
        });
    }
    if let Some(active) = step.episode_started {
        events.push(V5LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: active.breakout_ts,
            event: "episode_started",
            direction: active.direction.label(),
            qualification_ts_ms: Some(active.qualified_ts),
            episode_breakout_ts_ms: Some(active.breakout_ts),
            related_direction: None,
        });
    }
    if let Some(direction) = step.breakout_without_qualification {
        events.push(V5LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: ts,
            event: "breakout_without_historical_qualification",
            direction: direction.label(),
            qualification_ts_ms: None,
            episode_breakout_ts_ms: None,
            related_direction: None,
        });
    }
    if let Some(active) = step.retest_rearmed {
        events.push(V5LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: active
                .rearmed_ts
                .expect("V5 rearm event must retain completed-candle timestamp"),
            event: "retest_rearmed",
            direction: active.direction.label(),
            qualification_ts_ms: Some(active.qualified_ts),
            episode_breakout_ts_ms: Some(active.breakout_ts),
            related_direction: None,
        });
    }
}

fn v5_target_input_template() -> Vec<TargetInputCoverage> {
    V5_TARGETS
        .iter()
        .map(|target| TargetInputCoverage {
            name: target.name,
            symbol: target.symbol,
            expected_candles: inclusive_candle_count(target.start_ms, target.end_ms)
                .expect("V5 frozen target boundaries must align to 15m"),
            ready_candles: 0,
            ready: false,
        })
        .collect()
}

fn update_v5_target_input_coverage(
    symbol: &str,
    bars: &[PatternBar],
    coverage: &mut [TargetInputCoverage],
) {
    for (target, target_coverage) in V5_TARGETS.iter().zip(coverage.iter_mut()) {
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

fn audit_v5_targets(candidates: &[V2Candidate]) -> Vec<TargetAudit> {
    V5_TARGETS
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

fn audit_btc_wrong_short_lifecycle(events: &[V5LifecycleEvent]) -> V5BtcLifecycleAudit {
    let first_short_episode_breakout_ts_ms = events
        .iter()
        .find(|event| {
            event.event == "episode_started"
                && event.direction == "short"
                && event.ts_ms < BTC_WRONG_SHORT_SIGNAL_MS
        })
        .map(|event| event.ts_ms);
    let last_short_episode_breakout_ts_ms = events
        .iter()
        .rev()
        .find(|event| {
            event.event == "episode_started"
                && event.direction == "short"
                && event.ts_ms < BTC_WRONG_SHORT_SIGNAL_MS
        })
        .map(|event| event.ts_ms);
    let invalidated_ts_ms = events
        .iter()
        .find(|event| {
            event.event == "episode_invalidated_opposite_two_close_break"
                && event.direction == "short"
                && event.episode_breakout_ts_ms == last_short_episode_breakout_ts_ms
                && event.ts_ms < BTC_WRONG_SHORT_SIGNAL_MS
        })
        .map(|event| event.ts_ms);
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
    V5BtcLifecycleAudit {
        v3_reported_short_breakout_ts_ms: BTC_WRONG_SHORT_BREAKOUT_MS,
        first_short_episode_breakout_ts_ms,
        last_short_episode_breakout_before_old_signal_ts_ms: last_short_episode_breakout_ts_ms,
        invalidated_ts_ms,
        invalidation_confirmation_direction: invalidated_ts_ms.map(|_| "long"),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: new_short_episode_start_timestamps_ms.clone(),
        passed: invalidated_ts_ms.is_some() && new_short_episode_start_timestamps_ms.is_empty(),
    }
}

fn summarize_v5(candidates: &[V2Candidate], stages: V5StageCounts) -> V5Summary {
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
    V5Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v5_event_count(candidates),
        stages,
    }
}

fn effective_v5_event_count(candidates: &[V2Candidate]) -> usize {
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

fn decide_v5(
    summary: &V5Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
    btc_lifecycle: &V5BtcLifecycleAudit,
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
        "btc_short_episode_interrupted_before_old_signal",
        btc_lifecycle.passed,
    );
    gates.insert("all_six_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "candidate_count_between_9000_and_74000",
        (V5_MIN_CANDIDATES..=V5_MAX_CANDIDATES).contains(&summary.candidate_count),
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
            "V5 六张目标图、BTC episode 中断证据和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !definition_matches {
        (
            "rejected_definition_mismatch",
            "V5 至少一张正反样本不符合，或 BTC 旧空头 episode 未在旧信号前完成可审计中断；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V5 定义样本通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V5 冻结身份，明确区分历史资格与当前价格 episode。
#[derive(Debug, Clone, Serialize)]
pub struct V5Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V5 独立候选键。
    pub candidate_key: &'static str,
    /// V5 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V4 唯一改变的生命周期状态语义。
    pub only_variable: &'static str,
    /// V4 中保持冻结的形态阈值和回踩规则。
    pub unchanged_entry_policy: &'static str,
    /// 历史资格何时建立、替换和清空。
    pub qualification_policy: &'static str,
    /// 当前 episode 及武装被反向穿越中断的合同。
    pub episode_invalidation_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V5 无标签生命周期阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V5StageCounts {
    /// 新的连续长期关系首次锁存资格的次数。
    pub qualification_latches: usize,
    /// 有历史资格的两收盘 EMA576 穿越开启 episode 的次数。
    pub episode_starts: usize,
    /// 反向两收盘穿越终止旧 episode 与武装的次数。
    pub opposite_breakout_invalidations: usize,
    /// 两收盘穿越发生但对应历史资格尚不存在的次数。
    pub breakouts_without_qualification: usize,
    /// episode 内重新离开 EMA144 并武装回踩的次数。
    pub retest_rearms: usize,
    /// 回踩满足冻结缓冲守稳条件的次数。
    pub held_retests: usize,
    /// 回踩越出冻结缓冲边界的次数。
    pub failed_retests: usize,
}

/// V5 无标签覆盖与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V5Summary {
    /// 全部 V5 候选数。
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
    /// V5 资格、episode、中断、武装和回踩阶段计数。
    pub stages: V5StageCounts,
}

/// BTC 目标时间段内的一条信号时可见生命周期事件。
#[derive(Debug, Clone, Serialize)]
pub struct V5LifecycleEvent {
    /// OKX 永续合约标识，本报告只保留 BTC 审计窗口。
    pub symbol: String,
    /// 事件完成 K 时间，Unix 毫秒。
    pub ts_ms: i64,
    /// 资格、episode 开启、中断、无资格突破或再武装事件名。
    pub event: &'static str,
    /// 事件主要影响的 `long` 或 `short` 方向。
    pub direction: &'static str,
    /// 事件使用的历史资格时间；无资格突破时为 `None`。
    pub qualification_ts_ms: Option<i64>,
    /// 事件所属 episode 的两收盘突破时间；尚未开启时为 `None`。
    pub episode_breakout_ts_ms: Option<i64>,
    /// 中断事件的反向确认方向；其他事件为 `None`。
    pub related_direction: Option<&'static str>,
}

/// BTC 7 月 19 日旧空点的 episode 中断门禁。
#[derive(Debug, Clone, Serialize)]
pub struct V5BtcLifecycleAudit {
    /// V3 错误候选记录的原始空头突破时间，Unix 毫秒，用于对照旧报告。
    pub v3_reported_short_breakout_ts_ms: i64,
    /// 审计窗口内第一次空头 episode 的突破时间，Unix 毫秒。
    pub first_short_episode_breakout_ts_ms: Option<i64>,
    /// 旧信号前最后一次空头 episode 的突破时间；它才可能延续到旧信号。
    pub last_short_episode_breakout_before_old_signal_ts_ms: Option<i64>,
    /// 最后一次空头 episode 被反向两收盘终止的时间；未找到时为 `None`。
    pub invalidated_ts_ms: Option<i64>,
    /// 预期为 `long`；未找到中断时为 `None`。
    pub invalidation_confirmation_direction: Option<&'static str>,
    /// V3 曾错误触发的 BTC 空头信号时间，Unix 毫秒。
    pub old_wrong_signal_ts_ms: i64,
    /// 中断后、旧信号前重新开启的空头 episode；通过时必须为空。
    pub new_short_episode_start_timestamps_ms: Vec<i64>,
    /// true 表示旧 episode 已中断且旧信号前没有新的合格空头 episode。
    pub passed: bool,
}

/// V5 的完整 L1 机器产物；候选和生命周期字段都只使用当时已完成 K。
#[derive(Debug, Clone, Serialize)]
pub struct V5Report {
    /// V5 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V5 规则、标签与运行隔离身份。
    pub identity: V5Identity,
    /// 冻结行情、成员与六个目标窗口输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和生命周期阶段摘要。
    pub summary: V5Summary,
    /// 三张正样本与三张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// BTC 旧空头从 7 月 17 日到旧信号的中断门禁。
    pub btc_wrong_short_lifecycle_audit: V5BtcLifecycleAudit,
    /// BTC 7 月目标窗口内与资格、episode 和武装有关的完整信号时事件。
    pub btc_lifecycle_events: Vec<V5LifecycleEvent>,
    /// V5 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
