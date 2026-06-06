use anyhow::{anyhow, Context, Result};
use rust_quant_services::strategy::pre_major_listing_perp_catchup::ListingCatchupPriceBar;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize)]
struct AnnouncementInput {
    announcement_id: String,
    source: String,
    title: String,
    #[serde(default)]
    content: String,
    announced_at_ms: u64,
    detected_at_ms: u64,
    pre_announcement_price: f64,
    announcement_price: f64,
    entry_price: f64,
    btc_5m_return_pct: f64,
    eth_5m_return_pct: f64,
    opening_upper_wick_rejection: bool,
    fee_bps_per_side: f64,
    slippage_bps_per_side: f64,
    #[serde(default)]
    price_path: Vec<ListingCatchupPriceBar>,
}

#[derive(Debug, Serialize)]
struct CaptureRequest {
    announcement_id: String,
    announcement_exchange: String,
    base_asset: String,
    quote_asset: String,
    announced_at_ms: u64,
    detected_at_ms: u64,
    pre_announcement_price: f64,
    announcement_price: f64,
    entry_price: f64,
    btc_5m_return_pct: f64,
    eth_5m_return_pct: f64,
    opening_upper_wick_rejection: bool,
    fee_bps_per_side: f64,
    slippage_bps_per_side: f64,
    price_path: Vec<ListingCatchupPriceBar>,
}

#[derive(Debug, Serialize)]
struct AnnouncementOutput {
    request: CaptureRequest,
    production_note: &'static str,
}

fn main() -> Result<()> {
    let input_path = parse_input_path()?;
    let input = read_input(&input_path)?;
    let request = build_capture_request(input)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&AnnouncementOutput {
            request,
            production_note: "announcement_capture_request_only_live_trading_disabled",
        })?
    );
    Ok(())
}

fn parse_input_path() -> Result<String> {
    let mut input_path = std::env::var("PRE_MAJOR_LISTING_ANNOUNCEMENT_INPUT").ok();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input_path = args.next(),
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    input_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing --input or PRE_MAJOR_LISTING_ANNOUNCEMENT_INPUT"))
}

fn read_input(path: &str) -> Result<AnnouncementInput> {
    let body =
        fs::read_to_string(path).with_context(|| format!("read announcement input: {path}"))?;
    serde_json::from_str(&body).with_context(|| format!("parse announcement input JSON: {path}"))
}

fn build_capture_request(input: AnnouncementInput) -> Result<CaptureRequest> {
    let text = format!("{} {} {}", input.source, input.title, input.content);
    let exchange =
        source_exchange(&text).ok_or_else(|| anyhow!("unsupported_announcement_source"))?;
    if is_negative_listing_event(&text) {
        return Err(anyhow!("negative_or_delisting_announcement"));
    }
    if !is_positive_listing_event(&text) {
        return Err(anyhow!("not_a_major_listing_announcement"));
    }
    let base_asset = extract_base_asset(&text).ok_or_else(|| anyhow!("base_asset_not_detected"))?;

    Ok(CaptureRequest {
        announcement_id: input.announcement_id,
        announcement_exchange: exchange,
        base_asset,
        quote_asset: "USDT".to_string(),
        announced_at_ms: input.announced_at_ms,
        detected_at_ms: input.detected_at_ms,
        pre_announcement_price: input.pre_announcement_price,
        announcement_price: input.announcement_price,
        entry_price: input.entry_price,
        btc_5m_return_pct: input.btc_5m_return_pct,
        eth_5m_return_pct: input.eth_5m_return_pct,
        opening_upper_wick_rejection: input.opening_upper_wick_rejection,
        fee_bps_per_side: input.fee_bps_per_side,
        slippage_bps_per_side: input.slippage_bps_per_side,
        price_path: input.price_path,
    })
}

fn source_exchange(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("binance") || lower.contains("币安") {
        Some("binance".to_string())
    } else if lower.contains("okx") {
        Some("okx".to_string())
    } else {
        None
    }
}

fn is_negative_listing_event(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["delist", "delisting", "suspend trading", "下架", "暂停交易"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn is_positive_listing_event(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
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
    .any(|needle| lower.contains(needle))
}

fn extract_base_asset(text: &str) -> Option<String> {
    for candidate in parenthesized_tokens(text)
        .into_iter()
        .chain(usdt_pair_tokens(text))
    {
        let normalized = candidate.trim().to_ascii_uppercase();
        if is_plausible_base_asset(&normalized) {
            return Some(normalized);
        }
    }
    None
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

fn print_usage() {
    println!("Usage: pre_major_listing_announcement_to_capture --input <announcement.json>");
}
