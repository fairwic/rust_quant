//! V2 生命周期下，将“不有效跌破 EMA144”解释为收盘未越出同一 ATR 缓冲带的 L1 扫描。
//!
//! V3 只改变触碰当根的收盘守稳边界；长期状态、突破、活动方向和再武装均复用 V2。

pub mod dual_active_v4;
pub mod finite_episode_v5;
#[cfg(test)]
mod tests;

use super::*;

/// V3 独立候选身份，不能覆盖 V1、V2 或旧 EMA144/576 研究版本。
pub const V3_CANDIDATE_KEY: &str =
    "market_momentum_ema576_exclusive_active_ema144_buffered_hold_15m_v3";
/// V3 只把收盘守稳边界移到现有 `0.30 ATR14` 回踩区外沿。
pub const V3_RULE_VERSION: &str = "l1_v2_same_lifecycle_close_hold030_v3";

const V3_CLOSE_HOLD_BUFFER_ATR: f64 = RETEST_ZONE_ATR;

/// 连接本地 quant_core 并输出 V3 无结果标签候选账本。
pub async fn run_v3_l1_scan(output: &Path) -> Result<V3Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for buffered EMA144 hold L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v3_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V3 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V3 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v3_report(data: &BacktestDataSet) -> Result<V3Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(REQUIRED_PRE_EVALUATION_BARS as i64 * super::super::super::MS_15M)
        .context("V3 L1 warmup start overflow")?;
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
            .context("V3 evaluation start index overflow")?;
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
        scan_symbol_v3(
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
    let decision = decide_v3(&summary, &target_audits, &target_inputs);

    Ok(V3Report {
        schema_version: "market_momentum_ema576_buffered_ema144_hold_l1_v3",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V3Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V3_CANDIDATE_KEY,
            rule_version: V3_RULE_VERSION,
            only_variable: "replace V2 same-candle close at EMA144 with close inside the existing 0.30 ATR14 retest buffer",
            unchanged_entry_policy: "V2 regime144, price-side80%, two-close EMA576 breakout, exclusive active direction, repeated rearming, and EMA144 +/-0.30 ATR extreme boundary remain unchanged",
            hold_policy: "long close must be >= EMA144 - 0.30 ATR14; short close must be <= EMA144 + 0.30 ATR14; no later reclaim candle is read",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V3; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn scan_symbol_v3(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    stages: &mut V2StageCounts,
) -> Result<()> {
    let mut machine = ExclusiveActiveMachine::with_close_hold_buffer(V3_CLOSE_HOLD_BUFFER_ATR);
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

fn decide_v3(
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
            "V3 五张目标图和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !positive_targets_match || !negative_targets_clear {
        (
            "rejected_definition_mismatch",
            "V3 至少一张正样本未命中或反样本仍触发；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V3 目标图通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V3 冻结身份，明确只改变触碰当根的收盘守稳边界。
#[derive(Debug, Clone, Serialize)]
pub struct V3Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V3 独立候选键。
    pub candidate_key: &'static str,
    /// V3 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V2 唯一改变的守稳变量。
    pub only_variable: &'static str,
    /// V2 中保持冻结的形态和生命周期规则。
    pub unchanged_entry_policy: &'static str,
    /// 触碰当根在同一 ATR 缓冲带内收盘的合同。
    pub hold_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V3 的完整 L1 机器产物；候选字段沿用 V2 的信号时可见合同。
#[derive(Debug, Clone, Serialize)]
pub struct V3Report {
    /// V3 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V3 规则、标签和运行隔离身份。
    pub identity: V3Identity,
    /// 冻结行情与目标输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和生命周期阶段摘要。
    pub summary: V2Summary,
    /// 三张正样本与两张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// V3 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
