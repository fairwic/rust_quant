//! V9：保持 V6 入场不变，只验证 4% 止损对应的 2.0R 固定目标几何。
//!
//! L1 只反序列化源账本的信号时字段，不加载成交后的 K 线或收益标签。

use super::persistent_dynamic_retest_v2::{V6_CANDIDATE_KEY, V6_RULE_VERSION};
use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// V9 使用独立候选键，避免改变 V6 已冻结的交易语义。
pub const V9_CANDIDATE_KEY: &str = "market_momentum_ema144_576_persistent_retest_target2r_15m_v9";
/// V9 L1 只验证保护价格几何，不评估成交后的结果。
pub const V9_L1_RULE_VERSION: &str = "l1_v6_entry_fixed_sl04_target2r_geometry_v9";
/// V9 L2 固定成交、风险、退出和压力成本身份。
pub const V9_L2_RULE_VERSION: &str = "l2_v6_touch_limit_sl04_r20_hold24h_cost8bps_v1";

const EXPECTED_SOURCE_SHA256: &str =
    "a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_CANDIDATES: usize = 54_837;
const EXPECTED_TARGETS: usize = 3;
const STOP_LOSS_PCT: f64 = 0.04;
const TARGET_R: f64 = 2.0;

#[derive(Debug, Deserialize)]
struct SourceIdentity {
    candidate_key: String,
    rule_version: String,
}

#[derive(Debug, Deserialize)]
struct SourceCoverage {
    returned_symbol_count: usize,
    eligible_symbol_count: usize,
    dataset_fingerprint_sha256: String,
}

#[derive(Debug, Deserialize)]
struct SourceSummary {
    candidate_count: usize,
    by_direction: BTreeMap<String, usize>,
    by_symbol: BTreeMap<String, usize>,
    by_month_utc: BTreeMap<String, usize>,
    effective_market_events: usize,
}

#[derive(Debug, Deserialize)]
struct SourceDecision {
    status: String,
    outcome_evaluation_performed: bool,
}

/// 这里只读取信号时已经可见的限价边界；serde 会忽略源账本的其余诊断字段。
#[derive(Debug, Deserialize)]
struct SourceCandidate {
    symbol: String,
    direction: String,
    signal_ts_ms: i64,
    touch_zone_boundary: f64,
}

/// 用户三张图的命中时间来自 V6 无标签审计，V9 不重新解释入场。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct V9TargetAudit {
    pub name: String,
    pub symbol: String,
    pub direction: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub matched_signal_timestamps_ms: Vec<i64>,
    pub matched: bool,
}

#[derive(Debug, Deserialize)]
struct SourceReport {
    identity: SourceIdentity,
    coverage: SourceCoverage,
    summary: SourceSummary,
    decision: SourceDecision,
    target_audits: Vec<V9TargetAudit>,
    candidates: Vec<SourceCandidate>,
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Long,
    Short,
}

impl Direction {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "long" => Some(Self::Long),
            "short" => Some(Self::Short),
            _ => None,
        }
    }
}

/// 冻结源身份与 V9 唯一变量。
#[derive(Debug, Serialize)]
pub struct V9L1Identity {
    pub level: &'static str,
    pub candidate_key: &'static str,
    pub rule_version: &'static str,
    pub source_candidate_key: &'static str,
    pub source_l1_rule_version: &'static str,
    pub only_variable: &'static str,
    pub label_boundary: &'static str,
    pub runtime_boundary: &'static str,
}

/// 4% 初始风险和 2.0R 目标对应的多空镜像因子。
#[derive(Debug, Serialize)]
pub struct V9ProtectionGeometry {
    pub stop_loss_pct: f64,
    pub target_r: f64,
    pub long_stop_factor: f64,
    pub long_target_factor: f64,
    pub short_stop_factor: f64,
    pub short_target_factor: f64,
}

/// V9 不改变入场覆盖，只报告源账本中保护价可构造性。
#[derive(Debug, Serialize)]
pub struct V9L1Summary {
    pub source_candidates: usize,
    pub valid_geometry_candidates: usize,
    pub invalid_geometry_candidates: usize,
    pub invalid_candidate_examples: Vec<String>,
    pub by_direction: BTreeMap<String, usize>,
    pub symbol_count: usize,
    pub month_count: usize,
    pub effective_market_events: usize,
    pub returned_symbol_count: usize,
    pub eligible_symbol_count: usize,
}

/// 预注册的无标签 L1 门禁结果。
#[derive(Debug, Serialize)]
pub struct V9L1Decision {
    pub status: &'static str,
    pub gates: BTreeMap<&'static str, bool>,
    pub reason: String,
    pub outcome_evaluation_performed: bool,
}

/// V9 L1 几何机器报告，不包含逐笔结果字段。
#[derive(Debug, Serialize)]
pub struct V9L1Report {
    pub schema_version: &'static str,
    pub generated_at_utc: String,
    pub identity: V9L1Identity,
    pub source_l1_report_sha256: String,
    pub dataset_fingerprint_sha256: String,
    pub protection_geometry: V9ProtectionGeometry,
    pub summary: V9L1Summary,
    pub target_audits: Vec<V9TargetAudit>,
    pub decision: V9L1Decision,
}

/// 校验冻结 V6 账本并写出 V9 的 L1 保护价几何报告。
pub fn run_l1_geometry(source: &Path, output: &Path) -> Result<V9L1Report> {
    let bytes = std::fs::read(source)
        .with_context(|| format!("读取 EMA144/576 V6 L1 报告失败：{}", source.display()))?;
    let source_sha256 = sha256_hex(&bytes);
    if source_sha256 != EXPECTED_SOURCE_SHA256 {
        bail!("V9 source V6 L1 report SHA mismatch");
    }
    let source: SourceReport =
        serde_json::from_slice(&bytes).context("解析 EMA144/576 V6 L1 报告失败")?;
    validate_source_identity(&source)?;
    let report = build_report(source, source_sha256);
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 EMA144/576 V9 L1 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 EMA144/576 V9 L1 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 源账本必须仍是未读取结果且已获准预注册 L2 的 V6 完整身份。
fn validate_source_identity(source: &SourceReport) -> Result<()> {
    if source.identity.candidate_key != V6_CANDIDATE_KEY
        || source.identity.rule_version != V6_RULE_VERSION
    {
        bail!("V9 source V6 L1 strategy identity mismatch");
    }
    if source.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256 {
        bail!("V9 source V6 L1 dataset fingerprint mismatch");
    }
    if source.summary.candidate_count != EXPECTED_CANDIDATES
        || source.candidates.len() != EXPECTED_CANDIDATES
    {
        bail!("V9 source V6 L1 candidate count mismatch");
    }
    if source.decision.status != "coverage_pass_ready_for_l2_prereg"
        || source.decision.outcome_evaluation_performed
    {
        bail!("V9 source V6 L1 is not outcome-free and L2-eligible");
    }
    Ok(())
}

/// 对每个因果限价边界验证镜像保护价，不读取信号后的行情。
fn build_report(source: SourceReport, source_sha256: String) -> V9L1Report {
    let mut valid_geometry_candidates = 0;
    let mut invalid_examples = Vec::new();
    for candidate in &source.candidates {
        let valid = Direction::parse(&candidate.direction)
            .and_then(|direction| protection_prices(candidate.touch_zone_boundary, direction).ok())
            .is_some();
        if valid {
            valid_geometry_candidates += 1;
        } else if invalid_examples.len() < 20 {
            invalid_examples.push(format!(
                "{}:{}:{}",
                candidate.symbol, candidate.signal_ts_ms, candidate.direction
            ));
        }
    }
    let invalid_geometry_candidates = source
        .candidates
        .len()
        .saturating_sub(valid_geometry_candidates);
    let all_targets_match = source.target_audits.len() == EXPECTED_TARGETS
        && source.target_audits.iter().all(|audit| audit.matched);
    let mut gates = BTreeMap::new();
    gates.insert("source_l1_sha_identity_and_dataset_verified", true);
    gates.insert("source_l1_outcome_free_and_l2_eligible", true);
    gates.insert(
        "all_source_candidates_preserved",
        source.candidates.len() == EXPECTED_CANDIDATES,
    );
    gates.insert(
        "all_candidates_have_valid_4pct_stop_and_2r_target",
        invalid_geometry_candidates == 0,
    );
    gates.insert("all_three_user_targets_preserved", all_targets_match);
    let passed = gates.values().all(|passed| *passed);
    let decision = V9L1Decision {
        status: if passed {
            "coverage_pass_ready_for_l2_prereg"
        } else {
            "stop_geometry_gate_failed"
        },
        gates,
        reason: if passed {
            "V9 仅改变目标倍数，V6 全部入场候选与三张目标图保持不变，且 2.0R 多空保护价几何全部有效；下一步只能先冻结 L2 成本回放清单。".to_owned()
        } else {
            "至少一项预注册的源身份、覆盖或保护价几何门禁失败；V9 停止且不得读取成交后结果。"
                .to_owned()
        },
        outcome_evaluation_performed: false,
    };
    V9L1Report {
        schema_version: "market_momentum_ema144_576_persistent_retest_target2r_l1_v9",
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        identity: V9L1Identity {
            level: "L1_quick_research_no_outcome_labels",
            candidate_key: V9_CANDIDATE_KEY,
            rule_version: V9_L1_RULE_VERSION,
            source_candidate_key: V6_CANDIDATE_KEY,
            source_l1_rule_version: V6_RULE_VERSION,
            only_variable: "change only the fixed target from 0.52R to 2.0R while preserving the complete V6 entry, stop, holding, cost, conflict, and universe contracts",
            label_boundary: "reads only source identity, signal-time direction and resting-limit geometry; no future candle, fill outcome, exit, MFE, MAE, R, win, loss, or PnL",
            runtime_boundary: "research-only V9 L1; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        source_l1_report_sha256: source_sha256,
        dataset_fingerprint_sha256: source.coverage.dataset_fingerprint_sha256,
        protection_geometry: V9ProtectionGeometry {
            stop_loss_pct: STOP_LOSS_PCT,
            target_r: TARGET_R,
            long_stop_factor: 1.0 - STOP_LOSS_PCT,
            long_target_factor: 1.0 + STOP_LOSS_PCT * TARGET_R,
            short_stop_factor: 1.0 + STOP_LOSS_PCT,
            short_target_factor: 1.0 - STOP_LOSS_PCT * TARGET_R,
        },
        summary: V9L1Summary {
            source_candidates: source.summary.candidate_count,
            valid_geometry_candidates,
            invalid_geometry_candidates,
            invalid_candidate_examples: invalid_examples,
            by_direction: source.summary.by_direction,
            symbol_count: source.summary.by_symbol.len(),
            month_count: source.summary.by_month_utc.len(),
            effective_market_events: source.summary.effective_market_events,
            returned_symbol_count: source.coverage.returned_symbol_count,
            eligible_symbol_count: source.coverage.eligible_symbol_count,
        },
        target_audits: source.target_audits,
        decision,
    }
}

/// 从正入场价构造 4% 止损和 2.0R 目标，并逐项校验 R 距离。
fn protection_prices(entry: f64, direction: Direction) -> Result<(f64, f64), &'static str> {
    if !entry.is_finite() || entry <= 0.0 {
        return Err("entry_price_invalid");
    }
    let (stop, target) = match direction {
        Direction::Long => (
            entry * (1.0 - STOP_LOSS_PCT),
            entry * (1.0 + STOP_LOSS_PCT * TARGET_R),
        ),
        Direction::Short => (
            entry * (1.0 + STOP_LOSS_PCT),
            entry * (1.0 - STOP_LOSS_PCT * TARGET_R),
        ),
    };
    let risk = (entry - stop).abs();
    let reward = (target - entry).abs();
    if !stop.is_finite()
        || !target.is_finite()
        || stop <= 0.0
        || target <= 0.0
        || !approx_equal(risk / entry, STOP_LOSS_PCT)
        || !approx_equal(reward / risk, TARGET_R)
    {
        return Err("risk_or_target_geometry_invalid");
    }
    Ok((stop, target))
}

fn approx_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9 * left.abs().max(right.abs()).max(1.0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2.0R 保护价必须在多空方向保持完全镜像。
    #[test]
    fn target2r_geometry_is_mirrored() {
        let (long_stop, long_target) =
            protection_prices(100.0, Direction::Long).expect("long geometry");
        let (short_stop, short_target) =
            protection_prices(100.0, Direction::Short).expect("short geometry");
        assert!(approx_equal(long_stop, 96.0));
        assert!(approx_equal(long_target, 108.0));
        assert!(approx_equal(short_stop, 104.0));
        assert!(approx_equal(short_target, 92.0));
    }

    /// 非正或不可计算的信号时限价不能进入结果回放。
    #[test]
    fn target2r_geometry_rejects_invalid_entry() {
        assert!(protection_prices(0.0, Direction::Long).is_err());
        assert!(protection_prices(f64::NAN, Direction::Short).is_err());
    }
}
