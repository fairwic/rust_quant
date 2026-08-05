//! 在 V6 失败回踩终止规则上锁存历史长期资格的 L1 无结果标签扫描。
//!
//! 失败回踩仍删除当前 episode；历史资格独立保留，后续必须出现新的同方向
//! 两收盘 EMA576 突破，才允许建立新 episode。

pub mod close_invalidated_episode_v8;
#[cfg(test)]
mod tests;

use super::*;

/// V7 独立候选身份，不能覆盖 V6 或旧永久活动方向版本。
pub const V7_CANDIDATE_KEY: &str =
    "market_momentum_ema576_persistent_qualification_failed_retest_termination_15m_v7";
/// V7 精确规则身份；历史资格记忆变化必须新建版本。
pub const V7_RULE_VERSION: &str =
    "l1_v6_persistent_qualification_dual_episode_failed_retest_terminates_v7";

const V7_MIN_CANDIDATES: usize = 5_000;
const V7_MAX_CANDIDATES: usize = 50_000;
const BTC_JULY18_END_MS: i64 = 1_784_389_500_000;

#[derive(Debug, Clone, Copy)]
struct V7QualificationSnapshot {
    /// 当前连续长期关系首次满足完整资格的 Unix 毫秒时间戳。
    qualified_ts: i64,
    /// 建立资格时 EMA144/576 关系已连续保持的 15m K 根数。
    relation_age_bars: usize,
    /// 建立资格时最近 144 根收盘位于 EMA576 规定一侧的根数。
    price_side_bars: usize,
}

#[derive(Debug)]
struct V7QualificationTracker {
    /// 该积累器负责的多头或空头历史资格。
    direction: Direction,
    /// 当前连续 EMA144/576 关系已保持的 15m K 根数。
    relation_age_bars: usize,
    /// 当前关系内最近 144 根的价格侧窗口。
    price_side_window: VecDeque<bool>,
    /// 最近 144 根中满足规定价格侧的根数。
    price_side_bars: usize,
    /// true 表示当前连续关系已经写入过一次资格快照。
    qualified_in_current_run: bool,
    /// 最近一次完成的历史资格；关系反转与 episode 结束都不会删除。
    latched: Option<V7QualificationSnapshot>,
}

impl V7QualificationTracker {
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

    /// 每段连续长期关系只在首次完整满足 144/80% 合同时更新历史资格。
    fn step(&mut self, bar: ReadyBar) -> bool {
        if !self.direction.regime_holds(bar) {
            self.reset_run();
            return false;
        }
        self.relation_age_bars = self.relation_age_bars.saturating_add(1);
        let on_side = price_on_regime_side(self.direction, bar);
        self.price_side_window.push_back(on_side);
        self.price_side_bars += usize::from(on_side);
        if self.price_side_window.len() > REGIME_WINDOW_BARS {
            self.price_side_bars -= usize::from(
                self.price_side_window
                    .pop_front()
                    .expect("V7 price-side window length checked"),
            );
        }
        let qualified = self.relation_age_bars >= REGIME_WINDOW_BARS
            && self.price_side_window.len() == REGIME_WINDOW_BARS
            && self.price_side_bars * 100
                >= REGIME_WINDOW_BARS.saturating_mul(MIN_PRICE_SIDE_PERCENT);
        if self.qualified_in_current_run || !qualified {
            return false;
        }
        self.qualified_in_current_run = true;
        self.latched = Some(V7QualificationSnapshot {
            qualified_ts: bar.ts,
            relation_age_bars: self.relation_age_bars,
            price_side_bars: self.price_side_bars,
        });
        true
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

#[derive(Debug)]
struct V7Machine {
    /// 多头历史资格积累与锁存状态。
    long_qualification: V7QualificationTracker,
    /// 空头历史资格积累与锁存状态。
    short_qualification: V7QualificationTracker,
    /// 当前独立多头 episode。
    long_active: Option<ActiveDirection>,
    /// 当前独立空头 episode。
    short_active: Option<ActiveDirection>,
}

impl V7Machine {
    fn new() -> Self {
        Self {
            long_qualification: V7QualificationTracker::new(Direction::Long),
            short_qualification: V7QualificationTracker::new(Direction::Short),
            long_active: None,
            short_active: None,
        }
    }

    /// 历史资格先更新；合格新突破优先建立 episode，其他 K 才处理 V6 回踩规则。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V6StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V6StepResult::default();
        };
        let mut result = V6StepResult {
            qualified_regimes: usize::from(self.long_qualification.step(bar))
                + usize::from(self.short_qualification.step(bar)),
            ..V6StepResult::default()
        };
        let breakout_direction =
            [Direction::Long, Direction::Short]
                .into_iter()
                .find(|direction| {
                    super::super::super::super::super::super::breakout_at(bars, idx, *direction)
                });
        if let Some(direction) = breakout_direction {
            if let Some(qualification) = self.qualification(direction) {
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
                *self.active_mut(direction) = Some(active);
                result.episode_started = Some(active);
                result.retest_rearms = usize::from(rearmed);
                return result;
            }
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

    fn qualification(&self, direction: Direction) -> Option<V7QualificationSnapshot> {
        match direction {
            Direction::Long => self.long_qualification.latched,
            Direction::Short => self.short_qualification.latched,
        }
    }

    fn active_mut(&mut self, direction: Direction) -> &mut Option<ActiveDirection> {
        match direction {
            Direction::Long => &mut self.long_active,
            Direction::Short => &mut self.short_active,
        }
    }

    fn reset_all(&mut self) {
        self.long_qualification.reset_all();
        self.short_qualification.reset_all();
        self.long_active = None;
        self.short_active = None;
    }
}

/// 连接本机 quant_core 并输出 V7 无成交后标签候选与生命周期证据。
pub async fn run_v7_l1_scan(output: &Path) -> Result<V7Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V7 persistent qualification L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v7_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V7 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V7 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v7_report(data: &BacktestDataSet) -> Result<V7Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(
            REQUIRED_PRE_EVALUATION_BARS as i64 * super::super::super::super::super::super::MS_15M,
        )
        .context("V7 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut lifecycle_events = Vec::new();
    let mut stages = V7StageCounts::default();
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
            .context("V7 evaluation start index overflow")?;
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
        scan_symbol_v7(
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
    let btc_interrupted_by_july18 = btc_wrong_short_lifecycle_audit
        .invalidated_ts_ms
        .is_some_and(|ts| ts <= BTC_JULY18_END_MS)
        && btc_wrong_short_lifecycle_audit
            .new_short_episode_start_timestamps_ms
            .is_empty();
    let summary = summarize_v7(&candidates, stages);
    let decision = decide_v7(
        &summary,
        &target_audits,
        &target_inputs,
        &btc_wrong_short_lifecycle_audit,
        btc_interrupted_by_july18,
    );

    Ok(V7Report {
        schema_version: "market_momentum_ema576_persistent_qualification_failed_retest_termination_l1_v7",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V7Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V7_CANDIDATE_KEY,
            rule_version: V7_RULE_VERSION,
            only_variable: "replace V6 post-activation qualification reset with independent persistent long and short qualification latches; a failed retest still terminates only the current episode",
            unchanged_entry_policy: "V6 two-close EMA576 breakout, independent directional episodes, repeated rearming after successful retests, failed EMA144 retest termination, and the same +/-0.30 ATR14 buffer remain frozen",
            qualification_policy: "a completed regime144 plus price-side80% qualification persists across EMA relation changes, activations, and failed episodes; only an input gap clears it, while a later completed run may replace its timestamp",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V7; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn scan_symbol_v7(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V6LifecycleEvent>,
    stages: &mut V7StageCounts,
) -> Result<()> {
    let mut machine = V7Machine::new();
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualification_latches += step.qualified_regimes;
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

fn summarize_v7(candidates: &[V2Candidate], stages: V7StageCounts) -> V7Summary {
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
    V7Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v6_event_count(candidates),
        stages,
    }
}

fn decide_v7(
    summary: &V7Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
    btc_lifecycle: &V6BtcLifecycleAudit,
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
    gates.insert("btc_failed_retest_interrupt_exists", btc_lifecycle.passed);
    gates.insert(
        "btc_short_interrupted_by_july18_end",
        btc_interrupted_by_july18,
    );
    gates.insert("all_six_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "candidate_count_between_5000_and_50000",
        (V7_MIN_CANDIDATES..=V7_MAX_CANDIDATES).contains(&summary.candidate_count),
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
            "V7 六张目标图、BTC 7 月 18 日前中断和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !definition_matches {
        (
            "rejected_definition_mismatch",
            "V7 至少一张正反样本不符合，或 BTC 空头未在 7 月 18 日结束前中断；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V7 定义样本通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V7 冻结身份，明确历史资格与当前 episode 的不同生命周期。
#[derive(Debug, Clone, Serialize)]
pub struct V7Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V7 独立候选键。
    pub candidate_key: &'static str,
    /// V7 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V6 唯一改变的资格记忆变量。
    pub only_variable: &'static str,
    /// V6 中保持冻结的突破、episode、回踩和失败中断规则。
    pub unchanged_entry_policy: &'static str,
    /// 历史资格建立、替换和清空合同。
    pub qualification_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V7 无标签生命周期阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V7StageCounts {
    /// 新的连续长期关系首次锁存资格的次数。
    pub qualification_latches: usize,
    /// 有历史资格的新 EMA576 突破建立 episode 的次数。
    pub episode_starts: usize,
    /// episode 内重新离开 EMA144 并武装的次数。
    pub retest_rearms: usize,
    /// 回踩满足冻结缓冲守稳条件的次数。
    pub held_retests: usize,
    /// 回踩失守并终止当前 episode 的次数。
    pub failed_retest_invalidations: usize,
}

/// V7 无标签覆盖与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V7Summary {
    /// 全部 V7 候选数。
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
    /// V7 资格、episode、武装、成功和失败中断阶段计数。
    pub stages: V7StageCounts,
}

/// V7 的完整 L1 机器产物；候选与中断均只使用当时已完成 K。
#[derive(Debug, Clone, Serialize)]
pub struct V7Report {
    /// V7 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V7 规则、标签与运行隔离身份。
    pub identity: V7Identity,
    /// 冻结行情、成员与六个目标窗口输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和生命周期阶段摘要。
    pub summary: V7Summary,
    /// 三张正样本与三张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// BTC 旧空头失败回踩中断门禁。
    pub btc_wrong_short_lifecycle_audit: V6BtcLifecycleAudit,
    /// true 表示 BTC 旧空头最迟在北京时间 7 月 18 日结束前已中断。
    pub btc_interrupted_by_july18: bool,
    /// BTC 七月目标窗口的 episode 建立与失败中断事件。
    pub btc_lifecycle_events: Vec<V6LifecycleEvent>,
    /// V7 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
