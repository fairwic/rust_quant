//! V11 冻结候选账本的下一根开盘 L2 多币种成本诊断。

use super::super::l2::{
    replay::{replay_verified_candidate_ledger, ReplaySource},
    V10L2Identity, V10L2Report,
};
use super::*;
use anyhow::bail;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// V11 L2 成交和退出身份；保持 V10 风险合同，只改变候选生命周期。
pub const V11_L2_RULE_VERSION: &str =
    "l2_v11_timeout96_next_open_sl04_r052_hold24h_cost8bps_symbol_lock_v1";

const EXPECTED_L1_REPORT_SHA256: &str =
    "ebc2b886d1a64e6900e81900bc59a5b0a93c245437d8537642dbfe4f9b64e1e6";
const EXPECTED_DATASET_FINGERPRINT_SHA256: &str =
    "67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997";
const EXPECTED_L1_CANDIDATES: usize = 17_041;
const EXPECTED_RETURNED_SYMBOLS: usize = 60;
const EXPECTED_ELIGIBLE_SYMBOLS: usize = 44;
const EXPECTED_EXCLUDED_SYMBOLS: usize = 16;
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;

/// 执行 V11 Research-only L2 回放，不写数据库或注册运行态策略。
pub async fn run_v11_l2_replay(l1_source: &Path, output: &Path) -> Result<V10L2Report> {
    let bytes = std::fs::read(l1_source)
        .with_context(|| format!("读取 V11 L1 报告失败：{}", l1_source.display()))?;
    let report_sha256 = sha256_hex(&bytes);
    if report_sha256 != EXPECTED_L1_REPORT_SHA256 {
        bail!("V11 L1 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析 V11 L1 报告失败")?;
    validate_source_identity(&source)?;

    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V11 L2 replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let rebuilt = build_v11_report(&data)?;
    if rebuilt.coverage.dataset_fingerprint_sha256 != EXPECTED_DATASET_FINGERPRINT_SHA256
        || rebuilt.summary.candidate_count != EXPECTED_L1_CANDIDATES
        || rebuilt.coverage.returned_symbol_count != EXPECTED_RETURNED_SYMBOLS
        || rebuilt.coverage.eligible_symbol_count != EXPECTED_ELIGIBLE_SYMBOLS
        || rebuilt.coverage.excluded_symbols.len() != EXPECTED_EXCLUDED_SYMBOLS
    {
        bail!("reloaded V11 L1 identity mismatch");
    }
    let rebuilt_candidates = serde_json::to_value(&rebuilt.candidates)?;
    if source.pointer("/candidates") != Some(&rebuilt_candidates) {
        bail!("reloaded V11 candidate ledger differs from frozen L1");
    }

    let report = replay_verified_candidate_ledger(
        &data,
        ReplaySource::new(
            "market_momentum_ema576_post_signal_cross_timeout_l2_v11",
            V10L2Identity {
                level: "L2_local_multi_symbol_diagnostic",
                candidate_key: V11_CANDIDATE_KEY,
                source_l1_rule_version: V11_RULE_VERSION,
                rule_version: V11_L2_RULE_VERSION,
                only_variable: "evaluate the frozen 96-bar post-signal EMA144/576 cross-timeout candidate ledger under the unchanged V10 execution, risk, exit, cost, and symbol-lock contract",
                entry_policy: "signal is known only after its close; enter exactly at the next contiguous 15m candle open, otherwise block without compensation",
                initial_stop_policy: "4 percent of actual entry price, mirrored by direction",
                target_policy: "fixed 0.52R from actual entry with no break-even, trailing, partial, runner, or reversal",
                intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
                symbol_position_policy: "one open trade per symbol; signals through the exit candle are ignored; equal-time long sorts before short",
                per_side_cost_rate: PER_SIDE_COST_RATE,
                max_holding_ms: MAX_HOLDING_MS,
                funding_modeled: false,
                outcome_evaluation_performed: true,
                runtime_boundary: "research-only V11 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
            },
            report_sha256,
            rebuilt.coverage.dataset_fingerprint_sha256,
            rebuilt.coverage.returned_symbol_count,
            rebuilt.coverage.eligible_symbol_count,
            rebuilt.coverage.excluded_symbols.len(),
            super::super::l2::SetupEntryPolicy::AllowRepeated,
            super::super::l2::InitialRiskPolicy::FixedFourPercent,
            super::super::l2::TargetRiskPolicy::FixedGrossR,
            super::super::l2::EntryRiskGatePolicy::AllowAnyPositiveRisk,
            rebuilt.candidates,
        ),
    );
    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 V11 L2 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 V11 L2 报告失败：{}", output.display()))?;
    Ok(report)
}

/// 校验 L1 的策略、行情、候选数、无 outcome 和 ALGO 日期链晋级门禁。
pub(super) fn validate_source_identity(source: &Value) -> Result<()> {
    let string_at = |pointer: &str| {
        source
            .pointer(pointer)
            .and_then(Value::as_str)
            .with_context(|| format!("V11 L1 missing string field {pointer}"))
    };
    let usize_at = |pointer: &str| -> Result<usize> {
        let value = source
            .pointer(pointer)
            .and_then(Value::as_u64)
            .with_context(|| format!("V11 L1 missing unsigned field {pointer}"))?;
        usize::try_from(value).with_context(|| format!("V11 L1 field {pointer} exceeds usize"))
    };
    if string_at("/schema_version")? != "market_momentum_ema576_post_signal_cross_timeout_l1_v11"
        || string_at("/identity/candidate_key")? != V11_CANDIDATE_KEY
        || string_at("/identity/rule_version")? != V11_RULE_VERSION
    {
        bail!("V11 L1 strategy identity mismatch");
    }
    if string_at("/coverage/dataset_fingerprint_sha256")? != EXPECTED_DATASET_FINGERPRINT_SHA256
        || usize_at("/coverage/returned_symbol_count")? != EXPECTED_RETURNED_SYMBOLS
        || usize_at("/coverage/eligible_symbol_count")? != EXPECTED_ELIGIBLE_SYMBOLS
        || source
            .pointer("/coverage/excluded_symbols")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(EXPECTED_EXCLUDED_SYMBOLS)
        || usize_at("/summary/candidate_count")? != EXPECTED_L1_CANDIDATES
        || source
            .pointer("/candidates")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(EXPECTED_L1_CANDIDATES)
    {
        bail!("V11 L1 dataset, universe, or candidate count mismatch");
    }
    if string_at("/decision/status")? != "coverage_pass_ready_for_l2_prereg"
        || source
            .pointer("/decision/outcome_evaluation_performed")
            .and_then(Value::as_bool)
            != Some(false)
        || source
            .pointer("/algo_timeout_audit/passed")
            .and_then(Value::as_bool)
            != Some(true)
    {
        bail!("V11 L1 is not eligible for outcome replay");
    }
    if source
        .pointer("/candidates")
        .and_then(Value::as_array)
        .is_some_and(|candidates| {
            candidates.iter().any(|candidate| {
                candidate.get("execution_status").and_then(Value::as_str)
                    != Some("signal_confirmed_next_bar_open_not_evaluated_l1")
            })
        })
    {
        bail!("V11 L1 candidate execution boundary mismatch");
    }
    Ok(())
}

/// 计算冻结 L1 文件的十六进制 SHA-256，防止回放源被静默替换。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
