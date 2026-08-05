//! 用完成 K 收盘确认 EMA144 失效、影线单独越界只消费武装的 L1 扫描。
//!
//! V8 保留 V7 历史资格与候选触碰合同，只改变活动 episode 的失效确认时点。

pub mod armed_close_post_cross_recross_v9;
#[cfg(test)]
mod tests;

use super::*;

/// V8 独立候选身份，不能覆盖 V7 或旧有限 episode 研究。
pub const V8_CANDIDATE_KEY: &str =
    "market_momentum_ema576_persistent_qualification_close_invalidated_episode_15m_v8";
/// V8 精确规则身份；收盘失效语义变化必须新建版本。
pub const V8_RULE_VERSION: &str =
    "l1_v7_anybar_close_beyond_ema144_030_invalidates_wick_only_consumes_arm_v8";

const V8_MIN_CANDIDATES: usize = 5_000;
const V8_MAX_CANDIDATES: usize = 80_000;

#[derive(Debug, Default)]
struct V8StepResult {
    /// 本根新锁存的历史资格数。
    qualification_latches: usize,
    /// 本根由新 EMA576 突破建立或替换的 episode。
    episode_started: Option<ActiveDirection>,
    /// 本根重新离开 EMA144 后的武装总数。
    retest_rearms: usize,
    /// 本根满足冻结极值与收盘守稳的候选。
    candidates: Vec<V2CandidateCore>,
    /// 本根由完成 K 收盘越过 EMA144 缓冲终止的 episode。
    close_invalidations: Vec<FailedRetestInvalidationCore>,
    /// 只有影线越界、收盘守回边界内而取消本次武装的次数。
    wick_only_failed_retests: usize,
}

#[derive(Debug)]
struct V8Machine {
    /// 多头历史资格积累与锁存状态。
    long_qualification: V7QualificationTracker,
    /// 空头历史资格积累与锁存状态。
    short_qualification: V7QualificationTracker,
    /// 当前独立多头 episode。
    long_active: Option<ActiveDirection>,
    /// 当前独立空头 episode。
    short_active: Option<ActiveDirection>,
}

impl V8Machine {
    fn new() -> Self {
        Self {
            long_qualification: V7QualificationTracker::new(Direction::Long),
            short_qualification: V7QualificationTracker::new(Direction::Short),
            long_active: None,
            short_active: None,
        }
    }

    /// 新突破优先；其余 K 先检查无条件收盘失效，再评估武装和候选触碰。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V8StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V8StepResult::default();
        };
        let mut result = V8StepResult {
            qualification_latches: usize::from(self.long_qualification.step(bar))
                + usize::from(self.short_qualification.step(bar)),
            ..V8StepResult::default()
        };
        let breakout_direction =
            [Direction::Long, Direction::Short]
                .into_iter()
                .find(|direction| {
                    super::super::super::super::super::super::super::breakout_at(
                        bars, idx, *direction,
                    )
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
            let close_atr = close_hold_atr(active.direction, bar);
            if close_atr < -V3_CLOSE_HOLD_BUFFER_ATR {
                // 收盘失守与武装状态无关；否则成功回踩后刚消费武装会让旧 episode 逃过失效。
                result
                    .close_invalidations
                    .push(FailedRetestInvalidationCore {
                        active,
                        invalidated_ts: bar.ts,
                        extreme_to_ema144_atr: retest_extreme_atr(active.direction, bar),
                        close_to_ema144_atr: close_atr,
                    });
                *self.active_mut(direction) = None;
                continue;
            }
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
            } else {
                // 已排除收盘失守，剩余失败只能是影线越界；不把盘中刺穿升级为趋势失效。
                result.wick_only_failed_retests += 1;
            }
            active.rearmed_idx = None;
            active.rearmed_ts = None;
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

/// 连接本机 quant_core 并输出 V8 无成交后标签候选与收盘失效证据。
pub async fn run_v8_l1_scan(output: &Path) -> Result<V8Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V8 close-invalidated episode L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v8_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V8 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V8 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v8_report(data: &BacktestDataSet) -> Result<V8Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(
            REQUIRED_PRE_EVALUATION_BARS as i64
                * super::super::super::super::super::super::super::MS_15M,
        )
        .context("V8 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut lifecycle_events = Vec::new();
    let mut stages = V8StageCounts::default();
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
            .context("V8 evaluation start index overflow")?;
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
        scan_symbol_v8(
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
    let btc_wrong_short_lifecycle_audit = audit_btc_close_invalidation(&lifecycle_events);
    let btc_interrupted_by_july18 = btc_wrong_short_lifecycle_audit
        .invalidated_ts_ms
        .is_some_and(|ts| ts <= BTC_JULY18_END_MS)
        && btc_wrong_short_lifecycle_audit
            .new_short_episode_start_timestamps_ms
            .is_empty();
    let summary = summarize_v8(&candidates, stages);
    let decision = decide_v8(
        &summary,
        &target_audits,
        &target_inputs,
        &btc_wrong_short_lifecycle_audit,
        btc_interrupted_by_july18,
    );

    Ok(V8Report {
        schema_version: "market_momentum_ema576_persistent_qualification_close_invalidated_episode_l1_v8",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V8Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V8_CANDIDATE_KEY,
            rule_version: V8_RULE_VERSION,
            only_variable: "replace V7 armed extreme-or-close episode termination with any-completed-bar close beyond the directional EMA144 0.30 ATR14 boundary; a wick-only breach consumes the arm but preserves the episode",
            unchanged_entry_policy: "V7 persistent regime144 plus price-side80% qualification, two-close EMA576 breakout, independent directional episodes, rearming, and candidate extreme plus close hold rules remain frozen",
            invalidation_policy: "before rearming or touch evaluation, any active long closes below EMA144-0.30ATR14 or active short closes above EMA144+0.30ATR14 and is immediately deleted, regardless of arm state",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V8; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn scan_symbol_v8(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V8LifecycleEvent>,
    stages: &mut V8StageCounts,
) -> Result<()> {
    let mut machine = V8Machine::new();
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualification_latches += step.qualification_latches;
        stages.episode_starts += usize::from(step.episode_started.is_some());
        stages.retest_rearms += step.retest_rearms;
        stages.held_retests += step.candidates.len();
        stages.close_invalidations += step.close_invalidations.len();
        stages.wick_only_failed_retests += step.wick_only_failed_retests;
        if symbol == "BTC-USDT-SWAP" && (BTC_JULY_AUDIT_START_MS..=EVALUATION_END_MS).contains(&ts)
        {
            append_btc_events(symbol, &step, lifecycle_events);
        }
        for core in step.candidates {
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

fn append_btc_events(symbol: &str, step: &V8StepResult, events: &mut Vec<V8LifecycleEvent>) {
    if let Some(active) = step.episode_started {
        events.push(V8LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: active.breakout_ts,
            event: "episode_started",
            direction: active.direction.label(),
            qualification_ts_ms: active.qualified_ts,
            episode_breakout_ts_ms: active.breakout_ts,
            close_to_ema144_directional_atr: None,
        });
    }
    for invalidation in &step.close_invalidations {
        events.push(V8LifecycleEvent {
            symbol: symbol.to_owned(),
            ts_ms: invalidation.invalidated_ts,
            event: "episode_invalidated_close_beyond_ema144",
            direction: invalidation.active.direction.label(),
            qualification_ts_ms: invalidation.active.qualified_ts,
            episode_breakout_ts_ms: invalidation.active.breakout_ts,
            close_to_ema144_directional_atr: Some(invalidation.close_to_ema144_atr),
        });
    }
}

fn audit_btc_close_invalidation(events: &[V8LifecycleEvent]) -> V8BtcLifecycleAudit {
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
        event.event == "episode_invalidated_close_beyond_ema144"
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
    V8BtcLifecycleAudit {
        last_short_episode_breakout_before_old_signal_ts_ms: last_short_episode_breakout_ts_ms,
        invalidated_ts_ms,
        invalidation_close_to_ema144_directional_atr: invalidation
            .and_then(|event| event.close_to_ema144_directional_atr),
        old_wrong_signal_ts_ms: BTC_WRONG_SHORT_SIGNAL_MS,
        new_short_episode_start_timestamps_ms: new_short_episode_start_timestamps_ms.clone(),
        passed: invalidated_ts_ms.is_some() && new_short_episode_start_timestamps_ms.is_empty(),
    }
}

fn summarize_v8(candidates: &[V2Candidate], stages: V8StageCounts) -> V8Summary {
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
    V8Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v6_event_count(candidates),
        stages,
    }
}

fn decide_v8(
    summary: &V8Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
    btc_lifecycle: &V8BtcLifecycleAudit,
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
    gates.insert("btc_close_invalidation_exists", btc_lifecycle.passed);
    gates.insert(
        "btc_short_interrupted_by_july18_end",
        btc_interrupted_by_july18,
    );
    gates.insert("all_six_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "candidate_count_between_5000_and_80000",
        (V8_MIN_CANDIDATES..=V8_MAX_CANDIDATES).contains(&summary.candidate_count),
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
            "V8 六张目标图、BTC 7 月 18 日前收盘失效和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !definition_matches {
        (
            "rejected_definition_mismatch",
            "V8 至少一张正反样本不符合，或 BTC 空头未在 7 月 18 日结束前由收盘失效；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V8 定义样本通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V8 冻结身份，明确完成 K 收盘与影线在 episode 失效中的不同权重。
#[derive(Debug, Clone, Serialize)]
pub struct V8Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V8 独立候选键。
    pub candidate_key: &'static str,
    /// V8 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V7 唯一改变的 episode 失效确认变量。
    pub only_variable: &'static str,
    /// V7 中保持冻结的资格、突破、episode 和候选规则。
    pub unchanged_entry_policy: &'static str,
    /// 任意完成 K 收盘失守即终止、影线单独越界不终止的合同。
    pub invalidation_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V8 无标签生命周期阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V8StageCounts {
    /// 新的连续长期关系首次锁存资格的次数。
    pub qualification_latches: usize,
    /// 有历史资格的新 EMA576 突破建立 episode 的次数。
    pub episode_starts: usize,
    /// episode 内重新离开 EMA144 并武装的次数。
    pub retest_rearms: usize,
    /// 回踩极值与收盘都满足冻结守稳条件的次数。
    pub held_retests: usize,
    /// 任意完成 K 收盘越过 EMA144 反方向缓冲而终止 episode 的次数。
    pub close_invalidations: usize,
    /// 只有影线越界、收盘守回边界内而消费武装的次数。
    pub wick_only_failed_retests: usize,
}

/// V8 无标签覆盖与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V8Summary {
    /// 全部 V8 候选数。
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
    /// V8 资格、episode、候选、收盘失效和影线失败阶段计数。
    pub stages: V8StageCounts,
}

/// BTC 七月窗口内的 V8 episode 建立与收盘失效事件。
#[derive(Debug, Clone, Serialize)]
pub struct V8LifecycleEvent {
    /// 事件所属交易对。
    pub symbol: String,
    /// 事件完成 K 的 Unix 毫秒时间戳。
    pub ts_ms: i64,
    /// `episode_started` 或 `episode_invalidated_close_beyond_ema144`。
    pub event: &'static str,
    /// 事件所属多头或空头方向。
    pub direction: &'static str,
    /// 当前 episode 复用的历史资格时间。
    pub qualification_ts_ms: i64,
    /// 当前 episode 的两收盘 EMA576 突破时间。
    pub episode_breakout_ts_ms: i64,
    /// 失效收盘相对 EMA144 的方向归一化 ATR；建立事件为空。
    pub close_to_ema144_directional_atr: Option<f64>,
}

/// BTC 旧空头是否已被完成 K 收盘及时中断的无标签审计。
#[derive(Debug, Clone, Serialize)]
pub struct V8BtcLifecycleAudit {
    /// 旧错误信号前最后一次空头 episode 的突破时间。
    pub last_short_episode_breakout_before_old_signal_ts_ms: Option<i64>,
    /// 该空头 episode 的完成 K 收盘失效时间。
    pub invalidated_ts_ms: Option<i64>,
    /// 失效收盘相对 EMA144 的方向归一化 ATR。
    pub invalidation_close_to_ema144_directional_atr: Option<f64>,
    /// V4/V7 曾保留的 7 月 19 日旧空头信号时间。
    pub old_wrong_signal_ts_ms: i64,
    /// 失效后到旧信号前重新建立的空头 episode；通过时必须为空。
    pub new_short_episode_start_timestamps_ms: Vec<i64>,
    /// true 表示找到了对应收盘失效且之后没有新空头 episode。
    pub passed: bool,
}

/// V8 完整 L1 机器产物；不包含成交、退出或收益结果。
#[derive(Debug, Clone, Serialize)]
pub struct V8Report {
    /// V8 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V8 规则、标签与运行隔离身份。
    pub identity: V8Identity,
    /// 冻结行情、成员与六个目标窗口输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和生命周期阶段摘要。
    pub summary: V8Summary,
    /// 三张正样本与三张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// BTC 旧空头的完成 K 收盘失效门禁。
    pub btc_wrong_short_lifecycle_audit: V8BtcLifecycleAudit,
    /// true 表示 BTC 旧空头最迟在北京时间 7 月 18 日结束前已中断。
    pub btc_interrupted_by_july18: bool,
    /// BTC 七月窗口的 episode 建立与收盘失效事件。
    pub btc_lifecycle_events: Vec<V8LifecycleEvent>,
    /// V8 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
