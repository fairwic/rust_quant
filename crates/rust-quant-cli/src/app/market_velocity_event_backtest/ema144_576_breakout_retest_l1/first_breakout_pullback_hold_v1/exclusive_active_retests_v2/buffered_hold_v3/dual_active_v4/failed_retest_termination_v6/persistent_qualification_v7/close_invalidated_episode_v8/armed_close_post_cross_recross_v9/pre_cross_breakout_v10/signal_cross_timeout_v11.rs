//! 首个交叉前信号后 24 小时未完成 EMA144/576 方向交叉的 L1 扫描。
//!
//! V11 只增加旧资格链的单次动量确认期限；V10 的入场形态、其他中断和
//! Research-only 边界全部保持冻结。

pub mod first_entry_only_v12;
pub mod l2;
#[cfg(test)]
mod tests;

use super::*;
use serde::Serialize;
use std::collections::BTreeSet;

/// V11 独立候选身份，不能覆盖 V10。
pub const V11_CANDIDATE_KEY: &str = "market_momentum_ema576_post_signal_cross_timeout_15m_v11";
/// V11 精确规则身份；确认期限或失效语义变化必须新建版本。
pub const V11_RULE_VERSION: &str =
    "l1_v10_first_signal_requires_ema144_576_cross_within_96_bars_v11";

const SIGNAL_CROSS_TIMEOUT_BARS: usize = 96;
const V11_MIN_CANDIDATES: usize = 5_000;
const V11_MAX_CANDIDATES: usize = 80_000;
const V11_MIN_AFFECTED_CANDIDATES: usize = 3;
const V11_MIN_TIMEOUT_CHAINS: usize = 3;
const ALGO_FIRST_LONG_SIGNAL_MS: i64 = 1_784_066_400_000;
const ALGO_TIMEOUT_DEADLINE_MS: i64 = 1_784_152_800_000;
const ALGO_STALE_LONG_BREAKOUT_MS: i64 = 1_784_157_300_000;
const TIMEOUT_EVENT: &str = "post_signal_24h_without_ema144_576_cross";

/// 连接本机 quant_core 并输出 V11 无成交后标签候选、计时事件与目标审计。
pub async fn run_v11_l1_scan(output: &Path) -> Result<V11Report> {
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V11 post-signal cross-timeout L1 scan")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v11_report(&data)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V11 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V11 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 用同一份行情先重建冻结 V10 基线，再执行唯一的 96 根确认期限变量。
pub(super) fn build_v11_report(data: &BacktestDataSet) -> Result<V11Report> {
    let baseline = build_v10_report(data)?;
    let warmup_start_ms = EVALUATION_START_MS
        .checked_sub(
            REQUIRED_PRE_EVALUATION_BARS as i64
                * super::super::super::super::super::super::super::super::super::super::MS_15M,
        )
        .context("V11 L1 warmup start overflow")?;
    let excluded = baseline
        .coverage
        .excluded_symbols
        .iter()
        .map(|item| item.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    let mut lifecycle_events = Vec::new();
    let mut btc_lifecycle_events = Vec::new();
    let mut algo_episode_starts = Vec::new();
    let mut stages = V11StageCounts::default();

    let mut pairs = data.pairs.iter().collect::<Vec<_>>();
    pairs.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    for pair in pairs {
        if excluded.contains(pair.symbol.as_str()) {
            continue;
        }
        let candles = data
            .candles_15m_computed
            .get(&pair.symbol)
            .with_context(|| format!("missing computed candles for {}", pair.symbol))?;
        let (start_idx, end_idx) =
            complete_window_bounds(candles, warmup_start_ms, EVALUATION_END_MS)
                .with_context(|| format!("V11 lost frozen complete window for {}", pair.symbol))?;
        let ema576 = ema_close_series(candles, EMA_SLOW_PERIOD);
        let bars = pattern_bars(candles, &ema576);
        scan_symbol_v11(
            &pair.symbol,
            &bars,
            start_idx,
            end_idx,
            &mut candidates,
            &mut lifecycle_events,
            &mut btc_lifecycle_events,
            &mut algo_episode_starts,
            &mut stages,
        )?;
    }

    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.signal_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    lifecycle_events.sort_by(|left, right| {
        (left.event_ts_ms, left.direction, left.symbol.as_str()).cmp(&(
            right.event_ts_ms,
            right.direction,
            right.symbol.as_str(),
        ))
    });
    btc_lifecycle_events.sort_by_key(|event| event.ts_ms);
    algo_episode_starts.sort_unstable();

    let baseline_keys = candidate_keys(&baseline.candidates);
    let candidate_keys = candidate_keys(&candidates);
    let removed_candidate_count = baseline_keys.difference(&candidate_keys).count();
    let added_candidate_count = candidate_keys.difference(&baseline_keys).count();
    let affected_candidate_count = removed_candidate_count + added_candidate_count;
    let target_audits = audit_v6_targets(&candidates);
    let btc_wrong_short_lifecycle_audit = audit_btc_v9_lifecycle(&btc_lifecycle_events);
    let btc_interrupted_by_july18 = btc_wrong_short_lifecycle_audit
        .invalidated_ts_ms
        .is_some_and(|ts| ts <= BTC_JULY18_END_MS)
        && btc_wrong_short_lifecycle_audit
            .new_short_episode_start_timestamps_ms
            .is_empty();
    let algo_timeout_audit =
        audit_algo_timeout(&candidates, &lifecycle_events, &algo_episode_starts);
    let frozen_summary = summarize_v9(&candidates, stages.frozen_v10.clone());
    let summary = V11Summary {
        baseline_candidate_count: baseline.summary.candidate_count,
        candidate_count: frozen_summary.candidate_count,
        removed_candidate_count,
        added_candidate_count,
        affected_candidate_count,
        affected_candidate_ratio_pct: affected_candidate_count as f64 * 100.0
            / baseline.summary.candidate_count as f64,
        by_direction: frozen_summary.by_direction,
        by_cross_phase: frozen_summary.by_cross_phase,
        by_symbol: frozen_summary.by_symbol,
        by_month_utc: frozen_summary.by_month_utc,
        effective_market_events: frozen_summary.effective_market_events,
        stages,
    };
    let decision = decide_v11(
        &summary,
        &target_audits,
        &baseline.coverage.target_inputs,
        &algo_timeout_audit,
        &btc_wrong_short_lifecycle_audit,
        btc_interrupted_by_july18,
    );

    Ok(V11Report {
        schema_version: "market_momentum_ema576_post_signal_cross_timeout_l1_v11",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V11Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V11_CANDIDATE_KEY,
            rule_version: V11_RULE_VERSION,
            baseline_candidate_key: V10_CANDIDATE_KEY,
            only_variable: "after the first pre-cross trade signal on a historical qualification chain, require the directional EMA144/576 cross within 96 completed 15m bars or consume that episode and qualification",
            deadline_policy: "the signal close starts one non-resettable clock; the deadline bar may confirm first; same-direction breakouts, episode replacement, and later signals cannot extend it",
            unchanged_entry_policy: "V10 qualification, pre-cross price breakout activation, independent directions, rearming, held retest, wick-only consumption, armed close failure, and post-cross opposite EMA576 recross remain frozen",
            label_boundary: "no fill, future trade outcome, stop, target, MFE, MAE, exit, R, win, loss, cost, or PnL field is read",
            runtime_boundary: "research-only V11; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        coverage: baseline.coverage,
        summary,
        target_audits,
        algo_timeout_audit,
        btc_wrong_short_lifecycle_audit,
        btc_interrupted_by_july18,
        signal_cross_lifecycle_events: lifecycle_events,
        decision,
        candidates,
    })
}

/// 逐根推进 V11，并把冻结 V10 阶段与新增方向级计时分别记账。
#[allow(clippy::too_many_arguments)]
fn scan_symbol_v11(
    symbol: &str,
    bars: &[PatternBar],
    start_idx: usize,
    end_idx: usize,
    candidates: &mut Vec<V2Candidate>,
    lifecycle_events: &mut Vec<V11SignalCrossLifecycleEvent>,
    btc_lifecycle_events: &mut Vec<V9LifecycleEvent>,
    algo_episode_starts: &mut Vec<i64>,
    stages: &mut V11StageCounts,
) -> Result<()> {
    let mut machine = V9Machine::with_lifecycle_policy(true, Some(SIGNAL_CROSS_TIMEOUT_BARS));
    for idx in start_idx..=end_idx {
        let step = machine.step(bars, idx);
        let ts = bars[idx].ts;
        if !(EVALUATION_START_MS..=EVALUATION_END_MS).contains(&ts) {
            continue;
        }
        stages.frozen_v10.qualification_latches += step.qualification_latches;
        stages.frozen_v10.episode_starts += usize::from(step.episode_started.is_some());
        stages.frozen_v10.post_cross_latches += step.post_cross_latches.len();
        stages.frozen_v10.retest_rearms += step.retest_rearms.len();
        stages.frozen_v10.held_retests += step.held_retests.len();
        stages.frozen_v10.armed_close_invalidations += step
            .invalidations
            .iter()
            .filter(|event| event.reason == "armed_retest_close_beyond_ema144")
            .count();
        stages.frozen_v10.post_cross_recross_invalidations += step
            .invalidations
            .iter()
            .filter(|event| event.reason == "post_cross_opposite_two_close_ema576_breakout")
            .count();
        stages.frozen_v10.wick_only_failed_retests += step.wick_only_failed_retests.len();
        stages.signal_cross_deadline_starts += step.signal_cross_deadline_starts.len();
        stages.signal_cross_deadline_confirmations +=
            step.signal_cross_deadline_confirmations.len();
        stages.signal_cross_deadline_timeouts += step.signal_cross_deadline_timeouts.len();

        if symbol == "BTC-USDT-SWAP" && (BTC_JULY_AUDIT_START_MS..=EVALUATION_END_MS).contains(&ts)
        {
            append_btc_v9_events(symbol, &step, btc_lifecycle_events);
        }
        if symbol == "ALGO-USDT-SWAP" {
            if let Some(active) = step.episode_started {
                if active.core.direction == Direction::Long {
                    algo_episode_starts.push(active.core.breakout_ts);
                }
            }
        }
        append_signal_cross_events(
            symbol,
            "post_signal_cross_deadline_started",
            &step.signal_cross_deadline_starts,
            lifecycle_events,
        );
        append_signal_cross_events(
            symbol,
            "post_signal_cross_confirmed",
            &step.signal_cross_deadline_confirmations,
            lifecycle_events,
        );
        append_signal_cross_events(
            symbol,
            TIMEOUT_EVENT,
            &step.signal_cross_deadline_timeouts,
            lifecycle_events,
        );
        for core in step.candidates {
            candidates.push(candidate_from_v2_core(symbol, core)?);
        }
    }
    Ok(())
}

/// 将内部计时快照转换为可审计事件，同时固定首信号与截止时间的关系。
fn append_signal_cross_events(
    symbol: &str,
    event: &'static str,
    source: &[V9SignalCrossDeadlineEvent],
    events: &mut Vec<V11SignalCrossLifecycleEvent>,
) {
    events.extend(source.iter().map(|item| V11SignalCrossLifecycleEvent {
        symbol: symbol.to_owned(),
        direction: item.deadline.origin_active.core.direction.label(),
        event,
        first_signal_ts_ms: item.deadline.first_signal_ts,
        deadline_ts_ms: item.deadline.first_signal_ts
            + SIGNAL_CROSS_TIMEOUT_BARS as i64
                * super::super::super::super::super::super::super::super::super::super::MS_15M,
        event_ts_ms: item.ts,
        qualification_ts_ms: item.deadline.origin_active.core.qualified_ts,
        origin_breakout_ts_ms: item.deadline.origin_active.core.breakout_ts,
    }));
}

/// 建立只含信号身份的集合，用于 L1 比较覆盖变化而不读取任何 outcome。
fn candidate_keys(candidates: &[V2Candidate]) -> BTreeSet<(String, &'static str, i64)> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate.symbol.clone(),
                candidate.direction,
                candidate.signal_ts_ms,
            )
        })
        .collect()
}

/// 核对用户指定的 ALGO 首信号、固定截止点和旧资格再突破阻断链。
fn audit_algo_timeout(
    candidates: &[V2Candidate],
    events: &[V11SignalCrossLifecycleEvent],
    episode_starts: &[i64],
) -> V11AlgoTimeoutAudit {
    let first_signal_retained = candidates.iter().any(|candidate| {
        candidate.symbol == "ALGO-USDT-SWAP"
            && candidate.direction == "long"
            && candidate.signal_ts_ms == ALGO_FIRST_LONG_SIGNAL_MS
    });
    let deadline_start = events.iter().find(|event| {
        event.symbol == "ALGO-USDT-SWAP"
            && event.direction == "long"
            && event.event == "post_signal_cross_deadline_started"
            && event.first_signal_ts_ms == ALGO_FIRST_LONG_SIGNAL_MS
    });
    let timeout = events.iter().find(|event| {
        event.symbol == "ALGO-USDT-SWAP"
            && event.direction == "long"
            && event.event == TIMEOUT_EVENT
            && event.first_signal_ts_ms == ALGO_FIRST_LONG_SIGNAL_MS
    });
    let stale_breakout_episode_started = episode_starts.contains(&ALGO_STALE_LONG_BREAKOUT_MS);
    let stale_breakout_candidate_count = candidates
        .iter()
        .filter(|candidate| {
            candidate.symbol == "ALGO-USDT-SWAP"
                && candidate.direction == "long"
                && candidate.breakout_ts_ms == ALGO_STALE_LONG_BREAKOUT_MS
        })
        .count();
    let passed = first_signal_retained
        && deadline_start.is_some_and(|event| event.deadline_ts_ms == ALGO_TIMEOUT_DEADLINE_MS)
        && timeout.is_some_and(|event| {
            event.deadline_ts_ms == ALGO_TIMEOUT_DEADLINE_MS
                && event.event_ts_ms == ALGO_TIMEOUT_DEADLINE_MS
        })
        && !stale_breakout_episode_started
        && stale_breakout_candidate_count == 0;
    V11AlgoTimeoutAudit {
        first_signal_ts_ms: ALGO_FIRST_LONG_SIGNAL_MS,
        first_signal_retained,
        deadline_ts_ms: ALGO_TIMEOUT_DEADLINE_MS,
        timeout_invalidation_ts_ms: timeout.map(|event| event.event_ts_ms),
        invalidation_reason: timeout.map(|event| event.event),
        stale_breakout_ts_ms: ALGO_STALE_LONG_BREAKOUT_MS,
        stale_breakout_episode_started,
        stale_breakout_candidate_count,
        passed,
    }
}

/// 只用无标签覆盖、目标日期链与分散性决定是否允许进入 L2 预注册。
fn decide_v11(
    summary: &V11Summary,
    audits: &[TargetAudit],
    target_inputs: &[TargetInputCoverage],
    algo: &V11AlgoTimeoutAudit,
    btc: &V9BtcLifecycleAudit,
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
    gates.insert("algo_24h_timeout_chain_matches", algo.passed);
    gates.insert("all_three_positive_targets_match", positive_targets_match);
    gates.insert("all_three_negative_targets_clear", negative_targets_clear);
    gates.insert("btc_existing_lifecycle_still_valid", btc.passed);
    gates.insert(
        "btc_short_interrupted_by_july18_end",
        btc_interrupted_by_july18,
    );
    gates.insert("all_six_target_inputs_ready", target_inputs_ready);
    gates.insert(
        "affected_candidates_at_least_3",
        summary.affected_candidate_count >= V11_MIN_AFFECTED_CANDIDATES,
    );
    gates.insert(
        "timeout_chains_at_least_3",
        summary.stages.signal_cross_deadline_timeouts >= V11_MIN_TIMEOUT_CHAINS,
    );
    gates.insert(
        "candidate_count_between_5000_and_80000",
        (V11_MIN_CANDIDATES..=V11_MAX_CANDIDATES).contains(&summary.candidate_count),
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
            "V11 ALGO 24 小时失效日期链、既有目标图和无标签覆盖门禁全部通过；仍需独立预注册 L2，当前不含收益结论。",
        )
    } else if !algo.passed || !positive_targets_match || !negative_targets_clear {
        (
            "rejected_definition_mismatch",
            "V11 的 ALGO 目标日期链或既有正反样本不符合预注册定义；按门禁停止，不读取 outcome 调参。",
        )
    } else {
        (
            "rejected_coverage_gate",
            "V11 定义样本通过但影响量、覆盖或分散性门禁失败；停留 L1，不执行资金回放。",
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

/// V11 冻结身份，明确首信号计时属于旧资格链而非可替换 episode。
#[derive(Debug, Clone, Serialize)]
pub struct V11Identity {
    /// 当前研究等级；L1 不允许读取成交后标签。
    pub level: &'static str,
    /// V11 独立候选键，用于与 V10 结果并存审计。
    pub candidate_key: &'static str,
    /// 首信号确认期限的精确规则版本。
    pub rule_version: &'static str,
    /// 本轮唯一变量所基于的冻结 V10 候选身份。
    pub baseline_candidate_key: &'static str,
    /// 相对 V10 唯一允许变化的生命周期规则。
    pub only_variable: &'static str,
    /// 截止 K、不可重置和到期先确认的顺序合同。
    pub deadline_policy: &'static str,
    /// 本轮禁止改变的资格、突破、回踩和中断规则。
    pub unchanged_entry_policy: &'static str,
    /// L1 明确禁止读取的成交后字段边界。
    pub label_boundary: &'static str,
    /// Research-only 与生产运行态的隔离边界。
    pub runtime_boundary: &'static str,
}

/// V11 无标签生命周期阶段计数。
#[derive(Debug, Clone, Default, Serialize)]
pub struct V11StageCounts {
    /// 仍按 V10 口径统计的资格、episode、回踩和既有中断阶段。
    pub frozen_v10: V9StageCounts,
    /// 交叉前首信号首次启动方向级计时的资格链数量。
    pub signal_cross_deadline_starts: usize,
    /// 96 根期限内完成 EMA144/576 方向交叉的资格链数量。
    pub signal_cross_deadline_confirmations: usize,
    /// 到期未交叉并消费 episode 与旧资格的资格链数量。
    pub signal_cross_deadline_timeouts: usize,
}

/// V11 相对冻结 V10 的无标签候选与分散性摘要。
#[derive(Debug, Clone, Serialize)]
pub struct V11Summary {
    /// 同一行情重建得到的 V10 候选总数。
    pub baseline_candidate_count: usize,
    /// 应用 V11 生命周期后保留的候选总数。
    pub candidate_count: usize,
    /// V10 中存在、V11 中不再存在的信号身份数量。
    pub removed_candidate_count: usize,
    /// V11 新出现的信号身份数量；本轮预期为零。
    pub added_candidate_count: usize,
    /// 新增与删除信号身份的并集大小。
    pub affected_candidate_count: usize,
    /// 受影响信号占 V10 候选的百分比。
    pub affected_candidate_ratio_pct: f64,
    /// V11 多空候选分布。
    pub by_direction: BTreeMap<&'static str, usize>,
    /// 候选发生在 EMA144/576 交叉前后的分布。
    pub by_cross_phase: BTreeMap<&'static str, usize>,
    /// 各 OKX 永续合约候选数。
    pub by_symbol: BTreeMap<String, usize>,
    /// 各 UTC 月份候选数。
    pub by_month_utc: BTreeMap<String, usize>,
    /// 按方向和 60 分钟窗口归并的有效市场事件数。
    pub effective_market_events: usize,
    /// V10 冻结阶段与 V11 新增计时阶段计数。
    pub stages: V11StageCounts,
}

/// 一次首信号计时的启动、期限内确认或超时证据。
#[derive(Debug, Clone, Serialize)]
pub struct V11SignalCrossLifecycleEvent {
    /// 事件所属 OKX 永续合约。
    pub symbol: String,
    /// `long` 或 `short`，决定均线交叉确认方向。
    pub direction: &'static str,
    /// 计时启动、期限内确认或 24 小时超时事件名。
    pub event: &'static str,
    /// 启动本次不可重置计时的首信号完成时间，Unix 毫秒。
    pub first_signal_ts_ms: i64,
    /// 首信号后第 96 根完成 K 时间，Unix 毫秒。
    pub deadline_ts_ms: i64,
    /// 当前生命周期事件完成时间，Unix 毫秒。
    pub event_ts_ms: i64,
    /// 被计时链复用的历史长期资格完成时间，Unix 毫秒。
    pub qualification_ts_ms: i64,
    /// 启动计时的首信号所属 episode 突破时间，Unix 毫秒。
    pub origin_breakout_ts_ms: i64,
}

/// ALGO 2026-07-15/16 首信号到旧资格失效的无标签日期链审计。
#[derive(Debug, Clone, Serialize)]
pub struct V11AlgoTimeoutAudit {
    /// 用户指定且必须保留的多头首信号时间，Unix 毫秒。
    pub first_signal_ts_ms: i64,
    /// true 表示 V11 没有追溯删除首笔有效信号。
    pub first_signal_retained: bool,
    /// 固定 96 根计时截止时间，Unix 毫秒。
    pub deadline_ts_ms: i64,
    /// 实际消费旧资格的完成 K 时间；None 表示目标链没有超时事件。
    pub timeout_invalidation_ts_ms: Option<i64>,
    /// 超时事件原因；None 表示目标链未被该规则终止。
    pub invalidation_reason: Option<&'static str>,
    /// 用户指出不得复用旧资格的新价格突破时间，Unix 毫秒。
    pub stale_breakout_ts_ms: i64,
    /// true 表示错误地用陈旧资格建立了新 episode。
    pub stale_breakout_episode_started: bool,
    /// 属于陈旧突破的实际候选数；通过时必须为零。
    pub stale_breakout_candidate_count: usize,
    /// true 表示首信号、截止点、失效和后续阻断全部符合预注册。
    pub passed: bool,
}

/// V11 完整 L1 机器产物；不包含成交、退出或收益结果。
#[derive(Debug, Clone, Serialize)]
pub struct V11Report {
    /// V11 L1 JSON 字段合同版本。
    pub schema_version: &'static str,
    /// 报告生成时间，UTC RFC3339，不参与行情身份。
    pub generated_at_utc: String,
    /// 策略、唯一变量、标签和运行隔离身份。
    pub identity: V11Identity,
    /// 与 V10 相同的成员、窗口、缺失成员和行情指纹。
    pub coverage: L1Coverage,
    /// 相对 V10 的无标签候选变化与分散性。
    pub summary: V11Summary,
    /// 三张正样本与三张反样本的定义审计。
    pub target_audits: Vec<TargetAudit>,
    /// ALGO 7 月 15/16 日完整失效日期链。
    pub algo_timeout_audit: V11AlgoTimeoutAudit,
    /// BTC 既有反向中断日期链，防止新条件破坏冻结语义。
    pub btc_wrong_short_lifecycle_audit: V9BtcLifecycleAudit,
    /// true 表示 BTC 旧空头最迟在北京时间 7 月 18 日已中断。
    pub btc_interrupted_by_july18: bool,
    /// 全币种首信号计时启动、确认与超时事件账本。
    pub signal_cross_lifecycle_events: Vec<V11SignalCrossLifecycleEvent>,
    /// L1 停止或允许新建 L2 预注册的无标签结论。
    pub decision: L1Decision,
    /// 全量信号时可见候选账本，不含成交后结果。
    pub candidates: Vec<V2Candidate>,
}
