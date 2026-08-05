//! V11：EMA144 回踩订单武装 K 必须同时出现同方向成交额排名事件。
//!
//! 本层只连接 V6 信号时字段与同一根已完成 K 的 96 根成交额排名，禁止读取触碰后的结果。

use super::super::{
    config_from_env_and_args, kline_volume_rank_velocity::load_kline_volume_rank_events,
    load_backtest_data, MarketVelocityEventBacktestArgs, MarketVelocityEventSource,
    MarketVelocityTradeDirection, RadarEvent, MS_15M,
};
use super::persistent_dynamic_retest_v2::{
    build_v6_l1_report, V2Candidate, V2Report, V6_CANDIDATE_KEY, V6_RULE_VERSION,
};
use super::{frozen_l1_args, EVALUATION_END_MS};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;
use std::path::Path;

/// V11 使用独立候选键，不覆盖 V6 或当前生产动量策略。
pub const V11_CANDIDATE_KEY: &str = "market_momentum_15m_ema144_576_reexpansion_volume_rank_v11";
/// V11 只增加重扩张完成 K 的同方向成交额排名确认。
pub const V11_RULE_VERSION: &str = "l1_v6_reexpansion_same_candle_volume_rank_nonworse_v11";

const EXPECTED_V6_L1_SHA256: &str =
    "a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da";
const EXPECTED_V6_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_V6_CANDIDATES: usize = 54_837;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// 冻结成交额排名事件的生产同口径输入，避免运行时默认值漂移。
#[derive(Debug, Serialize)]
pub struct V11RankEventContract {
    pub event_source: &'static str,
    pub quote_turnover_measure: &'static str,
    pub lookback_candles: usize,
    pub minimum_delta_rank: i32,
    pub require_turnover_growth: bool,
    pub require_consecutive_improvement: bool,
    pub timestamp_alignment: &'static str,
    pub direction_alignment: &'static str,
}

/// 一条通过同 K、同方向成交额排名确认的 V6 回踩候选。
#[derive(Debug, Clone, Serialize)]
pub struct V11Candidate {
    pub symbol: String,
    pub direction: &'static str,
    pub signal_ts_ms: i64,
    pub signal_month_utc: String,
    pub qualified_ts_ms: i64,
    pub breakout_ts_ms: i64,
    pub active_since_ts_ms: i64,
    pub reexpanded_ts_ms: i64,
    pub rank_event_id: i64,
    pub rank_event_ts_ms: i64,
    pub rank_new_rank: i32,
    pub rank_delta_rank: i32,
    pub rank_price_change_pct: f64,
    pub anchor_ema144: f64,
    pub anchor_atr14: f64,
    pub touch_zone_boundary: f64,
    pub cross_phase: &'static str,
}

/// V11 从 V6 候选到同 K、同方向排名确认的逐层计数。
#[derive(Debug, Default, Serialize)]
pub struct V11Stages {
    pub rank_events_loaded: usize,
    pub source_v6_candidates: usize,
    pub exact_timestamp_rank_event_found: usize,
    pub same_direction_rank_event_found: usize,
    pub opposite_direction_rank_event_found: usize,
    pub no_rank_event_on_reexpansion: usize,
    pub final_candidates: usize,
}

/// 用户三张图只按最终候选的触碰时间做定义命中审计。
#[derive(Debug, Serialize)]
pub struct V11TargetAudit {
    pub name: &'static str,
    pub symbol: &'static str,
    pub direction: &'static str,
    pub start_ms: i64,
    pub end_ms: i64,
    pub matched_signal_timestamps_ms: Vec<i64>,
    pub matched: bool,
}

/// V11 无标签覆盖、分散性和可复现账本摘要。
#[derive(Debug, Serialize)]
pub struct V11Summary {
    pub source_candidate_count: usize,
    pub candidate_count: usize,
    pub candidate_reduction_pct: f64,
    pub by_direction: BTreeMap<&'static str, usize>,
    pub by_symbol: BTreeMap<String, usize>,
    pub by_month_utc: BTreeMap<String, usize>,
    pub effective_market_events: usize,
    pub candidate_ledger_sha256: String,
    pub stages: V11Stages,
}

/// V11 的独立研究身份与严格结果字段边界。
#[derive(Debug, Serialize)]
pub struct V11Identity {
    pub level: &'static str,
    pub candidate_key: &'static str,
    pub rule_version: &'static str,
    pub source_v6_candidate_key: &'static str,
    pub source_v6_rule_version: &'static str,
    pub only_variable: &'static str,
    pub label_boundary: &'static str,
    pub runtime_boundary: &'static str,
}

/// 预注册的目标、材料性和分散性联合门禁。
#[derive(Debug, Serialize)]
pub struct V11Decision {
    pub status: &'static str,
    pub gates: BTreeMap<&'static str, bool>,
    pub reason: String,
    pub outcome_evaluation_performed: bool,
}

/// V11 L1 机器报告；逐候选字段全部在触碰时已经可见。
#[derive(Debug, Serialize)]
pub struct V11Report {
    pub schema_version: &'static str,
    pub generated_at_utc: String,
    pub identity: V11Identity,
    pub source_v6_l1_report_sha256: String,
    pub source_v6_dataset_fingerprint_sha256: String,
    pub returned_symbol_count: usize,
    pub eligible_symbol_count: usize,
    pub rank_event_contract: V11RankEventContract,
    pub summary: V11Summary,
    pub target_audits: Vec<V11TargetAudit>,
    pub decision: V11Decision,
    pub candidates: Vec<V11Candidate>,
}

/// 校验冻结 V6 报告，加载成交额排名输入并写出 V11 无标签账本。
pub async fn run_v11_l1(v6_source: &Path, output: &Path) -> Result<V11Report> {
    let source_bytes = std::fs::read(v6_source)
        .with_context(|| format!("读取 EMA144/576 V6 L1 源报告失败：{}", v6_source.display()))?;
    let source_sha256 = sha256_hex(&source_bytes);
    if source_sha256 != EXPECTED_V6_L1_SHA256 {
        bail!("V11 source V6 L1 report SHA mismatch");
    }
    let config = config_from_env_and_args(frozen_l1_args()?)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 V11 L1")?;
    // V6 必须先按原始加载起点重建，否则更早历史会改变递归 EMA 的 SMA seed 与数据指纹。
    let data = load_backtest_data(&pool, &config.args).await?;
    let v6 = build_v6_l1_report(&data)?;
    validate_v6_identity(&v6)?;
    let rank_start_ms = v6
        .candidates
        .iter()
        .map(|candidate| candidate.reexpanded_ts_ms.saturating_add(MS_15M))
        .min()
        .context("V11 V6 source has no re-expansion timestamp")?;
    let eligible_symbols = v6.summary.by_symbol.keys().cloned().collect::<Vec<_>>();
    let rank_args = v11_rank_args(rank_start_ms)?;
    let rank_events =
        load_kline_volume_rank_events(&pool, &eligible_symbols, &rank_args, None).await?;
    let report = build_v11_report(v6, &rank_events, source_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("创建 EMA144/576 V11 L1 报告目录失败：{}", parent.display())
        })?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V11 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// V11 单独加载排名事件，起点取源账本最早重扩张完成时刻，不改变 V6 的 EMA seed。
fn v11_rank_args(rank_start_ms: i64) -> Result<MarketVelocityEventBacktestArgs> {
    let mut args = frozen_l1_args()?;
    args.event_start_ms = Some(rank_start_ms);
    args.kline_volume_rank_velocity = true;
    if args.event_source != MarketVelocityEventSource::Kline15m
        || args.trade_direction != MarketVelocityTradeDirection::Both
        || args.min_delta_rank != 0
        || args.max_delta_rank.is_some()
        || args.kline_volume_rank_require_turnover_growth
        || args.kline_volume_rank_require_consecutive_improvement
        || args.event_end_ms != Some(EVALUATION_END_MS)
    {
        bail!("V11 frozen volume-rank contract drifted");
    }
    Ok(args)
}

/// V6 必须严格复现冻结报告，禁止让排名加载范围悄悄改变原形态。
pub(super) fn validate_v6_identity(v6: &V2Report) -> Result<()> {
    if v6.identity.candidate_key != V6_CANDIDATE_KEY
        || v6.identity.rule_version != V6_RULE_VERSION
        || v6.coverage.dataset_fingerprint_sha256 != EXPECTED_V6_DATASET_FINGERPRINT_SHA256
        || v6.summary.candidate_count != EXPECTED_V6_CANDIDATES
        || v6.candidates.len() != EXPECTED_V6_CANDIDATES
        || v6.decision.status != "coverage_pass_ready_for_l2_prereg"
        || v6.decision.outcome_evaluation_performed
    {
        bail!(
            "V11 rebuilt V6 L1 identity mismatch: candidate_key={} rule_version={} fingerprint={} candidates={}/{} decision={} outcome={}",
            v6.identity.candidate_key,
            v6.identity.rule_version,
            v6.coverage.dataset_fingerprint_sha256,
            v6.summary.candidate_count,
            v6.candidates.len(),
            v6.decision.status,
            v6.decision.outcome_evaluation_performed,
        );
    }
    Ok(())
}

/// 以 `(symbol, reexpanded close time)` 精确连接已验证的 V6 与独立排名事件。
fn build_v11_report(
    v6: V2Report,
    rank_events: &[RadarEvent],
    source_sha256: String,
) -> Result<V11Report> {
    validate_v6_identity(&v6)?;
    let (candidates, stages) = filter_candidates(&v6.candidates, rank_events)?;
    let target_audits = audit_targets(&candidates);
    let by_direction = counts_by_static(candidates.iter().map(|candidate| candidate.direction));
    let by_symbol = counts_by(candidates.iter().map(|candidate| candidate.symbol.clone()));
    let by_month_utc = counts_by(
        candidates
            .iter()
            .map(|candidate| candidate.signal_month_utc.clone()),
    );
    let candidate_reduction_pct = EXPECTED_V6_CANDIDATES.saturating_sub(candidates.len()) as f64
        / EXPECTED_V6_CANDIDATES as f64
        * 100.0;
    let summary = V11Summary {
        source_candidate_count: EXPECTED_V6_CANDIDATES,
        candidate_count: candidates.len(),
        candidate_reduction_pct,
        by_direction,
        by_symbol,
        by_month_utc,
        effective_market_events: effective_market_event_count(&candidates),
        candidate_ledger_sha256: hash_candidates(&candidates),
        stages,
    };
    let decision = decide(&summary, &target_audits);
    Ok(V11Report {
        schema_version: "market_momentum_15m_ema144_576_reexpansion_volume_rank_l1_v11",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V11Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V11_CANDIDATE_KEY,
            rule_version: V11_RULE_VERSION,
            source_v6_candidate_key: V6_CANDIDATE_KEY,
            source_v6_rule_version: V6_RULE_VERSION,
            only_variable: "require the completed V6 0.75 ATR re-expansion candle to have a same-direction 96-candle quote-turnover rank event with non-worsening rank",
            label_boundary: "uses only V6 signal-time fields and the exact re-expansion completed-candle rank event; no post-touch candle, fill, exit, MFE, MAE, R, win, loss, or PnL",
            runtime_boundary: "research-only V11 L1; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        source_v6_l1_report_sha256: source_sha256,
        source_v6_dataset_fingerprint_sha256: v6.coverage.dataset_fingerprint_sha256,
        returned_symbol_count: v6.coverage.returned_symbol_count,
        eligible_symbol_count: v6.coverage.eligible_symbol_count,
        rank_event_contract: V11RankEventContract {
            event_source: "kline_15m_volume_rank_velocity",
            quote_turnover_measure: "rolling_96_confirmed_candles_sum(vol_ccy_x_close)",
            lookback_candles: 96,
            minimum_delta_rank: 0,
            require_turnover_growth: false,
            require_consecutive_improvement: false,
            timestamp_alignment: "rank_event_ts_ms == reexpanded_ts_ms + 15m",
            direction_alignment: "long requires positive candle return; short requires negative candle return",
        },
        summary,
        target_audits,
        decision,
        candidates,
    })
}

/// 每个 V6 候选只查看精确重扩张完成时刻；不存在事件时不向前后寻找替代 K。
fn filter_candidates(
    source: &[V2Candidate],
    events: &[RadarEvent],
) -> Result<(Vec<V11Candidate>, V11Stages)> {
    let mut by_symbol_and_time = BTreeMap::new();
    for event in events {
        let key = (event.symbol.as_str(), event.ts);
        if by_symbol_and_time.insert(key, event).is_some() {
            bail!(
                "V11 duplicate volume-rank event for {} at {}",
                event.symbol,
                event.ts
            );
        }
    }
    let mut stages = V11Stages {
        rank_events_loaded: events.len(),
        source_v6_candidates: source.len(),
        ..V11Stages::default()
    };
    let mut candidates = Vec::new();
    for candidate in source {
        let expected_event_ts = candidate.reexpanded_ts_ms.saturating_add(MS_15M);
        let Some(event) = by_symbol_and_time
            .get(&(candidate.symbol.as_str(), expected_event_ts))
            .copied()
        else {
            stages.no_rank_event_on_reexpansion += 1;
            continue;
        };
        stages.exact_timestamp_rank_event_found += 1;
        if !is_exact_same_direction_event(candidate, event) {
            stages.opposite_direction_rank_event_found += 1;
            continue;
        }
        stages.same_direction_rank_event_found += 1;
        candidates.push(V11Candidate {
            symbol: candidate.symbol.clone(),
            direction: candidate.direction,
            signal_ts_ms: candidate.signal_ts_ms,
            signal_month_utc: candidate.signal_month_utc.clone(),
            qualified_ts_ms: candidate.qualified_ts_ms,
            breakout_ts_ms: candidate.breakout_ts_ms,
            active_since_ts_ms: candidate.active_since_ts_ms,
            reexpanded_ts_ms: candidate.reexpanded_ts_ms,
            rank_event_id: event.id,
            rank_event_ts_ms: event.ts,
            rank_new_rank: event.new_rank,
            rank_delta_rank: event.delta_rank,
            rank_price_change_pct: event.price_change_pct,
            anchor_ema144: candidate.anchor_ema144,
            anchor_atr14: candidate.anchor_atr14,
            touch_zone_boundary: candidate.touch_zone_boundary,
            cross_phase: candidate.cross_phase,
        });
    }
    candidates.sort_by(|left, right| {
        (
            left.signal_ts_ms,
            left.symbol.as_str(),
            left.direction,
            left.reexpanded_ts_ms,
        )
            .cmp(&(
                right.signal_ts_ms,
                right.symbol.as_str(),
                right.direction,
                right.reexpanded_ts_ms,
            ))
    });
    stages.final_candidates = candidates.len();
    Ok((candidates, stages))
}

/// 精确完成时刻与镜像 K 线方向必须同时成立。
fn is_exact_same_direction_event(candidate: &V2Candidate, event: &RadarEvent) -> bool {
    candidate.symbol == event.symbol
        && event.ts == candidate.reexpanded_ts_ms.saturating_add(MS_15M)
        && match candidate.direction {
            "long" => event.price_change_pct > 0.0,
            "short" => event.price_change_pct < 0.0,
            _ => false,
        }
}

fn audit_targets(candidates: &[V11Candidate]) -> Vec<V11TargetAudit> {
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
                    && candidate.direction == "long"
                    && (start_ms..=end_ms).contains(&candidate.signal_ts_ms)
            })
            .map(|candidate| candidate.signal_ts_ms)
            .collect::<Vec<_>>();
        V11TargetAudit {
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

/// 只对预注册覆盖字段做停止判断，不能借用后验收益弥补门禁失败。
fn decide(summary: &V11Summary, targets: &[V11TargetAudit]) -> V11Decision {
    let long_count = summary
        .by_direction
        .get("long")
        .copied()
        .unwrap_or_default();
    let short_count = summary
        .by_direction
        .get("short")
        .copied()
        .unwrap_or_default();
    let targets_match = targets.len() == 3 && targets.iter().all(|target| target.matched);
    let reduction_material = (20.0..=95.0).contains(&summary.candidate_reduction_pct);
    let mut gates = BTreeMap::new();
    gates.insert(
        "source_v6_identity_dataset_and_outcome_boundary_verified",
        true,
    );
    gates.insert("all_three_user_targets_match", targets_match);
    gates.insert(
        "candidate_reduction_between_20_and_95_pct",
        reduction_material,
    );
    gates.insert("candidates_at_least_30", summary.candidate_count >= 30);
    gates.insert(
        "both_directions_at_least_10",
        long_count >= 10 && short_count >= 10,
    );
    gates.insert("symbols_at_least_8", summary.by_symbol.len() >= 8);
    gates.insert("utc_months_at_least_6", summary.by_month_utc.len() >= 6);
    gates.insert(
        "effective_events_at_least_15",
        summary.effective_market_events >= 15,
    );
    let passed = gates.values().all(|passed| *passed);
    V11Decision {
        status: if passed {
            "coverage_pass_ready_for_l2_prereg"
        } else if !targets_match {
            "rejected_definition_mismatch"
        } else if !reduction_material {
            "stop_materiality_gate_failed"
        } else {
            "stop_coverage_gate_failed"
        },
        reason: if passed {
            "V11 同 K、同方向成交额排名确认保留三张目标并形成材料性且分散的候选；下一步只能先冻结 L2 成本回放清单。".to_owned()
        } else {
            "V11 至少一项预注册目标、材料性或分散性门禁失败；按规则停止，不读取成交后结果或扩展事件窗口。".to_owned()
        },
        outcome_evaluation_performed: false,
        gates,
    }
}

fn effective_market_event_count(candidates: &[V11Candidate]) -> usize {
    let mut ordered = candidates
        .iter()
        .map(|candidate| (candidate.signal_ts_ms, candidate.direction))
        .collect::<Vec<_>>();
    ordered.sort_unstable();
    let mut last_by_direction = BTreeMap::new();
    let mut count = 0;
    for (ts, direction) in ordered {
        if last_by_direction
            .get(direction)
            .is_none_or(|last| ts.saturating_sub(*last) > EVENT_CLUSTER_WINDOW_MS)
        {
            count += 1;
        }
        last_by_direction.insert(direction, ts);
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

fn counts_by_static(values: impl Iterator<Item = &'static str>) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
}

fn hash_candidates(candidates: &[V11Candidate]) -> String {
    let mut hasher = Sha256::new();
    for candidate in candidates {
        hash_bytes(&mut hasher, candidate.symbol.as_bytes());
        hash_bytes(&mut hasher, candidate.direction.as_bytes());
        hasher.update(candidate.signal_ts_ms.to_le_bytes());
        hasher.update(candidate.reexpanded_ts_ms.to_le_bytes());
        hasher.update(candidate.rank_event_id.to_le_bytes());
        hasher.update(candidate.rank_event_ts_ms.to_le_bytes());
        hasher.update(candidate.rank_new_rank.to_le_bytes());
        hasher.update(candidate.rank_delta_rank.to_le_bytes());
        hasher.update(candidate.rank_price_change_pct.to_bits().to_le_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn source_candidate(direction: &'static str) -> V2Candidate {
        V2Candidate {
            symbol: "BTC-USDT-SWAP".to_owned(),
            direction,
            signal_ts_ms: 10 * MS_15M,
            signal_month_utc: "2026-07".to_owned(),
            qualified_ts_ms: MS_15M,
            breakout_ts_ms: 2 * MS_15M,
            active_since_ts_ms: 3 * MS_15M,
            reexpanded_ts_ms: 5 * MS_15M,
            bars_since_activation: 7,
            bars_since_reexpansion: 5,
            anchor_ema144: 100.0,
            anchor_atr14: 2.0,
            touch_zone_boundary: 100.6,
            touch_extreme: 100.5,
            touch_extreme_to_anchor_atr: 0.25,
            close_to_current_ema144_atr: 0.1,
            close_holds_current_ema144: true,
            cross_phase: "post_cross_retest",
            current_ema144: 100.1,
            current_ema576: 99.0,
            current_atr14: 2.0,
        }
    }

    fn rank_event(ts: i64, price_change_pct: f64) -> RadarEvent {
        RadarEvent {
            id: 1,
            exchange: "okx".to_owned(),
            symbol: "BTC-USDT-SWAP".to_owned(),
            ts,
            detected_at: "fixture".to_owned(),
            new_rank: 3,
            delta_rank: 0,
            current_price: 100.0,
            price_change_pct,
        }
    }

    #[test]
    fn exact_alignment_requires_completed_time_and_mirrored_direction() {
        let long = source_candidate("long");
        let completed_ts = long.reexpanded_ts_ms + MS_15M;
        assert!(is_exact_same_direction_event(
            &long,
            &rank_event(completed_ts, 0.2)
        ));
        assert!(!is_exact_same_direction_event(
            &long,
            &rank_event(long.reexpanded_ts_ms, 0.2)
        ));
        assert!(!is_exact_same_direction_event(
            &long,
            &rank_event(completed_ts, -0.2)
        ));
        let short = source_candidate("short");
        assert!(is_exact_same_direction_event(
            &short,
            &rank_event(completed_ts, -0.2)
        ));
    }

    #[test]
    fn decision_stops_when_reduction_is_not_material() {
        let summary = passing_summary_with_reduction(10.0);
        let decision = decide(&summary, &matched_targets());
        assert_eq!(decision.status, "stop_materiality_gate_failed");
        assert!(!decision.outcome_evaluation_performed);
    }

    #[test]
    fn decision_rejects_any_target_definition_mismatch() {
        let summary = passing_summary_with_reduction(50.0);
        let mut targets = matched_targets();
        targets[2].matched = false;
        targets[2].matched_signal_timestamps_ms.clear();
        let decision = decide(&summary, &targets);
        assert_eq!(decision.status, "rejected_definition_mismatch");
    }

    fn passing_summary_with_reduction(reduction: f64) -> V11Summary {
        let by_direction = BTreeMap::from([("long", 100), ("short", 100)]);
        let by_symbol = (0..8).map(|index| (format!("S{index}"), 25)).collect();
        let by_month_utc = (1..=6)
            .map(|month| (format!("2026-{month:02}"), 30))
            .collect();
        V11Summary {
            source_candidate_count: EXPECTED_V6_CANDIDATES,
            candidate_count: 200,
            candidate_reduction_pct: reduction,
            by_direction,
            by_symbol,
            by_month_utc,
            effective_market_events: 20,
            candidate_ledger_sha256: "fixture".to_owned(),
            stages: V11Stages::default(),
        }
    }

    fn matched_targets() -> Vec<V11TargetAudit> {
        ["nmr", "btc_1", "btc_2"]
            .into_iter()
            .map(|name| V11TargetAudit {
                name,
                symbol: "BTC-USDT-SWAP",
                direction: "long",
                start_ms: 0,
                end_ms: 1,
                matched_signal_timestamps_ms: vec![0],
                matched: true,
            })
            .collect()
    }
}
