//! Surviving static Top60 的独立严格数据合同。
//!
//! 该 cohort 明确只包含 manifest 生成时仍为 current-live 的合约，因此带有幸存者偏差。
//! 本模块只验证冻结成员、tick 与 K 线覆盖证据，不读取数据库，也不接入现有回放引擎。

use anyhow::{bail, Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// 静态 Top60 严格合同版本；既有 current-live V1 manifest 保持原样。
pub const STRICT_STATIC_SCHEMA_VERSION: u32 = 2;
/// 静态 cohort 必须恰好冻结的成员数。
pub const STRICT_STATIC_MEMBER_COUNT: usize = 60;
/// EMA596/696 等指标在评价开始前必须具备的连续预热天数。
pub const STRICT_STATIC_WARMUP_DAYS: u32 = 60;
/// 合同只接受 15 分钟 K 线。
pub const STRICT_STATIC_CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
/// 策略成交量唯一使用 OKX `vol_ccy`，不允许以 `vol` 或 `volume * close` 代替。
pub const STRICT_STATIC_VOLUME_FIELD: &str = "vol_ccy";
/// 同源 K 线必须是已确认的 OKX 15m `vol_ccy` 数据。
pub const STRICT_STATIC_CANDLE_SOURCE_KIND: &str = "okx_confirmed_15m_vol_ccy";
/// 唯一允许的静态幸存者 Top60 选择规则身份。
pub const STRICT_STATIC_SELECTION_RULE_ID: &str =
    "okx_surviving_static_usdt_swap_md5_top60_20260721_v1";
/// current-live instrument 资格只能来自该完整 OKX 公共快照端点。
pub const STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT: &str =
    "https://www.okx.com/api/v5/public/instruments?instType=SWAP";
/// K 线内容哈希使用的固定规范化协议。
pub const STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION: &str = "okx_15m_ohlcv_vol_ccy_f64_be_v1";
/// Manifest 内容哈希使用的固定规范化协议。
pub const STRICT_STATIC_MANIFEST_CANONICALIZATION_VERSION: &str =
    "strict_static_manifest_sorted_json_v1";

/// cohort 冻结时的交易状态。
///
/// V2 只接受明确的 `live`；暂停、退市、未上市或状态未知均失败关闭。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrictStaticMemberStatusV2 {
    Live,
}

/// 带幸存者偏差但可严格重放的静态 Top60 manifest。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticUniverseManifestV2 {
    /// 必须等于 `2`，防止旧 V1 manifest 被误读成严格合同。
    pub schema_version: u32,
    /// cohort 版本；成员、边界或来源证据改变后必须生成新版本。
    pub universe_version: String,
    /// manifest 生成时间，Unix 毫秒。
    pub generated_at_ms: i64,
    /// current-live 资格与排名实际冻结时间，Unix 毫秒。
    pub selection_timestamp_ms: i64,
    /// 获取完整 current-live instrument 快照的唯一 OKX 公共端点。
    pub instrument_source_endpoint: String,
    /// 完整 instrument API envelope 规范化后的 SHA-256。
    pub instrument_snapshot_sha256: String,
    /// 完整 instrument 快照实际观测时间，Unix 毫秒。
    pub instrument_snapshot_observed_at_ms: i64,
    /// 行情与 instrument 事实来源，V2 仅接受 `okx`。
    pub exchange: String,
    /// 产品类型，V2 仅接受 `perpetual_swap`。
    pub market_type: String,
    /// 计价币种，V2 仅接受 `USDT`。
    pub quote_currency: String,
    /// K 线周期，V2 仅接受 `15m`。
    pub timeframe: String,
    /// 全局预热起点，Unix 毫秒，包含该时刻。
    pub warmup_start_ms: i64,
    /// 正式评价起点，Unix 毫秒，包含该时刻。
    pub evaluation_start_ms: i64,
    /// 正式评价终点，Unix 毫秒，不包含该时刻。
    pub evaluation_end_exclusive_ms: i64,
    /// 指标预热天数；V2 固定为 60，运行时不得缩短。
    pub warmup_days: u32,
    /// 机器可审计的固定选币规则身份，必须精确等于合同常量。
    pub selection_rule_id: String,
    /// 查看回测结果前冻结的 current-live 候选与排名规则。
    pub selection_rule: String,
    /// 固定覆盖整个预热与评价窗口的 60 个唯一成员。
    pub members: Vec<StrictStaticUniverseMemberV2>,
}

/// 一个静态成员的资格、排名、价格步长与输入数据证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticUniverseMemberV2 {
    /// OKX 原始 USDT 永续标识，例如 `BTC-USDT-SWAP`。
    pub symbol: String,
    /// 合约首次上市时间，Unix 毫秒；必须不晚于全局预热起点。
    pub listed_at_ms: i64,
    /// cohort 冻结时状态；V2 只允许 `live`。
    pub status_at_selection: StrictStaticMemberStatusV2,
    /// 冻结选择结果中的排名，必须唯一覆盖 1 到 60。
    pub rank: u32,
    /// OKX `tickSz` 原始十进制字符串；保留小数位，不允许经 `f64` 往返。
    pub frozen_tick_size: String,
    /// 产生上市时间、current-live 状态和 tick 的 instrument 源 SHA-256。
    pub instrument_source_sha256: String,
    /// 从全局预热起点到评价终点的规范化 K 线内容指纹。
    pub candle_source: FrozenStaticCandleSourceFingerprintV2,
}

/// 静态成员完整 K 线输入的内容与边界指纹。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrozenStaticCandleSourceFingerprintV2 {
    /// 指纹所属的 OKX 原始 symbol；不得跨 symbol 复用。
    pub symbol: String,
    /// 产生内容哈希的规范化协议版本。
    pub canonicalization_version: String,
    /// 数据集内稳定的源标识，例如带 symbol 与边界的归档键。
    pub source_id: String,
    /// 规范化 K 线内容 SHA-256，而不是文件路径或表名哈希。
    pub sha256: String,
    /// 数据语义；V2 固定为已确认 OKX 15m `vol_ccy`。
    pub source_kind: String,
    /// 成交量字段；V2 固定为 `vol_ccy`。
    pub volume_field: String,
    /// `true` 表示指纹只含交易所确认完成的 K 线。
    pub confirmed_only: bool,
    /// 指纹覆盖的首根 K 线开盘时间，Unix 毫秒。
    pub first_timestamp_ms: i64,
    /// 指纹覆盖的末根 K 线开盘时间，Unix 毫秒。
    pub last_timestamp_ms: i64,
    /// 指纹覆盖的 15m K 线数量。
    pub candle_count: usize,
}

/// 运行前针对严格静态 manifest 生成的 60/60 覆盖证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticUniverseCoverageV2 {
    /// 必须与 manifest cohort 版本完全一致。
    pub universe_version: String,
    /// 该覆盖证据所绑定的 canonical sealed manifest SHA-256。
    pub manifest_sha256: String,
    /// 实际加载使用的全局预热起点，Unix 毫秒。
    pub warmup_start_ms: i64,
    /// 实际加载使用的正式评价起点，Unix 毫秒。
    pub evaluation_start_ms: i64,
    /// 实际加载使用的正式评价终点，Unix 毫秒，不包含。
    pub evaluation_end_exclusive_ms: i64,
    /// 60 个成员逐一对应的实际覆盖证据。
    pub members: Vec<StrictStaticMemberCoverageV2>,
}

/// 单成员的实际 tick、来源与精确 K 线覆盖证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrictStaticMemberCoverageV2 {
    /// 与 manifest 成员精确匹配的 OKX symbol。
    pub symbol: String,
    /// 由全局半开区间计算的应有 K 线数量。
    pub expected_candle_count: usize,
    /// 实际加载的 K 线数量。
    pub loaded_candle_count: usize,
    /// 实际首根开盘时间；`None` 表示没有加载到任何 K 线。
    pub first_timestamp_ms: Option<i64>,
    /// 实际末根开盘时间；`None` 表示没有加载到任何 K 线。
    pub last_timestamp_ms: Option<i64>,
    /// 实际 `confirm=1` 的 K 线数量，必须等于应有数量。
    pub confirmed_candle_count: usize,
    /// `true` 表示首尾之间每相邻两根严格相隔 15 分钟。
    pub is_contiguous_15m: bool,
    /// 首部、内部与尾部缺失根数之和；正式门禁只接受 0。
    pub missing_candle_count: usize,
    /// 实际回放使用的 OKX 原始 tick 字符串；必须逐字节等于 manifest 冻结值。
    pub frozen_tick_size: String,
    /// 实际 instrument 来源 SHA-256；必须等于 manifest 冻结值。
    pub instrument_source_sha256: String,
    /// 实际 K 线内容与来源指纹；必须等于 manifest 冻结值。
    pub candle_source: FrozenStaticCandleSourceFingerprintV2,
}

/// 60/60 正式门禁通过后的可审计摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictStaticFormalGatePassV2 {
    /// 已验证的 cohort 版本。
    pub universe_version: String,
    /// 已验证且与覆盖证据一致的 canonical manifest SHA-256。
    pub manifest_sha256: String,
    /// 已验证成员数量，固定为 60。
    pub symbol_count: usize,
    /// 每个成员预热段应有根数，固定为 5,760。
    pub warmup_candles_per_symbol: usize,
    /// 每个成员正式评价段应有根数。
    pub evaluation_candles_per_symbol: usize,
    /// 60 个成员实际纳入指纹的 K 线总数。
    pub covered_candle_count: usize,
}

/// 从 JSON 解码并验证 V2；未知字段和未知状态均由 serde 直接拒绝。
pub fn decode_and_validate_manifest_v2(raw: &[u8]) -> Result<StrictStaticUniverseManifestV2> {
    let manifest =
        serde_json::from_slice(raw).context("解析 strict static Top60 manifest V2 失败")?;
    validate_manifest_v2(&manifest)?;
    Ok(manifest)
}

/// 对已验证 manifest 的递归排序 JSON 计算带版本域隔离的 SHA-256。
pub fn canonical_manifest_sha256(manifest: &StrictStaticUniverseManifestV2) -> Result<String> {
    validate_manifest_v2(manifest)?;
    let value =
        serde_json::to_value(manifest).context("序列化 strict static Top60 manifest 失败")?;
    let canonical = canonicalize_json(value);
    let bytes =
        serde_json::to_vec(&canonical).context("编码 canonical strict static manifest 失败")?;
    let mut hasher = Sha256::new();
    hasher.update(STRICT_STATIC_MANIFEST_CANONICALIZATION_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// 生成绑定 symbol 与全局半开窗口的唯一 K 线来源标识。
pub fn canonical_candle_source_id(
    symbol: &str,
    first_timestamp_ms: i64,
    end_exclusive_ms: i64,
) -> Result<String> {
    validate_symbol(symbol)?;
    validate_half_open_window(first_timestamp_ms, end_exclusive_ms, "K 线来源窗口")?;
    Ok(format!(
        "quant_core:public.{}_candles_15m:{first_timestamp_ms}:{end_exclusive_ms}",
        symbol.to_ascii_lowercase()
    ))
}

/// 验证静态 cohort 自身的边界、current-live 资格、tick 与数据指纹。
pub fn validate_manifest_v2(manifest: &StrictStaticUniverseManifestV2) -> Result<()> {
    if manifest.schema_version != STRICT_STATIC_SCHEMA_VERSION {
        bail!("strict static Top60 manifest 必须使用 schema v2");
    }
    if manifest.universe_version.trim().is_empty()
        || manifest.generated_at_ms <= 0
        || manifest.selection_timestamp_ms <= 0
        || manifest.generated_at_ms < manifest.selection_timestamp_ms
        || manifest.instrument_snapshot_observed_at_ms <= 0
        || manifest.instrument_snapshot_observed_at_ms > manifest.selection_timestamp_ms
        || manifest.selection_rule.trim().is_empty()
    {
        bail!("strict static Top60 缺少版本、快照时间、冻结时间或选择规则");
    }
    if manifest.exchange != "okx"
        || manifest.market_type != "perpetual_swap"
        || manifest.quote_currency != "USDT"
        || manifest.timeframe != "15m"
    {
        bail!("strict static Top60 只接受 OKX USDT perpetual_swap 15m");
    }
    if manifest.warmup_days != STRICT_STATIC_WARMUP_DAYS {
        bail!("strict static Top60 必须冻结 60 天预热");
    }
    validate_half_open_window(
        manifest.warmup_start_ms,
        manifest.evaluation_start_ms,
        "预热窗口",
    )?;
    validate_half_open_window(
        manifest.evaluation_start_ms,
        manifest.evaluation_end_exclusive_ms,
        "评价窗口",
    )?;
    let required_warmup_start_ms = manifest
        .evaluation_start_ms
        .checked_sub(i64::from(STRICT_STATIC_WARMUP_DAYS) * 86_400_000)
        .context("strict static Top60 预热起点计算溢出")?;
    if manifest.warmup_start_ms != required_warmup_start_ms {
        bail!("strict static Top60 的 warmup_start 必须等于 evaluation_start 前 60 天");
    }
    if manifest.selection_timestamp_ms < manifest.evaluation_end_exclusive_ms
        || manifest.instrument_snapshot_observed_at_ms < manifest.evaluation_end_exclusive_ms
    {
        bail!("strict static Top60 的 instrument 快照与 selection 必须在评价结束后冻结");
    }
    if manifest.selection_rule_id != STRICT_STATIC_SELECTION_RULE_ID {
        bail!("strict static Top60 selection_rule_id 与固定合同不一致");
    }
    if manifest.instrument_source_endpoint != STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT {
        bail!("strict static Top60 instrument endpoint 与固定合同不一致");
    }
    validate_sha256(
        &manifest.instrument_snapshot_sha256,
        "instrument_snapshot_sha256",
        None,
    )?;
    validate_members(manifest)
}

/// 纯函数正式门禁：任一成员资格、tick、指纹或覆盖证据缺失都返回错误。
pub fn formal_gate(
    manifest: &StrictStaticUniverseManifestV2,
    coverage: &StrictStaticUniverseCoverageV2,
) -> Result<StrictStaticFormalGatePassV2> {
    validate_manifest_v2(manifest)?;
    if coverage.universe_version != manifest.universe_version {
        bail!("strict static Top60 覆盖证据与 manifest 版本不一致");
    }
    let manifest_sha256 = canonical_manifest_sha256(manifest)?;
    validate_sha256(&coverage.manifest_sha256, "manifest_sha256", None)?;
    if coverage.manifest_sha256 != manifest_sha256 {
        bail!("strict static Top60 覆盖证据绑定的 manifest SHA-256 不一致");
    }
    if coverage.warmup_start_ms != manifest.warmup_start_ms
        || coverage.evaluation_start_ms != manifest.evaluation_start_ms
        || coverage.evaluation_end_exclusive_ms != manifest.evaluation_end_exclusive_ms
    {
        bail!("strict static Top60 覆盖证据的全局半开边界与 manifest 不一致");
    }
    let actual_by_symbol = unique_coverage_by_symbol(coverage)?;
    let expected_total = candle_count(
        manifest.warmup_start_ms,
        manifest.evaluation_end_exclusive_ms,
    )?;
    let expected_last_ms = manifest
        .evaluation_end_exclusive_ms
        .checked_sub(STRICT_STATIC_CANDLE_INTERVAL_MS)
        .context("strict static Top60 末根时间下溢")?;
    let mut covered_candle_count = 0usize;

    for member in &manifest.members {
        let actual = actual_by_symbol
            .get(member.symbol.as_str())
            .with_context(|| format!("{} 缺少正式覆盖证据", member.symbol))?;
        if actual.expected_candle_count != expected_total
            || actual.loaded_candle_count != expected_total
            || actual.confirmed_candle_count != expected_total
            || actual.first_timestamp_ms != Some(manifest.warmup_start_ms)
            || actual.last_timestamp_ms != Some(expected_last_ms)
            || !actual.is_contiguous_15m
            || actual.missing_candle_count != 0
        {
            bail!("{} 的 60 天预热或评价 K 线覆盖不完整", member.symbol);
        }
        validate_tick_size(&actual.frozen_tick_size, &member.symbol)?;
        if actual.frozen_tick_size != member.frozen_tick_size {
            bail!("{} 的 frozen tick_size 与实际回放输入不一致", member.symbol);
        }
        if actual.instrument_source_sha256 != member.instrument_source_sha256 {
            bail!("{} 的 instrument 来源指纹漂移", member.symbol);
        }
        if actual.candle_source != member.candle_source {
            bail!("{} 的 K 线来源或内容指纹漂移", member.symbol);
        }
        covered_candle_count = covered_candle_count
            .checked_add(actual.loaded_candle_count)
            .context("strict static Top60 覆盖 K 线总数溢出")?;
    }

    Ok(StrictStaticFormalGatePassV2 {
        universe_version: manifest.universe_version.clone(),
        manifest_sha256,
        symbol_count: STRICT_STATIC_MEMBER_COUNT,
        warmup_candles_per_symbol: candle_count(
            manifest.warmup_start_ms,
            manifest.evaluation_start_ms,
        )?,
        evaluation_candles_per_symbol: candle_count(
            manifest.evaluation_start_ms,
            manifest.evaluation_end_exclusive_ms,
        )?,
        covered_candle_count,
    })
}

/// 60 个成员必须唯一，并完整覆盖排名 1 到 60。
fn validate_members(manifest: &StrictStaticUniverseManifestV2) -> Result<()> {
    if manifest.members.len() != STRICT_STATIC_MEMBER_COUNT {
        bail!("strict static Top60 必须恰好包含 60 个成员");
    }
    let expected_total = candle_count(
        manifest.warmup_start_ms,
        manifest.evaluation_end_exclusive_ms,
    )?;
    let mut symbols = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    let mut candle_source_ids = BTreeSet::new();
    let mut candle_source_hashes = BTreeSet::new();

    for (index, member) in manifest.members.iter().enumerate() {
        validate_symbol(&member.symbol)?;
        if !symbols.insert(member.symbol.as_str()) {
            bail!("strict static Top60 包含重复 symbol：{}", member.symbol);
        }
        let expected_rank = u32::try_from(index + 1).context("strict static Top60 排名溢出")?;
        if member.rank != expected_rank
            || member.rank as usize > STRICT_STATIC_MEMBER_COUNT
            || !ranks.insert(member.rank)
        {
            bail!("strict static Top60 成员数组必须严格按排名 1 到 60 排列");
        }
        if member.listed_at_ms <= 0 || member.listed_at_ms > manifest.warmup_start_ms {
            bail!("{} 的上市时间不足以覆盖 60 天预热", member.symbol);
        }
        validate_tick_size(&member.frozen_tick_size, &member.symbol)?;
        validate_sha256(
            &member.instrument_source_sha256,
            "instrument_source_sha256",
            Some(&member.symbol),
        )?;
        validate_candle_fingerprint(
            &member.symbol,
            &member.candle_source,
            manifest.warmup_start_ms,
            manifest.evaluation_end_exclusive_ms,
            expected_total,
        )?;
        if !candle_source_ids.insert(member.candle_source.source_id.as_str())
            || !candle_source_hashes.insert(member.candle_source.sha256.as_str())
        {
            bail!("{} 的 K 线指纹被其他 symbol 复用", member.symbol);
        }
    }
    if ranks.len() != STRICT_STATIC_MEMBER_COUNT {
        bail!("strict static Top60 没有完整覆盖排名 1 到 60");
    }
    Ok(())
}

/// 实际覆盖证据也必须恰好 60 个且不得重复或夹带额外成员。
fn unique_coverage_by_symbol(
    coverage: &StrictStaticUniverseCoverageV2,
) -> Result<BTreeMap<&str, &StrictStaticMemberCoverageV2>> {
    if coverage.members.len() != STRICT_STATIC_MEMBER_COUNT {
        bail!("strict static Top60 覆盖证据必须恰好包含 60 个成员");
    }
    let mut by_symbol = BTreeMap::new();
    for member in &coverage.members {
        if by_symbol.insert(member.symbol.as_str(), member).is_some() {
            bail!(
                "strict static Top60 覆盖证据包含重复 symbol：{}",
                member.symbol
            );
        }
    }
    Ok(by_symbol)
}

/// manifest 内的 K 线指纹必须精确覆盖全局预热与评价半开区间。
fn validate_candle_fingerprint(
    symbol: &str,
    fingerprint: &FrozenStaticCandleSourceFingerprintV2,
    expected_first_ms: i64,
    expected_end_exclusive_ms: i64,
    expected_count: usize,
) -> Result<()> {
    let expected_last_ms = expected_end_exclusive_ms
        .checked_sub(STRICT_STATIC_CANDLE_INTERVAL_MS)
        .context("strict static Top60 末根时间下溢")?;
    let expected_source_id =
        canonical_candle_source_id(symbol, expected_first_ms, expected_end_exclusive_ms)?;
    if fingerprint.symbol != symbol
        || fingerprint.canonicalization_version != STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION
        || fingerprint.source_id != expected_source_id
        || fingerprint.source_kind != STRICT_STATIC_CANDLE_SOURCE_KIND
        || fingerprint.volume_field != STRICT_STATIC_VOLUME_FIELD
        || !fingerprint.confirmed_only
    {
        bail!("{symbol} 缺少同源、已确认 15m vol_ccy K 线证据");
    }
    validate_sha256(&fingerprint.sha256, "candle_source.sha256", Some(symbol))?;
    if fingerprint.first_timestamp_ms != expected_first_ms
        || fingerprint.last_timestamp_ms != expected_last_ms
        || fingerprint.candle_count != expected_count
    {
        bail!("{symbol} 的 K 线指纹没有完整覆盖全局预热与评价窗口");
    }
    Ok(())
}

/// OKX 原始 `tickSz` 必须是正的普通十进制文本，不接受指数或浮点中间态。
fn validate_tick_size(value: &str, symbol: &str) -> Result<Decimal> {
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

/// 预热和评价范围统一使用 15m 对齐的半开区间。
fn validate_half_open_window(from_ms: i64, to_ms: i64, label: &str) -> Result<()> {
    if from_ms < 0
        || to_ms <= from_ms
        || from_ms.rem_euclid(STRICT_STATIC_CANDLE_INTERVAL_MS) != 0
        || to_ms.rem_euclid(STRICT_STATIC_CANDLE_INTERVAL_MS) != 0
    {
        bail!("{label}必须是非负且对齐 15m 的半开区间");
    }
    Ok(())
}

/// 计算严格 15m 半开区间应有的 K 线数量。
fn candle_count(from_ms: i64, to_ms: i64) -> Result<usize> {
    validate_half_open_window(from_ms, to_ms, "K 线窗口")?;
    usize::try_from((to_ms - from_ms) / STRICT_STATIC_CANDLE_INTERVAL_MS)
        .context("strict static Top60 K 线数量超出 usize")
}

/// 只接受 `<BASE>-USDT-SWAP`，避免数据加载层对 symbol 做隐式规范化。
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
        bail!("strict static Top60 成员不是严格的 OKX USDT SWAP 标识：{symbol}");
    }
    Ok(())
}

/// SHA-256 必须是非零小写十六进制，空占位符不能进入正式门禁。
fn validate_sha256(value: &str, field: &str, symbol: Option<&str>) -> Result<()> {
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

/// 递归排序 JSON object key；数组顺序保留，因为成员顺序就是冻结排名。
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

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_MS: i64 = 86_400_000;

    fn sha(seed: u64) -> String {
        format!("{seed:064x}")
    }

    fn manifest() -> StrictStaticUniverseManifestV2 {
        let evaluation_start_ms = 100 * DAY_MS;
        let warmup_start_ms = evaluation_start_ms - i64::from(STRICT_STATIC_WARMUP_DAYS) * DAY_MS;
        let evaluation_end_exclusive_ms = 160 * DAY_MS;
        let expected_count = ((evaluation_end_exclusive_ms - warmup_start_ms)
            / STRICT_STATIC_CANDLE_INTERVAL_MS) as usize;
        let members = (1..=STRICT_STATIC_MEMBER_COUNT)
            .map(|rank| {
                let symbol = format!("S{rank}-USDT-SWAP");
                StrictStaticUniverseMemberV2 {
                    symbol: symbol.clone(),
                    listed_at_ms: warmup_start_ms - DAY_MS,
                    status_at_selection: StrictStaticMemberStatusV2::Live,
                    rank: rank as u32,
                    frozen_tick_size: "0.001".to_owned(),
                    instrument_source_sha256: sha(10_000 + rank as u64),
                    candle_source: FrozenStaticCandleSourceFingerprintV2 {
                        symbol: symbol.clone(),
                        canonicalization_version: STRICT_STATIC_CANDLE_CANONICALIZATION_VERSION
                            .to_owned(),
                        source_id: canonical_candle_source_id(
                            &symbol,
                            warmup_start_ms,
                            evaluation_end_exclusive_ms,
                        )
                        .expect("fixture source id"),
                        sha256: sha(20_000 + rank as u64),
                        source_kind: STRICT_STATIC_CANDLE_SOURCE_KIND.to_owned(),
                        volume_field: STRICT_STATIC_VOLUME_FIELD.to_owned(),
                        confirmed_only: true,
                        first_timestamp_ms: warmup_start_ms,
                        last_timestamp_ms: evaluation_end_exclusive_ms
                            - STRICT_STATIC_CANDLE_INTERVAL_MS,
                        candle_count: expected_count,
                    },
                }
            })
            .collect();
        StrictStaticUniverseManifestV2 {
            schema_version: STRICT_STATIC_SCHEMA_VERSION,
            universe_version: "surviving_static_top60_fixture_v2".to_owned(),
            generated_at_ms: evaluation_end_exclusive_ms + 2,
            selection_timestamp_ms: evaluation_end_exclusive_ms + 1,
            instrument_source_endpoint: STRICT_STATIC_INSTRUMENT_SOURCE_ENDPOINT.to_owned(),
            instrument_snapshot_sha256: sha(1),
            instrument_snapshot_observed_at_ms: evaluation_end_exclusive_ms,
            exchange: "okx".to_owned(),
            market_type: "perpetual_swap".to_owned(),
            quote_currency: "USDT".to_owned(),
            timeframe: "15m".to_owned(),
            warmup_start_ms,
            evaluation_start_ms,
            evaluation_end_exclusive_ms,
            warmup_days: STRICT_STATIC_WARMUP_DAYS,
            selection_rule_id: STRICT_STATIC_SELECTION_RULE_ID.to_owned(),
            selection_rule: "current-live at frozen selection timestamp; fixture".to_owned(),
            members,
        }
    }

    fn coverage(manifest: &StrictStaticUniverseManifestV2) -> StrictStaticUniverseCoverageV2 {
        StrictStaticUniverseCoverageV2 {
            universe_version: manifest.universe_version.clone(),
            manifest_sha256: canonical_manifest_sha256(manifest).expect("fixture manifest hash"),
            warmup_start_ms: manifest.warmup_start_ms,
            evaluation_start_ms: manifest.evaluation_start_ms,
            evaluation_end_exclusive_ms: manifest.evaluation_end_exclusive_ms,
            members: manifest
                .members
                .iter()
                .map(|member| StrictStaticMemberCoverageV2 {
                    symbol: member.symbol.clone(),
                    expected_candle_count: member.candle_source.candle_count,
                    loaded_candle_count: member.candle_source.candle_count,
                    first_timestamp_ms: Some(member.candle_source.first_timestamp_ms),
                    last_timestamp_ms: Some(member.candle_source.last_timestamp_ms),
                    confirmed_candle_count: member.candle_source.candle_count,
                    is_contiguous_15m: true,
                    missing_candle_count: 0,
                    frozen_tick_size: member.frozen_tick_size.clone(),
                    instrument_source_sha256: member.instrument_source_sha256.clone(),
                    candle_source: member.candle_source.clone(),
                })
                .collect(),
        }
    }

    #[test]
    fn formal_gate_accepts_exact_static_sixty_of_sixty() {
        let manifest = manifest();
        let coverage = coverage(&manifest);

        let pass = formal_gate(&manifest, &coverage).expect("complete static Top60 must pass");

        assert_eq!(pass.symbol_count, 60);
        assert_eq!(pass.warmup_candles_per_symbol, 5_760);
        assert_eq!(pass.evaluation_candles_per_symbol, 5_760);
        assert_eq!(pass.covered_candle_count, 60 * 11_520);
        assert_eq!(pass.manifest_sha256, coverage.manifest_sha256);
    }

    #[test]
    fn manifest_rejects_wrong_global_boundaries_or_member_count() {
        let mut short_warmup = manifest();
        short_warmup.warmup_start_ms += STRICT_STATIC_CANDLE_INTERVAL_MS;
        assert!(validate_manifest_v2(&short_warmup).is_err());

        let mut fifty_nine = manifest();
        fifty_nine.members.pop();
        assert!(validate_manifest_v2(&fifty_nine).is_err());

        let mut duplicate_symbol = manifest();
        duplicate_symbol.members[1].symbol = duplicate_symbol.members[0].symbol.clone();
        assert!(validate_manifest_v2(&duplicate_symbol).is_err());
    }

    #[test]
    fn manifest_rejects_late_or_zero_listing_invalid_tick_rank_or_fingerprint() {
        let mut late_listing = manifest();
        late_listing.members[0].listed_at_ms = late_listing.warmup_start_ms + 1;
        assert!(validate_manifest_v2(&late_listing).is_err());

        let mut zero_listing = manifest();
        zero_listing.members[0].listed_at_ms = 0;
        assert!(validate_manifest_v2(&zero_listing).is_err());

        let mut invalid_tick = manifest();
        invalid_tick.members[0].frozen_tick_size = "0".to_owned();
        assert!(validate_manifest_v2(&invalid_tick).is_err());

        for invalid in [" 0.001", "-0.001", "1e-3", ".001", "0."] {
            let mut invalid_tick_text = manifest();
            invalid_tick_text.members[0].frozen_tick_size = invalid.to_owned();
            assert!(validate_manifest_v2(&invalid_tick_text).is_err());
        }

        let mut out_of_order_rank = manifest();
        out_of_order_rank.members.swap(0, 1);
        assert!(validate_manifest_v2(&out_of_order_rank).is_err());

        let mut duplicate_rank = manifest();
        duplicate_rank.members[1].rank = duplicate_rank.members[0].rank;
        assert!(validate_manifest_v2(&duplicate_rank).is_err());

        let mut missing_instrument_sha = manifest();
        missing_instrument_sha.members[0]
            .instrument_source_sha256
            .clear();
        assert!(validate_manifest_v2(&missing_instrument_sha).is_err());

        let mut missing_candle_fingerprint = manifest();
        missing_candle_fingerprint.members[0]
            .candle_source
            .sha256
            .clear();
        assert!(validate_manifest_v2(&missing_candle_fingerprint).is_err());

        let mut wrong_fingerprint_symbol = manifest();
        wrong_fingerprint_symbol.members[0].candle_source.symbol =
            wrong_fingerprint_symbol.members[1].symbol.clone();
        assert!(validate_manifest_v2(&wrong_fingerprint_symbol).is_err());

        let mut wrong_canonicalization = manifest();
        wrong_canonicalization.members[0]
            .candle_source
            .canonicalization_version = "unknown_v2".to_owned();
        assert!(validate_manifest_v2(&wrong_canonicalization).is_err());

        let mut wrong_source_window = manifest();
        wrong_source_window.members[0].candle_source.source_id = canonical_candle_source_id(
            &wrong_source_window.members[0].symbol,
            wrong_source_window.warmup_start_ms + STRICT_STATIC_CANDLE_INTERVAL_MS,
            wrong_source_window.evaluation_end_exclusive_ms,
        )
        .expect("aligned alternate source id");
        assert!(validate_manifest_v2(&wrong_source_window).is_err());

        let mut reused_source_id = manifest();
        reused_source_id.members[1].candle_source.source_id =
            reused_source_id.members[0].candle_source.source_id.clone();
        assert!(validate_manifest_v2(&reused_source_id).is_err());

        let mut reused_fingerprint = manifest();
        reused_fingerprint.members[1].candle_source.sha256 =
            reused_fingerprint.members[0].candle_source.sha256.clone();
        assert!(validate_manifest_v2(&reused_fingerprint).is_err());
    }

    #[test]
    fn manifest_rejects_unsealed_selection_or_instrument_snapshot_identity() {
        let mut wrong_rule = manifest();
        wrong_rule.selection_rule_id = "surviving_static_top60_latest".to_owned();
        assert!(validate_manifest_v2(&wrong_rule).is_err());

        let mut wrong_endpoint = manifest();
        wrong_endpoint.instrument_source_endpoint =
            "https://www.okx.com/api/v5/public/instruments".to_owned();
        assert!(validate_manifest_v2(&wrong_endpoint).is_err());

        let mut missing_snapshot_hash = manifest();
        missing_snapshot_hash.instrument_snapshot_sha256.clear();
        assert!(validate_manifest_v2(&missing_snapshot_hash).is_err());

        let mut selected_before_evaluation_end = manifest();
        selected_before_evaluation_end.selection_timestamp_ms =
            selected_before_evaluation_end.evaluation_end_exclusive_ms - 1;
        assert!(validate_manifest_v2(&selected_before_evaluation_end).is_err());

        let mut observed_before_evaluation_end = manifest();
        observed_before_evaluation_end.instrument_snapshot_observed_at_ms =
            observed_before_evaluation_end.evaluation_end_exclusive_ms - 1;
        assert!(validate_manifest_v2(&observed_before_evaluation_end).is_err());

        let mut observed_after_selection = manifest();
        observed_after_selection.instrument_snapshot_observed_at_ms =
            observed_after_selection.selection_timestamp_ms + 1;
        assert!(validate_manifest_v2(&observed_after_selection).is_err());

        let mut generated_before_selection = manifest();
        generated_before_selection.generated_at_ms =
            generated_before_selection.selection_timestamp_ms - 1;
        assert!(validate_manifest_v2(&generated_before_selection).is_err());
    }

    #[test]
    fn formal_gate_rejects_missing_incomplete_unconfirmed_or_drifted_coverage() {
        let manifest = manifest();

        let mut missing_member = coverage(&manifest);
        missing_member.members.pop();
        assert!(formal_gate(&manifest, &missing_member).is_err());

        let mut incomplete = coverage(&manifest);
        incomplete.members[0].loaded_candle_count -= 1;
        incomplete.members[0].missing_candle_count = 1;
        assert!(formal_gate(&manifest, &incomplete).is_err());

        let mut unconfirmed = coverage(&manifest);
        unconfirmed.members[0].confirmed_candle_count -= 1;
        assert!(formal_gate(&manifest, &unconfirmed).is_err());

        let mut discontinuous = coverage(&manifest);
        discontinuous.members[0].is_contiguous_15m = false;
        assert!(formal_gate(&manifest, &discontinuous).is_err());

        let mut tick_drift = coverage(&manifest);
        tick_drift.members[0].frozen_tick_size = "0.0010".to_owned();
        assert!(formal_gate(&manifest, &tick_drift).is_err());

        let mut fingerprint_drift = coverage(&manifest);
        fingerprint_drift.members[0].candle_source.sha256 = sha(999_999);
        assert!(formal_gate(&manifest, &fingerprint_drift).is_err());

        let mut missing_manifest_hash = coverage(&manifest);
        missing_manifest_hash.manifest_sha256.clear();
        assert!(formal_gate(&manifest, &missing_manifest_hash).is_err());

        let mut wrong_manifest_hash = coverage(&manifest);
        wrong_manifest_hash.manifest_sha256 = sha(999_998);
        assert!(formal_gate(&manifest, &wrong_manifest_hash).is_err());

        let stale_coverage = coverage(&manifest);
        let mut changed_manifest = manifest.clone();
        changed_manifest
            .selection_rule
            .push_str("; amended after coverage");
        assert!(formal_gate(&changed_manifest, &stale_coverage).is_err());
    }

    #[test]
    fn canonical_manifest_hash_is_deterministic_and_content_sensitive() {
        let manifest = manifest();
        let first = canonical_manifest_sha256(&manifest).expect("first hash");
        let second = canonical_manifest_sha256(&manifest).expect("second hash");
        assert_eq!(first, second);

        let mut changed = manifest.clone();
        changed.universe_version.push_str("_changed");
        let changed_hash = canonical_manifest_sha256(&changed).expect("changed hash");
        assert_ne!(first, changed_hash);
    }

    #[test]
    fn decoder_rejects_non_live_status() {
        let raw = serde_json::to_vec(&manifest()).expect("serialize fixture");
        let raw = String::from_utf8(raw)
            .expect("fixture JSON is UTF-8")
            .replace(
                "\"status_at_selection\":\"live\"",
                "\"status_at_selection\":\"suspended\"",
            );

        assert!(decode_and_validate_manifest_v2(raw.as_bytes()).is_err());
    }

    #[test]
    fn decoder_rejects_unknown_unsealed_fields() {
        let mut value = serde_json::to_value(manifest()).expect("serialize fixture");
        value
            .as_object_mut()
            .expect("manifest is an object")
            .insert(
                "instrument_snapshot_path".to_owned(),
                Value::String("/tmp/unsealed.json".to_owned()),
            );
        let raw = serde_json::to_vec(&value).expect("encode modified fixture");

        assert!(decode_and_validate_manifest_v2(&raw).is_err());
    }
}
