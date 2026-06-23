use anyhow::{anyhow, Context, Result};
use rust_quant_services::strategy::pre_major_listing_perp_catchup::ListingCatchupPriceBar;
use serde::{Deserialize, Serialize};
use std::fs;
#[derive(Debug, Deserialize)]
struct AnnouncementInput {
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
    /// 毫秒级时间戳或时长。
    detected_at_ms: u64,
    /// 价格数值。
    pre_announcement_price: f64,
    /// 价格数值。
    announcement_price: f64,
    /// 入场价格。
    entry_price: f64,
    /// BTC 5 分钟收益率百分比。
    btc_5m_return_pct: f64,
    /// ETH 5 分钟收益率百分比。
    eth_5m_return_pct: f64,
    /// 开盘上影线压制信号。
    opening_upper_wick_rejection: bool,
    /// 手续费bpsper方向，用于当前结构体的业务数据。
    fee_bps_per_side: f64,
    /// slippagebpsper方向，用于当前结构体的业务数据。
    slippage_bps_per_side: f64,
    #[serde(default)]
    /// 列表数据。
    price_path: Vec<ListingCatchupPriceBar>,
}
#[derive(Debug, Serialize)]
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
    /// 入场价格。
    entry_price: f64,
    /// BTC 5 分钟收益率百分比。
    btc_5m_return_pct: f64,
    /// ETH 5 分钟收益率百分比。
    eth_5m_return_pct: f64,
    /// openingupperwickrejection，用于构建接口请求。
    opening_upper_wick_rejection: bool,
    /// 手续费bpsper方向，用于构建接口请求。
    fee_bps_per_side: f64,
    /// slippagebpsper方向，用于构建接口请求。
    slippage_bps_per_side: f64,
    /// 列表数据。
    price_path: Vec<ListingCatchupPriceBar>,
}
#[derive(Debug, Serialize)]
struct AnnouncementOutput {
    /// 请求。
    request: CaptureRequest,
    /// 备注信息。
    production_note: &'static str,
}
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
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
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
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
/// 加载 量化核心 运行所需数据，并把缺失或异常交给调用方处理。
fn read_input(path: &str) -> Result<AnnouncementInput> {
    let body =
        fs::read_to_string(path).with_context(|| format!("read announcement input: {path}"))?;
    serde_json::from_str(&body).with_context(|| format!("parse announcement input JSON: {path}"))
}
/// 构建 量化核心 请求或响应载荷，把字段组装规则集中在同一入口。
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
/// 提供来源交易所的集中实现，避免量化核心调用方重复处理相同细节。
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
/// 判断 量化核心 条件是否满足，给上层流程提供布尔决策。
fn is_negative_listing_event(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["delist", "delisting", "suspend trading", "下架", "暂停交易"]
        .iter()
        .any(|needle| lower.contains(needle))
}
/// 判断 量化核心 条件是否满足，给上层流程提供布尔决策。
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
/// 解析输入参数并收敛为 量化核心 可使用的结构化值。
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
fn print_usage() {
    println!("Usage: pre_major_listing_announcement_to_capture --input <announcement.json>");
}
