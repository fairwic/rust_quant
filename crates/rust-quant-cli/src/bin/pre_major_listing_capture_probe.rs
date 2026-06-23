use anyhow::{anyhow, Context, Result};
use crypto_exc_all::{ExchangeId, Instrument, OrderBook, OrderBookQuery};
use rust_quant_services::strategy::pre_major_listing_perp_catchup::{
    ListingCatchupPaperProbeSeed, ListingCatchupPriceBar, ListingCatchupVenueProbe,
};
use rust_quant_services::CryptoExcAllGateway;
use serde::{Deserialize, Serialize};
use std::fs;
use std::str::FromStr;
use std::time::Instant;
#[derive(Debug, Clone, Deserialize)]
struct CaptureRequest {
    /// announcement ID。
    announcement_id: String,
    /// announcement交易所，用于构建接口请求。
    announcement_exchange: String,
    /// 基础资产，用于构建接口请求。
    base_asset: String,
    /// 计价资产，用于构建接口请求。
    quote_asset: String,
    /// 毫秒级时间戳或时长。
    announced_at_ms: u64,
    /// 毫秒级时间戳或时长。
    detected_at_ms: u64,
    /// 价格数值。
    pre_announcement_price: f64,
    /// 价格数值。
    announcement_price: f64,
    /// BTC 5 分钟收益率百分比。
    btc_5m_return_pct: f64,
    /// ETH 5 分钟收益率百分比。
    eth_5m_return_pct: f64,
    /// openingupperwickrejection，用于构建接口请求。
    opening_upper_wick_rejection: bool,
    /// 入场价格。
    entry_price: f64,
    /// 手续费bpsper方向，用于构建接口请求。
    fee_bps_per_side: f64,
    /// slippagebpsper方向，用于构建接口请求。
    slippage_bps_per_side: f64,
    /// 列表数据。
    price_path: Vec<ListingCatchupPriceBar>,
}
#[derive(Debug, Deserialize)]
struct OrderBookFixtureEnvelope {
    /// 列表数据。
    orderbooks: Vec<OrderBookFixture>,
}
#[derive(Debug, Deserialize)]
struct OrderBookFixture {
    /// 交易所名称。
    exchange: String,
    /// 交易对或资产符号。
    symbol: String,
    /// 列表数据。
    bids: Vec<[String; 2]>,
    /// 列表数据。
    asks: Vec<[String; 2]>,
    /// 毫秒级时间戳或时长。
    response_latency_ms: u64,
}
#[derive(Debug, Serialize)]
struct CaptureOutput {
    /// 列表数据。
    seeds: Vec<ListingCatchupPaperProbeSeed>,
    /// 列表数据。
    warnings: Vec<String>,
    /// 备注信息。
    production_note: &'static str,
}
#[derive(Debug)]
struct CaptureArgs {
    /// input路径，用于当前结构体的业务数据。
    input_path: String,
    /// orderbookfixture路径；为空时使用默认值或表示不限制。
    orderbook_fixture_path: Option<String>,
    /// 列表数据。
    exchanges: Vec<ExchangeId>,
}
#[tokio::main]
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
async fn main() -> Result<()> {
    let args = parse_args()?;
    let request = read_request(&args.input_path)?;
    let (candidates, warnings) = match args.orderbook_fixture_path {
        Some(path) => (read_fixture_candidates(&path)?, Vec::new()),
        None => capture_live_public_orderbooks(&request, &args.exchanges).await?,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&CaptureOutput {
            seeds: vec![build_seed(request, candidates)],
            warnings,
            production_note: "probe_capture_only_live_trading_disabled",
        })?
    );
    Ok(())
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
fn parse_args() -> Result<CaptureArgs> {
    let mut input_path = std::env::var("PRE_MAJOR_LISTING_CAPTURE_INPUT").ok();
    let mut fixture_path = std::env::var("PRE_MAJOR_LISTING_ORDERBOOK_FIXTURE").ok();
    let mut exchanges = parse_exchange_list(
        &std::env::var("PRE_MAJOR_LISTING_CAPTURE_EXCHANGES")
            .unwrap_or_else(|_| "bitget,bybit,gate".to_string()),
    )?;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input_path = args.next(),
            "--orderbook-fixture" => fixture_path = args.next(),
            "--exchanges" => {
                exchanges = parse_exchange_list(
                    &args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --exchanges"))?,
                )?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    Ok(CaptureArgs {
        input_path: input_path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("missing --input or PRE_MAJOR_LISTING_CAPTURE_INPUT"))?,
        orderbook_fixture_path: fixture_path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        exchanges,
    })
}
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
fn parse_exchange_list(value: &str) -> Result<Vec<ExchangeId>> {
    let exchanges = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| ExchangeId::from_str(item).map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;
    if exchanges.is_empty() {
        return Err(anyhow!("at least one exchange is required"));
    }
    Ok(exchanges)
}
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
fn read_request(path: &str) -> Result<CaptureRequest> {
    let body = fs::read_to_string(path).with_context(|| format!("read capture request: {path}"))?;
    serde_json::from_str(&body).with_context(|| format!("parse capture request JSON: {path}"))
}
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
fn read_fixture_candidates(path: &str) -> Result<Vec<ListingCatchupVenueProbe>> {
    let body =
        fs::read_to_string(path).with_context(|| format!("read orderbook fixture: {path}"))?;
    let fixture: OrderBookFixtureEnvelope = serde_json::from_str(&body)
        .with_context(|| format!("parse orderbook fixture JSON: {path}"))?;
    fixture
        .orderbooks
        .into_iter()
        .map(fixture_orderbook_to_probe)
        .collect()
}
/// 提供fixtureorderbooktoprobe的集中实现，避免量化核心调用方重复处理相同细节。
fn fixture_orderbook_to_probe(orderbook: OrderBookFixture) -> Result<ListingCatchupVenueProbe> {
    let best_bid = best_level_price(&orderbook.bids, "bid")?;
    let best_ask = best_level_price(&orderbook.asks, "ask")?;
    Ok(ListingCatchupVenueProbe {
        exchange: orderbook.exchange.trim().to_ascii_lowercase(),
        symbol: orderbook.symbol.trim().to_ascii_uppercase(),
        best_bid,
        best_ask,
        bid_depth_top5_usdt: depth_top5_usdt(&orderbook.bids)?,
        ask_depth_top5_usdt: depth_top5_usdt(&orderbook.asks)?,
        response_latency_ms: orderbook.response_latency_ms,
    })
}
/// 提供best层级价格的集中实现，避免量化核心调用方重复处理相同细节。
fn best_level_price(levels: &[[String; 2]], side: &str) -> Result<f64> {
    levels
        .first()
        .ok_or_else(|| anyhow!("{side} side is empty"))?
        .first()
        .ok_or_else(|| anyhow!("{side} level price is missing"))?
        .parse::<f64>()
        .with_context(|| format!("parse {side} best price"))
}
/// 提供depthtop5USDT的集中实现，避免量化核心调用方重复处理相同细节。
fn depth_top5_usdt(levels: &[[String; 2]]) -> Result<f64> {
    levels
        .iter()
        .take(5)
        .map(|level| {
            let price = level[0].parse::<f64>().context("parse level price")?;
            let size = level[1].parse::<f64>().context("parse level size")?;
            Ok(price * size)
        })
        .sum()
}
/// 提供capturelivepublicorderbooks的集中实现，避免量化核心调用方重复处理相同细节。
async fn capture_live_public_orderbooks(
    request: &CaptureRequest,
    exchanges: &[ExchangeId],
) -> Result<(Vec<ListingCatchupVenueProbe>, Vec<String>)> {
    let mut probes = Vec::new();
    let mut warnings = Vec::new();
    let instrument = Instrument::perp(&request.base_asset, &request.quote_asset);
    for exchange in exchanges {
        let started_at = Instant::now();
        match public_only_gateway(*exchange) {
            Ok(gateway) => match orderbook_probe(gateway, *exchange, instrument.clone()).await {
                Ok(mut probe) => {
                    probe.response_latency_ms = started_at.elapsed().as_millis() as u64;
                    probes.push(probe);
                }
                Err(error) => warnings.push(format!("{} orderbook skipped: {error}", exchange)),
            },
            Err(error) => warnings.push(format!("{} gateway skipped: {error}", exchange)),
        }
    }
    Ok((probes, warnings))
}
/// 提供publiconlygateway的集中实现，避免量化核心调用方重复处理相同细节。
fn public_only_gateway(exchange: ExchangeId) -> Result<CryptoExcAllGateway> {
    CryptoExcAllGateway::from_single_exchange_credentials(
        exchange,
        "public-only",
        "public-only",
        Some("public-only"),
        false,
    )
    .map_err(anyhow::Error::from)
}
/// 提供orderbookprobe的集中实现，避免量化核心调用方重复处理相同细节。
async fn orderbook_probe(
    gateway: CryptoExcAllGateway,
    exchange: ExchangeId,
    instrument: Instrument,
) -> Result<ListingCatchupVenueProbe> {
    let orderbook = gateway
        .orderbook(exchange, OrderBookQuery::new(instrument).with_limit(5))
        .await
        .map_err(anyhow::Error::from)?;
    orderbook_to_probe(orderbook)
}
/// 提供orderbooktoprobe的集中实现，避免量化核心调用方重复处理相同细节。
fn orderbook_to_probe(orderbook: OrderBook) -> Result<ListingCatchupVenueProbe> {
    let bids = orderbook
        .bids
        .iter()
        .map(|level| [level.price.clone(), level.size.clone()])
        .collect::<Vec<_>>();
    let asks = orderbook
        .asks
        .iter()
        .map(|level| [level.price.clone(), level.size.clone()])
        .collect::<Vec<_>>();
    Ok(ListingCatchupVenueProbe {
        exchange: orderbook.exchange.to_string(),
        symbol: orderbook.exchange_symbol,
        best_bid: best_level_price(&bids, "bid")?,
        best_ask: best_level_price(&asks, "ask")?,
        bid_depth_top5_usdt: depth_top5_usdt(&bids)?,
        ask_depth_top5_usdt: depth_top5_usdt(&asks)?,
        response_latency_ms: 0,
    })
}
/// 构建 量化核心 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_seed(
    request: CaptureRequest,
    candidates: Vec<ListingCatchupVenueProbe>,
) -> ListingCatchupPaperProbeSeed {
    ListingCatchupPaperProbeSeed {
        announcement_id: request.announcement_id,
        announcement_exchange: request.announcement_exchange,
        base_asset: request.base_asset,
        quote_asset: request.quote_asset,
        announced_at_ms: request.announced_at_ms,
        detected_at_ms: request.detected_at_ms,
        pre_announcement_price: request.pre_announcement_price,
        announcement_price: request.announcement_price,
        btc_5m_return_pct: request.btc_5m_return_pct,
        eth_5m_return_pct: request.eth_5m_return_pct,
        opening_upper_wick_rejection: request.opening_upper_wick_rejection,
        entry_price: request.entry_price,
        fee_bps_per_side: request.fee_bps_per_side,
        slippage_bps_per_side: request.slippage_bps_per_side,
        candidates,
        price_path: request.price_path,
    }
}
/// 执行输出usage步骤，串起量化核心需要的状态推进和错误处理。
fn print_usage() {
    println!(
        "Usage: pre_major_listing_capture_probe --input <request.json> [--orderbook-fixture <orderbooks.json>] [--exchanges bitget,bybit,gate]"
    );
}
