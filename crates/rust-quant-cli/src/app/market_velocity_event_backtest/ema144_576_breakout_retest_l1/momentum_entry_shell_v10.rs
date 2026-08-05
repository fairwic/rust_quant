//! V10：用 V6 EMA144 回踩替换现有多头 15m 动量的 FVG 延迟入场壳。
//!
//! 本模块只连接信号时可见的上游动量事件与 EMA 回踩候选，不读取任何退出或收益。

use super::super::{
    config_from_env_and_args, evaluate_events, filter_confirmed_events_by_entry_trigger,
    filter_confirmed_events_by_symbol, load_backtest_data, parse_paper_observation_args_from,
    BacktestDataSet, ConfirmedEvent, FvgEntryMode, MarketVelocityEventBacktestArgs,
    MarketVelocityEventSource, MarketVelocityTradeDirection, MS_15M,
};
use super::persistent_dynamic_retest_v2::{
    build_v6_l1_report, V2Candidate, V6_CANDIDATE_KEY, V6_RULE_VERSION,
};
use super::{frozen_l1_args, EVALUATION_END_MS, EVALUATION_START_MS};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// V10 是新的多头入场能力，不覆盖当前 FVG 生产 preset。
pub const V10_CANDIDATE_KEY: &str = "market_momentum_15m_ema144_576_retest_entry_shell_long_v10";
/// V10 只把延迟入场壳从 FVG 50% 回补替换为 V6 EMA144 回踩。
pub const V10_RULE_VERSION: &str =
    "l1_kline15m_base_breakout_replace_fvg50_with_v6_retest_wait24_long_v10";

const PRODUCTION_BASE_PRESET: &str =
    "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1";
const EXPECTED_V6_L1_SHA256: &str =
    "a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da";
const EXPECTED_V6_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_V6_CANDIDATES: usize = 54_837;
const ENTRY_WAIT_CANDLES: usize = 24;
const SYMBOL_COOLDOWN_CANDLES: usize = 4;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// 一个先发生的动量事件与随后 EMA144 回踩之间的完整无标签连接证据。
#[derive(Debug, Clone, Serialize)]
pub struct V10Candidate {
    pub symbol: String,
    pub direction: &'static str,
    pub momentum_event_id: i64,
    pub momentum_event_ts_ms: i64,
    pub momentum_new_rank: i32,
    pub momentum_delta_rank: i32,
    pub momentum_trigger: String,
    pub retest_signal_ts_ms: i64,
    pub wait_candles: usize,
    pub qualified_ts_ms: i64,
    pub breakout_ts_ms: i64,
    pub reexpanded_ts_ms: i64,
    pub anchor_ema144: f64,
    pub anchor_atr14: f64,
    pub touch_zone_boundary: f64,
    pub cross_phase: &'static str,
}

/// V10 从上游 raw event 到冷却后候选的逐层覆盖。
#[derive(Debug, Default, Serialize)]
pub struct V10Stages {
    pub raw_long_events_in_eligible_symbols: usize,
    pub upstream_entry_signal_pass: usize,
    pub upstream_breakout_trigger_pass: usize,
    pub mapped_to_v6_retest_within_24_candles: usize,
    pub duplicate_retest_mappings: usize,
    pub no_retest_within_24_candles: usize,
    pub symbol_cooldown_blocked: usize,
    pub final_candidates: usize,
}

/// 用户目标图只检查 V10 最终候选时间，不读取目标后的价格。
#[derive(Debug, Serialize)]
pub struct V10TargetAudit {
    pub name: &'static str,
    pub symbol: &'static str,
    pub direction: &'static str,
    pub start_ms: i64,
    pub end_ms: i64,
    pub matched_signal_timestamps_ms: Vec<i64>,
    pub matched: bool,
}

/// 无标签候选的分散性与可复现账本身份。
#[derive(Debug, Serialize)]
pub struct V10Summary {
    pub candidate_count: usize,
    pub by_symbol: BTreeMap<String, usize>,
    pub by_month_utc: BTreeMap<String, usize>,
    pub effective_market_events: usize,
    pub upstream_signal_ledger_sha256: String,
    pub candidate_ledger_sha256: String,
    pub stages: V10Stages,
}

/// V10 L1 冻结身份。
#[derive(Debug, Serialize)]
pub struct V10Identity {
    pub level: &'static str,
    pub candidate_key: &'static str,
    pub rule_version: &'static str,
    pub source_v6_candidate_key: &'static str,
    pub source_v6_rule_version: &'static str,
    pub production_base_preset: &'static str,
    pub only_variable: &'static str,
    pub label_boundary: &'static str,
    pub runtime_boundary: &'static str,
}

/// 预注册的 L1 覆盖门禁。
#[derive(Debug, Serialize)]
pub struct V10Decision {
    pub status: &'static str,
    pub gates: BTreeMap<&'static str, bool>,
    pub reason: String,
    pub outcome_evaluation_performed: bool,
}

/// V10 L1 机器报告；候选账本不含任何结果字段。
#[derive(Debug, Serialize)]
pub struct V10Report {
    pub schema_version: &'static str,
    pub generated_at_utc: String,
    pub identity: V10Identity,
    pub source_v6_l1_report_sha256: String,
    pub source_v6_dataset_fingerprint_sha256: String,
    pub returned_symbol_count: usize,
    pub eligible_symbol_count: usize,
    pub summary: V10Summary,
    pub target_audits: Vec<V10TargetAudit>,
    pub decision: V10Decision,
    pub candidates: Vec<V10Candidate>,
}

/// 校验 V6 源账本，加载冻结行情并写出 V10 无标签候选报告。
pub async fn run_v10_l1(v6_source: &Path, output: &Path) -> Result<V10Report> {
    let source_bytes = std::fs::read(v6_source)
        .with_context(|| format!("读取 EMA144/576 V6 L1 源报告失败：{}", v6_source.display()))?;
    let source_sha256 = sha256_hex(&source_bytes);
    if source_sha256 != EXPECTED_V6_L1_SHA256 {
        bail!("V10 source V6 L1 report SHA mismatch");
    }
    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 V10 L1")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_v10_report(&data, source_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("创建 EMA144/576 V10 L1 报告目录失败：{}", parent.display())
        })?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V10 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 复用生产 preset 的完整上游，只在内存中关闭 FVG 以取得壳层之前的信号。
fn production_base_args() -> Result<MarketVelocityEventBacktestArgs> {
    let mut args =
        parse_paper_observation_args_from(["--paper-strategy-preset", PRODUCTION_BASE_PRESET])?;
    if args.event_source != MarketVelocityEventSource::Kline15m
        || args.trade_direction != MarketVelocityTradeDirection::Long
        || args.fvg_entry_mode != FvgEntryMode::M15ImpulseRetrace
        || args.fvg_max_wait_candles != ENTRY_WAIT_CANDLES
        || args.entry_symbol_cooldown_candles != Some(SYMBOL_COOLDOWN_CANDLES)
        || args.entry_trigger_allowlist != ["breakout_previous_high"]
        || !approx_equal(args.stop_loss_pct, 0.04)
        || args.target_rs != [0.52]
        || !approx_equal(args.entry_max_distance_pct, 14.0)
        || !approx_equal(args.entry_min_volume_ratio, 1.3)
        || args.entry_min_rsi != Some(50.0)
        || args.entry_max_rsi != Some(90.0)
        || !args.entry_bollinger_breakout
        || args.entry_min_recent_drawdown_pct != Some(3.5)
        || args.entry_recent_drawdown_lookback_candles != 12
        || args.min_delta_rank != 0
    {
        bail!("V10 production base preset contract drifted");
    }
    args.fvg_entry_mode = FvgEntryMode::Off;
    Ok(args)
}

/// 先生成生产上游信号，再将每个事件映射到 24 根内首个 V6 多头回踩。
fn build_v10_report(data: &BacktestDataSet, source_sha256: String) -> Result<V10Report> {
    let v6 = build_v6_l1_report(data)?;
    if v6.identity.candidate_key != V6_CANDIDATE_KEY
        || v6.identity.rule_version != V6_RULE_VERSION
        || v6.coverage.dataset_fingerprint_sha256 != EXPECTED_V6_DATASET_FINGERPRINT_SHA256
        || v6.summary.candidate_count != EXPECTED_V6_CANDIDATES
        || v6.decision.status != "coverage_pass_ready_for_l2_prereg"
        || v6.decision.outcome_evaluation_performed
    {
        bail!("V10 rebuilt V6 L1 identity mismatch");
    }
    let eligible_symbols = v6
        .summary
        .by_symbol
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let base_args = production_base_args()?;
    let raw_long_events = data
        .events
        .iter()
        .filter(|event| {
            event.price_change_pct > 0.0
                && eligible_symbols.contains(&event.symbol)
                && (EVALUATION_START_MS..=EVALUATION_END_MS).contains(&event.ts)
        })
        .cloned()
        .collect::<Vec<_>>();
    let evaluation = evaluate_events(
        &raw_long_events,
        &data.candles_4h_computed,
        &data.candles_15m_computed,
        &data.candles_4h,
        &data.candles_1h,
        &data.candles_15m,
        &base_args,
    );
    let symbol_filtered = filter_confirmed_events_by_symbol(&evaluation.confirmed, &base_args);
    let mut upstream = filter_confirmed_events_by_entry_trigger(&symbol_filtered, &base_args);
    upstream.sort_by(|left, right| {
        (left.event.ts, left.event.symbol.as_str(), left.event.id).cmp(&(
            right.event.ts,
            right.event.symbol.as_str(),
            right.event.id,
        ))
    });
    let upstream_signal_ledger_sha256 = hash_upstream(&upstream);
    let (candidates, mut stages) = join_upstream_to_retests(&upstream, &v6.candidates);
    stages.raw_long_events_in_eligible_symbols = raw_long_events.len();
    stages.upstream_entry_signal_pass = evaluation
        .stage_counts
        .get("entry_signal_pass")
        .copied()
        .unwrap_or_default();
    stages.upstream_breakout_trigger_pass = upstream.len();
    stages.final_candidates = candidates.len();
    let target_audits = audit_targets(&candidates);
    let by_symbol = counts_by(candidates.iter().map(|candidate| candidate.symbol.clone()));
    let by_month_utc = counts_by(candidates.iter().filter_map(|candidate| {
        Utc.timestamp_millis_opt(candidate.retest_signal_ts_ms)
            .single()
            .map(|value| value.format("%Y-%m").to_string())
    }));
    let effective_market_events = effective_event_count(&candidates);
    let candidate_ledger_sha256 = hash_candidates(&candidates);
    let summary = V10Summary {
        candidate_count: candidates.len(),
        by_symbol,
        by_month_utc,
        effective_market_events,
        upstream_signal_ledger_sha256,
        candidate_ledger_sha256,
        stages,
    };
    let decision = decide(&summary, &target_audits);
    Ok(V10Report {
        schema_version: "market_momentum_15m_ema144_576_retest_entry_shell_long_l1_v10",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V10Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V10_CANDIDATE_KEY,
            rule_version: V10_RULE_VERSION,
            source_v6_candidate_key: V6_CANDIDATE_KEY,
            source_v6_rule_version: V6_RULE_VERSION,
            production_base_preset: PRODUCTION_BASE_PRESET,
            only_variable: "replace only the production long preset FVG 50 percent delayed-entry shell with the V6 EMA144 retest resting-order shell while keeping the 24-candle wait and every upstream momentum filter unchanged",
            label_boundary: "uses only completed upstream event/filter fields and V6 retest-touch fields; no post-entry candle, exit, MFE, MAE, R, win, loss, or PnL",
            runtime_boundary: "research-only V10 L1; existing FVG preset and all paper, readonly shadow, live, compose, and production pointers remain unchanged",
        },
        source_v6_l1_report_sha256: source_sha256,
        source_v6_dataset_fingerprint_sha256: v6.coverage.dataset_fingerprint_sha256,
        returned_symbol_count: v6.coverage.returned_symbol_count,
        eligible_symbol_count: eligible_symbols.len(),
        summary,
        target_audits,
        decision,
        candidates,
    })
}

/// 每个上游事件只尝试其窗口内首个回踩；同一回踩的重复映射不会寻找替代候选。
fn join_upstream_to_retests(
    upstream: &[ConfirmedEvent],
    v6_candidates: &[V2Candidate],
) -> (Vec<V10Candidate>, V10Stages) {
    let mut by_symbol: BTreeMap<&str, Vec<&V2Candidate>> = BTreeMap::new();
    for candidate in v6_candidates
        .iter()
        .filter(|candidate| candidate.direction == "long")
    {
        by_symbol
            .entry(candidate.symbol.as_str())
            .or_default()
            .push(candidate);
    }
    for candidates in by_symbol.values_mut() {
        candidates.sort_by_key(|candidate| candidate.signal_ts_ms);
    }
    let mut seen_retests = BTreeSet::new();
    let mut mapped = Vec::new();
    let mut stages = V10Stages::default();
    let latest_offset = (ENTRY_WAIT_CANDLES.saturating_sub(1) as i64).saturating_mul(MS_15M);
    for signal in upstream {
        let Some(symbol_candidates) = by_symbol.get(signal.event.symbol.as_str()) else {
            stages.no_retest_within_24_candles += 1;
            continue;
        };
        let start =
            symbol_candidates.partition_point(|candidate| candidate.signal_ts_ms < signal.event.ts);
        let deadline = signal.event.ts.saturating_add(latest_offset);
        let Some(retest) = symbol_candidates
            .get(start..)
            .and_then(|candidates| {
                candidates
                    .iter()
                    .find(|candidate| candidate.signal_ts_ms <= deadline)
            })
            .copied()
        else {
            stages.no_retest_within_24_candles += 1;
            continue;
        };
        let identity = (retest.symbol.as_str(), retest.signal_ts_ms);
        if !seen_retests.insert(identity) {
            stages.duplicate_retest_mappings += 1;
            continue;
        }
        stages.mapped_to_v6_retest_within_24_candles += 1;
        mapped.push(V10Candidate {
            symbol: retest.symbol.clone(),
            direction: "long",
            momentum_event_id: signal.event.id,
            momentum_event_ts_ms: signal.event.ts,
            momentum_new_rank: signal.event.new_rank,
            momentum_delta_rank: signal.event.delta_rank,
            momentum_trigger: signal.trigger.clone(),
            retest_signal_ts_ms: retest.signal_ts_ms,
            wait_candles: usize::try_from(
                retest.signal_ts_ms.saturating_sub(signal.event.ts) / MS_15M,
            )
            .unwrap_or(usize::MAX),
            qualified_ts_ms: retest.qualified_ts_ms,
            breakout_ts_ms: retest.breakout_ts_ms,
            reexpanded_ts_ms: retest.reexpanded_ts_ms,
            anchor_ema144: retest.anchor_ema144,
            anchor_atr14: retest.anchor_atr14,
            touch_zone_boundary: retest.touch_zone_boundary,
            cross_phase: retest.cross_phase,
        });
    }
    mapped.sort_by(|left, right| {
        (left.retest_signal_ts_ms, left.symbol.as_str())
            .cmp(&(right.retest_signal_ts_ms, right.symbol.as_str()))
    });
    let cooldown_ms = SYMBOL_COOLDOWN_CANDLES as i64 * MS_15M;
    let mut last_by_symbol = BTreeMap::new();
    mapped.retain(|candidate| {
        let keep = last_by_symbol
            .get(&candidate.symbol)
            .is_none_or(|last| candidate.retest_signal_ts_ms.saturating_sub(*last) >= cooldown_ms);
        if keep {
            last_by_symbol.insert(candidate.symbol.clone(), candidate.retest_signal_ts_ms);
        } else {
            stages.symbol_cooldown_blocked += 1;
        }
        keep
    });
    (mapped, stages)
}

fn audit_targets(candidates: &[V10Candidate]) -> Vec<V10TargetAudit> {
    [
        (
            "nmr_2026_07_01_user_chart",
            "NMR-USDT-SWAP",
            1_782_835_200_000,
            1_782_878_400_000,
        ),
        (
            "btc_2026_07_02_user_chart",
            "BTC-USDT-SWAP",
            1_782_943_200_000,
            1_782_964_800_000,
        ),
        (
            "btc_2026_07_12_user_chart",
            "BTC-USDT-SWAP",
            1_783_828_800_000,
            1_783_850_400_000,
        ),
    ]
    .into_iter()
    .map(|(name, symbol, start_ms, end_ms)| {
        let matched_signal_timestamps_ms = candidates
            .iter()
            .filter(|candidate| {
                candidate.symbol == symbol
                    && (start_ms..=end_ms).contains(&candidate.retest_signal_ts_ms)
            })
            .map(|candidate| candidate.retest_signal_ts_ms)
            .collect::<Vec<_>>();
        V10TargetAudit {
            name,
            symbol,
            direction: "long",
            start_ms,
            end_ms,
            matched: !matched_signal_timestamps_ms.is_empty(),
            matched_signal_timestamps_ms,
        }
    })
    .collect()
}

fn decide(summary: &V10Summary, targets: &[V10TargetAudit]) -> V10Decision {
    let mut gates = BTreeMap::new();
    gates.insert(
        "source_v6_identity_dataset_and_outcome_boundary_verified",
        true,
    );
    gates.insert(
        "all_three_user_targets_match",
        targets.iter().all(|target| target.matched),
    );
    gates.insert("candidates_at_least_30", summary.candidate_count >= 30);
    gates.insert("symbols_at_least_8", summary.by_symbol.len() >= 8);
    gates.insert("utc_months_at_least_6", summary.by_month_utc.len() >= 6);
    gates.insert(
        "effective_events_at_least_15",
        summary.effective_market_events >= 15,
    );
    gates.insert(
        "every_candidate_is_within_frozen_24_candle_wait",
        summary.candidate_count > 0,
    );
    let passed = gates.values().all(|passed| *passed);
    V10Decision {
        status: if passed {
            "coverage_pass_ready_for_l2_prereg"
        } else if !targets.iter().all(|target| target.matched) {
            "rejected_definition_mismatch"
        } else {
            "stop_coverage_gate_failed"
        },
        reason: if passed {
            "V10 在不改变 15m 动量上游和 24 根等待的前提下保留三张目标并形成分散候选；下一步只能先冻结 L2 成本回放清单。".to_owned()
        } else {
            "V10 至少一项预注册目标或覆盖门禁失败；按规则停止，不读取任何成交后结果或调整等待窗口。"
                .to_owned()
        },
        outcome_evaluation_performed: false,
        gates,
    }
}

fn effective_event_count(candidates: &[V10Candidate]) -> usize {
    let mut times = candidates
        .iter()
        .map(|candidate| candidate.retest_signal_ts_ms)
        .collect::<Vec<_>>();
    times.sort_unstable();
    let mut count = 0;
    let mut previous = None;
    for ts in times {
        if previous.is_none_or(|last| ts.saturating_sub(last) > EVENT_CLUSTER_WINDOW_MS) {
            count += 1;
        }
        previous = Some(ts);
    }
    count
}

fn counts_by(values: impl Iterator<Item = String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
}

fn hash_upstream(signals: &[ConfirmedEvent]) -> String {
    let mut hasher = Sha256::new();
    for signal in signals {
        hash_bytes(&mut hasher, signal.event.symbol.as_bytes());
        hasher.update(signal.event.id.to_le_bytes());
        hasher.update(signal.event.ts.to_le_bytes());
        hash_bytes(&mut hasher, signal.trigger.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn hash_candidates(candidates: &[V10Candidate]) -> String {
    let mut hasher = Sha256::new();
    for candidate in candidates {
        hash_bytes(&mut hasher, candidate.symbol.as_bytes());
        hasher.update(candidate.momentum_event_id.to_le_bytes());
        hasher.update(candidate.momentum_event_ts_ms.to_le_bytes());
        hasher.update(candidate.retest_signal_ts_ms.to_le_bytes());
        hasher.update(candidate.touch_zone_boundary.to_bits().to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn approx_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_base_contract_keeps_every_upstream_filter_and_only_turns_off_fvg() {
        let args = production_base_args().expect("production base args");
        assert_eq!(args.fvg_entry_mode, FvgEntryMode::Off);
        assert_eq!(args.fvg_max_wait_candles, ENTRY_WAIT_CANDLES);
        assert_eq!(args.entry_trigger_allowlist, ["breakout_previous_high"]);
        assert_eq!(args.entry_symbol_cooldown_candles, Some(4));
        assert!(args.entry_bollinger_breakout);
    }

    #[test]
    fn target_windows_remain_inclusive() {
        let candidate = V10Candidate {
            symbol: "NMR-USDT-SWAP".to_owned(),
            direction: "long",
            momentum_event_id: 1,
            momentum_event_ts_ms: 1_782_835_200_000,
            momentum_new_rank: 1,
            momentum_delta_rank: 0,
            momentum_trigger: "breakout_previous_high".to_owned(),
            retest_signal_ts_ms: 1_782_878_400_000,
            wait_candles: 0,
            qualified_ts_ms: 1,
            breakout_ts_ms: 1,
            reexpanded_ts_ms: 1,
            anchor_ema144: 1.0,
            anchor_atr14: 0.1,
            touch_zone_boundary: 1.03,
            cross_phase: "post_cross_retest",
        };
        let audits = audit_targets(&[candidate]);
        assert!(audits[0].matched);
    }
}
