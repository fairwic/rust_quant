use crate::app::okx_historical_universe::HistoricalUniverseManifest;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use rust_quant_strategies::CandleItem;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;
use zip::ZipArchive;

const MS_15M: i64 = 15 * 60 * 1_000;
const MS_30M: i64 = 30 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const OKX_ARCHIVE_UTC_OFFSET_MS: i64 = 8 * 60 * 60 * 1_000;
const FUNDING_MAX_AGE_MS: i64 = 8 * 60 * 60 * 1_000 + MS_15M;
const HISTORY_BARS: usize = 96;
const VOLUME_WINDOW: usize = 20;
const ATR_PERIOD: usize = 14;
const MIN_HISTORY_RETURN: f64 = -0.05;
const CLOSE_POSITION_MIN: f64 = 2.0 / 3.0;
const VOLUME_MEDIAN_MULTIPLIER: f64 = 1.5;
const FUNDING_COVERAGE_MIN_RATIO: f64 = 0.80;
const FUNDING_BOTTOM_RATIO: f64 = 0.20;
const STOP_ATR_BUFFER: f64 = 0.25;
const MIN_RISK_PCT: f64 = 0.5;
const MAX_RISK_PCT: f64 = 3.0;
const TARGET_R: f64 = 2.5;
const MAX_HOLDING_BARS: usize = 48 * 4;
const COST_RATE_PER_SIDE: f64 = 0.0008;
const DEFAULT_OKX_ARCHIVE_BASE: &str = "https://static.okx.com";

/// 冻结 V1 研究参数；只暴露数据位置和下载并发，不暴露策略阈值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingSqueezeResearchArgs {
    pub manifest: PathBuf,
    pub download_concurrency: usize,
    pub okx_archive_base: String,
}

/// 因果漏斗计数，用于区分 funding、价格形态和执行风控的损耗。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FundingSqueezeStageCounts {
    pub funding_eligible_bars: usize,
    pub history_drop_pass: usize,
    pub sweep_reclaim_pass: usize,
    pub volume_pass: usize,
    pub risk_blocked: usize,
    pub incomplete_outcomes: usize,
}

/// 单笔严格回放交易；初始结构风险固定为 1R。
#[derive(Debug, Clone, PartialEq)]
pub struct FundingSqueezeTrade {
    pub symbol: String,
    pub signal_ts: i64,
    pub funding_ts: i64,
    pub funding_rate: f64,
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub gross_r: f64,
    pub cost_r: f64,
    pub net_r: f64,
    pub exit_reason: &'static str,
}

/// 交易级固定 R 统计；统一资金通过前不代表可部署组合权益。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FundingSqueezeMetrics {
    pub trades: usize,
    pub net_sum_r: f64,
    pub net_expectancy_r: Option<f64>,
    pub profit_factor: Option<f64>,
    pub win_rate_pct: Option<f64>,
    pub trade_sharpe: Option<f64>,
    pub max_drawdown_r: f64,
    pub recovery_factor: Option<f64>,
}

/// 冻结 V1 的完整研究报告。
#[derive(Debug, Clone, PartialEq)]
pub struct FundingSqueezeResearchReport {
    pub universe_version: String,
    pub archive_days: usize,
    pub funding_rows: usize,
    pub funding_timestamps: usize,
    pub coverage_blocked_timestamps: usize,
    pub symbols: usize,
    pub stages: FundingSqueezeStageCounts,
    pub trades: Vec<FundingSqueezeTrade>,
    pub effective_events: usize,
    pub gross_zero_cost: FundingSqueezeMetrics,
    pub overall: FundingSqueezeMetrics,
    pub discovery: FundingSqueezeMetrics,
    pub validation: FundingSqueezeMetrics,
    pub double_cost: FundingSqueezeMetrics,
    pub monthly: Vec<(i64, FundingSqueezeMetrics)>,
    pub positive_months: usize,
    pub top_three_positive_symbols: Vec<String>,
    pub net_r_without_top_three_symbols: f64,
    pub exit_reasons: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseWindow {
    from_ms: i64,
    to_ms: i64,
    members: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UniverseSchedule {
    version: String,
    windows: Vec<UniverseWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RawFundingPoint {
    ts: i64,
    rate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FundingState {
    ts: i64,
    rate: f64,
    eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ReversalSignal {
    atr: f64,
}

/// 解析只读研究参数，未知参数失败，避免把命令变成结果驱动扫描器。
pub fn parse_funding_squeeze_research_args<I>(values: I) -> Result<FundingSqueezeResearchArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut values = values.into_iter();
    let mut manifest = None;
    let mut download_concurrency = 12usize;
    let mut okx_archive_base = DEFAULT_OKX_ARCHIVE_BASE.to_string();
    while let Some(arg) = values.next() {
        let value = |values: &mut I::IntoIter| {
            values
                .next()
                .ok_or_else(|| anyhow!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut values)?)),
            "--download-concurrency" => {
                download_concurrency = value(&mut values)?
                    .parse()
                    .context("parse --download-concurrency")?;
            }
            "--okx-archive-base" => {
                okx_archive_base = value(&mut values)?.trim_end_matches('/').to_string();
            }
            "--help" | "-h" => bail!(funding_squeeze_research_usage()),
            _ => bail!(
                "unknown argument: {arg}\n{}",
                funding_squeeze_research_usage()
            ),
        }
    }
    if !(1..=24).contains(&download_concurrency) {
        bail!("--download-concurrency must be between 1 and 24");
    }
    Ok(FundingSqueezeResearchArgs {
        manifest: manifest.context("--manifest is required")?,
        download_concurrency,
        okx_archive_base,
    })
}

/// 返回冻结 V1 的命令用法。
pub fn funding_squeeze_research_usage() -> &'static str {
    "Usage: market_funding_squeeze_reversal_research --manifest PATH [--download-concurrency 12]"
}

impl UniverseSchedule {
    /// 只接受连续 current-live-only OKX 15m 月度币池，避免研究时偷换成员。
    fn from_manifest(manifest: HistoricalUniverseManifest) -> Result<Self> {
        if manifest.schema_version != 1
            || manifest.exchange != "okx"
            || manifest.market_type != "perpetual_swap"
            || manifest.quote_currency != "USDT"
            || manifest.timeframe != "15m"
            || !manifest
                .selection_rule
                .starts_with("current-live OKX USDT swaps only")
        {
            bail!("funding squeeze research requires current-live-only OKX USDT swap 15m manifest");
        }
        let mut windows = manifest
            .months
            .into_iter()
            .map(|month| UniverseWindow {
                from_ms: month.effective_from_ms,
                to_ms: month.effective_to_ms,
                members: month
                    .members
                    .into_iter()
                    .map(|member| member.symbol.to_ascii_uppercase())
                    .collect(),
            })
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| window.from_ms);
        if windows.len() < 2
            || windows.iter().any(|window| {
                window.from_ms >= window.to_ms
                    || window.members.is_empty()
                    || window.members.iter().any(|symbol| !valid_symbol(symbol))
            })
            || windows
                .windows(2)
                .any(|pair| pair[0].to_ms != pair[1].from_ms)
        {
            bail!("funding squeeze research requires contiguous non-empty monthly windows");
        }
        Ok(Self {
            version: manifest.universe_version,
            windows,
        })
    }

    fn union_symbols(&self) -> Vec<String> {
        self.windows
            .iter()
            .flat_map(|window| window.members.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn window_at(&self, ts: i64) -> Option<&UniverseWindow> {
        self.windows
            .iter()
            .find(|window| ts >= window.from_ms && ts < window.to_ms)
    }
}

/// 下载覆盖研究期的官方逐日全永续 funding 归档，并只保留 manifest 当前幸存币。
async fn load_official_funding(
    client: &Client,
    schedule: &UniverseSchedule,
    symbols: &BTreeSet<String>,
    archive_base: &str,
    concurrency: usize,
) -> Result<(usize, BTreeMap<String, Vec<RawFundingPoint>>)> {
    let first_ms = schedule
        .windows
        .first()
        .context("missing first funding universe window")?
        .from_ms
        .saturating_sub(DAY_MS);
    let last_ms = schedule
        .windows
        .last()
        .context("missing last funding universe window")?
        .to_ms
        .saturating_sub(1);
    let first_day = timestamp_day(first_ms)?;
    let last_day = timestamp_day(last_ms)?;
    let mut days = Vec::new();
    let mut day = first_day;
    while day <= last_day {
        days.push(day);
        day = day
            .checked_add_signed(ChronoDuration::days(1))
            .context("funding archive day overflow")?;
    }
    let mut unique = BTreeMap::<(String, i64), f64>::new();
    for chunk in days.chunks(concurrency) {
        let mut tasks = JoinSet::new();
        for day in chunk.iter().copied() {
            let client = client.clone();
            let archive_base = archive_base.to_string();
            let symbols = symbols.clone();
            tasks.spawn(async move {
                let url = funding_archive_url(&archive_base, day);
                let bytes = download_with_retry(&client, &url).await?;
                let rows = parse_funding_archive(&bytes, day, &symbols)?;
                Ok::<_, anyhow::Error>(rows)
            });
        }
        while let Some(joined) = tasks.join_next().await {
            for (symbol, point) in joined.context("join OKX funding archive task")?? {
                let key = (symbol, point.ts);
                if let Some(existing) = unique.insert(key.clone(), point.rate) {
                    if existing.to_bits() != point.rate.to_bits() {
                        bail!("conflicting funding rows for {} at {}", key.0, key.1);
                    }
                }
            }
        }
    }
    let mut by_symbol = BTreeMap::<String, Vec<RawFundingPoint>>::new();
    for ((symbol, ts), rate) in unique {
        by_symbol
            .entry(symbol)
            .or_default()
            .push(RawFundingPoint { ts, rate });
    }
    for points in by_symbol.values_mut() {
        points.sort_by_key(|point| point.ts);
    }
    Ok((days.len(), by_symbol))
}

fn timestamp_day(timestamp_ms: i64) -> Result<NaiveDate> {
    Ok(Utc
        .timestamp_millis_opt(timestamp_ms.saturating_add(OKX_ARCHIVE_UTC_OFFSET_MS))
        .single()
        .context("funding timestamp outside supported range")?
        .date_naive())
}

fn funding_archive_url(base: &str, day: NaiveDate) -> String {
    format!(
        "{base}/cdn/okex/traderecords/swaprates/daily/{}/allswap-fundingrates-{}.zip?v=999",
        day.format("%Y%m%d"),
        day.format("%Y-%m-%d")
    )
}

async fn download_with_retry(client: &Client, url: &str) -> Result<Vec<u8>> {
    let mut last_error = None;
    for attempt in 0..4u64 {
        let response = async {
            client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await
        }
        .await;
        match response {
            Ok(bytes) => return Ok(bytes.to_vec()),
            Err(error) => {
                last_error = Some(error);
                if attempt < 3 {
                    sleep(Duration::from_millis(250 * (attempt + 1))).await;
                }
            }
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("download OKX funding archive failed")))
    .with_context(|| format!("download official funding archive {url}"))
}

/// 校验单日 ZIP、表头、日期和数值；不允许坏行被静默跳过。
fn parse_funding_archive(
    bytes: &[u8],
    expected_day: NaiveDate,
    symbols: &BTreeSet<String>,
) -> Result<Vec<(String, RawFundingPoint)>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("open OKX funding ZIP")?;
    if archive.len() != 1 {
        bail!("OKX funding archive must contain exactly one CSV");
    }
    let file = archive.by_index(0).context("open OKX funding CSV")?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("missing OKX funding CSV header")??;
    if header.trim_end_matches('\r') != "instrument_name,funding_rate,funding_time" {
        bail!("unexpected OKX funding CSV header: {header}");
    }
    let day_start = expected_day
        .and_hms_opt(0, 0, 0)
        .context("build funding day start")?
        .and_utc()
        .timestamp_millis()
        .saturating_sub(OKX_ARCHIVE_UTC_OFFSET_MS);
    let day_end = day_start + DAY_MS;
    let mut rows = Vec::new();
    for (line_index, line) in lines.enumerate() {
        let line = line.context("read OKX funding CSV row")?;
        let fields = line.trim_end_matches('\r').split(',').collect::<Vec<_>>();
        if fields.len() != 3 {
            bail!("invalid OKX funding CSV row {}", line_index + 2);
        }
        let symbol = fields[0].to_ascii_uppercase();
        if !symbols.contains(&symbol) {
            continue;
        }
        let rate = fields[1]
            .parse::<f64>()
            .with_context(|| format!("parse funding rate at row {}", line_index + 2))?;
        let ts = fields[2]
            .parse::<i64>()
            .with_context(|| format!("parse funding timestamp at row {}", line_index + 2))?;
        if !rate.is_finite() || rate.abs() > 0.1 || ts < day_start || ts >= day_end {
            bail!("invalid official funding row for {symbol} at {ts}");
        }
        rows.push((symbol, RawFundingPoint { ts, rate }));
    }
    Ok(rows)
}

/// 在每个结算时点只使用当月成员横截面，生成确定性的负 funding 最低五分位标签。
fn build_funding_states(
    schedule: &UniverseSchedule,
    raw: &BTreeMap<String, Vec<RawFundingPoint>>,
) -> (usize, usize, BTreeMap<String, Vec<FundingState>>) {
    let mut by_time = BTreeMap::<i64, BTreeMap<String, f64>>::new();
    let mut funding_rows = 0usize;
    for (symbol, points) in raw {
        for point in points {
            funding_rows += 1;
            by_time
                .entry(point.ts)
                .or_default()
                .insert(symbol.clone(), point.rate);
        }
    }
    let mut coverage_blocked = 0usize;
    let mut states = BTreeMap::<String, Vec<FundingState>>::new();
    for (ts, values) in by_time {
        let Some(window) = schedule.window_at(ts) else {
            continue;
        };
        let mut cross_section = window
            .members
            .iter()
            .filter_map(|symbol| values.get(symbol).map(|rate| (symbol.clone(), *rate)))
            .collect::<Vec<_>>();
        let minimum_coverage =
            (window.members.len() as f64 * FUNDING_COVERAGE_MIN_RATIO).ceil() as usize;
        if cross_section.len() < minimum_coverage {
            coverage_blocked += 1;
            continue;
        }
        cross_section.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let bottom_count = (cross_section.len() as f64 * FUNDING_BOTTOM_RATIO).ceil() as usize;
        for (rank, (symbol, rate)) in cross_section.into_iter().enumerate() {
            states.entry(symbol).or_default().push(FundingState {
                ts,
                rate,
                eligible: rate < 0.0 && rank < bottom_count,
            });
        }
    }
    for points in states.values_mut() {
        points.sort_by_key(|point| point.ts);
    }
    (funding_rows, coverage_blocked, states)
}

/// funding 至少提前一根完整 15m，且不能陈旧超过一个常规 8h 结算周期。
fn latest_funding_state(points: &[FundingState], event_ts: i64) -> Option<FundingState> {
    let visible_cutoff = event_ts.checked_sub(MS_15M)?;
    let index = points
        .partition_point(|point| point.ts <= visible_cutoff)
        .checked_sub(1)?;
    let point = points[index];
    (event_ts - point.ts <= FUNDING_MAX_AGE_MS).then_some(point)
}

/// 单币逐根扫描；funding、币池和 K 线都必须在信号时点已经可见。
fn scan_symbol(
    symbol: &str,
    candles: &[CandleItem],
    funding: &[FundingState],
    schedule: &UniverseSchedule,
    stages: &mut FundingSqueezeStageCounts,
) -> Vec<FundingSqueezeTrade> {
    let mut trades = Vec::new();
    let mut locked_until = None::<usize>;
    for index in HISTORY_BARS..candles.len().saturating_sub(1) {
        if locked_until.is_some_and(|exit_index| index <= exit_index) {
            continue;
        }
        let event_ts = candles[index].ts.saturating_add(MS_15M);
        if candles[index + 1].ts != event_ts
            || !schedule
                .window_at(event_ts)
                .is_some_and(|window| window.members.contains(symbol))
        {
            continue;
        }
        let Some(funding_state) = latest_funding_state(funding, event_ts) else {
            continue;
        };
        if !funding_state.eligible {
            continue;
        }
        stages.funding_eligible_bars += 1;
        let history_return = candles[index - 1].c / candles[index - HISTORY_BARS].c - 1.0;
        if !history_return.is_finite() || history_return > MIN_HISTORY_RETURN {
            continue;
        }
        stages.history_drop_pass += 1;
        if !sweep_reclaim_shape(candles, index) {
            continue;
        }
        stages.sweep_reclaim_pass += 1;
        let Some(signal) = reversal_signal(candles, index) else {
            continue;
        };
        stages.volume_pass += 1;
        match settle_trade(symbol, candles, index, funding_state, signal) {
            Some((trade, exit_index)) => {
                trades.push(trade);
                locked_until = Some(exit_index);
            }
            None if index + 1 + MAX_HOLDING_BARS > candles.len() => {
                stages.incomplete_outcomes += 1;
            }
            None => stages.risk_blocked += 1,
        }
    }
    trades
}

/// 识别扫破 24h 低点后的顶部收复，允许收阴长下影，但要求真实放量。
fn reversal_signal(candles: &[CandleItem], index: usize) -> Option<ReversalSignal> {
    if !sweep_reclaim_shape(candles, index) || index < VOLUME_WINDOW {
        return None;
    }
    let candle = &candles[index];
    let mut previous_volumes = candles[index - VOLUME_WINDOW..index]
        .iter()
        .map(|item| item.v)
        .collect::<Vec<_>>();
    if previous_volumes
        .iter()
        .any(|volume| !volume.is_finite() || *volume < 0.0)
        || !candle.v.is_finite()
    {
        return None;
    }
    previous_volumes.sort_by(f64::total_cmp);
    let median =
        (previous_volumes[VOLUME_WINDOW / 2 - 1] + previous_volumes[VOLUME_WINDOW / 2]) / 2.0;
    if median <= 0.0 || candle.v < median * VOLUME_MEDIAN_MULTIPLIER {
        return None;
    }
    Some(ReversalSignal {
        atr: atr_at(candles, index)?,
    })
}

fn sweep_reclaim_shape(candles: &[CandleItem], index: usize) -> bool {
    if index < HISTORY_BARS || index >= candles.len() {
        return false;
    }
    let candle = &candles[index];
    let prior_low = candles[index - HISTORY_BARS..index]
        .iter()
        .map(|item| item.l)
        .reduce(f64::min)
        .unwrap_or(f64::NAN);
    let range = candle.h - candle.l;
    let body = (candle.c - candle.o).abs();
    let lower_wick = candle.o.min(candle.c) - candle.l;
    let close_position = (candle.c - candle.l) / range;
    range.is_finite()
        && range > 0.0
        && candle.l < prior_low
        && candle.c > prior_low
        && close_position >= CLOSE_POSITION_MIN
        && lower_wick >= body
}

/// 下一根开盘成交后保守结算；同根止盈止损冲突始终按止损。
fn settle_trade(
    symbol: &str,
    candles: &[CandleItem],
    signal_index: usize,
    funding: FundingState,
    signal: ReversalSignal,
) -> Option<(FundingSqueezeTrade, usize)> {
    let entry_index = signal_index + 1;
    if entry_index + MAX_HOLDING_BARS > candles.len() {
        return None;
    }
    let entry = candles[entry_index].o;
    let stop = candles[signal_index].l - signal.atr * STOP_ATR_BUFFER;
    let initial_risk = entry - stop;
    let risk_pct = initial_risk / entry * 100.0;
    if !entry.is_finite()
        || !stop.is_finite()
        || initial_risk <= 0.0
        || !(MIN_RISK_PCT..=MAX_RISK_PCT).contains(&risk_pct)
    {
        return None;
    }
    let target = entry + initial_risk * TARGET_R;
    let last_index = entry_index + MAX_HOLDING_BARS - 1;
    let mut exit_index = last_index;
    let mut exit = candles[last_index].c;
    let mut gross_r = (exit - entry) / initial_risk;
    let mut exit_reason = "max_holding_timeout";
    for (offset, candle) in candles[entry_index..=last_index].iter().enumerate() {
        let absolute_index = entry_index + offset;
        if candle.l <= stop {
            exit_index = absolute_index;
            exit = stop;
            gross_r = -1.0;
            exit_reason = "sweep_stop";
            break;
        }
        if candle.h >= target {
            exit_index = absolute_index;
            exit = target;
            gross_r = TARGET_R;
            exit_reason = "target_2_5r";
            break;
        }
    }
    let cost_r = (entry + exit) * COST_RATE_PER_SIDE / initial_risk;
    Some((
        FundingSqueezeTrade {
            symbol: symbol.to_string(),
            signal_ts: candles[signal_index].ts.saturating_add(MS_15M),
            funding_ts: funding.ts,
            funding_rate: funding.rate,
            entry_ts: candles[entry_index].ts,
            exit_ts: candles[exit_index].ts.saturating_add(MS_15M),
            entry,
            stop,
            target,
            gross_r,
            cost_r,
            net_r: gross_r - cost_r,
            exit_reason,
        },
        exit_index,
    ))
}

/// 下载官方 funding、读取本地 quant_core 15m，并输出冻结 V1 结果。
pub async fn run_funding_squeeze_research(
    args: &FundingSqueezeResearchArgs,
    database_url: &str,
) -> Result<FundingSqueezeResearchReport> {
    let manifest: HistoricalUniverseManifest = serde_json::from_slice(
        &std::fs::read(&args.manifest)
            .with_context(|| format!("read universe manifest {}", args.manifest.display()))?,
    )
    .context("decode funding squeeze universe manifest")?;
    let schedule = UniverseSchedule::from_manifest(manifest)?;
    let symbols = schedule.union_symbols();
    let symbol_set = symbols.iter().cloned().collect::<BTreeSet<_>>();
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("build OKX funding archive client")?;
    let (archive_days, raw_funding) = load_official_funding(
        &client,
        &schedule,
        &symbol_set,
        &args.okx_archive_base,
        args.download_concurrency,
    )
    .await?;
    let funding_timestamps = raw_funding
        .values()
        .flat_map(|points| points.iter().map(|point| point.ts))
        .collect::<BTreeSet<_>>()
        .len();
    let (funding_rows, coverage_blocked_timestamps, funding_states) =
        build_funding_states(&schedule, &raw_funding);
    let first_window = schedule
        .windows
        .first()
        .context("missing first universe month")?;
    let last_window = schedule
        .windows
        .last()
        .context("missing last universe month")?;
    let load_start_ms = first_window.from_ms.saturating_sub(32 * DAY_MS);
    let load_end_ms = last_window.to_ms.saturating_add(2 * DAY_MS);
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(database_url)
        .await
        .context("connect quant_core for funding squeeze research")?;
    let mut stages = FundingSqueezeStageCounts::default();
    let mut trades = Vec::new();
    for symbol in &symbols {
        let candles = load_symbol_candles(&pool, symbol, load_start_ms, load_end_ms).await?;
        trades.extend(scan_symbol(
            symbol,
            &candles,
            funding_states.get(symbol).map(Vec::as_slice).unwrap_or(&[]),
            &schedule,
            &mut stages,
        ));
    }
    trades.sort_by(|left, right| {
        left.entry_ts
            .cmp(&right.entry_ts)
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    let split_ms = schedule
        .windows
        .get(schedule.windows.len() / 2)
        .map(|window| window.from_ms)
        .context("funding research requires discovery and validation months")?;
    let discovery_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts < split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let validation_trades = trades
        .iter()
        .filter(|trade| trade.entry_ts >= split_ms)
        .cloned()
        .collect::<Vec<_>>();
    let monthly = schedule
        .windows
        .iter()
        .map(|window| {
            let values = trades
                .iter()
                .filter(|trade| trade.entry_ts >= window.from_ms && trade.entry_ts < window.to_ms)
                .cloned()
                .collect::<Vec<_>>();
            (window.from_ms, metrics(&values, 1.0))
        })
        .collect::<Vec<_>>();
    let positive_months = monthly
        .iter()
        .filter(|(_, value)| value.net_sum_r > 0.0)
        .count();
    let (top_three_positive_symbols, net_r_without_top_three_symbols) =
        concentration_without_top_three(&trades);
    let mut exit_reasons = BTreeMap::<String, usize>::new();
    for trade in &trades {
        *exit_reasons
            .entry(trade.exit_reason.to_string())
            .or_default() += 1;
    }
    let report = FundingSqueezeResearchReport {
        universe_version: schedule.version.clone(),
        archive_days,
        funding_rows,
        funding_timestamps,
        coverage_blocked_timestamps,
        symbols: symbols.len(),
        stages,
        effective_events: effective_event_count(&trades),
        gross_zero_cost: metrics(&trades, 0.0),
        overall: metrics(&trades, 1.0),
        discovery: metrics(&discovery_trades, 1.0),
        validation: metrics(&validation_trades, 1.0),
        double_cost: metrics(&trades, 2.0),
        monthly,
        positive_months,
        top_three_positive_symbols,
        net_r_without_top_three_symbols,
        exit_reasons,
        trades,
    };
    print_report(&report);
    Ok(report)
}

async fn load_symbol_candles(
    pool: &PgPool,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<CandleItem>> {
    if !valid_symbol(symbol) {
        bail!("invalid manifest symbol {symbol}");
    }
    let table = format!("{}_candles_15m", symbol.to_ascii_lowercase());
    let query = format!(
        "SELECT ts, o, h, l, c, vol FROM \"{table}\" WHERE confirm = '1' AND ts >= $1 AND ts < $2 ORDER BY ts"
    );
    sqlx::query(&query)
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(pool)
        .await
        .with_context(|| format!("load funding squeeze candles from {table}"))?
        .into_iter()
        .map(|row| {
            Ok(CandleItem {
                ts: row.get("ts"),
                o: parse_number(row.get::<String, _>("o"))?,
                h: parse_number(row.get::<String, _>("h"))?,
                l: parse_number(row.get::<String, _>("l"))?,
                c: parse_number(row.get::<String, _>("c"))?,
                v: parse_number(row.get::<String, _>("vol"))?,
                confirm: 1,
            })
        })
        .collect()
}

fn atr_at(candles: &[CandleItem], index: usize) -> Option<f64> {
    if index + 1 < ATR_PERIOD {
        return None;
    }
    let start = index + 1 - ATR_PERIOD;
    let mut total = 0.0;
    for candle_index in start..=index {
        let candle = &candles[candle_index];
        let previous_close = candle_index
            .checked_sub(1)
            .map(|previous| candles[previous].c)
            .unwrap_or(candle.c);
        total += (candle.h - candle.l)
            .max((candle.h - previous_close).abs())
            .max((candle.l - previous_close).abs());
    }
    let atr = total / ATR_PERIOD as f64;
    (atr.is_finite() && atr > 0.0).then_some(atr)
}

fn metrics(trades: &[FundingSqueezeTrade], cost_multiplier: f64) -> FundingSqueezeMetrics {
    if trades.is_empty() {
        return FundingSqueezeMetrics::default();
    }
    let values = trades
        .iter()
        .map(|trade| trade.gross_r - trade.cost_r * cost_multiplier)
        .collect::<Vec<_>>();
    let net_sum_r = values.iter().sum::<f64>();
    let gross_profit = values.iter().filter(|value| **value > 0.0).sum::<f64>();
    let gross_loss = values
        .iter()
        .filter(|value| **value < 0.0)
        .map(|value| value.abs())
        .sum::<f64>();
    let mean = net_sum_r / values.len() as f64;
    let variance = if values.len() > 1 {
        values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64
    } else {
        0.0
    };
    let mut equity = 0.0_f64;
    let mut peak = 0.0_f64;
    let mut max_drawdown = 0.0_f64;
    for value in &values {
        equity += value;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(peak - equity);
    }
    FundingSqueezeMetrics {
        trades: values.len(),
        net_sum_r,
        net_expectancy_r: Some(mean),
        profit_factor: (gross_loss > 0.0).then_some(gross_profit / gross_loss),
        win_rate_pct: Some(
            values.iter().filter(|value| **value > 0.0).count() as f64 / values.len() as f64
                * 100.0,
        ),
        trade_sharpe: (variance > 0.0)
            .then_some(mean / variance.sqrt() * (values.len() as f64).sqrt()),
        max_drawdown_r: max_drawdown,
        recovery_factor: (max_drawdown > 0.0).then_some(net_sum_r / max_drawdown),
    }
}

fn effective_event_count(trades: &[FundingSqueezeTrade]) -> usize {
    let mut count = 0usize;
    let mut latest_cluster_ts = None::<i64>;
    for trade in trades {
        if latest_cluster_ts.is_none_or(|latest| trade.entry_ts - latest > MS_30M) {
            count += 1;
        }
        latest_cluster_ts = Some(trade.entry_ts);
    }
    count
}

fn concentration_without_top_three(trades: &[FundingSqueezeTrade]) -> (Vec<String>, f64) {
    let mut by_symbol = BTreeMap::<String, f64>::new();
    for trade in trades {
        *by_symbol.entry(trade.symbol.clone()).or_default() += trade.net_r;
    }
    let mut positive = by_symbol
        .into_iter()
        .filter(|(_, value)| *value > 0.0)
        .collect::<Vec<_>>();
    positive.sort_by(|left, right| right.1.total_cmp(&left.1));
    let top = positive
        .iter()
        .take(3)
        .map(|(symbol, _)| symbol.clone())
        .collect::<Vec<_>>();
    let removed = positive
        .iter()
        .take(3)
        .map(|(_, value)| *value)
        .sum::<f64>();
    (
        top,
        trades.iter().map(|trade| trade.net_r).sum::<f64>() - removed,
    )
}

fn print_report(report: &FundingSqueezeResearchReport) {
    println!(
        "funding_squeeze_research\tuniverse={}\tarchive_days={}\tfunding_rows={}\tfunding_timestamps={}\tcoverage_blocked_timestamps={}\tsymbols={}\tfunding_eligible_bars={}\thistory_drop_pass={}\tsweep_reclaim_pass={}\tvolume_pass={}\trisk_blocked={}\tincomplete={}\ttrades={}\teffective_events={}\tpositive_months={}",
        report.universe_version,
        report.archive_days,
        report.funding_rows,
        report.funding_timestamps,
        report.coverage_blocked_timestamps,
        report.symbols,
        report.stages.funding_eligible_bars,
        report.stages.history_drop_pass,
        report.stages.sweep_reclaim_pass,
        report.stages.volume_pass,
        report.stages.risk_blocked,
        report.stages.incomplete_outcomes,
        report.trades.len(),
        report.effective_events,
        report.positive_months,
    );
    for (label, value) in [
        ("gross_zero_cost", &report.gross_zero_cost),
        ("overall", &report.overall),
        ("discovery", &report.discovery),
        ("validation", &report.validation),
        ("double_cost", &report.double_cost),
    ] {
        print_metrics(label, value);
    }
    for (from_ms, value) in &report.monthly {
        print_metrics(&format!("month_{from_ms}"), value);
    }
    println!(
        "funding_squeeze_concentration\ttop_three={}\tnet_r_without_top_three={}\texit_reasons={}",
        report.top_three_positive_symbols.join(","),
        report.net_r_without_top_three_symbols,
        report
            .exit_reasons
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    );
}

fn print_metrics(label: &str, value: &FundingSqueezeMetrics) {
    println!(
        "funding_squeeze_metrics\twindow={}\ttrades={}\tnet_sum_r={}\tnet_ev_r={}\tpf={}\twin_rate_pct={}\ttrade_sharpe={}\tmax_drawdown_r={}\trecovery={}",
        label,
        value.trades,
        value.net_sum_r,
        optional(value.net_expectancy_r),
        optional(value.profit_factor),
        optional(value.win_rate_pct),
        optional(value.trade_sharpe),
        value.max_drawdown_r,
        optional(value.recovery_factor),
    );
}

fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_string(), |number| number.to_string())
}

fn parse_number(value: String) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse candle number {value}"))?;
    if !parsed.is_finite() {
        bail!("non-finite candle number {value}");
    }
    Ok(parsed)
}

fn valid_symbol(symbol: &str) -> bool {
    symbol.ends_with("-USDT-SWAP")
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
#[path = "market_funding_squeeze_reversal_research/tests.rs"]
mod tests;
