#[derive(Debug, Clone, PartialEq)]
pub struct ListingCatchupCandidate {
    pub exchange: String,
    pub symbol: String,
    pub spread_pct: f64,
    pub top5_depth_usdt: f64,
    pub response_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListingCatchupInput {
    pub announcement_exchange: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub detection_latency_secs: u64,
    pub pre_announcement_return_15m_pct: f64,
    pub btc_5m_return_pct: f64,
    pub eth_5m_return_pct: f64,
    pub opening_upper_wick_rejection: bool,
    pub candidates: Vec<ListingCatchupCandidate>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListingCatchupDecision {
    Trade {
        exchange: String,
        symbol: String,
        size_fraction_r: f64,
        stop_loss_pct: f64,
        take_profit_first_pct: f64,
        take_profit_second_pct: f64,
        max_hold_minutes: u32,
    },
    Reject {
        reason: String,
    },
}

const MAX_DETECTION_LATENCY_SECS: u64 = 120;
const MAX_PRE_ANNOUNCEMENT_RETURN_15M_PCT: f64 = 20.0;
const MACRO_DUMP_THRESHOLD_5M_PCT: f64 = -1.2;
const MAX_SPREAD_PCT: f64 = 0.35;
const MIN_TOP5_DEPTH_USDT: f64 = 50_000.0;
const VENUE_PRIORITY: [&str; 3] = ["bitget", "bybit", "gate"];

pub fn choose_secondary_perp_venue(input: &ListingCatchupInput) -> ListingCatchupDecision {
    if !is_major_listing_exchange(&input.announcement_exchange) {
        return reject("unsupported_announcement_exchange");
    }
    if input.detection_latency_secs > MAX_DETECTION_LATENCY_SECS {
        return reject("listing_latency_too_high");
    }
    if input.pre_announcement_return_15m_pct > MAX_PRE_ANNOUNCEMENT_RETURN_15M_PCT {
        return reject("pre_pump_too_large");
    }
    if input.btc_5m_return_pct <= MACRO_DUMP_THRESHOLD_5M_PCT
        || input.eth_5m_return_pct <= MACRO_DUMP_THRESHOLD_5M_PCT
    {
        return reject("macro_market_dumping");
    }
    if input.opening_upper_wick_rejection {
        return reject("opening_wick_rejection");
    }

    for venue in VENUE_PRIORITY {
        if let Some(candidate) = input.candidates.iter().find(|candidate| {
            candidate.exchange.eq_ignore_ascii_case(venue)
                && candidate.spread_pct <= MAX_SPREAD_PCT
                && candidate.top5_depth_usdt >= MIN_TOP5_DEPTH_USDT
        }) {
            return ListingCatchupDecision::Trade {
                exchange: venue.to_string(),
                symbol: candidate.symbol.clone(),
                size_fraction_r: 0.3,
                stop_loss_pct: 2.0,
                take_profit_first_pct: 3.0,
                take_profit_second_pct: 5.0,
                max_hold_minutes: 120,
            };
        }
    }

    reject("secondary_perp_depth_unready")
}

fn is_major_listing_exchange(exchange: &str) -> bool {
    matches!(
        exchange.trim().to_ascii_lowercase().as_str(),
        "binance" | "okx"
    )
}

fn reject(reason: &str) -> ListingCatchupDecision {
    ListingCatchupDecision::Reject {
        reason: reason.to_string(),
    }
}
