use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupCandidate {
    pub exchange: String,
    pub symbol: String,
    pub spread_pct: f64,
    pub top5_depth_usdt: f64,
    pub response_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupPriceBar {
    pub minute_after_entry: u32,
    pub high_price: f64,
    pub low_price: f64,
    pub close_price: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupPaperSample {
    pub announcement_id: String,
    pub input: ListingCatchupInput,
    pub entry_price: f64,
    pub price_path: Vec<ListingCatchupPriceBar>,
    pub fee_bps_per_side: f64,
    pub slippage_bps_per_side: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupVenueProbe {
    pub exchange: String,
    pub symbol: String,
    pub best_bid: f64,
    pub best_ask: f64,
    pub bid_depth_top5_usdt: f64,
    pub ask_depth_top5_usdt: f64,
    pub response_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupPaperProbeSeed {
    pub announcement_id: String,
    pub announcement_exchange: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub announced_at_ms: u64,
    pub detected_at_ms: u64,
    pub pre_announcement_price: f64,
    pub announcement_price: f64,
    pub btc_5m_return_pct: f64,
    pub eth_5m_return_pct: f64,
    pub opening_upper_wick_rejection: bool,
    pub entry_price: f64,
    pub fee_bps_per_side: f64,
    pub slippage_bps_per_side: f64,
    pub candidates: Vec<ListingCatchupVenueProbe>,
    pub price_path: Vec<ListingCatchupPriceBar>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupAcceptanceCriteria {
    pub min_trade_samples: usize,
    pub min_win_rate_pct: f64,
    pub require_positive_total_net_return: bool,
}

impl Default for ListingCatchupAcceptanceCriteria {
    fn default() -> Self {
        Self {
            min_trade_samples: 30,
            min_win_rate_pct: 60.0,
            require_positive_total_net_return: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupPaperTradeResult {
    pub announcement_id: String,
    pub dedupe_key: String,
    pub exchange: Option<String>,
    pub symbol: Option<String>,
    pub decision: String,
    pub exit_reason: Option<String>,
    pub net_return_pct: f64,
    pub winner: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListingCatchupPaperReport {
    pub total_samples: usize,
    pub unique_samples: usize,
    pub duplicate_samples: usize,
    pub trade_samples: usize,
    pub rejected_samples: usize,
    pub wins: usize,
    pub losses: usize,
    pub win_rate_pct: f64,
    pub total_net_return_pct: f64,
    pub average_net_return_pct: f64,
    pub production_status: String,
    pub blockers: Vec<String>,
    pub automatic_live_trading_allowed: bool,
    pub trade_results: Vec<ListingCatchupPaperTradeResult>,
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

pub fn build_listing_catchup_paper_sample(
    seed: ListingCatchupPaperProbeSeed,
) -> Result<ListingCatchupPaperSample, String> {
    validate_positive(seed.pre_announcement_price, "pre_announcement_price")?;
    validate_positive(seed.announcement_price, "announcement_price")?;
    validate_positive(seed.entry_price, "entry_price")?;
    if seed.detected_at_ms < seed.announced_at_ms {
        return Err("detected_at_before_announced_at".to_string());
    }

    let candidates = seed
        .candidates
        .into_iter()
        .map(venue_probe_to_candidate)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ListingCatchupPaperSample {
        announcement_id: seed.announcement_id,
        input: ListingCatchupInput {
            announcement_exchange: seed.announcement_exchange.trim().to_ascii_lowercase(),
            base_asset: seed.base_asset.trim().to_ascii_uppercase(),
            quote_asset: seed.quote_asset.trim().to_ascii_uppercase(),
            detection_latency_secs: (seed.detected_at_ms - seed.announced_at_ms) / 1_000,
            pre_announcement_return_15m_pct: round_pct(
                (seed.announcement_price / seed.pre_announcement_price - 1.0) * 100.0,
            ),
            btc_5m_return_pct: seed.btc_5m_return_pct,
            eth_5m_return_pct: seed.eth_5m_return_pct,
            opening_upper_wick_rejection: seed.opening_upper_wick_rejection,
            candidates,
        },
        entry_price: seed.entry_price,
        price_path: seed.price_path,
        fee_bps_per_side: seed.fee_bps_per_side,
        slippage_bps_per_side: seed.slippage_bps_per_side,
    })
}

pub fn evaluate_listing_catchup_paper(
    samples: Vec<ListingCatchupPaperSample>,
    criteria: ListingCatchupAcceptanceCriteria,
) -> ListingCatchupPaperReport {
    let total_samples = samples.len();
    let mut seen = HashSet::new();
    let mut duplicate_samples = 0usize;
    let mut trade_results = Vec::new();
    let mut rejected_samples = 0usize;

    for sample in samples {
        let dedupe_key = paper_sample_dedupe_key(&sample);
        if !seen.insert(dedupe_key.clone()) {
            duplicate_samples += 1;
            continue;
        }

        match choose_secondary_perp_venue(&sample.input) {
            ListingCatchupDecision::Trade {
                exchange,
                symbol,
                stop_loss_pct,
                take_profit_first_pct,
                take_profit_second_pct,
                max_hold_minutes,
                ..
            } => {
                let (exit_reason, net_return_pct) = simulate_paper_trade(
                    &sample,
                    stop_loss_pct,
                    take_profit_first_pct,
                    take_profit_second_pct,
                    max_hold_minutes,
                );
                trade_results.push(ListingCatchupPaperTradeResult {
                    announcement_id: sample.announcement_id,
                    dedupe_key,
                    exchange: Some(exchange),
                    symbol: Some(symbol),
                    decision: "trade".to_string(),
                    exit_reason: Some(exit_reason),
                    net_return_pct,
                    winner: net_return_pct > 0.0,
                });
            }
            ListingCatchupDecision::Reject { reason } => {
                rejected_samples += 1;
                trade_results.push(ListingCatchupPaperTradeResult {
                    announcement_id: sample.announcement_id,
                    dedupe_key,
                    exchange: None,
                    symbol: None,
                    decision: format!("reject:{reason}"),
                    exit_reason: None,
                    net_return_pct: 0.0,
                    winner: false,
                });
            }
        }
    }

    let trade_samples = trade_results
        .iter()
        .filter(|result| result.decision == "trade")
        .count();
    let wins = trade_results
        .iter()
        .filter(|result| result.decision == "trade" && result.winner)
        .count();
    let losses = trade_samples.saturating_sub(wins);
    let total_net_return_pct = round_pct(
        trade_results
            .iter()
            .filter(|result| result.decision == "trade")
            .map(|result| result.net_return_pct)
            .sum(),
    );
    let average_net_return_pct = if trade_samples == 0 {
        0.0
    } else {
        round_pct(total_net_return_pct / trade_samples as f64)
    };
    let win_rate_pct = if trade_samples == 0 {
        0.0
    } else {
        round_pct(wins as f64 * 100.0 / trade_samples as f64)
    };

    let mut blockers = Vec::new();
    if trade_samples < criteria.min_trade_samples {
        blockers.push("paper_trade_samples_below_minimum".to_string());
    }
    if win_rate_pct < criteria.min_win_rate_pct {
        blockers.push("paper_win_rate_below_minimum".to_string());
    }
    if criteria.require_positive_total_net_return && total_net_return_pct <= 0.0 {
        blockers.push("paper_total_net_return_not_positive".to_string());
    }

    ListingCatchupPaperReport {
        total_samples,
        unique_samples: total_samples.saturating_sub(duplicate_samples),
        duplicate_samples,
        trade_samples,
        rejected_samples,
        wins,
        losses,
        win_rate_pct,
        total_net_return_pct,
        average_net_return_pct,
        production_status: if blockers.is_empty() {
            "paper_ready".to_string()
        } else {
            "blocked".to_string()
        },
        blockers,
        automatic_live_trading_allowed: false,
        trade_results,
    }
}

fn paper_sample_dedupe_key(sample: &ListingCatchupPaperSample) -> String {
    format!(
        "{}:{}:{}:{}",
        sample.announcement_id.trim().to_ascii_lowercase(),
        sample
            .input
            .announcement_exchange
            .trim()
            .to_ascii_lowercase(),
        sample.input.base_asset.trim().to_ascii_uppercase(),
        sample.input.quote_asset.trim().to_ascii_uppercase()
    )
}

fn simulate_paper_trade(
    sample: &ListingCatchupPaperSample,
    stop_loss_pct: f64,
    take_profit_first_pct: f64,
    take_profit_second_pct: f64,
    max_hold_minutes: u32,
) -> (String, f64) {
    if sample.entry_price <= 0.0 || sample.price_path.is_empty() {
        return ("invalid_or_empty_price_path".to_string(), 0.0);
    }

    let stop_price = sample.entry_price * (1.0 - stop_loss_pct / 100.0);
    let take_profit_first = sample.entry_price * (1.0 + take_profit_first_pct / 100.0);
    let take_profit_second = sample.entry_price * (1.0 + take_profit_second_pct / 100.0);
    let mut first_half_return = None;
    let mut remaining_exit = None;
    let mut exit_reason = "max_hold_exit".to_string();

    for bar in sample
        .price_path
        .iter()
        .filter(|bar| bar.minute_after_entry <= max_hold_minutes)
    {
        if bar.low_price <= stop_price {
            remaining_exit = Some(stop_price);
            exit_reason = if first_half_return.is_some() {
                "stop_after_first_take_profit".to_string()
            } else {
                "stop_loss".to_string()
            };
            break;
        }

        if first_half_return.is_none() && bar.high_price >= take_profit_first {
            first_half_return = Some(net_return_pct_for_exit(sample, take_profit_first));
            exit_reason = "first_take_profit_then_timeout".to_string();
        }

        if bar.high_price >= take_profit_second {
            remaining_exit = Some(take_profit_second);
            exit_reason = "second_take_profit".to_string();
            break;
        }

        remaining_exit = Some(bar.close_price);
    }

    let Some(remaining_exit_price) = remaining_exit else {
        return ("no_price_within_hold_window".to_string(), 0.0);
    };
    let remaining_return = net_return_pct_for_exit(sample, remaining_exit_price);
    let net_return_pct = if let Some(first_half_return) = first_half_return {
        first_half_return * 0.5 + remaining_return * 0.5
    } else {
        remaining_return
    };

    (exit_reason, round_pct(net_return_pct))
}

fn net_return_pct_for_exit(sample: &ListingCatchupPaperSample, exit_price: f64) -> f64 {
    let entry_slippage = sample.slippage_bps_per_side / 10_000.0;
    let exit_slippage = sample.slippage_bps_per_side / 10_000.0;
    let effective_entry = sample.entry_price * (1.0 + entry_slippage);
    let effective_exit = exit_price * (1.0 - exit_slippage);
    let gross_pct = (effective_exit / effective_entry - 1.0) * 100.0;
    let round_trip_fee_pct = sample.fee_bps_per_side * 2.0 / 100.0;
    gross_pct - round_trip_fee_pct
}

fn venue_probe_to_candidate(
    probe: ListingCatchupVenueProbe,
) -> Result<ListingCatchupCandidate, String> {
    validate_positive(probe.best_bid, "best_bid")?;
    validate_positive(probe.best_ask, "best_ask")?;
    if probe.best_ask < probe.best_bid {
        return Err("best_ask_below_best_bid".to_string());
    }

    let mid = (probe.best_bid + probe.best_ask) / 2.0;
    validate_positive(mid, "mid_price")?;
    Ok(ListingCatchupCandidate {
        exchange: probe.exchange.trim().to_ascii_lowercase(),
        symbol: probe.symbol.trim().to_string(),
        spread_pct: round_pct((probe.best_ask - probe.best_bid) / mid * 100.0),
        top5_depth_usdt: probe.bid_depth_top5_usdt.min(probe.ask_depth_top5_usdt),
        response_latency_ms: probe.response_latency_ms,
    })
}

fn validate_positive(value: f64, field: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{field}_must_be_positive"))
    }
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

fn round_pct(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
