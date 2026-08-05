//! Surviving static Top60 的只读冻结与数据覆盖审计。
//!
//! 本模块刻意接受幸存者偏差：候选只来自运行时仍为 live 的 OKX USDT 永续。
//! 选中的 60 个成员先固化为 selection plan，之后才读取 K 线；数据缺口只会阻止
//! seal，绝不会触发替补，从而避免按数据可得性或盈亏事后换币。

use super::strict_static_universe::{
    canonical_candle_source_id, canonical_manifest_sha256, formal_gate,
    FrozenStaticCandleSourceFingerprintV2, StrictStaticMemberCoverageV2,
    StrictStaticMemberStatusV2, StrictStaticUniverseCoverageV2, StrictStaticUniverseManifestV2,
    StrictStaticUniverseMemberV2, STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION,
    STRICT_STATIC_CANDLE_INTERVAL_MS, STRICT_STATIC_CANDLE_SOURCE_KIND,
    STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT, STRICT_STATIC_MEMBER_COUNT,
    STRICT_STATIC_SCHEMA_VERSION, STRICT_STATIC_SELECTION_RULE_ID, STRICT_STATIC_VOLUME_FIELD,
    STRICT_STATIC_WARMUP_DAYS,
};
use super::Candle;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use reqwest::{Client, Proxy};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, postgres::PgRow, PgPool, Row};
use std::collections::BTreeSet;
use std::time::Duration;

/// 与既有冻结规则一致的预先承诺排序种子；不能根据覆盖率或 PnL 改写。
pub const SURVIVING_STATIC_TOP60_ORDER_SEED: &str = "top60_v36_direct_kline_20260721";
/// 稳定、可审计的静态幸存者币池选择规则。
pub const SURVIVING_STATIC_TOP60_SELECTION_RULE: &str = concat!(
    "surviving_static_top60_v2; current-live OKX linear USDT perpetuals; ",
    "current instruments listTime <= warmup_start; valid tickSz; ",
    "order by md5(top60_v36_direct_kline_20260721:symbol), symbol; first 60; ",
    "selection ignores candle coverage and PnL; delisted symbols excluded"
);

const DAY_MS: i64 = 86_400_000;
const SELECTION_PLAN_SCHEMA_VERSION: u32 = 1;

/// 构建冻结 selection plan 与严格覆盖快照所需的显式边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictStaticSnapshotBuildArgs {
    /// 新冻结 cohort 的唯一版本。
    pub universe_version: String,
    /// 正式评价起点，Unix 毫秒；预热起点固定向前 60 天。
    pub evaluation_start_ms: i64,
    /// 正式评价终点，Unix 毫秒，不包含该时刻。
    pub evaluation_end_exclusive_ms: i64,
    /// 可选显式代理；`None` 表示明确直连，不读取系统代理。
    pub proxy_url: Option<String>,
}

/// 即使行情不完整也必须落盘的 60 成员选择计划。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticSelectionPlanV2 {
    /// selection plan 自身的格式版本。
    pub schema_version: u32,
    /// 明示这是带幸存者偏差的静态 cohort，而不是历史 point-in-time universe。
    pub cohort_kind: String,
    /// 退市币已按用户口径排除。
    pub delisted_symbols_excluded: bool,
    /// 调用方明确接受只研究当前存续合约产生的幸存者偏差。
    pub survivorship_bias_accepted: bool,
    /// 与 sealed manifest 共用的版本身份。
    pub universe_version: String,
    /// 生成 selection plan 的时间，Unix 毫秒。
    pub generated_at_ms: i64,
    /// 读取 current-live instrument 快照的时间，Unix 毫秒。
    pub selection_timestamp_ms: i64,
    /// 完整 instrument 响应实际接收完成的时间，Unix 毫秒。
    pub instrument_snapshot_observed_at_ms: i64,
    /// 本次 current-live 资格唯一使用的 OKX 公共端点。
    pub instrument_source_endpoint: String,
    /// 当次完整 API envelope 递归排序后的 SHA-256。
    pub instrument_snapshot_sha256: String,
    /// 全局 60 天预热起点，包含。
    pub warmup_start_ms: i64,
    /// 正式评价起点，包含。
    pub evaluation_start_ms: i64,
    /// 正式评价终点，不包含。
    pub evaluation_end_exclusive_ms: i64,
    /// 固定预热天数。
    pub warmup_days: u32,
    /// 在查看覆盖率和回测结果前承诺的选币规则。
    pub selection_rule_id: String,
    /// 在查看覆盖率和回测结果前承诺的选币规则说明。
    pub selection_rule: String,
    /// 固定顺序的 60 个成员；后续 backfill 应只消费这组成员，不能自行替补。
    pub members: Vec<StrictStaticSelectionMemberV2>,
}

/// selection plan 中一名成员的冻结 instrument 事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticSelectionMemberV2 {
    /// OKX 原始合约标识。
    pub symbol: String,
    /// 由当次 OKX instrument `listTime` 冻结的上市时间。
    pub listed_at_ms: i64,
    /// MD5 预承诺排序后的 1-based 排名。
    pub rank: u32,
    /// 当前 live instrument 快照中的有效价格步长。
    pub frozen_tick_size: String,
    /// 规范化 instrument 语义内容的 SHA-256。
    pub instrument_source_sha256: String,
}

/// 一个成员可直接交给 Pine parity 回放器的冻结输入。
#[derive(Debug, Clone)]
pub struct StrictStaticSymbolSnapshotV2 {
    /// 与 selection plan 成员一致的 OKX symbol。
    pub symbol: String,
    /// 与 selection plan 一致的冻结 tick。
    pub tick_size: f64,
    /// `[warmup_start, evaluation_end_exclusive)` 内已确认的 OHLC+vol_ccy。
    pub candles: Vec<Candle>,
}

/// 冻结与覆盖审计的统一产物；JSON 不展开数百万根 K 线。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticSnapshotV2 {
    /// 无论数据完整与否都存在，供后续同源 backfill 直接读取。
    pub selection_plan: StrictStaticSelectionPlanV2,
    /// 60 个已选成员的逐币严格覆盖证据。
    pub coverage: StrictStaticUniverseCoverageV2,
    /// 只有正式门禁 60/60 时才存在。
    pub sealed_manifest: Option<StrictStaticUniverseManifestV2>,
    /// 完整覆盖的成员数量。
    pub complete_member_count: usize,
    /// `true` 当且仅当 sealed manifest 已通过正式门禁。
    pub sealed: bool,
    /// 阻止 seal 的稳定诊断；不会用于替换 selection plan 成员。
    pub seal_blockers: Vec<String>,
    /// 供同进程回放复用的精确输入，写 JSON 时有意省略。
    #[serde(skip, default)]
    pub symbols: Vec<StrictStaticSymbolSnapshotV2>,
}

#[derive(Debug)]
struct CandidateInstrument {
    symbol: String,
    listed_at_ms: i64,
    tick_size: String,
    instrument_source_sha256: String,
}

/// 从 quant_core 当前 live instrument 快照冻结 60 个成员并审计完整 15m 数据。
///
/// 该入口只读取 Core 专用连接串；K 线缺失返回 `sealed=false` 的正常诊断产物，
/// 使调用方可以先保存 selection plan，再按同一成员集合补数。
pub async fn build_strict_static_snapshot_from_quant_core(
    args: StrictStaticSnapshotBuildArgs,
) -> Result<StrictStaticSnapshotV2> {
    let selection_plan = freeze_strict_static_selection_plan(args).await?;
    audit_and_seal_strict_static_plan_from_quant_core(selection_plan).await
}

/// 从当次 OKX current-live instrument 响应冻结不可替换的 60 成员计划。
///
/// 该步骤不读取任何 K 线，因此成员资格不会被本地覆盖率、回测收益或已退市 ghost 影响。
pub async fn freeze_strict_static_selection_plan(
    args: StrictStaticSnapshotBuildArgs,
) -> Result<StrictStaticSelectionPlanV2> {
    validate_build_args(&args)?;
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(i64::from(STRICT_STATIC_WARMUP_DAYS) * DAY_MS)
        .context("计算 surviving static Top60 的 60 天预热起点时溢出")?;
    // Current-live 资格必须来自当次交易所响应；Core 的 upsert 表可能残留已退市 ghost。
    let (candidates, instrument_snapshot_sha256, instrument_snapshot_observed_at_ms) =
        load_current_live_okx_candidates(warmup_start_ms, args.proxy_url.as_deref()).await?;
    if instrument_snapshot_observed_at_ms < args.evaluation_end_exclusive_ms {
        bail!("current-live instrument 快照必须在评价窗口结束后观测，不能用未来窗口冻结成员");
    }
    if candidates.len() < STRICT_STATIC_MEMBER_COUNT {
        bail!(
            "OKX 当次公开响应中符合 live、USDT linear、上市时间和 tick 条件的永续只有 {} 个，无法冻结 60 个成员",
            candidates.len()
        );
    }
    let selection_timestamp_ms = Utc::now()
        .timestamp_millis()
        .max(instrument_snapshot_observed_at_ms);
    let generated_at_ms = Utc::now().timestamp_millis().max(selection_timestamp_ms);
    let selection_plan = build_selection_plan(
        &args,
        warmup_start_ms,
        instrument_snapshot_observed_at_ms,
        selection_timestamp_ms,
        generated_at_ms,
        instrument_snapshot_sha256,
        candidates
            .into_iter()
            .take(STRICT_STATIC_MEMBER_COUNT)
            .collect(),
    );
    validate_selection_plan(&selection_plan)?;
    Ok(selection_plan)
}

/// 只按已冻结计划读取 quant_core 并生成覆盖证据；不会访问 OKX 或重新排序成员。
pub async fn audit_and_seal_strict_static_plan_from_quant_core(
    selection_plan: StrictStaticSelectionPlanV2,
) -> Result<StrictStaticSnapshotV2> {
    validate_selection_plan(&selection_plan)?;
    let pool = connect_read_only_quant_core().await?;
    let result = build_snapshot_from_pool(&pool, selection_plan).await;
    pool.close().await;
    result
}

/// selection plan 已冻结后，仅按其中成员从同一个 Core 只读连接加载覆盖。
async fn build_snapshot_from_pool(
    pool: &PgPool,
    selection_plan: StrictStaticSelectionPlanV2,
) -> Result<StrictStaticSnapshotV2> {
    validate_selection_plan(&selection_plan)?;
    let warmup_start_ms = selection_plan.warmup_start_ms;
    let evaluation_end_exclusive_ms = selection_plan.evaluation_end_exclusive_ms;
    let expected_count = exact_candle_count(warmup_start_ms, evaluation_end_exclusive_ms)?;
    let mut symbols = Vec::with_capacity(STRICT_STATIC_MEMBER_COUNT);
    let mut coverages = Vec::with_capacity(STRICT_STATIC_MEMBER_COUNT);
    let mut seal_blockers = Vec::new();

    for member in &selection_plan.members {
        let tick_size = parse_tick_size_for_replay(&member.frozen_tick_size, &member.symbol)?;
        let loaded = load_symbol_candles(
            pool,
            &member.symbol,
            warmup_start_ms,
            evaluation_end_exclusive_ms,
        )
        .await;
        let candles = match loaded {
            Ok(candles) => candles,
            Err(error) => {
                seal_blockers.push(format!("{} K线读取失败：{error:#}", member.symbol));
                Vec::new()
            }
        };
        let coverage = build_member_coverage(
            member,
            &candles,
            warmup_start_ms,
            evaluation_end_exclusive_ms,
            expected_count,
        )?;
        if !member_coverage_is_complete(
            &coverage,
            warmup_start_ms,
            evaluation_end_exclusive_ms,
            expected_count,
        ) {
            seal_blockers.push(format!(
                "{} 覆盖不完整：loaded={} expected={} missing={} contiguous={}",
                member.symbol,
                coverage.loaded_candle_count,
                coverage.expected_candle_count,
                coverage.missing_candle_count,
                coverage.is_contiguous_15m
            ));
        }
        coverages.push(coverage);
        symbols.push(StrictStaticSymbolSnapshotV2 {
            symbol: member.symbol.clone(),
            tick_size,
            candles,
        });
    }

    let mut coverage = StrictStaticUniverseCoverageV2 {
        universe_version: selection_plan.universe_version.clone(),
        manifest_sha256: String::new(),
        warmup_start_ms,
        evaluation_start_ms: selection_plan.evaluation_start_ms,
        evaluation_end_exclusive_ms,
        members: coverages,
    };
    let complete_member_count = coverage
        .members
        .iter()
        .filter(|member| {
            member_coverage_is_complete(
                member,
                warmup_start_ms,
                evaluation_end_exclusive_ms,
                expected_count,
            )
        })
        .count();
    let sealed_manifest =
        if complete_member_count == STRICT_STATIC_MEMBER_COUNT && seal_blockers.is_empty() {
            let manifest = manifest_from_complete_snapshot(&selection_plan, &coverage);
            coverage.manifest_sha256 = canonical_manifest_sha256(&manifest)
                .context("计算 surviving static Top60 sealed manifest SHA-256 失败")?;
            formal_gate(&manifest, &coverage)
                .context("surviving static Top60 完整覆盖仍未通过正式门禁")?;
            Some(manifest)
        } else {
            None
        };

    Ok(StrictStaticSnapshotV2 {
        selection_plan,
        coverage,
        sealed: sealed_manifest.is_some(),
        sealed_manifest,
        complete_member_count,
        seal_blockers,
        symbols,
    })
}

/// 从冻结器初始或 sealed JSON 的根节点读取并严格验证 `selection_plan`。
pub fn decode_and_validate_selection_plan_from_snapshot(
    raw: &[u8],
) -> Result<StrictStaticSelectionPlanV2> {
    let root: Value =
        serde_json::from_slice(raw).context("解析 surviving static Top60 快照 JSON 失败")?;
    let plan = root
        .get("selection_plan")
        .cloned()
        .context("surviving static Top60 快照缺少 root.selection_plan")?;
    let plan: StrictStaticSelectionPlanV2 =
        serde_json::from_value(plan).context("解析 root.selection_plan 失败")?;
    validate_selection_plan(&plan)?;
    Ok(plan)
}

/// 验证已保存计划的时间、来源、原始 tick、成员顺序和固定半开窗口。
pub fn validate_selection_plan(plan: &StrictStaticSelectionPlanV2) -> Result<()> {
    if plan.schema_version != SELECTION_PLAN_SCHEMA_VERSION
        || plan.cohort_kind != "surviving_static_top60"
        || !plan.delisted_symbols_excluded
        || !plan.survivorship_bias_accepted
    {
        bail!("selection plan 必须是明确排除退市币并接受幸存者偏差的 schema v1");
    }
    if plan.universe_version.trim().is_empty()
        || plan.generated_at_ms <= 0
        || plan.selection_timestamp_ms <= 0
        || plan.instrument_snapshot_observed_at_ms <= 0
        || plan.instrument_snapshot_observed_at_ms > plan.selection_timestamp_ms
        || plan.selection_timestamp_ms > plan.generated_at_ms
        || plan.instrument_snapshot_observed_at_ms < plan.evaluation_end_exclusive_ms
    {
        bail!("selection plan 缺少有效版本，或 instrument 观测、选择、生成时间顺序错误");
    }
    if plan.instrument_source_endpoint != STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT
        || plan.selection_rule_id != STRICT_STATIC_SELECTION_RULE_ID
        || plan.selection_rule != SURVIVING_STATIC_TOP60_SELECTION_RULE
    {
        bail!("selection plan 的 instrument 端点或预承诺选择规则与严格合同不一致");
    }
    validate_sha256_text(
        &plan.instrument_snapshot_sha256,
        "instrument_snapshot_sha256",
        None,
    )?;
    if plan.warmup_days != STRICT_STATIC_WARMUP_DAYS {
        bail!("selection plan 必须冻结 60 天预热");
    }
    let required_warmup_start_ms = plan
        .evaluation_start_ms
        .checked_sub(i64::from(STRICT_STATIC_WARMUP_DAYS) * DAY_MS)
        .context("计算 selection plan 预热起点时溢出")?;
    if plan.warmup_start_ms != required_warmup_start_ms {
        bail!("selection plan 的 warmup_start 必须等于 evaluation_start 前 60 天");
    }
    exact_candle_count(plan.warmup_start_ms, plan.evaluation_start_ms)?;
    exact_candle_count(plan.evaluation_start_ms, plan.evaluation_end_exclusive_ms)?;
    if plan.members.len() != STRICT_STATIC_MEMBER_COUNT {
        bail!("selection plan 必须恰好冻结 60 个成员");
    }

    let mut symbols = BTreeSet::new();
    let mut previous_order_key: Option<([u8; 16], String)> = None;
    for (index, member) in plan.members.iter().enumerate() {
        validate_symbol(&member.symbol)?;
        if !symbols.insert(member.symbol.as_str()) {
            bail!("selection plan 包含重复 symbol：{}", member.symbol);
        }
        let expected_rank = u32::try_from(index + 1).context("selection plan 成员排名超出 u32")?;
        if member.rank != expected_rank {
            bail!("selection plan 成员数组必须严格按排名 1 到 60 排列");
        }
        if member.listed_at_ms <= 0 || member.listed_at_ms > plan.warmup_start_ms {
            bail!("{} 的 listTime 不足以覆盖完整预热窗口", member.symbol);
        }
        validate_tick_size_text(&member.frozen_tick_size, &member.symbol)?;
        validate_sha256_text(
            &member.instrument_source_sha256,
            "instrument_source_sha256",
            Some(&member.symbol),
        )?;
        let order_key = (selection_md5_key(&member.symbol), member.symbol.clone());
        if previous_order_key
            .as_ref()
            .is_some_and(|previous| previous >= &order_key)
        {
            bail!("selection plan 成员没有遵循固定 MD5 后再按 symbol 的顺序");
        }
        previous_order_key = Some(order_key);
    }
    Ok(())
}

/// 解码正式 runner 可消费的 sealed 快照，并拒绝成员、覆盖或 manifest SHA 漂移。
pub fn decode_and_validate_sealed_snapshot(raw: &[u8]) -> Result<StrictStaticSnapshotV2> {
    let snapshot: StrictStaticSnapshotV2 =
        serde_json::from_slice(raw).context("解析 sealed surviving static Top60 快照失败")?;
    validate_sealed_snapshot(&snapshot)?;
    Ok(snapshot)
}

/// 验证 sealed 快照确实由保存的 selection plan 原样升格且达到 60/60。
pub fn validate_sealed_snapshot(snapshot: &StrictStaticSnapshotV2) -> Result<()> {
    validate_selection_plan(&snapshot.selection_plan)?;
    if !snapshot.sealed
        || snapshot.complete_member_count != STRICT_STATIC_MEMBER_COUNT
        || !snapshot.seal_blockers.is_empty()
    {
        bail!("surviving static Top60 快照尚未达到 sealed 60/60");
    }
    let manifest = snapshot
        .sealed_manifest
        .as_ref()
        .context("sealed 快照缺少 sealed_manifest")?;
    let expected_manifest =
        manifest_from_complete_snapshot(&snapshot.selection_plan, &snapshot.coverage);
    if manifest != &expected_manifest {
        bail!("sealed manifest 与保存的 selection plan 或覆盖成员不一致");
    }
    formal_gate(manifest, &snapshot.coverage)
        .context("sealed surviving static Top60 快照未通过正式门禁")?;
    Ok(())
}

/// 使用保存的成员和边界重新读取 quant_core，并要求 canonical manifest SHA 不漂移。
pub async fn reaudit_sealed_snapshot_from_quant_core(
    saved: &StrictStaticSnapshotV2,
) -> Result<StrictStaticSnapshotV2> {
    validate_sealed_snapshot(saved)?;
    let saved_manifest = saved
        .sealed_manifest
        .as_ref()
        .context("保存的 sealed 快照缺少 manifest")?;
    let saved_manifest_sha = canonical_manifest_sha256(saved_manifest)?;
    let current =
        audit_and_seal_strict_static_plan_from_quant_core(saved.selection_plan.clone()).await?;
    let current_manifest = current
        .sealed_manifest
        .as_ref()
        .context("重新审计后覆盖不足 60/60，不能运行正式回测")?;
    let current_manifest_sha = canonical_manifest_sha256(current_manifest)?;
    if current_manifest_sha != saved_manifest_sha {
        bail!(
            "重新审计后的 canonical manifest SHA 漂移：saved={} current={}",
            saved_manifest_sha,
            current_manifest_sha
        );
    }
    Ok(current)
}

/// 只允许 Core 专用 URL，并在唯一连接上设置默认只读事务。
async fn connect_read_only_quant_core() -> Result<PgPool> {
    let database_url = quant_core_database_url_from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .context("连接 quant_core 失败")?;
    sqlx::query("SET default_transaction_read_only = on")
        .execute(&pool)
        .await
        .context("启用 quant_core Research 只读会话失败")?;
    let current_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .context("读取当前数据库名称失败")?;
    if current_database != "quant_core" {
        pool.close().await;
        bail!("Research 冻结器只允许读取 quant_core，当前连接到 {current_database}");
    }
    Ok(pool)
}

/// 禁止回退到通用 `DATABASE_URL`，避免误读 quant_web。
fn quant_core_database_url_from_env() -> Result<String> {
    for key in [
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
        }
    }
    bail!("必须设置 QUANT_CORE_DATABASE_URL 或 POSTGRES_QUANT_CORE_DATABASE_URL")
}

/// 从当次 OKX instruments 响应选出 current-live 候选，彻底绕过本地 stale status。
async fn load_current_live_okx_candidates(
    warmup_start_ms: i64,
    proxy_url: Option<&str>,
) -> Result<(Vec<CandidateInstrument>, String, i64)> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        // Research 冻结必须明确直连或显式代理，不能继承不可审计的系统代理状态。
        .no_proxy();
    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url).context("配置显式 OKX 代理失败")?);
    }
    let client = builder
        .build()
        .context("构造 OKX current-live instrument 只读客户端失败")?;
    let payload: Value = client
        .get(STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT)
        .send()
        .await
        .context("请求 OKX current-live SWAP instruments 失败")?
        .error_for_status()
        .context("OKX current-live SWAP instruments 返回非成功状态")?
        .json()
        .await
        .context("解析 OKX current-live SWAP instruments 失败")?;
    let instrument_snapshot_observed_at_ms = Utc::now().timestamp_millis();
    let snapshot_sha256 = sha256_json(&canonicalize_json(payload.clone()))?;
    let code = payload
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if code != "0" {
        bail!(
            "OKX current-live SWAP instruments 业务失败：code={} msg={}",
            code,
            payload
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or_default()
        );
    }
    let instruments = payload
        .get("data")
        .and_then(Value::as_array)
        .context("OKX current-live SWAP instruments 缺少 data 数组")?;
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for instrument in instruments {
        if !is_live_linear_usdt_crypto_swap(instrument) {
            continue;
        }
        let symbol = instrument
            .get("instId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase();
        validate_symbol(&symbol)?;
        if !seen.insert(symbol.clone()) {
            bail!("OKX 当次 current-live instruments 包含重复 symbol：{symbol}");
        }
        let Some(listed_at_ms) = instrument
            .get("listTime")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0 && *value <= warmup_start_ms)
        else {
            continue;
        };
        let Some(tick_size) = instrument.get("tickSz").and_then(Value::as_str) else {
            continue;
        };
        if validate_tick_size_text(tick_size, &symbol).is_err() {
            continue;
        }
        candidates.push(CandidateInstrument {
            symbol,
            listed_at_ms,
            tick_size: tick_size.to_owned(),
            instrument_source_sha256: sha256_json(&canonicalize_json(instrument.clone()))?,
        });
    }
    // 排名在 Rust 中复算 PostgreSQL md5 的字节序，数据库和 K 线可用性均不参与资格选择。
    candidates.sort_by(|left, right| {
        selection_md5_key(&left.symbol)
            .cmp(&selection_md5_key(&right.symbol))
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    Ok((
        candidates,
        snapshot_sha256,
        instrument_snapshot_observed_at_ms,
    ))
}

/// OKX 必须在本次响应明确声明 live、linear、USDT 结算且属于 crypto。
fn is_live_linear_usdt_crypto_swap(instrument: &Value) -> bool {
    instrument.get("instType").and_then(Value::as_str) == Some("SWAP")
        && instrument.get("state").and_then(Value::as_str) == Some("live")
        && instrument.get("ctType").and_then(Value::as_str) == Some("linear")
        && instrument.get("settleCcy").and_then(Value::as_str) == Some("USDT")
        && instrument
            .get("instId")
            .and_then(Value::as_str)
            .is_some_and(|symbol| symbol.ends_with("-USDT-SWAP"))
        && instrument.get("instCategory").and_then(Value::as_str) == Some("1")
}

/// selection plan 在访问任一 K 线表之前完成，后续只允许补数、禁止换成员。
fn build_selection_plan(
    args: &StrictStaticSnapshotBuildArgs,
    warmup_start_ms: i64,
    instrument_snapshot_observed_at_ms: i64,
    selection_timestamp_ms: i64,
    generated_at_ms: i64,
    instrument_snapshot_sha256: String,
    selected: Vec<CandidateInstrument>,
) -> StrictStaticSelectionPlanV2 {
    StrictStaticSelectionPlanV2 {
        schema_version: SELECTION_PLAN_SCHEMA_VERSION,
        cohort_kind: "surviving_static_top60".to_owned(),
        delisted_symbols_excluded: true,
        survivorship_bias_accepted: true,
        universe_version: args.universe_version.clone(),
        generated_at_ms,
        selection_timestamp_ms,
        instrument_snapshot_observed_at_ms,
        instrument_source_endpoint: STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT.to_owned(),
        instrument_snapshot_sha256,
        warmup_start_ms,
        evaluation_start_ms: args.evaluation_start_ms,
        evaluation_end_exclusive_ms: args.evaluation_end_exclusive_ms,
        warmup_days: STRICT_STATIC_WARMUP_DAYS,
        selection_rule_id: STRICT_STATIC_SELECTION_RULE_ID.to_owned(),
        selection_rule: SURVIVING_STATIC_TOP60_SELECTION_RULE.to_owned(),
        members: selected
            .iter()
            .enumerate()
            .map(|(index, member)| StrictStaticSelectionMemberV2 {
                symbol: member.symbol.clone(),
                listed_at_ms: member.listed_at_ms,
                rank: (index + 1) as u32,
                frozen_tick_size: member.tick_size.clone(),
                instrument_source_sha256: member.instrument_source_sha256.clone(),
            })
            .collect(),
    }
}

/// 只读取目标 symbol 的已确认 OHLC+vol_ccy；表不存在时返回空覆盖而不是换币。
async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_exclusive_ms: i64,
) -> Result<Vec<Candle>> {
    let table_name = candle_table_name(symbol)?;
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = $1
        )",
    )
    .bind(&table_name)
    .fetch_one(pool)
    .await
    .with_context(|| format!("检查 {symbol} 15m K线表失败"))?;
    if !table_exists {
        return Ok(Vec::new());
    }
    let query = format!(
        "SELECT ts, o, h, l, c, vol_ccy \
         FROM \"{table_name}\" \
         WHERE confirm = '1' AND ts >= $1 AND ts < $2 \
         ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(start_ms)
        .bind(end_exclusive_ms)
        .fetch_all(pool)
        .await
        .with_context(|| format!("读取 {symbol} 已确认 15m OHLC+vol_ccy 失败"))?
        .iter()
        .map(|row| parse_candle_row(row, symbol))
        .collect()
}

/// 生成与 sealed manifest 完全同形的逐币来源和覆盖证据。
fn build_member_coverage(
    member: &StrictStaticSelectionMemberV2,
    candles: &[Candle],
    start_ms: i64,
    end_exclusive_ms: i64,
    expected_count: usize,
) -> Result<StrictStaticMemberCoverageV2> {
    let aligned_unique_count = candles
        .iter()
        .filter(|candle| {
            candle.timestamp_ms >= start_ms
                && candle.timestamp_ms < end_exclusive_ms
                && (candle.timestamp_ms - start_ms).rem_euclid(STRICT_STATIC_CANDLE_INTERVAL_MS)
                    == 0
        })
        .map(|candle| candle.timestamp_ms)
        .collect::<BTreeSet<_>>()
        .len();
    let is_contiguous_15m = candles.len() == aligned_unique_count
        && candles.windows(2).all(|pair| {
            pair[1].timestamp_ms - pair[0].timestamp_ms == STRICT_STATIC_CANDLE_INTERVAL_MS
        });
    let first_timestamp_ms = candles.first().map(|candle| candle.timestamp_ms);
    let last_timestamp_ms = candles.last().map(|candle| candle.timestamp_ms);
    let fingerprint = FrozenStaticCandleSourceFingerprintV2 {
        symbol: member.symbol.clone(),
        canonicalization_version: STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION.to_owned(),
        source_id: canonical_candle_source_id(&member.symbol, start_ms, end_exclusive_ms)?,
        sha256: candle_sha256(&member.symbol, start_ms, end_exclusive_ms, candles),
        source_kind: STRICT_STATIC_CANDLE_SOURCE_KIND.to_owned(),
        volume_field: STRICT_STATIC_VOLUME_FIELD.to_owned(),
        confirmed_only: true,
        first_timestamp_ms: first_timestamp_ms.unwrap_or(0),
        last_timestamp_ms: last_timestamp_ms.unwrap_or(0),
        candle_count: candles.len(),
    };
    Ok(StrictStaticMemberCoverageV2 {
        symbol: member.symbol.clone(),
        expected_candle_count: expected_count,
        loaded_candle_count: candles.len(),
        first_timestamp_ms,
        last_timestamp_ms,
        confirmed_candle_count: candles.len(),
        is_contiguous_15m,
        missing_candle_count: expected_count.saturating_sub(aligned_unique_count),
        frozen_tick_size: member.frozen_tick_size.clone(),
        instrument_source_sha256: member.instrument_source_sha256.clone(),
        candle_source: fingerprint,
    })
}

/// 完整性判断与 `formal_gate` 使用同一半开窗口口径。
fn member_coverage_is_complete(
    coverage: &StrictStaticMemberCoverageV2,
    start_ms: i64,
    end_exclusive_ms: i64,
    expected_count: usize,
) -> bool {
    coverage.expected_candle_count == expected_count
        && coverage.loaded_candle_count == expected_count
        && coverage.confirmed_candle_count == expected_count
        && coverage.first_timestamp_ms == Some(start_ms)
        && coverage.last_timestamp_ms
            == end_exclusive_ms.checked_sub(STRICT_STATIC_CANDLE_INTERVAL_MS)
        && coverage.is_contiguous_15m
        && coverage.missing_candle_count == 0
}

/// 只有 60 个成员均完整时才把 selection plan 升格为正式 manifest。
fn manifest_from_complete_snapshot(
    plan: &StrictStaticSelectionPlanV2,
    coverage: &StrictStaticUniverseCoverageV2,
) -> StrictStaticUniverseManifestV2 {
    let members = plan
        .members
        .iter()
        .zip(&coverage.members)
        .map(|(selected, actual)| StrictStaticUniverseMemberV2 {
            symbol: selected.symbol.clone(),
            listed_at_ms: selected.listed_at_ms,
            status_at_selection: StrictStaticMemberStatusV2::Live,
            rank: selected.rank,
            frozen_tick_size: selected.frozen_tick_size.clone(),
            instrument_source_sha256: selected.instrument_source_sha256.clone(),
            candle_source: actual.candle_source.clone(),
        })
        .collect();
    StrictStaticUniverseManifestV2 {
        schema_version: STRICT_STATIC_SCHEMA_VERSION,
        universe_version: plan.universe_version.clone(),
        generated_at_ms: plan.generated_at_ms,
        selection_timestamp_ms: plan.selection_timestamp_ms,
        instrument_source_endpoint: plan.instrument_source_endpoint.clone(),
        instrument_snapshot_sha256: plan.instrument_snapshot_sha256.clone(),
        instrument_snapshot_observed_at_ms: plan.instrument_snapshot_observed_at_ms,
        exchange: "okx".to_owned(),
        market_type: "perpetual_swap".to_owned(),
        quote_currency: "USDT".to_owned(),
        timeframe: "15m".to_owned(),
        warmup_start_ms: plan.warmup_start_ms,
        evaluation_start_ms: plan.evaluation_start_ms,
        evaluation_end_exclusive_ms: plan.evaluation_end_exclusive_ms,
        warmup_days: plan.warmup_days,
        selection_rule_id: plan.selection_rule_id.clone(),
        selection_rule: plan.selection_rule.clone(),
        members,
    }
}

/// 递归排序 JSON object key，避免数据库或 serde map 顺序改变来源身份。
fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries = object.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut canonical = Map::new();
            for (key, value) in entries {
                canonical.insert(key, canonicalize_json(value));
            }
            Value::Object(canonical)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

/// 对规范 JSON 计算小写 SHA-256。
fn sha256_json(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value).context("序列化 canonical instrument JSON 失败")?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// K 线 SHA 直接哈希回放消费的时间和 f64 bit，文本小数格式变化不会伪造数据漂移。
fn candle_sha256(symbol: &str, start_ms: i64, end_exclusive_ms: i64, candles: &[Candle]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(symbol.as_bytes());
    hasher.update([0]);
    hasher.update(start_ms.to_be_bytes());
    hasher.update(end_exclusive_ms.to_be_bytes());
    for candle in candles {
        hasher.update(candle.timestamp_ms.to_be_bytes());
        hasher.update(candle.open.to_bits().to_be_bytes());
        hasher.update(candle.high.to_bits().to_be_bytes());
        hasher.update(candle.low.to_bits().to_be_bytes());
        hasher.update(candle.close.to_bits().to_be_bytes());
        hasher.update(candle.volume.to_bits().to_be_bytes());
    }
    hex::encode(hasher.finalize())
}

/// 把 VARCHAR 行转为策略实际消费的有限 OHLC+vol_ccy。
fn parse_candle_row(row: &PgRow, symbol: &str) -> Result<Candle> {
    let timestamp_ms: i64 = row.try_get("ts")?;
    let candle = Candle {
        timestamp_ms,
        open: parse_row_number(row, "o", symbol, timestamp_ms)?,
        high: parse_row_number(row, "h", symbol, timestamp_ms)?,
        low: parse_row_number(row, "l", symbol, timestamp_ms)?,
        close: parse_row_number(row, "c", symbol, timestamp_ms)?,
        volume: parse_row_number(row, "vol_ccy", symbol, timestamp_ms)?,
    };
    if !candle.is_valid() {
        bail!("{symbol} 在 {timestamp_ms} 的 OHLC+vol_ccy 不满足 Candle 不变量");
    }
    Ok(candle)
}

/// 数值解析失败必须阻止 seal，不能把坏行静默当作缺失或零。
fn parse_row_number(row: &PgRow, column: &str, symbol: &str, timestamp_ms: i64) -> Result<f64> {
    let raw: Option<String> = row.try_get(column)?;
    let raw = raw.with_context(|| format!("{symbol} {timestamp_ms} 的 {column} 为空"))?;
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("解析 {symbol} {timestamp_ms} 的 {column}={raw}"))?;
    if !value.is_finite() {
        bail!("{symbol} {timestamp_ms} 的 {column} 不是有限数值");
    }
    Ok(value)
}

/// 计算对齐 15m 半开窗口中的精确应有根数。
fn exact_candle_count(start_ms: i64, end_exclusive_ms: i64) -> Result<usize> {
    if start_ms < 0
        || end_exclusive_ms <= start_ms
        || start_ms.rem_euclid(STRICT_STATIC_CANDLE_INTERVAL_MS) != 0
        || end_exclusive_ms.rem_euclid(STRICT_STATIC_CANDLE_INTERVAL_MS) != 0
    {
        bail!("surviving static Top60 必须使用非负、对齐 15m 的半开窗口");
    }
    usize::try_from((end_exclusive_ms - start_ms) / STRICT_STATIC_CANDLE_INTERVAL_MS)
        .context("surviving static Top60 K线数量超出 usize")
}

/// 入口先验证全局边界，防止建立数据库连接后才发现参数无效。
fn validate_build_args(args: &StrictStaticSnapshotBuildArgs) -> Result<()> {
    if args.universe_version.trim().is_empty() {
        bail!("--universe-version 不能为空");
    }
    let warmup_start_ms = args
        .evaluation_start_ms
        .checked_sub(i64::from(STRICT_STATIC_WARMUP_DAYS) * DAY_MS)
        .context("计算 60 天预热起点时溢出")?;
    exact_candle_count(warmup_start_ms, args.evaluation_start_ms)?;
    exact_candle_count(args.evaluation_start_ms, args.evaluation_end_exclusive_ms)?;
    Ok(())
}

/// 保留 OKX `tickSz` 原始文本，只接受正的普通十进制格式。
fn validate_tick_size_text(value: &str, symbol: &str) -> Result<Decimal> {
    let mut decimal_point_count = 0usize;
    let mut integer_digit_count = 0usize;
    let mut fraction_digit_count = 0usize;
    for byte in value.bytes() {
        match byte {
            b'0'..=b'9' if decimal_point_count == 0 => integer_digit_count += 1,
            b'0'..=b'9' => fraction_digit_count += 1,
            b'.' if decimal_point_count == 0 => decimal_point_count += 1,
            _ => bail!("{symbol} 的 frozen tick_size 不是 OKX 原始普通十进制字符串"),
        }
    }
    if value.is_empty()
        || integer_digit_count == 0
        || (decimal_point_count == 1 && fraction_digit_count == 0)
    {
        bail!("{symbol} 的 frozen tick_size 不是完整十进制字符串");
    }
    let tick = Decimal::from_str_exact(value)
        .with_context(|| format!("{symbol} 的 frozen tick_size 超出 Decimal 精度"))?;
    if tick <= Decimal::ZERO {
        bail!("{symbol} 的 frozen tick_size 必须大于 0");
    }
    Ok(tick)
}

/// runner 边界只在这里把已验证的原始 tick 文本转换一次为有限 `f64`。
fn parse_tick_size_for_replay(value: &str, symbol: &str) -> Result<f64> {
    validate_tick_size_text(value, symbol)?;
    let tick = value
        .parse::<f64>()
        .with_context(|| format!("解析 {symbol} 的 frozen tick_size={value} 失败"))?;
    if !tick.is_finite() || tick <= 0.0 {
        bail!("{symbol} 的 frozen tick_size 无法安全转换为回放输入");
    }
    Ok(tick)
}

/// 来源身份必须是非零小写 SHA-256，禁止空值或占位哈希进入计划。
fn validate_sha256_text(value: &str, field: &str, symbol: Option<&str>) -> Result<()> {
    let valid = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0');
    if !valid {
        bail!(
            "{}{} 必须是非零小写 SHA-256",
            symbol
                .map(|value| format!("{value} 的 "))
                .unwrap_or_default(),
            field
        );
    }
    Ok(())
}

/// symbol 经严格校验后才能拼接为带双引号的表名。
fn validate_symbol(symbol: &str) -> Result<()> {
    let parts = symbol.split('-').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0].is_empty()
        || parts[1] != "USDT"
        || parts[2] != "SWAP"
        || !parts[0]
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        bail!("不是严格的 OKX USDT SWAP 标识：{symbol}");
    }
    Ok(())
}

/// 返回当前 quant_core 15m 表命名；不接受任意 SQL identifier。
fn candle_table_name(symbol: &str) -> Result<String> {
    validate_symbol(symbol)?;
    Ok(format!("{}_candles_15m", symbol.to_ascii_lowercase()))
}

/// 复现 PostgreSQL `md5(seed || ':' || symbol)` 的 16 字节排序键。
fn selection_md5_key(symbol: &str) -> [u8; 16] {
    md5_digest(format!("{SURVIVING_STATIC_TOP60_ORDER_SEED}:{symbol}").as_bytes())
}

/// 最小 MD5 实现只服务冻结排序兼容，不作为安全哈希；所有来源身份仍使用 SHA-256。
fn md5_digest(input: &[u8]) -> [u8; 16] {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const CONSTANTS: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0 = 0x6745_2301_u32;
    let mut b0 = 0xefcd_ab89_u32;
    let mut c0 = 0x98ba_dcfe_u32;
    let mut d0 = 0x1032_5476_u32;
    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_le_bytes(
                chunk[offset..offset + 4]
                    .try_into()
                    .expect("MD5 chunk always contains 16 complete words"),
            );
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for index in 0..64 {
            let (function, word_index) = match index {
                0..=15 => ((b & c) | ((!b) & d), index),
                16..=31 => ((d & b) | ((!d) & c), (5 * index + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | !d), (7 * index) % 16),
            };
            let next_b = b.wrapping_add(
                a.wrapping_add(function)
                    .wrapping_add(CONSTANTS[index])
                    .wrapping_add(words[word_index])
                    .rotate_left(SHIFTS[index]),
            );
            (a, b, c, d) = (d, next_b, b, c);
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut digest = [0_u8; 16];
    for (index, value) in [a0, b0, c0, d0].into_iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected_member() -> StrictStaticSelectionMemberV2 {
        StrictStaticSelectionMemberV2 {
            symbol: "BTC-USDT-SWAP".to_owned(),
            listed_at_ms: 1,
            rank: 1,
            frozen_tick_size: "0.1".to_owned(),
            instrument_source_sha256: "1".repeat(64),
        }
    }

    fn sha(seed: usize) -> String {
        format!("{seed:064x}")
    }

    fn selection_plan() -> StrictStaticSelectionPlanV2 {
        let evaluation_start_ms = 100 * DAY_MS;
        let evaluation_end_exclusive_ms = 101 * DAY_MS;
        let warmup_start_ms = evaluation_start_ms - i64::from(STRICT_STATIC_WARMUP_DAYS) * DAY_MS;
        let mut symbols = (1..=STRICT_STATIC_MEMBER_COUNT)
            .map(|index| format!("S{index}-USDT-SWAP"))
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| {
            selection_md5_key(left)
                .cmp(&selection_md5_key(right))
                .then_with(|| left.cmp(right))
        });
        StrictStaticSelectionPlanV2 {
            schema_version: SELECTION_PLAN_SCHEMA_VERSION,
            cohort_kind: "surviving_static_top60".to_owned(),
            delisted_symbols_excluded: true,
            survivorship_bias_accepted: true,
            universe_version: "surviving_static_top60_fixture_v2".to_owned(),
            generated_at_ms: evaluation_end_exclusive_ms + 2,
            selection_timestamp_ms: evaluation_end_exclusive_ms + 1,
            instrument_snapshot_observed_at_ms: evaluation_end_exclusive_ms,
            instrument_source_endpoint: STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT.to_owned(),
            instrument_snapshot_sha256: sha(1),
            warmup_start_ms,
            evaluation_start_ms,
            evaluation_end_exclusive_ms,
            warmup_days: STRICT_STATIC_WARMUP_DAYS,
            selection_rule_id: STRICT_STATIC_SELECTION_RULE_ID.to_owned(),
            selection_rule: SURVIVING_STATIC_TOP60_SELECTION_RULE.to_owned(),
            members: symbols
                .into_iter()
                .enumerate()
                .map(|(index, symbol)| StrictStaticSelectionMemberV2 {
                    symbol,
                    listed_at_ms: warmup_start_ms - DAY_MS,
                    rank: (index + 1) as u32,
                    frozen_tick_size: "0.001".to_owned(),
                    instrument_source_sha256: sha(index + 10),
                })
                .collect(),
        }
    }

    fn sealed_snapshot() -> StrictStaticSnapshotV2 {
        let plan = selection_plan();
        let expected_count =
            exact_candle_count(plan.warmup_start_ms, plan.evaluation_end_exclusive_ms).unwrap();
        let members = plan
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let candle_source = FrozenStaticCandleSourceFingerprintV2 {
                    symbol: member.symbol.clone(),
                    canonicalization_version: STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION
                        .to_owned(),
                    source_id: canonical_candle_source_id(
                        &member.symbol,
                        plan.warmup_start_ms,
                        plan.evaluation_end_exclusive_ms,
                    )
                    .unwrap(),
                    sha256: sha(index + 1_000),
                    source_kind: STRICT_STATIC_CANDLE_SOURCE_KIND.to_owned(),
                    volume_field: STRICT_STATIC_VOLUME_FIELD.to_owned(),
                    confirmed_only: true,
                    first_timestamp_ms: plan.warmup_start_ms,
                    last_timestamp_ms: plan.evaluation_end_exclusive_ms
                        - STRICT_STATIC_CANDLE_INTERVAL_MS,
                    candle_count: expected_count,
                };
                StrictStaticMemberCoverageV2 {
                    symbol: member.symbol.clone(),
                    expected_candle_count: expected_count,
                    loaded_candle_count: expected_count,
                    first_timestamp_ms: Some(plan.warmup_start_ms),
                    last_timestamp_ms: Some(
                        plan.evaluation_end_exclusive_ms - STRICT_STATIC_CANDLE_INTERVAL_MS,
                    ),
                    confirmed_candle_count: expected_count,
                    is_contiguous_15m: true,
                    missing_candle_count: 0,
                    frozen_tick_size: member.frozen_tick_size.clone(),
                    instrument_source_sha256: member.instrument_source_sha256.clone(),
                    candle_source,
                }
            })
            .collect();
        let mut coverage = StrictStaticUniverseCoverageV2 {
            universe_version: plan.universe_version.clone(),
            manifest_sha256: String::new(),
            warmup_start_ms: plan.warmup_start_ms,
            evaluation_start_ms: plan.evaluation_start_ms,
            evaluation_end_exclusive_ms: plan.evaluation_end_exclusive_ms,
            members,
        };
        let manifest = manifest_from_complete_snapshot(&plan, &coverage);
        coverage.manifest_sha256 = canonical_manifest_sha256(&manifest).unwrap();
        StrictStaticSnapshotV2 {
            selection_plan: plan,
            coverage,
            sealed_manifest: Some(manifest),
            complete_member_count: STRICT_STATIC_MEMBER_COUNT,
            sealed: true,
            seal_blockers: Vec::new(),
            symbols: Vec::new(),
        }
    }

    fn candle(timestamp_ms: i64) -> Candle {
        Candle {
            timestamp_ms,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 10.0,
        }
    }

    #[test]
    fn md5_sorting_key_matches_postgres_compatible_digest() {
        assert_eq!(
            hex::encode(md5_digest(b"abc")),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            selection_md5_key("BTC-USDT-SWAP"),
            selection_md5_key("BTC-USDT-SWAP")
        );
    }

    #[test]
    fn okx_filter_rejects_delisted_or_non_linear_instruments() {
        let live = serde_json::json!({
            "instType": "SWAP",
            "state": "live",
            "ctType": "linear",
            "settleCcy": "USDT",
            "instId": "BTC-USDT-SWAP",
            "instCategory": "1"
        });
        assert!(is_live_linear_usdt_crypto_swap(&live));

        let mut delisted = live.clone();
        delisted["state"] = Value::String("suspend".to_owned());
        assert!(!is_live_linear_usdt_crypto_swap(&delisted));

        let mut inverse = live;
        inverse["ctType"] = Value::String("inverse".to_owned());
        assert!(!is_live_linear_usdt_crypto_swap(&inverse));
    }

    #[test]
    fn incomplete_coverage_is_diagnostic_and_cannot_be_sealed() {
        let interval = STRICT_STATIC_CANDLE_INTERVAL_MS;
        let candles = vec![candle(0), candle(2 * interval)];
        let coverage =
            build_member_coverage(&selected_member(), &candles, 0, 3 * interval, 3).unwrap();

        assert_eq!(coverage.loaded_candle_count, 2);
        assert_eq!(coverage.missing_candle_count, 1);
        assert!(!coverage.is_contiguous_15m);
        assert!(!member_coverage_is_complete(&coverage, 0, 3 * interval, 3));
    }

    #[test]
    fn exact_coverage_has_stable_canonical_hash() {
        let interval = STRICT_STATIC_CANDLE_INTERVAL_MS;
        let candles = vec![candle(0), candle(interval), candle(2 * interval)];
        let left = build_member_coverage(&selected_member(), &candles, 0, 3 * interval, 3).unwrap();
        let right =
            build_member_coverage(&selected_member(), &candles, 0, 3 * interval, 3).unwrap();

        assert!(member_coverage_is_complete(&left, 0, 3 * interval, 3));
        assert_eq!(left.candle_source.sha256, right.candle_source.sha256);
        assert_eq!(left.candle_source.sha256.len(), 64);
    }

    #[test]
    fn canonical_json_sorts_nested_object_keys() {
        let left = serde_json::json!({"b": 2, "a": {"y": 1, "x": 0}});
        let right = serde_json::json!({"a": {"x": 0, "y": 1}, "b": 2});

        assert_eq!(
            sha256_json(&canonicalize_json(left)).unwrap(),
            sha256_json(&canonicalize_json(right)).unwrap()
        );
    }

    #[test]
    fn build_args_require_exact_sixty_day_aligned_warmup() {
        let evaluation_start_ms = 100 * DAY_MS;
        validate_build_args(&StrictStaticSnapshotBuildArgs {
            universe_version: "surviving_static_top60_fixture".to_owned(),
            evaluation_start_ms,
            evaluation_end_exclusive_ms: evaluation_start_ms + DAY_MS,
            proxy_url: None,
        })
        .unwrap();

        assert!(validate_build_args(&StrictStaticSnapshotBuildArgs {
            universe_version: String::new(),
            evaluation_start_ms,
            evaluation_end_exclusive_ms: evaluation_start_ms + DAY_MS,
            proxy_url: None,
        })
        .is_err());
    }

    #[test]
    fn plan_rejects_rule_listing_tick_order_and_window_drift() {
        let plan = selection_plan();
        validate_selection_plan(&plan).unwrap();

        let mut bad_rule = plan.clone();
        bad_rule.selection_rule_id = "wrong".to_owned();
        assert!(validate_selection_plan(&bad_rule).is_err());

        let mut zero_listing = plan.clone();
        zero_listing.members[0].listed_at_ms = 0;
        assert!(validate_selection_plan(&zero_listing).is_err());

        let mut invalid_tick = plan.clone();
        invalid_tick.members[0].frozen_tick_size = "1e-3".to_owned();
        assert!(validate_selection_plan(&invalid_tick).is_err());

        let mut wrong_order = plan.clone();
        wrong_order.members.swap(0, 1);
        wrong_order.members[0].rank = 1;
        wrong_order.members[1].rank = 2;
        assert!(validate_selection_plan(&wrong_order).is_err());

        let mut wrong_window = plan;
        wrong_window.warmup_start_ms += STRICT_STATIC_CANDLE_INTERVAL_MS;
        assert!(validate_selection_plan(&wrong_window).is_err());
    }

    #[test]
    fn saved_plan_is_decoded_and_promoted_without_reselection() {
        let expected = selection_plan();
        let raw = serde_json::to_vec(&serde_json::json!({
            "selection_plan": expected.clone(),
            "sealed": false
        }))
        .unwrap();
        let decoded = decode_and_validate_selection_plan_from_snapshot(&raw).unwrap();

        assert_eq!(decoded, expected);
        let snapshot = sealed_snapshot();
        let manifest = snapshot.sealed_manifest.as_ref().unwrap();
        assert_eq!(
            snapshot
                .selection_plan
                .members
                .iter()
                .map(|member| (&member.symbol, member.rank))
                .collect::<Vec<_>>(),
            manifest
                .members
                .iter()
                .map(|member| (&member.symbol, member.rank))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sealed_snapshot_rejects_saved_manifest_sha_drift() {
        let snapshot = sealed_snapshot();
        let raw = serde_json::to_vec(&snapshot).unwrap();
        decode_and_validate_sealed_snapshot(&raw).unwrap();

        let mut drifted = snapshot;
        drifted.coverage.manifest_sha256 = "f".repeat(64);
        let raw = serde_json::to_vec(&drifted).unwrap();
        assert!(decode_and_validate_sealed_snapshot(&raw).is_err());
    }
}
