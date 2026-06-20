fn compute_event_price_change_pct(
    current_price: Option<Decimal>,
    previous_price: Option<Decimal>,
) -> Option<Decimal> {
    let current_price = current_price?;
    let previous_price = previous_price?;
    if previous_price <= Decimal::ZERO {
        return None;
    }
    Some((current_price - previous_price) / previous_price * Decimal::new(100, 0))
}

fn price_direction(price_change_pct: Option<Decimal>) -> String {
    match price_change_pct {
        Some(value) if value > Decimal::ZERO => "up".to_string(),
        Some(value) if value < Decimal::ZERO => "down".to_string(),
        Some(_) => "flat".to_string(),
        None => "unknown".to_string(),
    }
}

fn is_top50_rank(rank: Option<i32>) -> bool {
    rank.is_some_and(|value| value > 0 && value <= MARKET_RANK_TOP_BOUNDARY)
}

fn compute_rank_delta(old_rank: Option<i32>, new_rank: Option<i32>) -> Option<i32> {
    Some(old_rank? - new_rank?)
}

#[derive(Debug, Clone)]
struct MarketRankTechnicalCapture {
    status: String,
    snapshot: Option<MarketRankTechnicalSnapshot>,
}

impl MarketRankTechnicalCapture {
    fn new(status: impl Into<String>, snapshot: Option<MarketRankTechnicalSnapshot>) -> Self {
        Self {
            status: status.into(),
            snapshot,
        }
    }

    fn not_requested() -> Self {
        Self::new("not_requested", None)
    }
}

fn build_rank_velocity_event(
    symbol: &str,
    timeframe: &str,
    old_rank: Option<i32>,
    new_rank: i32,
    delta: Option<i32>,
    volume_24h_quote: Option<Decimal>,
    current_price: Option<Decimal>,
    previous_price: Option<Decimal>,
    detected_at: DateTime<Utc>,
    technical_capture: MarketRankTechnicalCapture,
) -> MarketRankEvent {
    let price_change_pct = compute_event_price_change_pct(current_price, previous_price);
    MarketRankEvent {
        id: None,
        exchange: "okx".to_string(),
        symbol: symbol.to_string(),
        event_type: MarketRankEventType::RankVelocity,
        timeframe: Some(timeframe.to_string()),
        old_rank,
        new_rank: Some(new_rank),
        delta_rank: delta,
        volume_24h_quote,
        current_price,
        previous_price,
        price_change_pct,
        price_direction: price_direction(price_change_pct),
        technical_snapshot_status: technical_capture.status,
        technical_snapshot: technical_capture.snapshot,
        detected_at,
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}

fn build_top_list_event(
    symbol: &str,
    is_entry: bool,
    old_rank: Option<i32>,
    new_rank: Option<i32>,
    volume_24h_quote: Option<Decimal>,
    current_price: Option<Decimal>,
    previous_price: Option<Decimal>,
    detected_at: DateTime<Utc>,
    technical_capture: MarketRankTechnicalCapture,
) -> MarketRankEvent {
    let price_change_pct = compute_event_price_change_pct(current_price, previous_price);
    MarketRankEvent {
        id: None,
        exchange: "okx".to_string(),
        symbol: symbol.to_string(),
        event_type: if is_entry {
            MarketRankEventType::TopEntry
        } else {
            MarketRankEventType::TopExit
        },
        timeframe: None,
        old_rank,
        new_rank,
        delta_rank: compute_rank_delta(old_rank, new_rank),
        volume_24h_quote,
        current_price,
        previous_price,
        price_change_pct,
        price_direction: price_direction(price_change_pct),
        technical_snapshot_status: technical_capture.status,
        technical_snapshot: technical_capture.snapshot,
        detected_at,
        source: "scanner_service".to_string(),
        notification_state: "pending".to_string(),
    }
}
