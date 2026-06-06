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
    announcement_id: String,
    announcement_exchange: String,
    base_asset: String,
    quote_asset: String,
    announced_at_ms: u64,
    detected_at_ms: u64,
    pre_announcement_price: f64,
    announcement_price: f64,
    btc_5m_return_pct: f64,
    eth_5m_return_pct: f64,
    opening_upper_wick_rejection: bool,
    entry_price: f64,
    fee_bps_per_side: f64,
    slippage_bps_per_side: f64,
    price_path: Vec<ListingCatchupPriceBar>,
}

#[derive(Debug, Deserialize)]
struct OrderBookFixtureEnvelope {
    orderbooks: Vec<OrderBookFixture>,
}

#[derive(Debug, Deserialize)]
struct OrderBookFixture {
    exchange: String,
    symbol: String,
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
    response_latency_ms: u64,
}

#[derive(Debug, Serialize)]
struct CaptureOutput {
    seeds: Vec<ListingCatchupPaperProbeSeed>,
    warnings: Vec<String>,
    production_note: &'static str,
}

#[derive(Debug)]
struct CaptureArgs {
    input_path: String,
    orderbook_fixture_path: Option<String>,
    exchanges: Vec<ExchangeId>,
}

#[tokio::main]
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

fn read_request(path: &str) -> Result<CaptureRequest> {
    let body = fs::read_to_string(path).with_context(|| format!("read capture request: {path}"))?;
    serde_json::from_str(&body).with_context(|| format!("parse capture request JSON: {path}"))
}

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

fn best_level_price(levels: &[[String; 2]], side: &str) -> Result<f64> {
    levels
        .first()
        .ok_or_else(|| anyhow!("{side} side is empty"))?
        .first()
        .ok_or_else(|| anyhow!("{side} level price is missing"))?
        .parse::<f64>()
        .with_context(|| format!("parse {side} best price"))
}

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

fn print_usage() {
    println!(
        "Usage: pre_major_listing_capture_probe --input <request.json> [--orderbook-fixture <orderbooks.json>] [--exchanges bitget,bybit,gate]"
    );
}
