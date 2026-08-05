//! V12：保持 V11 精确同 K 排名确认，只把全员面板改为稳定的 95% 可用面板。
//!
//! 该版本修正信号时成交额输入缺口；V6 形态与 V11 排名、方向、时间对齐全部不变。

use super::super::{
    config_from_env_and_args,
    kline_volume_rank_velocity::{
        load_kline_volume_rank_events_stable_95pct, StablePanelRankEvent,
    },
    load_backtest_data, MarketVelocityEventBacktestArgs, MarketVelocityEventSource,
    MarketVelocityTradeDirection, MS_15M,
};
use super::persistent_dynamic_retest_v2::{
    build_v6_l1_report, V2Candidate, V6_CANDIDATE_KEY, V6_RULE_VERSION,
};
use super::reexpansion_volume_rank_v11::validate_v6_identity;
use super::{frozen_l1_args, EVALUATION_END_MS};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::collections::BTreeMap;
use std::path::Path;

/// V12 独立候选键；已有 V6、V11 和生产版本均保持冻结。
pub const V12_CANDIDATE_KEY: &str =
    "market_momentum_15m_ema144_576_reexpansion_volume_rank_stable_panel_v12";
/// V12 唯一改变成交额排名快照的面板可用性政策。
pub const V12_RULE_VERSION: &str = "l1_v11_exact_reexpansion_stable_available_panel95_v12";

const EXPECTED_V6_L1_SHA256: &str =
    "a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da";
const EXPECTED_V11_L1_SHA256: &str =
    "02bbb99a7337c5213c25e5d503268d13c49bd4fe84d4aeeb61d69da6677104dd";
const EXPECTED_V6_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_V6_CANDIDATES: usize = 54_837;
const EVENT_CLUSTER_WINDOW_MS: i64 = 60 * 60 * 1_000;

/// 一条通过稳定面板同 K、同方向排名确认的 EMA144 回踩候选。
#[derive(Debug, Clone, Serialize)]
pub struct V12Candidate {
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
    pub actual_panel_size: usize,
    pub expected_panel_size: usize,
    pub anchor_ema144: f64,
    pub anchor_atr14: f64,
    pub touch_zone_boundary: f64,
    pub cross_phase: &'static str,
}

/// V12 稳定面板过滤的逐层覆盖。
#[derive(Debug, Default, Serialize)]
pub struct V12Stages {
    pub stable_panel_rank_events_loaded: usize,
    pub source_v6_candidates: usize,
    pub exact_timestamp_rank_event_found: usize,
    pub same_direction_rank_event_found: usize,
    pub opposite_direction_rank_event_found: usize,
    pub no_rank_event_on_reexpansion: usize,
    pub final_candidates: usize,
}

/// 三张用户图只按最终候选触碰时间做定义审计。
#[derive(Debug, Serialize)]
pub struct V12TargetAudit {
    pub name: &'static str,
    pub symbol: &'static str,
    pub direction: &'static str,
    pub start_ms: i64,
    pub end_ms: i64,
    pub matched_signal_timestamps_ms: Vec<i64>,
    pub matched: bool,
}

/// V12 候选覆盖、面板大小分布与可复现账本身份。
#[derive(Debug, Serialize)]
pub struct V12Summary {
    pub source_candidate_count: usize,
    pub candidate_count: usize,
    pub candidate_reduction_pct: f64,
    pub by_direction: BTreeMap<&'static str, usize>,
    pub by_symbol: BTreeMap<String, usize>,
    pub by_month_utc: BTreeMap<String, usize>,
    pub by_actual_panel_size: BTreeMap<usize, usize>,
    pub effective_market_events: usize,
    pub candidate_ledger_sha256: String,
    pub stages: V12Stages,
}

/// V12 的单变量身份与研究运行边界。
#[derive(Debug, Serialize)]
pub struct V12Identity {
    pub level: &'static str,
    pub candidate_key: &'static str,
    pub rule_version: &'static str,
    pub source_v6_candidate_key: &'static str,
    pub source_v6_rule_version: &'static str,
    pub source_v11_rule_version: &'static str,
    pub only_variable: &'static str,
    pub label_boundary: &'static str,
    pub runtime_boundary: &'static str,
}

/// 稳定面板的全部冻结参数。
#[derive(Debug, Serialize)]
pub struct V12PanelContract {
    pub expected_universe_members: usize,
    pub minimum_coverage_pct: usize,
    pub minimum_available_members: usize,
    pub adjacent_snapshots_require_identical_members: bool,
    pub rank_lookback_candles: usize,
    pub quote_turnover_measure: &'static str,
    pub minimum_delta_rank: i32,
    pub timestamp_alignment: &'static str,
    pub direction_alignment: &'static str,
}

/// 预注册 L1 门禁，不含任何交易结果字段。
#[derive(Debug, Serialize)]
pub struct V12Decision {
    pub status: &'static str,
    pub gates: BTreeMap<&'static str, bool>,
    pub reason: String,
    pub outcome_evaluation_performed: bool,
}

/// V12 L1 机器报告；所有候选字段在触碰时已经可见。
#[derive(Debug, Serialize)]
pub struct V12Report {
    pub schema_version: &'static str,
    pub generated_at_utc: String,
    pub identity: V12Identity,
    pub source_v6_l1_report_sha256: String,
    pub source_v11_l1_report_sha256: String,
    pub source_v6_dataset_fingerprint_sha256: String,
    pub returned_symbol_count: usize,
    pub eligible_symbol_count: usize,
    pub panel_contract: V12PanelContract,
    pub summary: V12Summary,
    pub target_audits: Vec<V12TargetAudit>,
    pub decision: V12Decision,
    pub candidates: Vec<V12Candidate>,
}

/// 校验 V6 与 V11 冻结证据，独立加载 95% 稳定面板并写出无标签账本。
pub async fn run_v12_l1(v6_source: &Path, v11_source: &Path, output: &Path) -> Result<V12Report> {
    let v6_sha256 = validate_source_sha(v6_source, EXPECTED_V6_L1_SHA256, "V6")?;
    let v11_sha256 = validate_source_sha(v11_source, EXPECTED_V11_L1_SHA256, "V11")?;
    let config = config_from_env_and_args(frozen_l1_args()?)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 V12 L1")?;
    // 原始 V6 使用冻结加载起点复建，稳定面板数据另行加载，避免改变递归 EMA seed。
    let data = load_backtest_data(&pool, &config.args).await?;
    let v6 = build_v6_l1_report(&data)?;
    validate_v6_identity(&v6)?;
    if v6.coverage.dataset_fingerprint_sha256 != EXPECTED_V6_DATASET_FINGERPRINT_SHA256
        || v6.summary.candidate_count != EXPECTED_V6_CANDIDATES
    {
        bail!("V12 rebuilt V6 dataset or candidate identity mismatch");
    }
    let rank_start_ms = v6
        .candidates
        .iter()
        .map(|candidate| candidate.reexpanded_ts_ms.saturating_add(MS_15M))
        .min()
        .context("V12 V6 source has no re-expansion timestamp")?;
    let eligible_symbols = v6.summary.by_symbol.keys().cloned().collect::<Vec<_>>();
    let rank_args = v12_rank_args(rank_start_ms)?;
    let rank_events =
        load_kline_volume_rank_events_stable_95pct(&pool, &eligible_symbols, &rank_args, None)
            .await?;
    let report = build_report(v6, &rank_events, v6_sha256, v11_sha256)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("创建 EMA144/576 V12 L1 报告目录失败：{}", parent.display())
        })?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V12 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

fn validate_source_sha(source: &Path, expected: &str, label: &str) -> Result<String> {
    let bytes = std::fs::read(source).with_context(|| {
        format!(
            "读取 EMA144/576 {label} L1 源报告失败：{}",
            source.display()
        )
    })?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        bail!("V12 source {label} L1 report SHA mismatch");
    }
    Ok(actual)
}

/// 除面板政策由专用加载器决定外，排名时间、方向和阈值全部冻结为 V11。
fn v12_rank_args(rank_start_ms: i64) -> Result<MarketVelocityEventBacktestArgs> {
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
        bail!("V12 frozen volume-rank contract drifted");
    }
    Ok(args)
}

fn build_report(
    v6: super::persistent_dynamic_retest_v2::V2Report,
    rank_events: &[StablePanelRankEvent],
    v6_sha256: String,
    v11_sha256: String,
) -> Result<V12Report> {
    let (candidates, stages) = filter_candidates(&v6.candidates, rank_events)?;
    let target_audits = audit_targets(&candidates);
    let by_direction = counts_by_static(candidates.iter().map(|candidate| candidate.direction));
    let by_symbol = counts_by(candidates.iter().map(|candidate| candidate.symbol.clone()));
    let by_month_utc = counts_by(
        candidates
            .iter()
            .map(|candidate| candidate.signal_month_utc.clone()),
    );
    let by_actual_panel_size = counts_by_usize(
        candidates
            .iter()
            .map(|candidate| candidate.actual_panel_size),
    );
    let candidate_reduction_pct = EXPECTED_V6_CANDIDATES.saturating_sub(candidates.len()) as f64
        / EXPECTED_V6_CANDIDATES as f64
        * 100.0;
    let summary = V12Summary {
        source_candidate_count: EXPECTED_V6_CANDIDATES,
        candidate_count: candidates.len(),
        candidate_reduction_pct,
        by_direction,
        by_symbol,
        by_month_utc,
        by_actual_panel_size,
        effective_market_events: effective_market_event_count(&candidates),
        candidate_ledger_sha256: hash_candidates(&candidates),
        stages,
    };
    let decision = decide(&summary, &target_audits);
    Ok(V12Report {
        schema_version: "market_momentum_15m_ema144_576_reexpansion_volume_rank_stable_panel_l1_v12",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V12Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V12_CANDIDATE_KEY,
            rule_version: V12_RULE_VERSION,
            source_v6_candidate_key: V6_CANDIDATE_KEY,
            source_v6_rule_version: V6_RULE_VERSION,
            source_v11_rule_version: "l1_v6_reexpansion_same_candle_volume_rank_nonworse_v11",
            only_variable: "replace V11 complete-panel availability with at least 95 percent available members while requiring identical actual membership in adjacent rank snapshots",
            label_boundary: "uses only V6 signal-time fields and exact re-expansion completed-candle stable-panel rank evidence; no post-touch candle, fill, exit, MFE, MAE, R, win, loss, or PnL",
            runtime_boundary: "research-only V12 L1; existing complete-panel loader and all paper, readonly shadow, live, compose, and production pointers remain unchanged",
        },
        source_v6_l1_report_sha256: v6_sha256,
        source_v11_l1_report_sha256: v11_sha256,
        source_v6_dataset_fingerprint_sha256: v6.coverage.dataset_fingerprint_sha256,
        returned_symbol_count: v6.coverage.returned_symbol_count,
        eligible_symbol_count: v6.coverage.eligible_symbol_count,
        panel_contract: V12PanelContract {
            expected_universe_members: v6.coverage.eligible_symbol_count,
            minimum_coverage_pct: 95,
            minimum_available_members: v6.coverage.eligible_symbol_count.saturating_mul(95).div_ceil(100),
            adjacent_snapshots_require_identical_members: true,
            rank_lookback_candles: 96,
            quote_turnover_measure: "rolling_96_confirmed_candles_sum(vol_ccy_x_close)",
            minimum_delta_rank: 0,
            timestamp_alignment: "rank_event_ts_ms == reexpanded_ts_ms + 15m",
            direction_alignment: "long requires positive candle return; short requires negative candle return",
        },
        summary,
        target_audits,
        decision,
        candidates,
    })
}

/// 每个候选只消费精确重扩张完成时刻的稳定面板事件，不搜索相邻 K。
fn filter_candidates(
    source: &[V2Candidate],
    rank_events: &[StablePanelRankEvent],
) -> Result<(Vec<V12Candidate>, V12Stages)> {
    let mut by_symbol_and_time = BTreeMap::new();
    for ranked in rank_events {
        let key = (ranked.event.symbol.as_str(), ranked.event.ts);
        if by_symbol_and_time.insert(key, ranked).is_some() {
            bail!(
                "V12 duplicate stable-panel rank event for {} at {}",
                ranked.event.symbol,
                ranked.event.ts
            );
        }
    }
    let mut stages = V12Stages {
        stable_panel_rank_events_loaded: rank_events.len(),
        source_v6_candidates: source.len(),
        ..V12Stages::default()
    };
    let mut candidates = Vec::new();
    for candidate in source {
        let expected_event_ts = candidate.reexpanded_ts_ms.saturating_add(MS_15M);
        let Some(ranked) = by_symbol_and_time
            .get(&(candidate.symbol.as_str(), expected_event_ts))
            .copied()
        else {
            stages.no_rank_event_on_reexpansion += 1;
            continue;
        };
        stages.exact_timestamp_rank_event_found += 1;
        let direction_matches = match candidate.direction {
            "long" => ranked.event.price_change_pct > 0.0,
            "short" => ranked.event.price_change_pct < 0.0,
            _ => false,
        };
        if !direction_matches {
            stages.opposite_direction_rank_event_found += 1;
            continue;
        }
        stages.same_direction_rank_event_found += 1;
        candidates.push(V12Candidate {
            symbol: candidate.symbol.clone(),
            direction: candidate.direction,
            signal_ts_ms: candidate.signal_ts_ms,
            signal_month_utc: candidate.signal_month_utc.clone(),
            qualified_ts_ms: candidate.qualified_ts_ms,
            breakout_ts_ms: candidate.breakout_ts_ms,
            active_since_ts_ms: candidate.active_since_ts_ms,
            reexpanded_ts_ms: candidate.reexpanded_ts_ms,
            rank_event_id: ranked.event.id,
            rank_event_ts_ms: ranked.event.ts,
            rank_new_rank: ranked.event.new_rank,
            rank_delta_rank: ranked.event.delta_rank,
            rank_price_change_pct: ranked.event.price_change_pct,
            actual_panel_size: ranked.actual_panel_size,
            expected_panel_size: ranked.expected_panel_size,
            anchor_ema144: candidate.anchor_ema144,
            anchor_atr14: candidate.anchor_atr14,
            touch_zone_boundary: candidate.touch_zone_boundary,
            cross_phase: candidate.cross_phase,
        });
    }
    candidates.sort_by(|left, right| {
        (left.signal_ts_ms, left.symbol.as_str(), left.direction).cmp(&(
            right.signal_ts_ms,
            right.symbol.as_str(),
            right.direction,
        ))
    });
    stages.final_candidates = candidates.len();
    Ok((candidates, stages))
}

fn audit_targets(candidates: &[V12Candidate]) -> Vec<V12TargetAudit> {
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
        V12TargetAudit {
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

fn decide(summary: &V12Summary, targets: &[V12TargetAudit]) -> V12Decision {
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
    let material_reduction = (3.0..=60.0).contains(&summary.candidate_reduction_pct);
    let mut gates = BTreeMap::new();
    gates.insert("all_three_user_targets_match", targets_match);
    gates.insert(
        "candidate_reduction_between_3_and_60_pct",
        material_reduction,
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
    gates.insert(
        "all_candidates_use_at_least_95pct_panel",
        summary.by_actual_panel_size.keys().all(|size| *size >= 42),
    );
    let passed = gates.values().all(|gate| *gate);
    V12Decision {
        status: if passed {
            "coverage_pass_ready_for_l2_prereg"
        } else if !targets_match {
            "rejected_definition_mismatch"
        } else if !material_reduction {
            "stop_materiality_gate_failed"
        } else {
            "stop_coverage_gate_failed"
        },
        gates,
        reason: if passed {
            "V12 稳定 95% 面板保留三张目标，并形成材料性且分散的同 K 排名候选；下一步只能先冻结 L2 成本回放清单。".to_owned()
        } else {
            "V12 至少一项预注册目标、材料性、面板或分散性门禁失败；按规则停止，不读取成交后结果。"
                .to_owned()
        },
        outcome_evaluation_performed: false,
    }
}

fn effective_market_event_count(candidates: &[V12Candidate]) -> usize {
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

fn counts_by_usize(values: impl Iterator<Item = usize>) -> BTreeMap<usize, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    counts
}

fn hash_candidates(candidates: &[V12Candidate]) -> String {
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
        hasher.update(candidate.actual_panel_size.to_le_bytes());
        hasher.update(candidate.expected_panel_size.to_le_bytes());
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
    use crate::app::market_velocity_event_backtest::RadarEvent;

    #[test]
    fn stable_43_of_44_panel_preserves_exact_same_direction_candidate() {
        let source = vec![fixture_candidate()];
        let ranked = vec![StablePanelRankEvent {
            event: RadarEvent {
                id: 1,
                exchange: "okx".to_owned(),
                symbol: "BTC-USDT-SWAP".to_owned(),
                ts: 6 * MS_15M,
                detected_at: "fixture".to_owned(),
                new_rank: 1,
                delta_rank: 0,
                current_price: 100.0,
                price_change_pct: 0.2,
            },
            actual_panel_size: 43,
            expected_panel_size: 44,
        }];
        let (candidates, stages) = filter_candidates(&source, &ranked).expect("filter");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].actual_panel_size, 43);
        assert_eq!(stages.same_direction_rank_event_found, 1);
    }

    fn fixture_candidate() -> V2Candidate {
        V2Candidate {
            symbol: "BTC-USDT-SWAP".to_owned(),
            direction: "long",
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
}
