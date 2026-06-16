use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use crypto_exc_all::{
    BitgetExchangeConfig, BybitExchangeConfig, Candle, CandleQuery, CryptoSdk, ExchangeId,
    GateExchangeConfig, Instrument, SdkConfig,
};
use rust_quant_services::exchange::CryptoExcAllGateway;
use rust_quant_services::strategy::pre_major_listing_perp_catchup::{
    build_listing_catchup_paper_sample, evaluate_listing_catchup_paper,
    ListingCatchupAcceptanceCriteria, ListingCatchupPaperProbeSeed, ListingCatchupPaperReport,
    ListingCatchupPriceBar, ListingCatchupVenueProbe,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

const DEFAULT_DATABASE_URL: &str = "postgres://postgres:postgres123@localhost:5432/quant_news";
const DEFAULT_LIMIT: i64 = 80;
const DEFAULT_DETECTION_LATENCY_SECS: u64 = 30;
const LOOKBACK_MS: u64 = 15 * 60 * 1_000;
const HOLD_WINDOW_MS: u64 = 120 * 60 * 1_000;
const FEE_BPS_PER_SIDE: f64 = 5.0;
const SLIPPAGE_BPS_PER_SIDE: f64 = 8.0;
const PROXY_SPREAD_PCT: f64 = 0.20;
const MIN_PROXY_DEPTH_USDT: f64 = 50_000.0;
const DEFAULT_REPLAY_SOURCES: &str = "binance_announcements,okx_announcements";

#[derive(Debug, Clone, Copy)]
enum AnnouncementSource {
    Binance,
    Okx,
}

impl AnnouncementSource {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "binance" | "binance_announcements" => Ok(Self::Binance),
            "okx" | "okx_announcements" => Ok(Self::Okx),
            other => Err(anyhow!(
                "unsupported announcement source: {other}; supported: binance_announcements,okx_announcements"
            )),
        }
    }

    fn source_name(self) -> &'static str {
        match self {
            Self::Binance => "binance_announcements",
            Self::Okx => "okx_announcements",
        }
    }

    fn table_name(self) -> &'static str {
        match self {
            Self::Binance => "news_items_binance_announcements",
            Self::Okx => "news_items_okx_announcements",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ReplayAnnouncement {
    announcement_id: String,
    source: String,
    title: String,
    #[serde(default)]
    content: String,
    announced_at_ms: u64,
    #[serde(default)]
    detected_assets: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FixtureCandle {
    open_time: u64,
    high: f64,
    low: f64,
    close: f64,
    #[serde(default)]
    quote_volume: Option<f64>,
    #[serde(default)]
    volume: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct FixtureVenueCandles {
    exchange: String,
    base_asset: String,
    candles: Vec<FixtureCandle>,
}

#[derive(Debug, Clone, Deserialize)]
struct FixtureInput {
    announcements: Vec<ReplayAnnouncement>,
    #[serde(default)]
    venue_candles: Vec<FixtureVenueCandles>,
}

#[derive(Debug, Serialize)]
struct ReplaySkipped {
    announcement_id: String,
    title: String,
    reason: String,
    details: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReplayOutput {
    mode: &'static str,
    source: String,
    production_note: &'static str,
    automatic_live_trading_allowed: bool,
    limitations: Vec<&'static str>,
    announcements_read: usize,
    samples_built: usize,
    skipped: Vec<ReplaySkipped>,
    report: ListingCatchupPaperReport,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let criteria = ListingCatchupAcceptanceCriteria {
        min_trade_samples: args.min_trade_samples,
        min_win_rate_pct: args.min_win_rate_pct,
        require_positive_total_net_return: true,
    };

    let (source, announcements, fixture_candles) = if let Some(path) = args.fixture_path.as_deref()
    {
        let fixture = read_fixture(path)?;
        (
            format!("fixture:{path}"),
            fixture.announcements,
            Some(fixture.venue_candles),
        )
    } else {
        let database_url = args
            .database_url
            .as_deref()
            .unwrap_or(DEFAULT_DATABASE_URL)
            .to_string();
        (
            format!("quant_news_db:{}", source_names(&args.sources)),
            load_major_listing_announcements_from_db(&database_url, &args.sources, args.limit)
                .await?,
            None,
        )
    };

    let gateway = if fixture_candles.is_none() {
        Some(public_gateway()?)
    } else {
        None
    };
    let (samples, skipped) = build_samples(
        announcements.clone(),
        gateway.as_ref(),
        fixture_candles.as_deref(),
        args.detection_latency_secs,
    )
    .await?;
    let report = evaluate_listing_catchup_paper(samples, criteria);

    let output = ReplayOutput {
        mode: "historical_kline_proxy",
        source,
        production_note: "paper_replay_only_live_trading_disabled",
        automatic_live_trading_allowed: false,
        limitations: vec![
            "historical_orderbook_depth_unavailable",
            "depth_and_spread_are_kline_liquidity_proxies",
            "result_is_not_live_execution_authorization",
        ],
        announcements_read: announcements.len(),
        samples_built: report.total_samples,
        skipped,
        report,
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Debug)]
struct CliArgs {
    fixture_path: Option<String>,
    database_url: Option<String>,
    sources: Vec<AnnouncementSource>,
    limit: i64,
    detection_latency_secs: u64,
    min_trade_samples: usize,
    min_win_rate_pct: f64,
}

fn parse_args() -> Result<CliArgs> {
    let mut fixture_path = None;
    let mut database_url = std::env::var("DATABASE_URL").ok();
    let mut sources = parse_sources(
        &std::env::var("PRE_MAJOR_LISTING_REPLAY_SOURCES")
            .unwrap_or_else(|_| DEFAULT_REPLAY_SOURCES.to_string()),
    )?;
    let mut limit = std::env::var("PRE_MAJOR_LISTING_REPLAY_LIMIT")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_LIMIT);
    let mut detection_latency_secs = std::env::var("PRE_MAJOR_LISTING_DETECTION_LATENCY_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DETECTION_LATENCY_SECS);
    let mut min_trade_samples = std::env::var("PRE_MAJOR_LISTING_MIN_TRADE_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default();
    let mut min_win_rate_pct = std::env::var("PRE_MAJOR_LISTING_MIN_WIN_RATE_PCT")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => fixture_path = args.next(),
            "--database-url" => database_url = args.next(),
            "--sources" => {
                sources = parse_sources(
                    &args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --sources"))?,
                )?;
            }
            "--limit" => {
                limit = parse_next(&mut args, "--limit")?;
            }
            "--detection-latency-secs" => {
                detection_latency_secs = parse_next(&mut args, "--detection-latency-secs")?;
            }
            "--min-trade-samples" => {
                min_trade_samples = parse_next(&mut args, "--min-trade-samples")?;
            }
            "--min-win-rate-pct" => {
                min_win_rate_pct = parse_next(&mut args, "--min-win-rate-pct")?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    Ok(CliArgs {
        fixture_path,
        database_url,
        sources,
        limit: limit.max(1),
        detection_latency_secs,
        min_trade_samples,
        min_win_rate_pct,
    })
}

fn parse_sources(value: &str) -> Result<Vec<AnnouncementSource>> {
    let mut sources = Vec::new();
    for raw in value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let source = AnnouncementSource::parse(raw)?;
        let already_added = sources
            .iter()
            .any(|existing: &AnnouncementSource| existing.source_name() == source.source_name());
        if !already_added {
            sources.push(source);
        }
    }
    if sources.is_empty() {
        return Err(anyhow!("at least one announcement source is required"));
    }
    Ok(sources)
}

fn source_names(sources: &[AnnouncementSource]) -> String {
    sources
        .iter()
        .map(|source| source.source_name())
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_next<T>(args: &mut impl Iterator<Item = String>, name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    args.next()
        .ok_or_else(|| anyhow!("missing value for {name}"))?
        .parse::<T>()
        .map_err(|err| anyhow!("invalid {name}: {err}"))
}

fn read_fixture(path: &str) -> Result<FixtureInput> {
    let body = fs::read_to_string(path).with_context(|| format!("read replay fixture: {path}"))?;
    serde_json::from_str(&body).with_context(|| format!("parse replay fixture JSON: {path}"))
}

async fn load_major_listing_announcements_from_db(
    database_url: &str,
    sources: &[AnnouncementSource],
    limit: i64,
) -> Result<Vec<ReplayAnnouncement>> {
    let pool = PgPool::connect(database_url)
        .await
        .with_context(|| "connect quant_news database for major listing announcements")?;
    let per_source_limit = limit.max(1);
    let mut announcements = Vec::new();
    for source in sources {
        let mut source_announcements =
            load_source_announcements_from_db(&pool, *source, per_source_limit).await?;
        announcements.append(&mut source_announcements);
    }
    announcements.sort_by(|left, right| right.announced_at_ms.cmp(&left.announced_at_ms));
    announcements.truncate(limit.max(1) as usize);
    Ok(announcements)
}

async fn load_source_announcements_from_db(
    pool: &PgPool,
    source: AnnouncementSource,
    limit: i64,
) -> Result<Vec<ReplayAnnouncement>> {
    let query = format!(
        r#"
        SELECT news_id, title, content, published_at, detected_assets
        FROM {}
        WHERE is_deleted = false
          AND signal_category = 'listing'
          AND (
            lower(title || ' ' || content) LIKE '%will list%'
            OR lower(title || ' ' || content) LIKE '%to list%'
            OR lower(title || ' ' || content) LIKE '%lists%'
            OR lower(title || ' ' || content) LIKE '%new listing%'
            OR lower(title || ' ' || content) LIKE '%spot trading%'
            OR lower(title || ' ' || content) LIKE '%trading will open%'
          )
          AND lower(title || ' ' || content) NOT LIKE '%delist%'
          AND lower(title || ' ' || content) NOT LIKE '%suspend trading%'
          AND lower(title || ' ' || content) NOT LIKE '%binance alpha%'
          AND lower(title || ' ' || content) NOT LIKE '%binance wallet%'
          AND lower(title || ' ' || content) NOT LIKE '%binance earn%'
        ORDER BY published_at DESC
        LIMIT $1
        "#,
        source.table_name()
    );
    let rows = sqlx::query(&query)
        .bind(limit)
        .fetch_all(pool)
        .await
        .with_context(|| format!("query {} listing rows", source.table_name()))?;

    rows.into_iter()
        .map(|row| {
            let published_at: DateTime<Utc> = row.try_get("published_at")?;
            Ok(ReplayAnnouncement {
                announcement_id: row.try_get("news_id")?,
                source: source.source_name().to_string(),
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                announced_at_ms: published_at.timestamp_millis().max(0) as u64,
                detected_assets: row.try_get("detected_assets").unwrap_or_default(),
            })
        })
        .collect::<std::result::Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

fn public_gateway() -> Result<CryptoExcAllGateway> {
    let sdk = CryptoSdk::from_config(SdkConfig {
        bitget: Some(BitgetExchangeConfig {
            api_key: "public-only".to_string(),
            api_secret: "public-only".to_string(),
            passphrase: "public-only".to_string(),
            api_url: None,
            api_timeout_ms: Some(10_000),
            proxy_url: None,
            product_type: Some("USDT-FUTURES".to_string()),
        }),
        bybit: Some(BybitExchangeConfig {
            api_key: "public-only".to_string(),
            api_secret: "public-only".to_string(),
            api_url: None,
            api_timeout_ms: Some(10_000),
            recv_window_ms: Some(5_000),
            proxy_url: None,
            category: Some("linear".to_string()),
        }),
        gate: Some(GateExchangeConfig {
            api_key: "public-only".to_string(),
            api_secret: "public-only".to_string(),
            api_url: None,
            api_timeout_ms: Some(10_000),
            proxy_url: None,
            settle: Some("usdt".to_string()),
        }),
        ..SdkConfig::default()
    })?;
    Ok(CryptoExcAllGateway::from_sdk(sdk))
}

async fn build_samples(
    announcements: Vec<ReplayAnnouncement>,
    gateway: Option<&CryptoExcAllGateway>,
    fixture_candles: Option<&[FixtureVenueCandles]>,
    detection_latency_secs: u64,
) -> Result<(
    Vec<rust_quant_services::strategy::pre_major_listing_perp_catchup::ListingCatchupPaperSample>,
    Vec<ReplaySkipped>,
)> {
    let fixture_index = fixture_candles.map(index_fixture_candles);
    let mut samples = Vec::new();
    let mut skipped = Vec::new();

    for announcement in announcements {
        let Some(base_asset) = extract_base_asset(&announcement) else {
            skipped.push(skip(&announcement, "base_asset_not_detected"));
            continue;
        };
        if !is_positive_major_listing(&announcement) {
            skipped.push(skip(&announcement, "not_positive_major_listing"));
            continue;
        }

        let mut venue_inputs = Vec::new();
        let mut venue_failures = Vec::new();
        for exchange in [ExchangeId::Bitget, ExchangeId::Bybit, ExchangeId::Gate] {
            let candles = if let Some(index) = fixture_index.as_ref() {
                index
                    .get(&(exchange.as_str().to_string(), base_asset.clone()))
                    .cloned()
                    .unwrap_or_default()
            } else {
                match fetch_candles(
                    gateway.expect("gateway exists without fixture"),
                    exchange,
                    &base_asset,
                    announcement.announced_at_ms,
                )
                .await
                {
                    Ok(candles) => candles,
                    Err(error) => {
                        venue_failures.push(format!("{}:api_error:{error}", exchange.as_str()));
                        Vec::new()
                    }
                }
            };
            if candles.is_empty() {
                venue_failures.push(format!("{}:no_candles", exchange.as_str()));
                continue;
            }
            match build_venue_input(
                exchange,
                &base_asset,
                announcement.announced_at_ms,
                detection_latency_secs,
                candles,
            ) {
                Ok(venue) => venue_inputs.push(venue),
                Err(reason) => venue_failures.push(format!("{}:{reason}", exchange.as_str())),
            }
        }

        let Some(selected) = venue_inputs.into_iter().next() else {
            skipped.push(skip_with_details(
                &announcement,
                "secondary_venue_kline_unavailable",
                venue_failures,
            ));
            continue;
        };
        let seed = ListingCatchupPaperProbeSeed {
            announcement_id: announcement.announcement_id.clone(),
            announcement_exchange: "binance".to_string(),
            base_asset: base_asset.clone(),
            quote_asset: "USDT".to_string(),
            announced_at_ms: announcement.announced_at_ms,
            detected_at_ms: announcement.announced_at_ms + detection_latency_secs * 1_000,
            pre_announcement_price: selected.pre_announcement_price,
            announcement_price: selected.announcement_price,
            btc_5m_return_pct: 0.0,
            eth_5m_return_pct: 0.0,
            opening_upper_wick_rejection: false,
            entry_price: selected.entry_price,
            fee_bps_per_side: FEE_BPS_PER_SIDE,
            slippage_bps_per_side: SLIPPAGE_BPS_PER_SIDE,
            candidates: vec![selected.probe],
            price_path: selected.price_path,
        };
        match build_listing_catchup_paper_sample(seed) {
            Ok(sample) => samples.push(sample),
            Err(reason) => skipped.push(skip(&announcement, &reason)),
        }
    }

    Ok((samples, skipped))
}

#[derive(Debug)]
struct VenueInput {
    pre_announcement_price: f64,
    announcement_price: f64,
    entry_price: f64,
    probe: ListingCatchupVenueProbe,
    price_path: Vec<ListingCatchupPriceBar>,
}

fn build_venue_input(
    exchange: ExchangeId,
    base_asset: &str,
    announced_at_ms: u64,
    detection_latency_secs: u64,
    mut candles: Vec<FixtureCandle>,
) -> Result<VenueInput, String> {
    candles.sort_by_key(|candle| candle.open_time);
    let pre_time = announced_at_ms.saturating_sub(LOOKBACK_MS);
    let detected_at = announced_at_ms + detection_latency_secs * 1_000;
    let pre = last_at_or_before(&candles, pre_time)
        .or_else(|| last_before(&candles, announced_at_ms))
        .ok_or_else(|| "missing_pre_announcement_candle".to_string())?;
    let announcement = last_at_or_before(&candles, announced_at_ms)
        .or_else(|| first_at_or_after(&candles, announced_at_ms))
        .ok_or_else(|| "missing_announcement_candle".to_string())?;
    let entry = first_at_or_after(&candles, detected_at)
        .or_else(|| first_at_or_after(&candles, announced_at_ms))
        .ok_or_else(|| "missing_entry_candle".to_string())?;
    let quote_volume = candles
        .iter()
        .filter(|candle| candle.open_time >= announced_at_ms)
        .filter_map(candle_quote_volume)
        .fold(0.0_f64, f64::max);
    if quote_volume < MIN_PROXY_DEPTH_USDT {
        return Err(format!("proxy_depth_below_min:{quote_volume:.2}"));
    }

    let price_path = candles
        .iter()
        .filter(|candle| candle.open_time > entry.open_time)
        .filter(|candle| candle.open_time <= entry.open_time + HOLD_WINDOW_MS)
        .map(|candle| ListingCatchupPriceBar {
            minute_after_entry: ((candle.open_time - entry.open_time) / 60_000) as u32,
            high_price: candle.high,
            low_price: candle.low,
            close_price: candle.close,
        })
        .collect::<Vec<_>>();
    if price_path.is_empty() {
        return Err("empty_post_entry_price_path".to_string());
    }

    let half_spread = PROXY_SPREAD_PCT / 100.0 / 2.0;
    Ok(VenueInput {
        pre_announcement_price: pre.close,
        announcement_price: announcement.close,
        entry_price: entry.close,
        probe: ListingCatchupVenueProbe {
            exchange: exchange.as_str().to_string(),
            symbol: Instrument::perp(base_asset, "USDT").symbol_for(exchange),
            best_bid: entry.close * (1.0 - half_spread),
            best_ask: entry.close * (1.0 + half_spread),
            bid_depth_top5_usdt: quote_volume,
            ask_depth_top5_usdt: quote_volume,
            response_latency_ms: 0,
        },
        price_path,
    })
}

async fn fetch_candles(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    base_asset: &str,
    announced_at_ms: u64,
) -> Result<Vec<FixtureCandle>> {
    let start = announced_at_ms.saturating_sub(LOOKBACK_MS);
    let end = announced_at_ms + HOLD_WINDOW_MS;
    let query = CandleQuery::new(Instrument::perp(base_asset, "USDT"), "1m")
        .with_start_time(start)
        .with_end_time(end)
        .with_limit(200);
    let started = Instant::now();
    let candles = gateway.candles(exchange, query).await?;
    let _latency = started.elapsed().as_millis();
    Ok(candles
        .into_iter()
        .filter_map(FixtureCandle::from_sdk)
        .collect())
}

impl FixtureCandle {
    fn from_sdk(candle: Candle) -> Option<Self> {
        let open_time = candle.open_time?;
        let high = candle.high.parse::<f64>().ok()?;
        let low = candle.low.parse::<f64>().ok()?;
        let close = candle.close.parse::<f64>().ok()?;
        Some(Self {
            open_time,
            high,
            low,
            close,
            quote_volume: candle
                .quote_volume
                .as_deref()
                .and_then(|value| value.parse::<f64>().ok()),
            volume: candle.volume.parse::<f64>().ok(),
        })
    }
}

fn index_fixture_candles(
    candles: &[FixtureVenueCandles],
) -> HashMap<(String, String), Vec<FixtureCandle>> {
    candles
        .iter()
        .map(|item| {
            (
                (
                    item.exchange.trim().to_ascii_lowercase(),
                    item.base_asset.trim().to_ascii_uppercase(),
                ),
                item.candles.clone(),
            )
        })
        .collect()
}

fn last_at_or_before(candles: &[FixtureCandle], time: u64) -> Option<&FixtureCandle> {
    candles.iter().rev().find(|candle| candle.open_time <= time)
}

fn last_before(candles: &[FixtureCandle], time: u64) -> Option<&FixtureCandle> {
    candles.iter().rev().find(|candle| candle.open_time < time)
}

fn first_at_or_after(candles: &[FixtureCandle], time: u64) -> Option<&FixtureCandle> {
    candles.iter().find(|candle| candle.open_time >= time)
}

fn candle_quote_volume(candle: &FixtureCandle) -> Option<f64> {
    candle
        .quote_volume
        .or_else(|| candle.volume.map(|volume| volume * candle.close))
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn extract_base_asset(announcement: &ReplayAnnouncement) -> Option<String> {
    for token in parenthesized_tokens(&announcement.title)
        .into_iter()
        .chain(usdt_pair_tokens(&announcement.title))
        .chain(usdt_pair_tokens(&announcement.content))
        .chain(listing_symbol_tokens(&announcement.title))
        .chain(listing_symbol_tokens(&announcement.content))
        .chain(csv_assets(&announcement.detected_assets))
    {
        let normalized = token.trim().to_ascii_uppercase();
        if is_plausible_base_asset(&normalized) {
            return Some(normalized);
        }
    }
    None
}

fn csv_assets(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn listing_symbol_tokens(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut tokens = Vec::new();
    for marker in ["will list ", "to list ", "lists "] {
        let mut search_from = 0;
        while let Some(relative) = lower[search_from..].find(marker) {
            let start = search_from + relative + marker.len();
            let tail = &text[start..];
            for raw in tail.split(|ch: char| {
                !(ch.is_ascii_alphanumeric() || ch == '/' || ch == '-' || ch == '_')
            }) {
                let trimmed = raw
                    .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                    .trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(symbol) = uppercase_listing_symbol(trimmed) {
                    tokens.push(symbol);
                    break;
                }
                break;
            }
            search_from = start;
        }
    }
    tokens
}

fn uppercase_listing_symbol(token: &str) -> Option<String> {
    let normalized = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
        .to_ascii_uppercase();
    let uppercase_or_digit_count = token
        .chars()
        .filter(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        .count();
    if uppercase_or_digit_count >= 2 && is_plausible_base_asset(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn parenthesized_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('(') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find(')') else {
            break;
        };
        tokens.push(after_start[..end].to_string());
        rest = &after_start[end + 1..];
    }
    tokens
}

fn usdt_pair_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '/' || ch == '-' || ch == '_'))
        .filter_map(|raw| {
            let token = raw.trim().to_ascii_uppercase();
            for separator in ["/USDT", "-USDT", "_USDT", "USDT"] {
                if let Some(base) = token.strip_suffix(separator) {
                    if !base.is_empty() {
                        return Some(base.to_string());
                    }
                }
            }
            None
        })
        .collect()
}

fn is_plausible_base_asset(symbol: &str) -> bool {
    matches!(symbol.len(), 2..=20)
        && symbol.chars().all(|value| value.is_ascii_alphanumeric())
        && !matches!(
            symbol,
            "USD" | "USDT" | "USDC" | "BUSD" | "FDUSD" | "DAI" | "ETF" | "SEC" | "OKX" | "BINANCE"
        )
}

fn is_positive_major_listing(announcement: &ReplayAnnouncement) -> bool {
    let text = format!(
        "{} {} {}",
        announcement.source, announcement.title, announcement.content
    )
    .to_ascii_lowercase();
    if text.contains("expiry perps")
        || text.contains("delivery contracts")
        || text.contains("quarterly")
        || (text.contains("adds") && text.contains("trading pair"))
    {
        return false;
    }
    if announcement.source == "okx_announcements" && !text.contains("spot") {
        return false;
    }
    let has_listing_verb = [
        "will list",
        "to list",
        " lists ",
        "new listing",
        "trading will open",
        "上线",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    if !has_listing_verb {
        return false;
    }
    (text.contains("binance") || text.contains("okx"))
        && !text.contains("binance alpha")
        && !text.contains("binance wallet")
        && !text.contains("binance earn")
        && [
            "will list",
            "to list",
            "lists",
            "new listing",
            "spot trading",
            "trading will open",
            "上线",
            "新增",
        ]
        .iter()
        .any(|needle| text.contains(needle))
}

fn skip(announcement: &ReplayAnnouncement, reason: &str) -> ReplaySkipped {
    skip_with_details(announcement, reason, Vec::new())
}

fn skip_with_details(
    announcement: &ReplayAnnouncement,
    reason: &str,
    details: Vec<String>,
) -> ReplaySkipped {
    ReplaySkipped {
        announcement_id: announcement.announcement_id.clone(),
        title: announcement.title.clone(),
        reason: reason.to_string(),
        details,
    }
}

fn print_usage() {
    println!(
        "Usage: pre_major_listing_historical_replay [--database-url <url>] [--sources binance_announcements,okx_announcements] [--limit 80] [--fixture <fixture.json>] [--min-trade-samples 30] [--min-win-rate-pct 60]"
    );
}
