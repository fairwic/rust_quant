//! V12 稳定成交额面板候选的授权过滤与 L2 成本回放入口。

use super::*;
use crate::app::market_velocity_event_backtest::ema144_576_breakout_retest_l1::reexpansion_volume_rank_stable_panel_v12::{
    V12_CANDIDATE_KEY, V12_RULE_VERSION,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

/// V12 L2 固定沿用 V6 的成交、风险、退出与成本，只过滤 L1 已授权候选。
pub const V12_L2_RULE_VERSION: &str = "l2_v12_touch_limit_sl04_r052_hold24h_cost8bps_v1";

const EXPECTED_V12_L1_SHA256: &str =
    "201114b4ae1e519793f2988f000e00b5751c05fffc710ad0146e28340eb14dd1";
const EXPECTED_V11_L1_SHA256: &str =
    "02bbb99a7337c5213c25e5d503268d13c49bd4fe84d4aeeb61d69da6677104dd";
const EXPECTED_V12_CANDIDATE_LEDGER_SHA256: &str =
    "e597abb7cb3eac6318a32556101bfbe91440d1f78cdaf7901d171ecf881c0cb4";
const EXPECTED_V12_CANDIDATES: usize = 48_048;

const V12_REPLAY: ReplayVariant = ReplayVariant {
    schema_version: "market_momentum_ema144_576_reexpansion_volume_rank_stable_panel_l2_v12",
    candidate_key: V12_CANDIDATE_KEY,
    source_l1_rule_version: V12_RULE_VERSION,
    expected_l1_candidates: EXPECTED_V12_CANDIDATES,
    rule_version: V12_L2_RULE_VERSION,
    only_variable: "filter the frozen V6 EMA144/576 retest entries to the V12 exact re-expansion same-direction stable 95 percent volume-rank panel authorization",
    target_policy: "fixed 0.52R from actual entry price with no protection, trailing, partial, or runner",
    target_r: V6_TARGET_R,
    runtime_boundary: "research-only V12 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
};

#[derive(Debug, Deserialize)]
struct AuthorizationIdentity {
    candidate_key: String,
    rule_version: String,
    source_v6_candidate_key: String,
    source_v6_rule_version: String,
    source_v11_rule_version: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationPanelContract {
    expected_universe_members: usize,
    minimum_coverage_pct: usize,
    minimum_available_members: usize,
    adjacent_snapshots_require_identical_members: bool,
    rank_lookback_candles: usize,
    minimum_delta_rank: i32,
}

#[derive(Debug, Deserialize)]
struct AuthorizationSummary {
    source_candidate_count: usize,
    candidate_count: usize,
    candidate_ledger_sha256: String,
}

#[derive(Debug, Deserialize)]
struct AuthorizationTargetAudit {
    matched: bool,
}

#[derive(Debug, Deserialize)]
struct AuthorizationDecision {
    status: String,
    outcome_evaluation_performed: bool,
}

#[derive(Debug, Deserialize)]
struct AuthorizationCandidate {
    symbol: String,
    direction: String,
    signal_ts_ms: i64,
}

#[derive(Debug, Deserialize)]
struct AuthorizationReport {
    identity: AuthorizationIdentity,
    source_v6_l1_report_sha256: String,
    source_v11_l1_report_sha256: String,
    source_v6_dataset_fingerprint_sha256: String,
    returned_symbol_count: usize,
    eligible_symbol_count: usize,
    panel_contract: AuthorizationPanelContract,
    summary: AuthorizationSummary,
    target_audits: Vec<AuthorizationTargetAudit>,
    decision: AuthorizationDecision,
    candidates: Vec<AuthorizationCandidate>,
}

/// 校验 V6 原始账本与 V12 无标签授权后，只回放授权候选。
pub async fn run_stable_panel_v12_l2_replay(
    v6_l1_source: &Path,
    v12_l1_authorization: &Path,
    output: &Path,
) -> Result<EmaRetestL2Report> {
    let (mut l1, _) = load_and_validate_l1(v6_l1_source)?;
    let (authorized_ids, authorization_sha256) = load_v12_authorization(v12_l1_authorization)?;
    l1.candidates
        .retain(|candidate| authorized_ids.contains(&candidate_id(candidate)));
    if l1.candidates.len() != EXPECTED_V12_CANDIDATES {
        bail!("V12 authorization did not map exactly onto V6 candidates");
    }
    let retained_ids = l1
        .candidates
        .iter()
        .map(candidate_id)
        .collect::<BTreeSet<_>>();
    if retained_ids != authorized_ids {
        bail!("V12 retained candidate identities differ from authorization ledger");
    }

    let config = config_from_env_and_args(frozen_l1_args()?)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for EMA144/576 V12 L2 replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let report = build_l2_report(&data, l1, authorization_sha256, V12_REPLAY)?;
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("创建 EMA144/576 V12 L2 报告目录失败：{}", parent.display())
        })?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V12 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 完整 SHA、数据身份、面板政策和 3/3 目标共同授权 outcome 回放。
fn load_v12_authorization(source: &Path) -> Result<(BTreeSet<String>, String)> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 EMA144/576 V12 L1 授权失败：{}", source.display()))?;
    let sha256 = sha256_hex(&bytes);
    if sha256 != EXPECTED_V12_L1_SHA256 {
        bail!("V12 L1 authorization SHA mismatch");
    }
    let report: AuthorizationReport =
        serde_json::from_slice(&bytes).context("解析 EMA144/576 V12 L1 授权失败")?;
    if report.identity.candidate_key != V12_CANDIDATE_KEY
        || report.identity.rule_version != V12_RULE_VERSION
        || report.identity.source_v6_candidate_key != V6_CANDIDATE_KEY
        || report.identity.source_v6_rule_version != V6_RULE_VERSION
        || report.identity.source_v11_rule_version
            != "l1_v6_reexpansion_same_candle_volume_rank_nonworse_v11"
        || report.source_v6_l1_report_sha256 != EXPECTED_L1_REPORT_SHA256
        || report.source_v11_l1_report_sha256 != EXPECTED_V11_L1_SHA256
        || report.source_v6_dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || report.returned_symbol_count != 60
        || report.eligible_symbol_count != 44
        || report.panel_contract.expected_universe_members != 44
        || report.panel_contract.minimum_coverage_pct != 95
        || report.panel_contract.minimum_available_members != 42
        || !report
            .panel_contract
            .adjacent_snapshots_require_identical_members
        || report.panel_contract.rank_lookback_candles != 96
        || report.panel_contract.minimum_delta_rank != 0
        || report.summary.source_candidate_count != EXPECTED_L1_CANDIDATES
        || report.summary.candidate_count != EXPECTED_V12_CANDIDATES
        || report.summary.candidate_ledger_sha256 != EXPECTED_V12_CANDIDATE_LEDGER_SHA256
        || report.target_audits.len() != 3
        || !report.target_audits.iter().all(|target| target.matched)
        || report.decision.status != "coverage_pass_ready_for_l2_prereg"
        || report.decision.outcome_evaluation_performed
        || report.candidates.len() != EXPECTED_V12_CANDIDATES
    {
        bail!("V12 L1 authorization identity or coverage gate mismatch");
    }
    let authorized_ids = report
        .candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}:{}:{}",
                candidate.symbol, candidate.signal_ts_ms, candidate.direction
            )
        })
        .collect::<BTreeSet<_>>();
    if authorized_ids.len() != EXPECTED_V12_CANDIDATES {
        bail!("V12 L1 authorization contains duplicate candidate identities");
    }
    Ok((authorized_ids, sha256))
}

fn candidate_id(candidate: &L1InputCandidate) -> String {
    format!(
        "{}:{}:{}",
        candidate.symbol, candidate.signal_ts_ms, candidate.direction
    )
}
