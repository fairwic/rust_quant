use super::{DeleveragingResearchArgs, UniverseSchedule};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;

const FOUR_HOURS_MS: i64 = 4 * 60 * 60 * 1_000;
const EIGHT_HOURS_MS: i64 = 8 * 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const TWO_DAYS_MS: i64 = 2 * DAY_MS;
const COVERAGE_MIN_RATIO: f64 = 0.80;
const OI_BOTTOM_RATIO: f64 = 0.20;
const TAKER_TOP_RATIO: f64 = 0.30;
const RATIO_BOTTOM_RATIO: f64 = 0.30;
const PAGE_LIMIT: usize = 100;
const MAX_PAGES: usize = 16;
const REQUEST_PAUSE_MS: u64 = 450;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextAudit {
    pub symbols: usize,
    pub oi_rows: usize,
    pub taker_rows: usize,
    pub ratio_rows: usize,
    pub oi_coverage_blocked: usize,
    pub taker_coverage_blocked: usize,
    pub ratio_coverage_blocked: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct OiState {
    pub ts: i64,
    pub change: f64,
    pub eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TakerState {
    pub ts: i64,
    pub sell_share: f64,
    pub eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RatioState {
    pub ts: i64,
    pub ratio: f64,
    pub change: f64,
    pub eligible: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SymbolStates {
    oi: Vec<OiState>,
    taker: Vec<TakerState>,
    ratio: Vec<RatioState>,
}

/// 已按当月成员完成横截面排名的因果状态；读取时仍执行完整周期延迟和陈旧检查。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct ContextStates {
    by_symbol: BTreeMap<String, SymbolStates>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ContextCache {
    schema_version: u32,
    universe_version: String,
    generated_at_ms: i64,
    first_window_ms: i64,
    last_window_ms: i64,
    source_base: String,
    taker_period: String,
    ratio_period: String,
    oi_period: String,
    symbols: BTreeMap<String, RawSymbolContext>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct RawSymbolContext {
    oi: Vec<OiPoint>,
    taker: Vec<TakerPoint>,
    ratio: Vec<RatioPoint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct OiPoint {
    ts: i64,
    value: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct TakerPoint {
    ts: i64,
    sell: f64,
    buy: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct RatioPoint {
    ts: i64,
    ratio: f64,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope {
    code: String,
    msg: String,
    data: Vec<Vec<String>>,
}

impl ContextStates {
    pub(super) fn oi_at(&self, symbol: &str, event_ts: i64) -> Option<OiState> {
        let points = &self.by_symbol.get(symbol)?.oi;
        latest_delayed(points, event_ts, DAY_MS, TWO_DAYS_MS, |point| point.ts)
    }

    pub(super) fn taker_at(&self, symbol: &str, event_ts: i64) -> Option<TakerState> {
        let points = &self.by_symbol.get(symbol)?.taker;
        latest_delayed(points, event_ts, FOUR_HOURS_MS, EIGHT_HOURS_MS, |point| {
            point.ts
        })
    }

    pub(super) fn ratio_at(&self, symbol: &str, event_ts: i64) -> Option<RatioState> {
        let points = &self.by_symbol.get(symbol)?.ratio;
        latest_delayed(points, event_ts, FOUR_HOURS_MS, EIGHT_HOURS_MS, |point| {
            point.ts
        })
    }
}

/// 从不可变缓存读取，或按 manifest 完整拉取 OKX Rubik 上下文后原子发布缓存。
pub(super) async fn load_context_states(
    args: &DeleveragingResearchArgs,
    schedule: &UniverseSchedule,
) -> Result<(ContextStates, ContextAudit)> {
    let first_window_ms = schedule
        .windows
        .first()
        .context("missing first deleveraging window")?
        .from_ms;
    let last_window_ms = schedule
        .windows
        .last()
        .context("missing last deleveraging window")?
        .to_ms;
    let cache = if args.context_cache.exists() {
        let cache: ContextCache = serde_json::from_slice(
            &std::fs::read(&args.context_cache)
                .with_context(|| format!("read context cache {}", args.context_cache.display()))?,
        )
        .context("decode deleveraging context cache")?;
        validate_cache(
            &cache,
            schedule,
            first_window_ms,
            last_window_ms,
            &args.okx_base,
        )?;
        cache
    } else {
        let cache = fetch_context_cache(args, schedule, first_window_ms, last_window_ms).await?;
        write_cache_atomic(&args.context_cache, &cache)?;
        cache
    };
    Ok(build_states(schedule, &cache))
}

fn validate_cache(
    cache: &ContextCache,
    schedule: &UniverseSchedule,
    first_window_ms: i64,
    last_window_ms: i64,
    source_base: &str,
) -> Result<()> {
    let expected = schedule
        .union_symbols()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual = cache.symbols.keys().cloned().collect::<BTreeSet<_>>();
    if cache.schema_version != 1
        || cache.universe_version != schedule.version
        || cache.first_window_ms != first_window_ms
        || cache.last_window_ms != last_window_ms
        || cache.source_base != source_base
        || cache.taker_period != "4H"
        || cache.ratio_period != "4H"
        || cache.oi_period != "1D"
        || actual != expected
    {
        bail!("deleveraging context cache does not match the frozen manifest and periods");
    }
    Ok(())
}

async fn fetch_context_cache(
    args: &DeleveragingResearchArgs,
    schedule: &UniverseSchedule,
    first_window_ms: i64,
    last_window_ms: i64,
) -> Result<ContextCache> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build OKX Rubik client")?;
    let symbols = schedule.union_symbols();
    let start_4h = first_window_ms.saturating_sub(EIGHT_HOURS_MS);
    let start_1d = first_window_ms.saturating_sub(TWO_DAYS_MS);
    let mut raw = BTreeMap::<String, RawSymbolContext>::new();
    for chunk in symbols.chunks(args.download_concurrency) {
        let mut tasks = JoinSet::new();
        for symbol in chunk.iter().cloned() {
            let client = client.clone();
            let base = args.okx_base.clone();
            tasks.spawn(async move {
                let taker = fetch_taker(&client, &base, &symbol, start_4h, last_window_ms).await?;
                let ratio = fetch_ratio(&client, &base, &symbol, start_4h, last_window_ms).await?;
                Ok::<_, anyhow::Error>((symbol, taker, ratio))
            });
        }
        while let Some(joined) = tasks.join_next().await {
            let (symbol, taker, ratio) = joined.context("join OKX contract context task")??;
            raw.insert(
                symbol,
                RawSymbolContext {
                    taker,
                    ratio,
                    ..Default::default()
                },
            );
        }
    }
    for symbol in &symbols {
        let base_ccy = symbol
            .strip_suffix("-USDT-SWAP")
            .with_context(|| format!("invalid OKX USDT swap symbol {symbol}"))?;
        let oi = fetch_oi(&client, &args.okx_base, base_ccy, start_1d, last_window_ms).await?;
        raw.get_mut(symbol)
            .with_context(|| format!("missing raw contract context for {symbol}"))?
            .oi = oi;
        sleep(Duration::from_millis(REQUEST_PAUSE_MS)).await;
    }
    Ok(ContextCache {
        schema_version: 1,
        universe_version: schedule.version.clone(),
        generated_at_ms: Utc::now().timestamp_millis(),
        first_window_ms,
        last_window_ms,
        source_base: args.okx_base.clone(),
        taker_period: "4H".to_owned(),
        ratio_period: "4H".to_owned(),
        oi_period: "1D".to_owned(),
        symbols: raw,
    })
}

async fn fetch_taker(
    client: &Client,
    base: &str,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<TakerPoint>> {
    let rows = fetch_paged_contract_rows(
        client,
        base,
        "/api/v5/rubik/stat/taker-volume-contract",
        symbol,
        start_ms,
        end_ms,
        &[("period", "4H"), ("unit", "2")],
        3,
    )
    .await?;
    rows.into_iter()
        .map(|row| {
            let ts = parse_i64(&row[0], "taker ts")?;
            let sell = parse_nonnegative(&row[1], "taker sell")?;
            let buy = parse_nonnegative(&row[2], "taker buy")?;
            Ok(TakerPoint { ts, sell, buy })
        })
        .collect()
}

async fn fetch_ratio(
    client: &Client,
    base: &str,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<RatioPoint>> {
    let rows = fetch_paged_contract_rows(
        client,
        base,
        "/api/v5/rubik/stat/contracts/long-short-account-ratio-contract-top-trader",
        symbol,
        start_ms,
        end_ms,
        &[("period", "4H")],
        2,
    )
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(RatioPoint {
                ts: parse_i64(&row[0], "long-short ts")?,
                ratio: parse_positive(&row[1], "long-short ratio")?,
            })
        })
        .collect()
}

async fn fetch_oi(
    client: &Client,
    base: &str,
    currency: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<OiPoint>> {
    let url = format!("{base}/api/v5/rubik/stat/contracts/open-interest-volume");
    let envelope = get_envelope_with_retry(
        client,
        &url,
        &[
            ("ccy", currency.to_owned()),
            ("period", "1D".to_owned()),
            ("begin", start_ms.to_string()),
            ("end", end_ms.to_string()),
        ],
    )
    .await
    .with_context(|| format!("fetch market-wide OI for {currency}"))?;
    parse_envelope_rows(envelope, 3, "open-interest-volume")?
        .into_iter()
        .filter_map(|row| {
            let ts = match parse_i64(&row[0], "OI ts") {
                Ok(ts) => ts,
                Err(error) => return Some(Err(error)),
            };
            if ts < start_ms || ts > end_ms {
                return Some(Ok(None));
            }
            Some(parse_nonnegative(&row[1], "OI value").map(|value| Some(OiPoint { ts, value })))
        })
        .collect::<Result<Vec<_>>>()
        .map(|points| points.into_iter().flatten().collect())
}

async fn fetch_paged_contract_rows(
    client: &Client,
    base: &str,
    path: &str,
    symbol: &str,
    start_ms: i64,
    end_ms: i64,
    extra: &[(&str, &str)],
    columns: usize,
) -> Result<Vec<Vec<String>>> {
    let url = format!("{base}{path}");
    let mut cursor_end = end_ms;
    let mut unique = BTreeMap::<i64, Vec<String>>::new();
    for page in 0..MAX_PAGES {
        let mut query = vec![
            ("instId", symbol.to_owned()),
            ("begin", start_ms.to_string()),
            ("end", cursor_end.to_string()),
            ("limit", PAGE_LIMIT.to_string()),
        ];
        query.extend(extra.iter().map(|(key, value)| (*key, (*value).to_owned())));
        let envelope = get_envelope_with_retry(client, &url, &query)
            .await
            .with_context(|| format!("fetch {path} for {symbol} page {}", page + 1))?;
        let rows = parse_envelope_rows(envelope, columns, path)?;
        if rows.is_empty() {
            break;
        }
        let mut oldest = i64::MAX;
        for row in rows {
            let ts = parse_i64(&row[0], "contract context ts")?;
            oldest = oldest.min(ts);
            if ts < start_ms || ts > end_ms {
                continue;
            }
            if let Some(existing) = unique.insert(ts, row.clone()) {
                if existing != row {
                    bail!("conflicting {path} rows for {symbol} at {ts}");
                }
            }
        }
        if oldest <= start_ms {
            break;
        }
        // Rubik 会把毫秒游标向 4H 桶取整；只减 1ms 可能仍返回同一最老桶。
        // 直接退一个完整桶，当前最老桶已保存，下一页从相邻桶继续，不产生时间缺口。
        let next_end = oldest.saturating_sub(FOUR_HOURS_MS);
        if next_end >= cursor_end {
            bail!("non-progressing {path} pagination for {symbol}");
        }
        cursor_end = next_end;
        sleep(Duration::from_millis(REQUEST_PAUSE_MS)).await;
    }
    if unique.is_empty() {
        bail!("{path} returned no frozen-window rows for {symbol}");
    }
    Ok(unique.into_values().collect())
}

async fn get_envelope_with_retry(
    client: &Client,
    url: &str,
    query: &[(&str, String)],
) -> Result<ApiEnvelope> {
    let mut last_error = None;
    for attempt in 0..4u64 {
        let response = client.get(url).query(query).send().await;
        match response {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.json::<ApiEnvelope>().await {
                    Ok(envelope) => return Ok(envelope),
                    Err(error) => last_error = Some(anyhow::Error::from(error)),
                },
                Err(error) => last_error = Some(anyhow::Error::from(error)),
            },
            Err(error) => last_error = Some(anyhow::Error::from(error)),
        }
        sleep(Duration::from_millis(250 * (attempt + 1))).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow!("OKX request retry loop produced no error")))
}

fn parse_envelope_rows(
    envelope: ApiEnvelope,
    columns: usize,
    metric: &str,
) -> Result<Vec<Vec<String>>> {
    if envelope.code != "0" {
        bail!(
            "OKX {metric} failed: code={} msg={}",
            envelope.code,
            envelope.msg
        );
    }
    if envelope.data.iter().any(|row| row.len() != columns) {
        bail!("OKX {metric} returned an unexpected column count");
    }
    Ok(envelope.data)
}

fn build_states(
    schedule: &UniverseSchedule,
    cache: &ContextCache,
) -> (ContextStates, ContextAudit) {
    let mut audit = ContextAudit {
        symbols: cache.symbols.len(),
        oi_rows: cache.symbols.values().map(|value| value.oi.len()).sum(),
        taker_rows: cache.symbols.values().map(|value| value.taker.len()).sum(),
        ratio_rows: cache.symbols.values().map(|value| value.ratio.len()).sum(),
        ..Default::default()
    };
    let mut states = ContextStates::default();
    let mut oi_by_ts = BTreeMap::<i64, Vec<(String, f64)>>::new();
    let mut taker_by_ts = BTreeMap::<i64, Vec<(String, f64)>>::new();
    let mut ratio_by_ts = BTreeMap::<i64, Vec<(String, f64, f64, f64)>>::new();
    for (symbol, raw) in &cache.symbols {
        for pair in raw.oi.windows(2) {
            if pair[0].value > 0.0 && pair[1].ts - pair[0].ts <= TWO_DAYS_MS {
                let change = pair[1].value / pair[0].value - 1.0;
                if change.is_finite() {
                    oi_by_ts
                        .entry(pair[1].ts)
                        .or_default()
                        .push((symbol.clone(), change));
                }
            }
        }
        for point in &raw.taker {
            let total = point.sell + point.buy;
            if total > 0.0 {
                taker_by_ts
                    .entry(point.ts)
                    .or_default()
                    .push((symbol.clone(), point.sell / total));
            }
        }
        for pair in raw.ratio.windows(2) {
            if pair[0].ratio > 0.0 && pair[1].ts - pair[0].ts <= EIGHT_HOURS_MS {
                let change = pair[1].ratio / pair[0].ratio - 1.0;
                if change.is_finite() {
                    ratio_by_ts.entry(pair[1].ts).or_default().push((
                        symbol.clone(),
                        pair[0].ratio,
                        pair[1].ratio,
                        change,
                    ));
                }
            }
        }
    }
    build_oi_states(schedule, oi_by_ts, &mut states, &mut audit);
    build_taker_states(schedule, taker_by_ts, &mut states, &mut audit);
    build_ratio_states(schedule, ratio_by_ts, &mut states, &mut audit);
    for symbol in states.by_symbol.values_mut() {
        symbol.oi.sort_by_key(|point| point.ts);
        symbol.taker.sort_by_key(|point| point.ts);
        symbol.ratio.sort_by_key(|point| point.ts);
    }
    (states, audit)
}

fn build_oi_states(
    schedule: &UniverseSchedule,
    grouped: BTreeMap<i64, Vec<(String, f64)>>,
    states: &mut ContextStates,
    audit: &mut ContextAudit,
) {
    for (ts, values) in grouped {
        let Some(window) = schedule.window_at(ts.saturating_add(DAY_MS)) else {
            continue;
        };
        let mut values = active_values(window, values);
        if !coverage_pass(window.members.len(), values.len()) {
            audit.oi_coverage_blocked += 1;
            continue;
        }
        values.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let eligible_count = rank_count(values.len(), OI_BOTTOM_RATIO);
        for (rank, (symbol, change)) in values.into_iter().enumerate() {
            states
                .by_symbol
                .entry(symbol)
                .or_default()
                .oi
                .push(OiState {
                    ts,
                    change,
                    eligible: change < 0.0 && rank < eligible_count,
                });
        }
    }
}

fn build_taker_states(
    schedule: &UniverseSchedule,
    grouped: BTreeMap<i64, Vec<(String, f64)>>,
    states: &mut ContextStates,
    audit: &mut ContextAudit,
) {
    for (ts, values) in grouped {
        let Some(window) = schedule.window_at(ts.saturating_add(FOUR_HOURS_MS)) else {
            continue;
        };
        let mut values = active_values(window, values);
        if !coverage_pass(window.members.len(), values.len()) {
            audit.taker_coverage_blocked += 1;
            continue;
        }
        values.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        });
        let eligible_count = rank_count(values.len(), TAKER_TOP_RATIO);
        for (rank, (symbol, sell_share)) in values.into_iter().enumerate() {
            states
                .by_symbol
                .entry(symbol)
                .or_default()
                .taker
                .push(TakerState {
                    ts,
                    sell_share,
                    eligible: sell_share > 0.5 && rank < eligible_count,
                });
        }
    }
}

fn build_ratio_states(
    schedule: &UniverseSchedule,
    grouped: BTreeMap<i64, Vec<(String, f64, f64, f64)>>,
    states: &mut ContextStates,
    audit: &mut ContextAudit,
) {
    for (ts, mut values) in grouped {
        let Some(window) = schedule.window_at(ts.saturating_add(FOUR_HOURS_MS)) else {
            continue;
        };
        values.retain(|(symbol, _, _, _)| window.members.contains(symbol));
        if !coverage_pass(window.members.len(), values.len()) {
            audit.ratio_coverage_blocked += 1;
            continue;
        }
        values.sort_by(|left, right| {
            left.3
                .total_cmp(&right.3)
                .then_with(|| left.0.cmp(&right.0))
        });
        let eligible_count = rank_count(values.len(), RATIO_BOTTOM_RATIO);
        for (rank, (symbol, prior_ratio, ratio, change)) in values.into_iter().enumerate() {
            states
                .by_symbol
                .entry(symbol)
                .or_default()
                .ratio
                .push(RatioState {
                    ts,
                    ratio,
                    change,
                    eligible: prior_ratio > 1.0 && change < 0.0 && rank < eligible_count,
                });
        }
    }
}

fn active_values(
    window: &super::UniverseWindow,
    mut values: Vec<(String, f64)>,
) -> Vec<(String, f64)> {
    values.retain(|(symbol, _)| window.members.contains(symbol));
    values
}

fn coverage_pass(expected: usize, actual: usize) -> bool {
    actual >= (expected as f64 * COVERAGE_MIN_RATIO).ceil() as usize
}

fn rank_count(total: usize, ratio: f64) -> usize {
    (total as f64 * ratio).ceil() as usize
}

fn latest_delayed<T: Copy>(
    points: &[T],
    event_ts: i64,
    visibility_delay_ms: i64,
    max_raw_age_ms: i64,
    timestamp: impl Fn(T) -> i64,
) -> Option<T> {
    let cutoff = event_ts.checked_sub(visibility_delay_ms)?;
    let index = points
        .partition_point(|point| timestamp(*point) <= cutoff)
        .checked_sub(1)?;
    let point = points[index];
    (event_ts - timestamp(point) <= max_raw_age_ms).then_some(point)
}

fn write_cache_atomic(path: &Path, cache: &ContextCache) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create context cache directory {}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(cache).context("encode deleveraging context cache")?,
    )
    .with_context(|| format!("write context cache {}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .with_context(|| format!("publish context cache {}", path.display()))?;
    Ok(())
}

fn parse_i64(value: &str, label: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("parse {label}"))
}

fn parse_nonnegative(value: &str, label: &str) -> Result<f64> {
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse {label}"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        bail!("{label} must be finite and nonnegative");
    }
    Ok(parsed)
}

fn parse_positive(value: &str, label: &str) -> Result<f64> {
    let parsed = parse_nonnegative(value, label)?;
    if parsed <= 0.0 {
        bail!("{label} must be positive");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_schedule(symbols: &[String]) -> UniverseSchedule {
        UniverseSchedule {
            version: "fixture".to_owned(),
            windows: vec![super::super::UniverseWindow {
                from_ms: 0,
                to_ms: 10 * DAY_MS,
                members: symbols.iter().cloned().collect(),
            }],
        }
    }

    #[test]
    fn context_states_wait_for_complete_period_and_expire() {
        let state = OiState {
            ts: DAY_MS,
            change: -0.2,
            eligible: true,
        };
        let mut context = ContextStates::default();
        context
            .by_symbol
            .entry("BTC-USDT-SWAP".to_owned())
            .or_default()
            .oi
            .push(state);

        assert_eq!(context.oi_at("BTC-USDT-SWAP", 2 * DAY_MS - 1), None);
        assert_eq!(context.oi_at("BTC-USDT-SWAP", 2 * DAY_MS), Some(state));
        assert_eq!(context.oi_at("BTC-USDT-SWAP", 3 * DAY_MS + 1), None);
    }

    #[test]
    fn cross_sectional_states_use_frozen_rank_bands() {
        let symbols = ["A", "B", "C", "D", "E"]
            .into_iter()
            .map(|base| format!("{base}-USDT-SWAP"))
            .collect::<Vec<_>>();
        let schedule = fixture_schedule(&symbols);
        let mut raw = BTreeMap::new();
        for (index, symbol) in symbols.iter().enumerate() {
            let oi_change = -0.5 + index as f64 * 0.1;
            let sell_share = 0.9 - index as f64 * 0.15;
            let ratio_change = -0.5 + index as f64 * 0.1;
            raw.insert(
                symbol.clone(),
                RawSymbolContext {
                    oi: vec![
                        OiPoint {
                            ts: DAY_MS,
                            value: 100.0,
                        },
                        OiPoint {
                            ts: 2 * DAY_MS,
                            value: 100.0 * (1.0 + oi_change),
                        },
                    ],
                    taker: vec![TakerPoint {
                        ts: FOUR_HOURS_MS,
                        sell: sell_share,
                        buy: 1.0 - sell_share,
                    }],
                    ratio: vec![
                        RatioPoint { ts: 0, ratio: 3.0 },
                        RatioPoint {
                            ts: FOUR_HOURS_MS,
                            ratio: 3.0 * (1.0 + ratio_change),
                        },
                    ],
                },
            );
        }
        let cache = ContextCache {
            schema_version: 1,
            universe_version: "fixture".to_owned(),
            generated_at_ms: 0,
            first_window_ms: 0,
            last_window_ms: 10 * DAY_MS,
            source_base: "fixture".to_owned(),
            taker_period: "4H".to_owned(),
            ratio_period: "4H".to_owned(),
            oi_period: "1D".to_owned(),
            symbols: raw,
        };

        let (states, audit) = build_states(&schedule, &cache);

        assert_eq!(audit.oi_coverage_blocked, 0);
        assert!(states.by_symbol["A-USDT-SWAP"].oi[0].eligible);
        assert!(!states.by_symbol["B-USDT-SWAP"].oi[0].eligible);
        assert!(states.by_symbol["A-USDT-SWAP"].taker[0].eligible);
        assert!(states.by_symbol["B-USDT-SWAP"].taker[0].eligible);
        assert!(!states.by_symbol["C-USDT-SWAP"].taker[0].eligible);
        assert!(states.by_symbol["A-USDT-SWAP"].ratio[0].eligible);
        assert!(states.by_symbol["B-USDT-SWAP"].ratio[0].eligible);
        assert!(!states.by_symbol["C-USDT-SWAP"].ratio[0].eligible);
    }
}
