//! V11 信号账本下，同一长期资格 setup 只允许首笔真实成交的 L2 诊断。
//!
//! 信号仍完整保留用于审计；只有成功解析且未被持仓锁阻塞的实际成交才消费 setup。

pub mod structural_stop_v13;

use super::super::l2::{
    replay::{replay_verified_candidate_ledger, ReplaySource},
    EntryRiskGatePolicy, InitialRiskPolicy, SetupEntryPolicy, TargetRiskPolicy, V10L2Identity,
    V10L2Report,
};
use super::*;
use anyhow::bail;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// V12 独立研究身份；入场生命周期变化不能覆盖 V11。
pub const V12_CANDIDATE_KEY: &str = "market_momentum_ema576_first_filled_entry_per_setup_15m_v12";
/// V12 精确 L2 规则版本；固定风险与退出仍沿用 V11。
pub const V12_L2_RULE_VERSION: &str =
    "l2_v12_v11_first_filled_per_setup_next_open_sl04_r052_hold24h_cost8bps_v1";

const EXPECTED_V11_L1_REPORT_SHA256: &str =
    "ebc2b886d1a64e6900e81900bc59a5b0a93c245437d8537642dbfe4f9b64e1e6";
const PER_SIDE_COST_RATE: f64 = 0.0008;
const MAX_HOLDING_MS: i64 = 24 * 60 * 60 * 1_000;

/// 执行 V12 首笔成交限制回放，并保持 V11 的 4% 初始止损。
pub async fn run_v12_l2_replay(l1_source: &Path, output: &Path) -> Result<V10L2Report> {
    let report = replay_v11_variant(
        l1_source,
        "market_momentum_ema576_first_filled_entry_per_setup_l2_v12",
        V10L2Identity {
            level: "L2_local_multi_symbol_diagnostic",
            candidate_key: V12_CANDIDATE_KEY,
            source_l1_rule_version: V11_RULE_VERSION,
            rule_version: V12_L2_RULE_VERSION,
            only_variable: "after the first real fill for a symbol, direction, and long-term qualification setup, block every later candidate from that same setup",
            entry_policy: "signals remain auditable; enter at the next contiguous 15m open only when the symbol is unlocked and the setup has no prior real fill; blocked or unresolved signals do not consume the setup",
            initial_stop_policy: "unchanged V11 fixed 4 percent of actual entry price, mirrored by direction",
            target_policy: "unchanged fixed 0.52R from actual entry with no break-even, trailing, partial, runner, or reversal",
            intrabar_conflict_policy: "entry candle included; stop first when stop and target are both touched in one candle",
            symbol_position_policy: "one open trade per symbol plus one filled trade per symbol x direction x setup_ts; equal-time long sorts before short",
            per_side_cost_rate: PER_SIDE_COST_RATE,
            max_holding_ms: MAX_HOLDING_MS,
            funding_modeled: false,
            outcome_evaluation_performed: true,
            runtime_boundary: "research-only V12 L2; not registered in paper, readonly shadow, live worker, compose, or production presets",
        },
        SetupEntryPolicy::FirstFilledPerSetup,
        InitialRiskPolicy::FixedFourPercent,
    )
    .await?;
    write_report(output, &report, "V12")?;
    Ok(report)
}

/// 重建并逐字段校验 V11 账本后，才允许研究版本改变成交或风险政策。
pub(super) async fn replay_v11_variant(
    l1_source: &Path,
    schema_version: &'static str,
    identity: V10L2Identity,
    setup_entry_policy: SetupEntryPolicy,
    initial_risk_policy: InitialRiskPolicy,
) -> Result<V10L2Report> {
    let bytes = std::fs::read(l1_source)
        .with_context(|| format!("读取 V11 L1 报告失败：{}", l1_source.display()))?;
    let report_sha256 = sha256_hex(&bytes);
    if report_sha256 != EXPECTED_V11_L1_REPORT_SHA256 {
        bail!("V12/V13 source V11 L1 report SHA mismatch");
    }
    let source: Value = serde_json::from_slice(&bytes).context("解析 V11 L1 报告失败")?;
    super::l2::validate_source_identity(&source)?;

    let args = frozen_l1_args()?;
    let config = config_from_env_and_args(args)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .context("connect quant_core Postgres for V12/V13 verified replay")?;
    let data = load_backtest_data(&pool, &config.args).await?;
    let rebuilt = build_v11_report(&data)?;
    let rebuilt_candidates = serde_json::to_value(&rebuilt.candidates)?;
    if source.pointer("/candidates") != Some(&rebuilt_candidates)
        || source
            .pointer("/coverage/dataset_fingerprint_sha256")
            .and_then(Value::as_str)
            != Some(rebuilt.coverage.dataset_fingerprint_sha256.as_str())
        || source
            .pointer("/coverage/returned_symbol_count")
            .and_then(Value::as_u64)
            != Some(rebuilt.coverage.returned_symbol_count as u64)
        || source
            .pointer("/coverage/eligible_symbol_count")
            .and_then(Value::as_u64)
            != Some(rebuilt.coverage.eligible_symbol_count as u64)
        || source
            .pointer("/coverage/excluded_symbols")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(rebuilt.coverage.excluded_symbols.len())
    {
        bail!("reloaded V11 dataset or candidate ledger differs from frozen L1");
    }

    Ok(replay_verified_candidate_ledger(
        &data,
        ReplaySource::new(
            schema_version,
            identity,
            report_sha256,
            rebuilt.coverage.dataset_fingerprint_sha256,
            rebuilt.coverage.returned_symbol_count,
            rebuilt.coverage.eligible_symbol_count,
            rebuilt.coverage.excluded_symbols.len(),
            setup_entry_policy,
            initial_risk_policy,
            TargetRiskPolicy::FixedGrossR,
            EntryRiskGatePolicy::AllowAnyPositiveRisk,
            rebuilt.candidates,
        ),
    ))
}

/// 序列化独立机器结果，不写数据库或改变任何运行态注册。
pub(super) fn write_report(output: &Path, report: &V10L2Report, version: &str) -> Result<()> {
    let serialized = serde_json::to_string_pretty(report)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建 {version} L2 报告目录失败：{}", parent.display()))?;
    }
    std::fs::write(output, format!("{serialized}\n"))
        .with_context(|| format!("写入 {version} L2 报告失败：{}", output.display()))?;
    Ok(())
}

/// 计算冻结 V11 文件身份，防止研究入口静默切换候选账本。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
