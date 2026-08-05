//! V3 规则下分别保留 long 与 short 活动突破，不让反向突破删除旧方向的 L1 扫描。
//!
//! V4 只改变活动方向所有权；资格、突破、再武装和 EMA144 缓冲守稳条件全部冻结。

pub mod failed_retest_termination_v6;
#[cfg(test)]
mod tests;

use super::*;

/// V4 独立候选身份，不能覆盖单活动方向 V3 或任何既有研究版本。
pub const V4_CANDIDATE_KEY: &str = "market_momentum_ema576_dual_active_ema144_buffered_hold_15m_v4";
/// V4 只把活动突破从全局互斥改为多空分别保存。
pub const V4_RULE_VERSION: &str = "l1_v3_dual_active_same030_hold_v4";

#[derive(Debug, Default)]
struct V4StepResult {
    qualified_regimes: usize,
    activated_direction: bool,
    replaced_same_direction: bool,
    retest_rearms: usize,
    failed_retests: usize,
    candidates: Vec<V2CandidateCore>,
}

#[derive(Debug)]
struct DualActiveMachine {
    long_tracker: RegimeTracker,
    short_tracker: RegimeTracker,
    long_active: Option<ActiveDirection>,
    short_active: Option<ActiveDirection>,
}

impl DualActiveMachine {
    fn new() -> Self {
        Self {
            long_tracker: RegimeTracker::new(Direction::Long),
            short_tracker: RegimeTracker::new(Direction::Short),
            long_active: None,
            short_active: None,
        }
    }

    /// 新突破只替换同方向上下文；为保持 V3 因果顺序，当根不再处理已有回踩单。
    fn step(&mut self, bars: &[PatternBar], idx: usize) -> V4StepResult {
        let Some(bar) = bars.get(idx).copied().and_then(PatternBar::ready) else {
            self.reset_all();
            return V4StepResult::default();
        };
        let long = self.long_tracker.step(bars, idx, bar);
        let short = self.short_tracker.step(bars, idx, bar);
        let mut result = V4StepResult {
            qualified_regimes: usize::from(long.qualified) + usize::from(short.qualified),
            ..V4StepResult::default()
        };

        if let Some(activation) = long.activation.or(short.activation) {
            result.activated_direction = true;
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
            let slot = match activation.direction {
                Direction::Long => &mut self.long_active,
                Direction::Short => &mut self.short_active,
            };
            result.replaced_same_direction = slot.replace(active).is_some();
            result.retest_rearms = usize::from(rearmed);
            // V4 只改变活动所有权；资格仍沿用 V3 的每次突破后重新积累合同。
            self.long_tracker.reset();
            self.short_tracker.reset();
            return result;
        }

        for slot in [&mut self.long_active, &mut self.short_active] {
            let Some(mut active) = *slot else {
                continue;
            };
            if active.rearmed_idx.is_none() {
                if departed_from_ema144(active.direction, bar) {
                    active.rearmed_idx = Some(idx);
                    active.rearmed_ts = Some(bar.ts);
                    result.retest_rearms += 1;
                }
                *slot = Some(active);
                continue;
            }
            if !retest_zone_reached(active.direction, bar) {
                *slot = Some(active);
                continue;
            }

            if retest_holds_with_close_buffer(active.direction, bar, V3_CLOSE_HOLD_BUFFER_ATR) {
                result.candidates.push(V2CandidateCore {
                    active,
                    signal_idx: idx,
                    signal_bar: bar,
                });
            } else {
                result.failed_retests += 1;
            }
            active.rearmed_idx = None;
            active.rearmed_ts = None;
            *slot = Some(active);
        }
        result
    }

    fn reset_all(&mut self) {
        self.long_tracker.reset();
        self.short_tracker.reset();
        self.long_active = None;
        self.short_active = None;
    }
}

/// 连接本地 quant_core 并输出 V4 无结果标签候选账本。
pub async fn run_v4_l1_scan(output: &Path) -> Result<V4Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for dual-active EMA144 retest L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v4_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V4 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V4 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v4_report(data: &BacktestDataSet) -> Result<V4Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * super::super::super::super::MS_15M)
        .context("V4 L1 warmup start overflow")?;
    let expected_window_candles = inclusive_candle_count(warmup_start_ms, EVALUATION_END_MS)?;
    let mut excluded_symbols = Vec::new();
    let mut candidates = Vec::new();
    let mut stages = V4StageCounts::default();
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
            .context("V4 evaluation start index overflow")?;
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
        scan_symbol_v4(
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
    let summary = summarize_v4(&candidates, stages);
    let decision = decide_v4(&summary, &target_audits, &target_inputs);

    Ok(V4Report {
        schema_version: "market_momentum_ema576_dual_active_ema144_hold_l1_v4",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V4Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V4_CANDIDATE_KEY,
            rule_version: V4_RULE_VERSION,
            only_variable: "replace V3 globally exclusive active breakout with independently retained long and short active breakouts",
            unchanged_entry_policy: "V3 regime144, price-side80%, two-close EMA576 breakout, repeat rearming, and EMA144 +/-0.30 ATR buffered hold remain unchanged",
            active_policy: "a new breakout replaces only the same direction; each direction must depart from and retest EMA144 from its own favorable side",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V4; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn scan_symbol_v4(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    stages: &mut V4StageCounts,
) -> Result<()> {
    let mut machine = DualActiveMachine::new();
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.qualified_regimes += step.qualified_regimes;
        stages.activated_directions += usize::from(step.activated_direction);
        stages.same_direction_replacements += usize::from(step.replaced_same_direction);
        stages.retest_rearms += step.retest_rearms;
        stages.failed_retests += step.failed_retests;
        stages.held_retests += step.candidates.len();
        stages.dual_signal_bars += usize::from(step.candidates.len() > 1);
        for core in step.candidates {
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

fn summarize_v4(candidates: &[V2Candidate], stages: V4StageCounts) -> V4Summary {
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
    V4Summary {
        candidate_count: candidates.len(),
        by_direction,
        by_cross_phase,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_v2_event_count(candidates),
        stages,
    }
}

fn decide_v4(
    summary: &V4Summary,
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
        "candidate_count_between_20000_and_80000",
        (20_000..=80_000).contains(&summary.candidate_count),
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
            "V4 五张目标图和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !positive_targets_match || !negative_targets_clear {
        (
            "rejected_definition_mismatch",
            "V4 至少一张正样本未命中或反样本仍触发；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V4 目标图通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V4 冻结身份，明确只改变活动方向所有权。
#[derive(Debug, Clone, Serialize)]
pub struct V4Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V4 独立候选键。
    pub candidate_key: &'static str,
    /// V4 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V3 唯一改变的方向所有权变量。
    pub only_variable: &'static str,
    /// V3 中保持冻结的形态、缓冲和再武装规则。
    pub unchanged_entry_policy: &'static str,
    /// 多空活动上下文独立保存合同。
    pub active_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V4 双活动方向阶段计数，不包含 outcome。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V4StageCounts {
    /// 新鲜长期状态资格完成次数。
    pub qualified_regimes: usize,
    /// 建立或更新某一活动方向的合格 EMA576 突破次数。
    pub activated_directions: usize,
    /// 同方向较新突破替换旧上下文的次数。
    pub same_direction_replacements: usize,
    /// 两个活动方向各自重新离开 EMA144 并武装的总次数。
    pub retest_rearms: usize,
    /// 回踩满足冻结缓冲守稳条件的次数。
    pub held_retests: usize,
    /// 回踩越出冻结缓冲边界的次数。
    pub failed_retests: usize,
    /// 同一完成 K 同时产生 long 与 short 候选的次数。
    pub dual_signal_bars: usize,
}

/// V4 无标签覆盖和分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V4Summary {
    /// 全部 V4 候选数。
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
    /// V4 资格、方向更新、再武装和回踩阶段计数。
    pub stages: V4StageCounts,
}

/// V4 的完整 L1 机器产物；候选字段沿用 V2 的信号时可见合同。
#[derive(Debug, Clone, Serialize)]
pub struct V4Report {
    /// V4 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V4 规则、标签和运行隔离身份。
    pub identity: V4Identity,
    /// 冻结行情与目标输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和双活动方向阶段摘要。
    pub summary: V4Summary,
    /// 三张正样本与两张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// V4 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
