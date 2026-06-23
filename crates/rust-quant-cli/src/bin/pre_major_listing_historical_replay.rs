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
    /// 封装当前函数，减少量化核心调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "binance" | "binance_announcements" => Ok(Self::Binance),
            "okx" | "okx_announcements" => Ok(Self::Okx),
            other => Err(anyhow!(
                "unsupported announcement source: {other}; supported: binance_announcements,okx_announcements"
            )),
        }
    }
    /// 提供来源名称的集中实现，避免量化核心调用方重复处理相同细节。
    fn source_name(self) -> &'static str {
        match self {
            Self::Binance => "binance_announcements",
            Self::Okx => "okx_announcements",
        }
    }
    /// 提供table名称的集中实现，避免量化核心调用方重复处理相同细节。
    fn table_name(self) -> &'static str {
        match self {
            Self::Binance => "news_items_binance_announcements",
            Self::Okx => "news_items_okx_announcements",
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
struct ReplayAnnouncement {
    /// announcement ID。
    announcement_id: String,
    /// 数据来源。
    source: String,
    /// 标题。
    title: String,
    #[serde(default)]
    /// 正文内容。
    content: String,
    /// 毫秒级时间戳或时长。
    announced_at_ms: u64,
    #[serde(default)]
    /// detected资产，用于当前结构体的业务数据。
    detected_assets: String,
}
#[derive(Debug, Clone, Deserialize)]
struct FixtureCandle {
    /// 开仓时间。
    open_time: u64,
    /// 最高价。
    high: f64,
    /// 最低价。
    low: f64,
    /// 收盘价。
    close: f64,
    #[serde(default)]
    /// 数量数值。
    quote_volume: Option<f64>,
    #[serde(default)]
    /// 成交量。
    volume: Option<f64>,
}
#[derive(Debug, Clone, Deserialize)]
struct FixtureVenueCandles {
    /// 交易所名称。
    exchange: String,
    /// 基础资产，用于当前结构体的业务数据。
    base_asset: String,
    /// 列表数据。
    candles: Vec<FixtureCandle>,
}
#[derive(Debug, Clone, Deserialize)]
struct FixtureInput {
    /// 列表数据。
    announcements: Vec<ReplayAnnouncement>,
    #[serde(default)]
    /// 列表数据。
    venue_candles: Vec<FixtureVenueCandles>,
}
#[derive(Debug, Serialize)]
struct ReplaySkipped {
    /// announcement ID。
    announcement_id: String,
    /// 标题。
    title: String,
    /// 原因说明。
    reason: String,
    /// 列表数据。
    details: Vec<String>,
}
#[derive(Debug, Serialize)]
struct ReplayOutput {
    /// 模式。
    mode: &'static str,
    /// 数据来源。
    source: String,
    /// 备注信息。
    production_note: &'static str,
    /// 是否允许该操作。
    automatic_live_trading_allowed: bool,
    /// 列表数据。
    limitations: Vec<&'static str>,
    /// announcementsread。
    announcements_read: usize,
    /// samplesbuilt。
    samples_built: usize,
    /// 列表数据。
    skipped: Vec<ReplaySkipped>,
    /// 报告。
    report: ListingCatchupPaperReport,
}
#[tokio::main]
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
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
    /// fixture路径；为空时使用默认值或表示不限制。
    fixture_path: Option<String>,
    /// databaseURL；为空时使用默认值或表示不限制。
    database_url: Option<String>,
    /// 列表数据。
    sources: Vec<AnnouncementSource>,
    /// 查询数量上限。
    limit: i64,
    /// 秒级时长。
    detection_latency_secs: u64,
    /// 最小tradesamples，用于控制策略触发门槛。
    min_trade_samples: usize,
    /// 最小胜率百分比。
    min_win_rate_pct: f64,
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
fn parse_args() -> Result<CliArgs> {
    let mut fixture_path = None;
    let mut database_url = quant_news_database_url_from_env();
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
/// 提供quantnewsdatabaseURLfrom环境变量的集中实现，避免量化核心调用方重复处理相同细节。
fn quant_news_database_url_from_env() -> Option<String> {
    let envs: HashMap<String, String> = std::env::vars().collect();
    quant_news_database_url_from_map(&envs)
}
/// 提供quantnewsdatabaseURLfrommap的集中实现，避免量化核心调用方重复处理相同细节。
fn quant_news_database_url_from_map(envs: &HashMap<String, String>) -> Option<String> {
    non_empty_env(envs, "QUANT_NEWS_DATABASE_URL")
        .or_else(|| non_empty_env(envs, "POSTGRES_QUANT_NEWS_DATABASE_URL"))
        .or_else(|| {
            non_empty_env(envs, "DATABASE_URL")
                .filter(|url| database_url_targets(url, "quant_news"))
        })
        .map(str::to_string)
}
/// 读取非空环境变量值，避免空字符串覆盖有效默认配置。
fn non_empty_env<'a>(envs: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    envs.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}
/// 封装数据库URL指向，减少量化核心调用方重复实现相同细节。
fn database_url_targets(database_url: &str, database_name: &str) -> bool {
    database_url
        .split('?')
        .next()
        .unwrap_or(database_url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(|name| name.eq_ignore_ascii_case(database_name))
        .unwrap_or(false)
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
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
/// 提供来源names的集中实现，避免量化核心调用方重复处理相同细节。
fn source_names(sources: &[AnnouncementSource]) -> String {
    sources
        .iter()
        .map(|source| source.source_name())
        .collect::<Vec<_>>()
        .join(",")
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
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
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
fn read_fixture(path: &str) -> Result<FixtureInput> {
    let body = fs::read_to_string(path).with_context(|| format!("read replay fixture: {path}"))?;
    serde_json::from_str(&body).with_context(|| format!("parse replay fixture JSON: {path}"))
}
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
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
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
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
/// 提供publicgateway的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 构建 量化核心 请求或响应载荷，把字段组装规则集中在同一入口。
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
    /// 价格数值。
    pre_announcement_price: f64,
    /// 价格数值。
    announcement_price: f64,
    /// 入场价格。
    entry_price: f64,
    /// probe。
    probe: ListingCatchupVenueProbe,
    /// 列表数据。
    price_path: Vec<ListingCatchupPriceBar>,
}
/// 构建 量化核心 请求或响应载荷，把字段组装规则集中在同一入口。
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
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 从外部输入转换为内部模型，隔离 量化核心 的字段适配细节。
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
/// 封装索引fixturecandles，减少量化核心调用方重复实现相同细节。
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
/// 判断K 线quote成交量，给量化核心流程提供布尔结果。
fn candle_quote_volume(candle: &FixtureCandle) -> Option<f64> {
    candle
        .quote_volume
        .or_else(|| candle.volume.map(|volume| volume * candle.close))
        .filter(|value| value.is_finite() && *value > 0.0)
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
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
/// 提供CSVassets的集中实现，避免量化核心调用方重复处理相同细节。
fn csv_assets(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
/// 提供上市交易对tokens的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 提供uppercase上市交易对的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 提供parenthesizedtokens的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 提供USDTpairtokens的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 判断 量化核心 条件是否满足，给上层流程提供布尔决策。
fn is_plausible_base_asset(symbol: &str) -> bool {
    matches!(symbol.len(), 2..=20)
        && symbol.chars().all(|value| value.is_ascii_alphanumeric())
        && !matches!(
            symbol,
            "USD" | "USDT" | "USDC" | "BUSD" | "FDUSD" | "DAI" | "ETF" | "SEC" | "OKX" | "BINANCE"
        )
}
/// 判断 量化核心 条件是否满足，给上层流程提供布尔决策。
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
/// 提供skipwithdetails的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 执行输出usage步骤，串起量化核心需要的状态推进和错误处理。
fn print_usage() {
    println!(
        "Usage: pre_major_listing_historical_replay [--database-url <url>] [--sources binance_announcements,okx_announcements] [--limit 80] [--fixture <fixture.json>] [--min-trade-samples 30] [--min-win-rate-pct 60]"
    );
}
#[cfg(test)]
mod tests {
    use super::quant_news_database_url_from_map;
    use std::collections::HashMap;
    #[test]
    fn quant_news_database_url_ignores_quant_web_fallback() {
        let envs = HashMap::from([(
            "DATABASE_URL".to_string(),
            "postgres://postgres:secret@localhost:5432/quant_web".to_string(),
        )]);
        assert_eq!(quant_news_database_url_from_map(&envs), None);
    }
    #[test]
    fn quant_news_database_url_prefers_explicit_news_url() {
        let envs = HashMap::from([
            (
                "QUANT_NEWS_DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_news".to_string(),
            ),
            (
                "DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_web".to_string(),
            ),
        ]);
        assert_eq!(
            quant_news_database_url_from_map(&envs),
            Some("postgres://postgres:secret@localhost:5432/quant_news".to_string())
        );
    }
}
