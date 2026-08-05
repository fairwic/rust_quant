//! 只允许价格在 EMA144/576 方向交叉前建立 episode 的 L1 扫描。
//!
//! V10 保留 V9 的完整生命周期，只把迟到的 post-cross 价格突破排除在新 episode 之外。

pub mod l2;
pub mod signal_cross_timeout_v11;
#[cfg(test)]
mod tests;

use super::*;

/// V10 独立候选身份，不能覆盖 V9。
pub const V10_CANDIDATE_KEY: &str = "market_momentum_ema576_pre_cross_breakout_episode_15m_v10";
/// V10 精确规则身份；episode 建立时序变化必须新建版本。
pub const V10_RULE_VERSION: &str = "l1_v9_episode_start_requires_pre_ema144_576_cross_v10";

const V10_MIN_CANDIDATES: usize = 5_000;
const V10_MAX_CANDIDATES: usize = 80_000;

/// 连接本机 quant_core 并输出 V10 无成交后标签候选与 BTC 日期链。
pub async fn run_v10_l1_scan(output: &Path) -> Result<V10Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V10 pre-cross breakout L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v10_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V10 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V10 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn build_v10_report(data: &BacktestDataSet) -> Result<V10Report> {
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(
            REQUIRED_PRE_EVALUATION_BARS as i64
                * super::super::super::super::super::super::super::super::super::MS_15M,
        )
        .context("V10 L1 warmup start overflow")?;
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
            .context("V10 evaluation start index overflow")?;
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
        scan_symbol_with_v9_machine(
            &pair.symbol,
            &bars,
            start_idx,
            end_idx,
            &mut candidates,
            &mut lifecycle_events,
            &mut stages,
            true,
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
    let decision = decide_v10(
        &summary,
        &target_audits,
        &target_inputs,
        &btc_wrong_short_lifecycle_audit,
        btc_interrupted_by_july18,
    );

    Ok(V10Report {
        schema_version: "market_momentum_ema576_pre_cross_breakout_episode_l1_v10",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V10Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V10_CANDIDATE_KEY,
            rule_version: V10_RULE_VERSION,
            only_variable: "a new long episode requires EMA144<=EMA576 on its two-close price breakout confirmation bar; a new short episode requires EMA144>=EMA576",
            unchanged_entry_policy: "V9 historical qualification, independent episode ownership, rearming, held retest, wick-only arm consumption, armed close failure, and post-cross opposite EMA576 recross invalidation remain frozen",
            activation_policy: "price must lead the EMA144/576 directional cross when creating an episode; after creation, the episode may cross and emit post-cross EMA144 retests",
            label_boundary: "no fill, future candle, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V10; not registered in paper, readonly shadow, live worker, compose, or production presets",
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

fn decide_v10(
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
        (V10_MIN_CANDIDATES..=V10_MAX_CANDIDATES).contains(&summary.candidate_count),
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
            "V10 六张目标图、价格先于均线交叉的建立顺序、BTC 完整中断日期链和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !definition_matches {
        (
            "rejected_definition_mismatch",
            "V10 至少一张正反样本不符合，或 BTC 日期链被建立时序过滤破坏；按预注册停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V10 定义样本通过但覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V10 冻结身份，明确价格突破必须先于 EMA144/576 方向交叉。
#[derive(Debug, Clone, Serialize)]
pub struct V10Identity {
    /// 当前研究等级。
    pub level: &'static str,
    /// V10 独立候选键。
    pub candidate_key: &'static str,
    /// V10 精确规则版本。
    pub rule_version: &'static str,
    /// 相对 V9 唯一改变的新 episode 建立时序变量。
    pub only_variable: &'static str,
    /// V9 中保持冻结的资格、回踩和中断规则。
    pub unchanged_entry_policy: &'static str,
    /// 价格领先均线交叉、episode 后续仍可跨入 post-cross 的合同。
    pub activation_policy: &'static str,
    /// L1 禁止读取的成交后标签。
    pub label_boundary: &'static str,
    /// 与运行态和生产的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V10 完整 L1 机器产物；不包含成交、退出或收益结果。
#[derive(Debug, Clone, Serialize)]
pub struct V10Report {
    /// V10 报告字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339。
    pub generated_at_utc: String,
    /// V10 规则、标签与运行隔离身份。
    pub identity: V10Identity,
    /// 冻结行情、成员与六个目标窗口输入证据。
    pub coverage: L1Coverage,
    /// 无标签覆盖和 V9 冻结生命周期阶段摘要。
    pub summary: V9Summary,
    /// 三张正样本与三张反样本审计。
    pub target_audits: Vec<TargetAudit>,
    /// BTC 旧空头完整日期链门禁。
    pub btc_wrong_short_lifecycle_audit: V9BtcLifecycleAudit,
    /// true 表示 BTC 旧空头最迟在北京时间 7 月 18 日结束前已中断。
    pub btc_interrupted_by_july18: bool,
    /// BTC 七月窗口的 episode 生命周期事件。
    pub btc_lifecycle_events: Vec<V9LifecycleEvent>,
    /// V10 L1 停止或升级门禁。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本。
    pub candidates: Vec<V2Candidate>,
}
