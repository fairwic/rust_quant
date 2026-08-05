use super::Candle;
use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, postgres::PgRow, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 固定使用这份 current-live Top60 manifest，避免回放时静默换成当前交易所币种列表。
pub const FROZEN_UNIVERSE_MANIFEST_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/research_manifests/market_filtered_volume_v9_top60_current_live_20260723.json"
);

/// Manifest 的内容哈希是冻结身份的一部分；文件改名或只保留 version 都不足以防止成员漂移。
pub const FROZEN_UNIVERSE_MANIFEST_SHA256: &str =
    "3fd267ca5cf1ecee8199232729da0e6db917803f6e7a1b363fa84e0ba75d5a4f";

/// 本适配器只接受已经冻结的研究币池版本，不把相似 manifest 当作同一份证据。
pub const FROZEN_UNIVERSE_VERSION: &str = "top60_v36_direct_kline_20260721_frozen_20260723";

const FROZEN_MEMBER_COUNT: usize = 60;
const FROZEN_GENERATED_AT_MS: i64 = 1_784_764_800_000;
const FROZEN_WINDOW_START_MS: i64 = 1_751_328_000_000;
const FROZEN_WINDOW_END_MS: i64 = 1_784_470_500_001;
const CANDLE_INTERVAL_MS: i64 = 15 * 60 * 1_000;
const DAY_MS: i64 = 86_400_000;
/// EMA596/696、周 P90 与 MACD 的同源预热期；不属于评价窗口，也不产生入场。
pub const FROZEN_WARMUP_DAYS: i64 = 60;
const FROZEN_SELECTION_RULE: &str = "current-live OKX USDT swaps only; md5(top60_v36_direct_kline_20260721:symbol) first 60; frozen before fresh v3/v9 comparison";
const FROZEN_CLASSIFICATION_BOUNDARY: &str =
    "exchange_symbols status=live, market_type=perpetual, contract_type=linear";

/// 经过哈希、数据域和成员校验后的单窗口冻结币池。
///
/// 该结构只描述 DatasetManifest 的只读选择结果，不拥有原始市场事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenUniverseSpec {
    pub universe_version: String,
    pub manifest_sha256: String,
    pub window_start_ms: i64,
    /// 不包含的查询上界；当前 manifest 用“最后一根开盘时间 + 1ms”表达。
    pub window_end_ms: i64,
    /// 保留 manifest 中的冻结顺序，不能按数据库返回顺序重新排列。
    pub symbols: Vec<String>,
}

/// 一段连续缺失的 15m 开盘时间；使用首尾时间而不是模糊的自然日范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingCandleRange {
    pub first_timestamp_ms: i64,
    pub last_timestamp_ms: i64,
    pub candle_count: usize,
}

/// 单币数据覆盖诊断；缺口只被报告，不会用未确认 K 线或其他 volume 字段补齐。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolCoverageDiagnostic {
    pub expected_candle_count: usize,
    pub loaded_candle_count: usize,
    pub first_timestamp_ms: Option<i64>,
    pub last_timestamp_ms: Option<i64>,
    pub missing_candle_count: usize,
    pub missing_ranges: Vec<MissingCandleRange>,
    pub is_complete: bool,
}

/// 一个冻结成员的价格步长、已确认 K 线和覆盖证据。
#[derive(Debug, Clone)]
pub struct FrozenSymbolCandles {
    pub symbol: String,
    pub tick_size: f64,
    /// 含 60 天预热和正式评价窗口；回放入口仍只允许评价窗口内生成信号。
    pub candles: Vec<Candle>,
    pub warmup_expected_candle_count: usize,
    pub warmup_loaded_candle_count: usize,
    pub warmup_is_complete: bool,
    pub coverage: SymbolCoverageDiagnostic,
}

/// 整个 Top60 加载结果的聚合覆盖诊断，供调用方在回放前显式 fail-close。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniverseCoverageDiagnostic {
    pub expected_symbol_count: usize,
    pub returned_symbol_count: usize,
    pub symbols_with_candles: usize,
    pub complete_symbol_count: usize,
    pub expected_candles_per_symbol: usize,
    pub expected_total_candle_count: usize,
    pub loaded_total_candle_count: usize,
    pub missing_total_candle_count: usize,
    pub warmup_expected_candles_per_symbol: usize,
    pub warmup_loaded_total_candle_count: usize,
    pub warmup_complete_symbol_count: usize,
}

/// 冻结 Top60 的完整只读数据集；不包含连接池或任何可写数据库能力。
#[derive(Debug, Clone)]
pub struct FrozenUniverseData {
    pub universe_version: String,
    pub manifest_sha256: String,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub symbols: Vec<FrozenSymbolCandles>,
    pub coverage: UniverseCoverageDiagnostic,
}

/// 读取并严格校验仓库内冻结 manifest，供连接数据库前先确认 Dataset identity。
pub fn load_frozen_universe_spec() -> Result<FrozenUniverseSpec> {
    let path = Path::new(FROZEN_UNIVERSE_MANIFEST_PATH);
    let raw = std::fs::read(path)
        .with_context(|| format!("读取冻结币池 manifest 失败：{}", path.display()))?;
    parse_and_validate_manifest(&raw)
        .with_context(|| format!("冻结币池 manifest 校验失败：{}", path.display()))
}

/// 从明确的 quant_core 环境变量建立只读数据加载链路，并在结果返回前关闭连接池。
///
/// 这里故意不接受通用 `DATABASE_URL`，避免继承 quant_web shell 环境后读错事实库。
pub async fn load_frozen_top60_from_quant_core() -> Result<FrozenUniverseData> {
    let spec = load_frozen_universe_spec()?;
    let pool = connect_quant_core_from_env().await?;
    let result = load_from_pool(&pool, &spec).await;
    pool.close().await;
    result
}

/// 解析 manifest 后同时锁定原始字节哈希和语义字段，防止只更新 version 的伪冻结。
fn parse_and_validate_manifest(raw: &[u8]) -> Result<FrozenUniverseSpec> {
    let actual_sha256 = hex::encode(Sha256::digest(raw));
    if actual_sha256 != FROZEN_UNIVERSE_MANIFEST_SHA256 {
        bail!(
            "冻结币池 manifest 哈希漂移：expected={}, actual={}",
            FROZEN_UNIVERSE_MANIFEST_SHA256,
            actual_sha256
        );
    }

    let manifest: HistoricalUniverseManifest =
        serde_json::from_slice(raw).context("解析冻结币池 manifest JSON")?;
    if manifest.schema_version != 1
        || manifest.universe_version != FROZEN_UNIVERSE_VERSION
        || manifest.generated_at_ms != FROZEN_GENERATED_AT_MS
    {
        bail!("冻结币池 identity 必须保持 schema v1、固定 version 和 generated_at");
    }
    if manifest.exchange != "okx"
        || manifest.market_type != "perpetual_swap"
        || manifest.quote_currency != "USDT"
        || manifest.timeframe != "15m"
    {
        bail!("冻结币池只允许 OKX USDT linear perpetual 15m 数据域");
    }
    if manifest.selection_rule != FROZEN_SELECTION_RULE
        || manifest.source.instruments_endpoint != "quant_core.exchange_symbols"
        || manifest.source.candlestick_archive_format != "quant_core per-symbol 15m tables"
        || manifest.source.classification_boundary != FROZEN_CLASSIFICATION_BOUNDARY
    {
        bail!("冻结币池选择规则或 quant_core 来源边界发生漂移");
    }
    if manifest.months.len() != 1 {
        bail!("冻结币池必须且只能包含一个预先固定的回放窗口");
    }

    let window = &manifest.months[0];
    if window.effective_from_ms != FROZEN_WINDOW_START_MS
        || window.effective_to_ms != FROZEN_WINDOW_END_MS
        || window.archive_candidate_families != FROZEN_MEMBER_COUNT
        || window.archive_files_available != FROZEN_MEMBER_COUNT
        || window.complete_candidates != FROZEN_MEMBER_COUNT
        || window.members.len() != FROZEN_MEMBER_COUNT
    {
        bail!("冻结币池窗口或 60 成员计数发生漂移");
    }

    let mut seen = BTreeSet::new();
    let mut symbols = Vec::with_capacity(FROZEN_MEMBER_COUNT);
    for member in &window.members {
        validate_swap_symbol(&member.symbol)?;
        if !seen.insert(member.symbol.clone()) {
            bail!("冻结币池包含重复成员：{}", member.symbol);
        }
        symbols.push(member.symbol.clone());
    }

    expected_candle_count(window.effective_from_ms, window.effective_to_ms)?;
    Ok(FrozenUniverseSpec {
        universe_version: manifest.universe_version,
        manifest_sha256: actual_sha256,
        window_start_ms: window.effective_from_ms,
        window_end_ms: window.effective_to_ms,
        symbols,
    })
}

/// 只从 Core 专用变量选择连接串，并拒绝空值或隐式 Web 数据库回退。
fn quant_core_database_url_from_env() -> Result<String> {
    for key in [
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_owned());
            }
        }
    }
    bail!("必须设置 QUANT_CORE_DATABASE_URL 或 POSTGRES_QUANT_CORE_DATABASE_URL")
}

/// 建立小型只读加载池，并用数据库自身返回的名称再次阻止跨库误读。
async fn connect_quant_core_from_env() -> Result<PgPool> {
    let database_url = quant_core_database_url_from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("连接 quant_core 失败")?;
    let current_database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .context("读取当前数据库名称失败")?;
    if current_database != "quant_core" {
        pool.close().await;
        bail!(
            "Core Research 只允许读取 quant_core，当前连接到 {}",
            current_database
        );
    }
    Ok(pool)
}

/// 按 manifest 顺序加载每个成员，避免数据库行顺序改变下游同时点决策顺序。
async fn load_from_pool(pool: &PgPool, spec: &FrozenUniverseSpec) -> Result<FrozenUniverseData> {
    let tick_sizes = load_tick_sizes(pool, &spec.symbols).await?;
    let warmup_start_ms = spec
        .window_start_ms
        .checked_sub(FROZEN_WARMUP_DAYS * DAY_MS)
        .context("计算冻结币池预热起点时溢出")?;
    let warmup_expected_candle_count =
        exact_exclusive_candle_count(warmup_start_ms, spec.window_start_ms)?;
    let mut loaded_symbols = Vec::with_capacity(spec.symbols.len());
    for symbol in &spec.symbols {
        let tick_size = *tick_sizes
            .get(symbol)
            .with_context(|| format!("冻结成员缺少有效 tick_size：{symbol}"))?;
        let candles =
            load_symbol_candles(pool, symbol, warmup_start_ms, spec.window_end_ms).await?;
        let warmup_candles = candles
            .iter()
            .copied()
            .take_while(|candle| candle.timestamp_ms < spec.window_start_ms)
            .collect::<Vec<_>>();
        let evaluation_candles = candles
            .iter()
            .copied()
            .filter(|candle| candle.timestamp_ms >= spec.window_start_ms)
            .collect::<Vec<_>>();
        let warmup_coverage = build_symbol_coverage(
            warmup_start_ms,
            spec.window_start_ms - CANDLE_INTERVAL_MS + 1,
            &warmup_candles,
        )?;
        let coverage = build_symbol_coverage(
            spec.window_start_ms,
            spec.window_end_ms,
            &evaluation_candles,
        )?;
        loaded_symbols.push(FrozenSymbolCandles {
            symbol: symbol.clone(),
            tick_size,
            candles,
            warmup_expected_candle_count,
            warmup_loaded_candle_count: warmup_candles.len(),
            warmup_is_complete: warmup_coverage.is_complete,
            coverage,
        });
    }

    let expected_candles_per_symbol =
        expected_candle_count(spec.window_start_ms, spec.window_end_ms)?;
    let loaded_total_candle_count = loaded_symbols
        .iter()
        .map(|symbol| symbol.coverage.loaded_candle_count)
        .sum();
    let missing_total_candle_count = loaded_symbols
        .iter()
        .map(|symbol| symbol.coverage.missing_candle_count)
        .sum();
    let coverage = UniverseCoverageDiagnostic {
        expected_symbol_count: FROZEN_MEMBER_COUNT,
        returned_symbol_count: loaded_symbols.len(),
        symbols_with_candles: loaded_symbols
            .iter()
            .filter(|symbol| !symbol.candles.is_empty())
            .count(),
        complete_symbol_count: loaded_symbols
            .iter()
            .filter(|symbol| symbol.coverage.is_complete)
            .count(),
        expected_candles_per_symbol,
        expected_total_candle_count: expected_candles_per_symbol
            .checked_mul(FROZEN_MEMBER_COUNT)
            .context("冻结币池期望 K 线总数溢出")?,
        loaded_total_candle_count,
        missing_total_candle_count,
        warmup_expected_candles_per_symbol: warmup_expected_candle_count,
        warmup_loaded_total_candle_count: loaded_symbols
            .iter()
            .map(|symbol| symbol.warmup_loaded_candle_count)
            .sum(),
        warmup_complete_symbol_count: loaded_symbols
            .iter()
            .filter(|symbol| symbol.warmup_is_complete)
            .count(),
    };

    Ok(FrozenUniverseData {
        universe_version: spec.universe_version.clone(),
        manifest_sha256: spec.manifest_sha256.clone(),
        window_start_ms: spec.window_start_ms,
        window_end_ms: spec.window_end_ms,
        symbols: loaded_symbols,
        coverage,
    })
}

/// 从 `exchange_symbols` 读取冻结成员的 linear perpetual 价格步长，并要求 60 个成员全覆盖。
///
/// 币池已经由 manifest 锁定生成时的 live 状态，因此这里不按运行日的 `status` 再筛选，
/// 避免成员后来退市后历史回放无法重现；tick_size 本身仍属于运行时元数据缺口。
async fn load_tick_sizes(pool: &PgPool, symbols: &[String]) -> Result<BTreeMap<String, f64>> {
    let rows = sqlx::query(
        r#"
        SELECT exchange_symbol, tick_size
        FROM exchange_symbols
        WHERE exchange = 'okx'
          AND market_type = 'perpetual'
          AND contract_type = 'linear'
        ORDER BY exchange_symbol
        "#,
    )
    .fetch_all(pool)
    .await
    .context("读取 OKX live linear perpetual tick_size 失败")?;

    let requested = symbols.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut tick_sizes = BTreeMap::new();
    for row in rows {
        let symbol: String = row
            .try_get("exchange_symbol")
            .context("解析 exchange_symbols.exchange_symbol")?;
        if !requested.contains(symbol.as_str()) {
            continue;
        }
        let raw_tick_size: Option<String> = row
            .try_get("tick_size")
            .with_context(|| format!("解析 {symbol} tick_size"))?;
        let tick_size = parse_positive_number(
            raw_tick_size
                .as_deref()
                .with_context(|| format!("{symbol} tick_size 为空"))?,
            "tick_size",
            &symbol,
            None,
        )?;
        if tick_sizes.insert(symbol.clone(), tick_size).is_some() {
            bail!("exchange_symbols 返回重复冻结成员：{symbol}");
        }
    }

    let missing = symbols
        .iter()
        .filter(|symbol| !tick_sizes.contains_key(*symbol))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "冻结币池有 {} 个成员缺少 linear perpetual tick_size：{}",
            missing.len(),
            missing.join(",")
        );
    }
    Ok(tick_sizes)
}

/// 读取单币已确认 15m 行；成交量只映射 `vol_ccy`，禁止替换为 `vol` 或乘以 close。
async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<Candle>> {
    let table_name = symbol_candle_table_name(symbol)?;
    let query = format!(
        "SELECT ts, o, h, l, c, vol_ccy \
         FROM \"{table_name}\" \
         WHERE confirm = '1' AND ts >= $1 AND ts < $2 \
         ORDER BY ts"
    );
    let rows = sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(pool)
        .await
        .with_context(|| format!("读取 {symbol} 已确认 15m K 线失败"))?;

    let candles = rows
        .into_iter()
        .map(|row| parse_candle_row(&row, symbol))
        .collect::<Result<Vec<_>>>()?;
    validate_candle_sequence(symbol, start_ms, end_ms, &candles)?;
    Ok(candles)
}

/// 把一行 VARCHAR OHLCV 转成有限数值；任何坏行都必须暴露，不能静默丢弃后伪装为完整覆盖。
fn parse_candle_row(row: &PgRow, symbol: &str) -> Result<Candle> {
    let timestamp_ms: i64 = row
        .try_get("ts")
        .with_context(|| format!("解析 {symbol} K 线 ts"))?;
    let candle = Candle {
        timestamp_ms,
        open: parse_row_number(row, "o", symbol, timestamp_ms)?,
        high: parse_row_number(row, "h", symbol, timestamp_ms)?,
        low: parse_row_number(row, "l", symbol, timestamp_ms)?,
        close: parse_row_number(row, "c", symbol, timestamp_ms)?,
        volume: parse_row_number(row, "vol_ccy", symbol, timestamp_ms)?,
    };
    if !candle.is_valid() {
        bail!("{symbol} 在 {timestamp_ms} 的已确认 OHLC/vol_ccy 不满足 Candle 不变量");
    }
    Ok(candle)
}

/// 解析数据库字符串数值并拒绝 NULL、NaN 和无穷值，保持指标输入可重放。
fn parse_row_number(row: &PgRow, column: &str, symbol: &str, timestamp_ms: i64) -> Result<f64> {
    let raw: Option<String> = row
        .try_get(column)
        .with_context(|| format!("读取 {symbol} {timestamp_ms} 的 {column}"))?;
    parse_finite_number(
        raw.as_deref()
            .with_context(|| format!("{symbol} {timestamp_ms} 的 {column} 为空"))?,
        column,
        symbol,
        Some(timestamp_ms),
    )
}

/// 解析有限数值并保留字段、币种和可选时间上下文，便于定位坏数据。
fn parse_finite_number(
    raw: &str,
    field: &str,
    symbol: &str,
    timestamp_ms: Option<i64>,
) -> Result<f64> {
    let value = raw.parse::<f64>().with_context(|| match timestamp_ms {
        Some(timestamp_ms) => format!("解析 {symbol} {timestamp_ms} 的 {field}={raw}"),
        None => format!("解析 {symbol} 的 {field}={raw}"),
    })?;
    if !value.is_finite() {
        bail!("{symbol} 的 {field} 必须是有限数值");
    }
    Ok(value)
}

/// tick 必须严格为正；零或负数不能用于止损与目标价对齐。
fn parse_positive_number(
    raw: &str,
    field: &str,
    symbol: &str,
    timestamp_ms: Option<i64>,
) -> Result<f64> {
    let value = parse_finite_number(raw, field, symbol, timestamp_ms)?;
    if value <= 0.0 {
        bail!("{symbol} 的 {field} 必须大于 0");
    }
    Ok(value)
}

/// 校验返回序列严格升序且落在冻结 15m 网格内；允许缺口，但禁止重复和错位时间。
fn validate_candle_sequence(
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
    candles: &[Candle],
) -> Result<()> {
    for candle in candles {
        if candle.timestamp_ms < start_ms
            || candle.timestamp_ms >= end_ms
            || (candle.timestamp_ms - start_ms).rem_euclid(CANDLE_INTERVAL_MS) != 0
        {
            bail!(
                "{symbol} K 线时间 {} 不在冻结 15m 网格内",
                candle.timestamp_ms
            );
        }
    }
    for pair in candles.windows(2) {
        if pair[1].timestamp_ms <= pair[0].timestamp_ms {
            bail!(
                "{symbol} K 线时间不严格递增：{} -> {}",
                pair[0].timestamp_ms,
                pair[1].timestamp_ms
            );
        }
    }
    Ok(())
}

/// 由严格白名单 symbol 构造分表名；动态 SQL 前不提供通用转义回退。
fn symbol_candle_table_name(symbol: &str) -> Result<String> {
    validate_swap_symbol(symbol)?;
    let identifier = format!("{}_candles_15m", symbol.to_ascii_lowercase());
    if !identifier.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
    }) {
        bail!("派生 K 线表名不满足严格标识符白名单：{identifier}");
    }
    Ok(identifier)
}

/// 只接受 `<BASE>-USDT-SWAP`，BASE 只能含大写字母或数字，杜绝标识符注入和隐式规范化。
fn validate_swap_symbol(symbol: &str) -> Result<()> {
    let parts = symbol.split('-').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0].is_empty()
        || parts[1] != "USDT"
        || parts[2] != "SWAP"
        || !parts[0]
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        bail!("冻结成员不是严格的 OKX USDT SWAP 标识：{symbol}");
    }
    Ok(())
}

/// 计算窗口应有的开盘时间数量；当前上界必须等于最后一根开盘时间加 1ms。
fn expected_candle_count(start_ms: i64, end_ms: i64) -> Result<usize> {
    if end_ms <= start_ms
        || start_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
        || (end_ms - 1).rem_euclid(CANDLE_INTERVAL_MS) != 0
    {
        bail!("冻结窗口必须使用对齐 15m 的起点和“最后时间 + 1ms”上界");
    }
    let count = (end_ms - 1 - start_ms) / CANDLE_INTERVAL_MS + 1;
    usize::try_from(count).context("冻结窗口 K 线数量超出 usize")
}

/// 计算标准 `[start,end)` 15m 区间根数，专用于不含评价上界 `+1ms` 约定的预热段。
fn exact_exclusive_candle_count(start_ms: i64, end_ms: i64) -> Result<usize> {
    if end_ms <= start_ms
        || start_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
        || end_ms.rem_euclid(CANDLE_INTERVAL_MS) != 0
        || (end_ms - start_ms).rem_euclid(CANDLE_INTERVAL_MS) != 0
    {
        bail!("预热窗口必须是严格对齐的 15m 半开区间");
    }
    usize::try_from((end_ms - start_ms) / CANDLE_INTERVAL_MS).context("预热窗口 K 线数量超出 usize")
}

/// 把首部、内部和尾部缺口都展开为明确时间段，避免只看首尾时间误判覆盖完整。
fn build_symbol_coverage(
    start_ms: i64,
    end_ms: i64,
    candles: &[Candle],
) -> Result<SymbolCoverageDiagnostic> {
    let expected_candle_count = expected_candle_count(start_ms, end_ms)?;
    let last_expected_timestamp_ms = end_ms - 1;
    let mut next_expected_timestamp_ms = start_ms;
    let mut missing_ranges = Vec::new();

    for candle in candles {
        if candle.timestamp_ms > next_expected_timestamp_ms {
            missing_ranges.push(missing_range(
                next_expected_timestamp_ms,
                candle.timestamp_ms - CANDLE_INTERVAL_MS,
            )?);
        }
        next_expected_timestamp_ms = candle
            .timestamp_ms
            .checked_add(CANDLE_INTERVAL_MS)
            .context("推进覆盖诊断时间时溢出")?;
    }
    if next_expected_timestamp_ms <= last_expected_timestamp_ms {
        missing_ranges.push(missing_range(
            next_expected_timestamp_ms,
            last_expected_timestamp_ms,
        )?);
    }

    let missing_candle_count = missing_ranges.iter().map(|range| range.candle_count).sum();
    Ok(SymbolCoverageDiagnostic {
        expected_candle_count,
        loaded_candle_count: candles.len(),
        first_timestamp_ms: candles.first().map(|candle| candle.timestamp_ms),
        last_timestamp_ms: candles.last().map(|candle| candle.timestamp_ms),
        missing_candle_count,
        is_complete: candles.len() == expected_candle_count && missing_ranges.is_empty(),
        missing_ranges,
    })
}

/// 把两个已对齐的缺口端点转换为包含首尾的缺失 K 线段。
fn missing_range(first_timestamp_ms: i64, last_timestamp_ms: i64) -> Result<MissingCandleRange> {
    if last_timestamp_ms < first_timestamp_ms
        || (last_timestamp_ms - first_timestamp_ms).rem_euclid(CANDLE_INTERVAL_MS) != 0
    {
        bail!("缺失 K 线区间没有对齐 15m 网格");
    }
    let count = (last_timestamp_ms - first_timestamp_ms) / CANDLE_INTERVAL_MS + 1;
    Ok(MissingCandleRange {
        first_timestamp_ms,
        last_timestamp_ms,
        candle_count: usize::try_from(count).context("缺失 K 线数量超出 usize")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造只用于覆盖诊断的有效 Candle，避免测试依赖指标或数据库。
    fn candle(timestamp_ms: i64) -> Candle {
        Candle {
            timestamp_ms,
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 10.0,
        }
    }

    #[test]
    fn repository_manifest_matches_frozen_top60_identity() {
        let raw = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/research_manifests/market_filtered_volume_v9_top60_current_live_20260723.json"
        ));
        let spec = parse_and_validate_manifest(raw).expect("manifest must remain frozen");

        assert_eq!(spec.universe_version, FROZEN_UNIVERSE_VERSION);
        assert_eq!(spec.symbols.len(), FROZEN_MEMBER_COUNT);
        assert!(spec.symbols.iter().any(|symbol| symbol == "BTC-USDT-SWAP"));
        assert!(!spec.symbols.iter().any(|symbol| symbol == "ETH-USDT-SWAP"));
    }

    #[test]
    fn table_identifier_only_accepts_strict_usdt_swap_symbols() {
        assert_eq!(
            symbol_candle_table_name("1INCH-USDT-SWAP").unwrap(),
            "1inch-usdt-swap_candles_15m"
        );
        assert!(symbol_candle_table_name("btc-USDT-SWAP").is_err());
        assert!(symbol_candle_table_name("BTC-USDT").is_err());
        assert!(symbol_candle_table_name("BTC-USDT-SWAP\";DROP").is_err());
    }

    #[test]
    fn coverage_reports_leading_internal_and_trailing_gaps() {
        let start_ms = 0;
        let end_ms = 4 * CANDLE_INTERVAL_MS + 1;
        let candles = vec![candle(CANDLE_INTERVAL_MS), candle(3 * CANDLE_INTERVAL_MS)];

        let coverage = build_symbol_coverage(start_ms, end_ms, &candles).unwrap();

        assert_eq!(coverage.expected_candle_count, 5);
        assert_eq!(coverage.loaded_candle_count, 2);
        assert_eq!(coverage.missing_candle_count, 3);
        assert_eq!(coverage.missing_ranges.len(), 3);
        assert!(!coverage.is_complete);
    }

    #[test]
    fn complete_15m_window_has_no_missing_ranges() {
        let start_ms = 0;
        let end_ms = 2 * CANDLE_INTERVAL_MS + 1;
        let candles = vec![
            candle(0),
            candle(CANDLE_INTERVAL_MS),
            candle(2 * CANDLE_INTERVAL_MS),
        ];

        validate_candle_sequence("BTC-USDT-SWAP", start_ms, end_ms, &candles).unwrap();
        let coverage = build_symbol_coverage(start_ms, end_ms, &candles).unwrap();

        assert!(coverage.is_complete);
        assert_eq!(coverage.missing_candle_count, 0);
        assert!(coverage.missing_ranges.is_empty());
    }
}
