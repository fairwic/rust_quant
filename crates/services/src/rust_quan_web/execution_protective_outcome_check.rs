use anyhow::{anyhow, bail, Result};
use crypto_exc_all::{
    CancelOrderRequest, ExchangeId, Order, OrderSide, Position, ProtectiveOrderRequest,
    ProtectiveOrderWorkingType,
};
use serde_json::{json, Value};

use super::execution_audit::redact_error_message;
use super::execution_payload::{parse_exchange, parse_instrument, parse_side};
use super::execution_protection::{
    protective_order_query_candidates_from_ack, protective_order_query_to_sync_outcome,
    ProtectionSyncOutcome,
};
use crate::exchange::CryptoExcAllGateway;

const PROTECTIVE_OUTCOME_CONFIRM_ENV: &str = "PROTECTIVE_OUTCOME_CONFIRM";
const PROTECTIVE_OUTCOME_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LIVE_PROTECTIVE_ORDER";

#[derive(Debug, Clone)]
struct ProtectiveOutcomeCheckConfig {
    exchange: ExchangeId,
    symbol: String,
    side: OrderSide,
    position_side: Option<String>,
    stop_price: Option<String>,
    trigger_factor: f64,
    client_order_id: String,
}

impl ProtectiveOutcomeCheckConfig {
    fn from_env() -> Result<Self> {
        let confirmation = std::env::var(PROTECTIVE_OUTCOME_CONFIRM_ENV).ok();
        if confirmation.as_deref().map(str::trim) != Some(PROTECTIVE_OUTCOME_CONFIRM_TOKEN) {
            bail!(
                "{PROTECTIVE_OUTCOME_CONFIRM_ENV}={PROTECTIVE_OUTCOME_CONFIRM_TOKEN} is required before creating a live protective order"
            );
        }

        let exchange = std::env::var("PROTECTIVE_OUTCOME_EXCHANGE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "binance".to_string());
        let exchange = parse_exchange(&exchange)?;
        if exchange != ExchangeId::Binance {
            bail!(
                "protective outcome check currently supports Binance standalone stop-market only"
            );
        }

        let symbol = std::env::var("PROTECTIVE_OUTCOME_SYMBOL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "ETHUSDT".to_string());
        ensure_eth_only_symbol(&symbol)?;

        let side = std::env::var("PROTECTIVE_OUTCOME_SIDE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_side(&value))
            .transpose()?
            .unwrap_or(OrderSide::Sell);
        let position_side = std::env::var("PROTECTIVE_OUTCOME_POSITION_SIDE")
            .ok()
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty());
        let stop_price = std::env::var("PROTECTIVE_OUTCOME_STOP_PRICE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let trigger_factor = std::env::var("PROTECTIVE_OUTCOME_TRIGGER_FACTOR")
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or_else(|| match side {
                OrderSide::Sell => 0.5,
                OrderSide::Buy => 1.5,
            });
        let client_order_id = std::env::var("PROTECTIVE_OUTCOME_CLIENT_ORDER_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                format!(
                    "rq-protect-outcome-{}",
                    chrono::Utc::now().timestamp_millis()
                )
            });

        Ok(Self {
            exchange,
            symbol,
            side,
            position_side,
            stop_price,
            trigger_factor,
            client_order_id,
        })
    }
}

pub async fn run_protective_order_outcome_check_from_env() -> Result<Value> {
    let config = ProtectiveOutcomeCheckConfig::from_env()?;
    let instrument = parse_instrument(&config.symbol)?;
    let gateway = CryptoExcAllGateway::from_env()?;

    let positions = gateway
        .positions(config.exchange, Some(&instrument))
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only position check failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let non_zero_position_count = protective_outcome_position_preflight(&positions)?;
    ensure_protective_side_matches_existing_position(&config, &positions)?;

    let open_orders = gateway
        .open_orders(
            config.exchange,
            crypto_exc_all::OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
        )
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only open-orders check failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let active_open_order_count = open_orders
        .iter()
        .filter(|order| active_order_status(order.status.as_deref()))
        .count();
    if active_open_order_count > 0 {
        bail!(
            "refusing protective outcome check because ETHUSDT has active open orders: active_open_order_count={active_open_order_count}"
        );
    }
    let effective_position_side = protective_position_side(&config, &positions);

    let ticker = gateway
        .ticker(config.exchange, &instrument)
        .await
        .map_err(|error| {
            anyhow!(
                "public ticker read failed before protective order: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let stop_price = match config.stop_price {
        Some(stop_price) => stop_price,
        None => derived_stop_price(&ticker.last_price, config.trigger_factor)?,
    };
    validate_stop_price_not_immediate(config.side, &stop_price, &ticker.last_price)?;

    let mut request =
        ProtectiveOrderRequest::stop_market(instrument.clone(), config.side, stop_price.clone())
            .with_close_position(true)
            .with_working_type(ProtectiveOrderWorkingType::MarkPrice)
            .with_price_protect(true)
            .with_client_order_id(config.client_order_id.clone());
    if let Some(position_side) = effective_position_side.as_deref() {
        request = request.with_position_side(position_side);
    }

    let ack = gateway
        .place_protective_order(config.exchange, request.clone())
        .await
        .map_err(|error| {
            anyhow!(
                "live protective order placement failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let queries = protective_order_query_candidates_from_ack(
        &instrument,
        &ack,
        Some(config.client_order_id.clone()),
    )?;
    let mut queried_order = None;
    let mut outcome = ProtectionSyncOutcome::uncertain(
        "query_protective_order",
        "no protective order query was attempted",
    );
    for query in &queries {
        match gateway
            .protective_order(config.exchange, query.clone())
            .await
        {
            Ok(order) => {
                outcome = protective_order_query_to_sync_outcome(Ok(order.clone()));
                queried_order = Some(order);
            }
            Err(error) => {
                outcome = protective_order_query_to_sync_outcome(Err(error));
            }
        }
        if matches!(outcome, ProtectionSyncOutcome::Confirmed { .. }) {
            break;
        }
    }

    let cancel_request = cancel_request_from_order_or_client_id(
        &instrument,
        queried_order.as_ref(),
        ack.order_id.as_deref(),
        &config.client_order_id,
    );
    let cancel_result = gateway
        .cancel_protective_order(config.exchange, cancel_request)
        .await;
    if !matches!(outcome, ProtectionSyncOutcome::Confirmed { .. }) {
        let _ = cancel_result;
        bail!("protective order was placed but not confirmed active: {outcome:?}");
    }
    let cancel_ack = cancel_result.map_err(|error| {
        anyhow!(
            "protective order confirmed but cancellation failed: {}",
            redact_error_message(error.to_string())
        )
    })?;

    let post_cancel_query = gateway
        .protective_order(
            config.exchange,
            crypto_exc_all::ProtectiveOrderQuery::by_client_order_id(
                instrument.clone(),
                config.client_order_id.clone(),
            ),
        )
        .await;
    let post_cancel_active = match post_cancel_query {
        Ok(order) => active_order_status(order.status.as_deref()),
        Err(_) => false,
    };
    if post_cancel_active {
        bail!("protective order is still active after cancellation");
    }

    Ok(json!({
        "contract_version": "v2",
        "exchange": config.exchange.as_str(),
        "protective_order_mode": "independent_stop_market",
        "symbol": config.symbol,
        "side": match config.side {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        },
        "position_side": effective_position_side,
        "stop_price": stop_price,
        "client_order_id": config.client_order_id,
        "preflight": {
            "signed_read_only": true,
            "position_count": positions.len(),
            "non_zero_position_count": non_zero_position_count,
            "open_order_count": open_orders.len(),
            "active_open_order_count": active_open_order_count,
        },
        "outcome": format!("{outcome:?}"),
        "ack_status": ack.status,
        "cancel_status": cancel_ack.status,
        "post_cancel_active": post_cancel_active,
        "main_order_place_allowed": false,
        "repeat_open_order_allowed": false,
        "protective_order_place_allowed": true,
        "protective_order_created": true,
        "protective_order_cancelled": true,
    }))
}

fn ensure_eth_only_symbol(symbol: &str) -> Result<()> {
    let normalized: String = symbol
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    if normalized == "ETHUSDT" || normalized.starts_with("ETHUSDT") {
        return Ok(());
    }
    bail!("protective outcome check only allows ETHUSDT / ETH-USDT-SWAP in this MVP");
}

fn protective_position_side(
    config: &ProtectiveOutcomeCheckConfig,
    positions: &[Position],
) -> Option<String> {
    if config.position_side.is_some() {
        return config.position_side.clone();
    }
    if config.exchange != ExchangeId::Binance || !binance_positions_indicate_hedge_mode(positions) {
        return None;
    }
    Some(match config.side {
        OrderSide::Sell => "LONG".to_string(),
        OrderSide::Buy => "SHORT".to_string(),
    })
}

fn protective_outcome_position_preflight(positions: &[Position]) -> Result<usize> {
    let non_zero_position_count = positions
        .iter()
        .filter(|position| positive_decimal_text(&position.size))
        .count();
    if non_zero_position_count == 0 {
        bail!(
            "protective outcome check requires an existing ETHUSDT position; refusing to create a close-position protective order while flat"
        );
    }
    if non_zero_position_count > 1 {
        bail!(
            "protective outcome check requires exactly one ETHUSDT position: non_zero_position_count={non_zero_position_count}"
        );
    }
    Ok(non_zero_position_count)
}

fn ensure_protective_side_matches_existing_position(
    config: &ProtectiveOutcomeCheckConfig,
    positions: &[Position],
) -> Result<()> {
    let existing_position_side = active_position_side(positions);
    let requested_position_side = config
        .position_side
        .as_deref()
        .map(normalized_position_side)
        .or_else(|| existing_position_side.clone());
    if let (Some(existing), Some(requested)) = (&existing_position_side, &requested_position_side) {
        if matches!(existing.as_str(), "LONG" | "SHORT")
            && matches!(requested.as_str(), "LONG" | "SHORT")
            && existing != requested
        {
            bail!(
                "explicit protective position side {requested} does not match existing ETHUSDT {existing} position"
            );
        }
    }

    match requested_position_side.as_deref() {
        Some("LONG") if config.side != OrderSide::Sell => {
            bail!("ETHUSDT LONG position requires SELL protective order side")
        }
        Some("SHORT") if config.side != OrderSide::Buy => {
            bail!("ETHUSDT SHORT position requires BUY protective order side")
        }
        _ => Ok(()),
    }
}

fn active_position_side(positions: &[Position]) -> Option<String> {
    positions
        .iter()
        .find(|position| positive_decimal_text(&position.size))
        .and_then(|position| position.side.as_deref())
        .map(normalized_position_side)
        .filter(|side| !side.is_empty())
}

fn normalized_position_side(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn binance_positions_indicate_hedge_mode(positions: &[Position]) -> bool {
    positions.iter().any(|position| {
        matches!(
            position
                .side
                .as_deref()
                .map(|side| side.trim().to_ascii_uppercase())
                .as_deref(),
            Some("LONG" | "SHORT")
        )
    })
}

fn derived_stop_price(last_price: &str, factor: f64) -> Result<String> {
    let last_price = last_price
        .trim()
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid ticker last_price before protective order: {error}"))?;
    if !last_price.is_finite() || last_price <= 0.0 {
        bail!("invalid non-positive ticker last_price before protective order");
    }
    Ok(format!("{:.2}", last_price * factor))
}

fn validate_stop_price_not_immediate(
    side: OrderSide,
    stop_price: &str,
    last_price: &str,
) -> Result<()> {
    let stop_price = parse_positive_price(stop_price, "protective stop_price")?;
    let last_price = parse_positive_price(last_price, "ticker last_price")?;
    match side {
        OrderSide::Sell if stop_price >= last_price => bail!(
            "SELL protective stop_price would trigger immediately: stop_price={stop_price}, last_price={last_price}"
        ),
        OrderSide::Buy if stop_price <= last_price => bail!(
            "BUY protective stop_price would trigger immediately: stop_price={stop_price}, last_price={last_price}"
        ),
        _ => Ok(()),
    }
}

fn parse_positive_price(value: &str, label: &str) -> Result<f64> {
    let parsed = value
        .trim()
        .parse::<f64>()
        .map_err(|error| anyhow!("invalid {label}: {error}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        bail!("invalid non-positive {label}");
    }
    Ok(parsed)
}

fn cancel_request_from_order_or_client_id(
    instrument: &crypto_exc_all::Instrument,
    order: Option<&Order>,
    ack_order_id: Option<&str>,
    client_order_id: &str,
) -> CancelOrderRequest {
    if let Some(order_id) = order
        .and_then(|order| order.order_id.as_deref())
        .or(ack_order_id)
        .filter(|value| !value.trim().is_empty())
    {
        return CancelOrderRequest::by_order_id(instrument.clone(), order_id.to_string());
    }
    CancelOrderRequest::by_client_order_id(instrument.clone(), client_order_id.to_string())
}

fn positive_decimal_text(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|parsed| parsed.is_finite() && parsed.abs() > 0.0)
}

fn active_order_status(status: Option<&str>) -> bool {
    let normalized = status.unwrap_or_default().trim().to_ascii_uppercase();
    !matches!(
        normalized.as_str(),
        "CANCELED" | "CANCELLED" | "FILLED" | "CLOSED" | "REJECTED" | "EXPIRED"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(side: &str) -> crypto_exc_all::Position {
        position_with_size(side, "0")
    }

    fn position_with_size(side: &str, size: &str) -> crypto_exc_all::Position {
        crypto_exc_all::Position {
            exchange: ExchangeId::Binance,
            instrument: crypto_exc_all::Instrument::perp("ETH", "USDT"),
            exchange_symbol: "ETHUSDT".to_string(),
            side: Some(side.to_string()),
            size: size.to_string(),
            entry_price: None,
            mark_price: None,
            unrealized_pnl: None,
            leverage: None,
            margin_mode: None,
            liquidation_price: None,
            raw: json!({"positionSide": side, "positionAmt": size}),
        }
    }

    #[test]
    fn protective_outcome_check_is_eth_only() {
        ensure_eth_only_symbol("ETHUSDT").unwrap();
        ensure_eth_only_symbol("ETH-USDT-SWAP").unwrap();
        assert!(ensure_eth_only_symbol("LINKUSDT").is_err());
        assert!(ensure_eth_only_symbol("BTCUSDT").is_err());
    }

    #[test]
    fn derived_stop_price_uses_configured_trigger_factor() {
        assert_eq!(derived_stop_price("2400.12", 0.5).unwrap(), "1200.06");
        assert_eq!(derived_stop_price("2400", 1.5).unwrap(), "3600.00");
        assert!(derived_stop_price("0", 0.5).is_err());
    }

    #[test]
    fn protective_position_side_infers_binance_hedge_side_from_read_only_positions() {
        let mut config = ProtectiveOutcomeCheckConfig {
            exchange: ExchangeId::Binance,
            symbol: "ETHUSDT".to_string(),
            side: OrderSide::Sell,
            position_side: None,
            stop_price: None,
            trigger_factor: 0.5,
            client_order_id: "rq-protect-outcome-test".to_string(),
        };
        assert_eq!(
            protective_position_side(&config, &[position("LONG"), position("SHORT")]).as_deref(),
            Some("LONG")
        );

        config.side = OrderSide::Buy;
        assert_eq!(
            protective_position_side(&config, &[position("LONG"), position("SHORT")]).as_deref(),
            Some("SHORT")
        );
    }

    #[test]
    fn protective_position_side_omits_one_way_both_side() {
        let config = ProtectiveOutcomeCheckConfig {
            exchange: ExchangeId::Binance,
            symbol: "ETHUSDT".to_string(),
            side: OrderSide::Sell,
            position_side: None,
            stop_price: None,
            trigger_factor: 0.5,
            client_order_id: "rq-protect-outcome-test".to_string(),
        };

        assert_eq!(
            protective_position_side(&config, &[position("BOTH")]).as_deref(),
            None
        );
    }

    #[test]
    fn protective_position_side_prefers_explicit_env_value() {
        let config = ProtectiveOutcomeCheckConfig {
            exchange: ExchangeId::Binance,
            symbol: "ETHUSDT".to_string(),
            side: OrderSide::Sell,
            position_side: Some("BOTH".to_string()),
            stop_price: None,
            trigger_factor: 0.5,
            client_order_id: "rq-protect-outcome-test".to_string(),
        };

        assert_eq!(
            protective_position_side(&config, &[position("LONG"), position("SHORT")]).as_deref(),
            Some("BOTH")
        );
    }

    #[test]
    fn protective_outcome_position_preflight_rejects_flat_account_before_live_mutation() {
        let error = protective_outcome_position_preflight(&[position("LONG"), position("SHORT")])
            .expect_err("flat account must not create a close-position protective order");

        assert!(error
            .to_string()
            .contains("requires an existing ETHUSDT position"));
    }

    #[test]
    fn protective_outcome_position_preflight_allows_single_existing_position() {
        let count = protective_outcome_position_preflight(&[
            position_with_size("LONG", "0.001"),
            position("SHORT"),
        ])
        .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn protective_outcome_side_must_match_existing_position_direction() {
        let mut config = ProtectiveOutcomeCheckConfig {
            exchange: ExchangeId::Binance,
            symbol: "ETHUSDT".to_string(),
            side: OrderSide::Sell,
            position_side: None,
            stop_price: None,
            trigger_factor: 0.5,
            client_order_id: "rq-protect-outcome-test".to_string(),
        };

        ensure_protective_side_matches_existing_position(
            &config,
            &[position_with_size("LONG", "0.001")],
        )
        .expect("sell protective order may close long");

        config.side = OrderSide::Buy;
        let long_error = ensure_protective_side_matches_existing_position(
            &config,
            &[position_with_size("LONG", "0.001")],
        )
        .expect_err("buy protective order must not be attached to long");
        assert!(long_error
            .to_string()
            .contains("LONG position requires SELL"));

        config.side = OrderSide::Sell;
        let short_error = ensure_protective_side_matches_existing_position(
            &config,
            &[position_with_size("SHORT", "0.001")],
        )
        .expect_err("sell protective order must not be attached to short");
        assert!(short_error
            .to_string()
            .contains("SHORT position requires BUY"));
    }

    #[test]
    fn protective_stop_price_must_not_immediately_trigger() {
        validate_stop_price_not_immediate(OrderSide::Sell, "2399.99", "2400.00")
            .expect("sell stop below market is valid");
        validate_stop_price_not_immediate(OrderSide::Buy, "2400.01", "2400.00")
            .expect("buy stop above market is valid");

        assert!(
            validate_stop_price_not_immediate(OrderSide::Sell, "2400.00", "2400.00")
                .expect_err("sell stop at market would trigger immediately")
                .to_string()
                .contains("would trigger immediately")
        );
        assert!(
            validate_stop_price_not_immediate(OrderSide::Buy, "2399.99", "2400.00")
                .expect_err("buy stop below market would trigger immediately")
                .to_string()
                .contains("would trigger immediately")
        );
    }
}
