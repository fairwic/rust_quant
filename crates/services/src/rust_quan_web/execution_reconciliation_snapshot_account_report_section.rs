pub fn build_exchange_account_snapshot_report_request(
    config: &ReconciliationSnapshotCheckConfig,
    positions: &[Position],
    open_orders: &[Order],
    order_history: &[Order],
    fills: &[Fill],
    balances: &[Balance],
    account_bills: &[AccountBill],
) -> Result<ExchangeAccountSnapshotReportRequest> {
    let snapshot_at = web_naive_datetime_string(chrono::Utc::now().naive_utc());
    let source_ref = exchange_account_snapshot_source_ref(config);
    let mut orders = Vec::new();
    for order in open_orders.iter().chain(order_history.iter()) {
        if order.exchange != config.exchange
            || !same_exchange_symbol(&order.exchange_symbol, &config.symbol)
        {
            continue;
        }
        let Some(external_order_id) = trimmed_optional(order.order_id.as_deref()) else {
            continue;
        };
        let order_side =
            trimmed_optional(order.side.as_deref()).unwrap_or_else(|| "unknown".to_string());
        let order_status =
            trimmed_optional(order.status.as_deref()).unwrap_or_else(|| "unknown".to_string());
        let price = decimal_option(order.average_price.as_deref())
            .or_else(|| decimal_option(order.price.as_deref()));
        let filled_qty = decimal_option(order.filled_size.as_deref());
        let filled_quote = multiply_optional(price, filled_qty);
        let observed_at = order
            .updated_at
            .or(order.created_at)
            .and_then(timestamp_millis_to_naive_string)
            .or_else(|| Some(snapshot_at.clone()));

        orders.push(ExchangeAccountOrderSnapshotInput {
            external_order_id,
            order_side,
            order_status,
            price,
            filled_qty,
            filled_quote,
            fee_amount: None,
            raw_payload_json: Some(
                json!({
                    "source": "signed_read_only_account_snapshot",
                    "kind": "order",
                    "exchange": order.exchange.as_str(),
                    "symbol": order.exchange_symbol,
                    "order": order.raw,
                })
                .to_string(),
            ),
            observed_at,
        });
    }

    let trades = fills
        .iter()
        .filter(|fill| fill.exchange == config.exchange)
        .filter(|fill| same_exchange_symbol(&fill.exchange_symbol, &config.symbol))
        .filter_map(|fill| {
            let external_trade_id = trimmed_optional(fill.trade_id.as_deref())?;
            let side =
                trimmed_optional(fill.side.as_deref()).unwrap_or_else(|| "unknown".to_string());
            let price = decimal_option(fill.price.as_deref());
            let quantity = decimal_option(fill.size.as_deref());
            Some(ExchangeAccountTradeSnapshotInput {
                external_trade_id,
                external_order_id: trimmed_optional(fill.order_id.as_deref()),
                side,
                quantity,
                quote_amount: multiply_optional(price, quantity),
                fee_amount: decimal_option(fill.fee.as_deref()),
                price,
                raw_payload_json: Some(
                    json!({
                        "source": "signed_read_only_account_snapshot",
                        "kind": "fill",
                        "exchange": fill.exchange.as_str(),
                        "symbol": fill.exchange_symbol,
                        "fill": fill.raw,
                    })
                    .to_string(),
                ),
                executed_at: fill
                    .timestamp
                    .and_then(timestamp_millis_to_naive_string)
                    .or_else(|| Some(snapshot_at.clone())),
            })
        })
        .collect();

    let positions = positions
        .iter()
        .filter(|position| position.exchange == config.exchange)
        .filter(|position| same_exchange_symbol(&position.exchange_symbol, &config.symbol))
        .filter(|position| positive_decimal_text(&position.size))
        .filter_map(|position| {
            let quantity = decimal_option(Some(position.size.as_str()))?;
            let entry_price = decimal_option(position.entry_price.as_deref());
            let mark_price = decimal_option(position.mark_price.as_deref());
            let quote_amount = multiply_optional(mark_price.or(entry_price), Some(quantity));
            Some(ExchangeAccountPositionSnapshotInput {
                side: trimmed_optional(position.side.as_deref())
                    .unwrap_or_else(|| "unknown".to_string()),
                quantity,
                quote_amount,
                leverage: decimal_option(position.leverage.as_deref()),
                margin_mode: trimmed_optional(position.margin_mode.as_deref()),
                liquidation_price: decimal_option(position.liquidation_price.as_deref()),
                margin_ratio: None,
                unrealized_pnl: decimal_option(position.unrealized_pnl.as_deref()),
                protective_order_status: None,
                raw_payload_json: Some(
                    json!({
                        "source": "signed_read_only_account_snapshot",
                        "kind": "position",
                        "exchange": position.exchange.as_str(),
                        "symbol": position.exchange_symbol,
                        "position": position.raw,
                    })
                    .to_string(),
                ),
                snapshot_at: Some(snapshot_at.clone()),
            })
        })
        .collect();

    let balances = balances
        .iter()
        .filter(|balance| balance.exchange == config.exchange)
        .filter_map(|balance| {
            let asset = trimmed_optional(Some(balance.asset.as_str()))?.to_ascii_uppercase();
            let wallet_balance = decimal_option(Some(balance.total.as_str()));
            let available_balance = decimal_option(Some(balance.available.as_str()));
            if wallet_balance.is_none() && available_balance.is_none() {
                return None;
            }
            Some(ExchangeAccountBalanceSnapshotInput {
                equity_usdt: balance_equity_usdt(balance, wallet_balance),
                asset,
                wallet_balance,
                available_balance,
                raw_payload_json: Some(
                    json!({
                        "source": "signed_read_only_account_snapshot",
                        "kind": "balance",
                        "exchange": balance.exchange.as_str(),
                        "asset": balance.asset,
                        "balance": balance.raw,
                    })
                    .to_string(),
                ),
                snapshot_at: Some(snapshot_at.clone()),
            })
        })
        .collect();

    let bills = account_bills
        .iter()
        .filter(|bill| bill.exchange == config.exchange)
        .filter_map(|bill| {
            if let Some(exchange_symbol) = bill.exchange_symbol.as_deref() {
                if !same_exchange_symbol(exchange_symbol, &config.symbol) {
                    return None;
                }
            } else {
                return None;
            }
            let external_bill_id = trimmed_optional(bill.bill_id.as_deref())?;
            let asset = trimmed_optional(bill.asset.as_deref())?.to_ascii_uppercase();
            let balance_change = decimal_option(bill.balance_change.as_deref());
            let fee_amount = decimal_option(bill.fee.as_deref());
            let pnl_amount = decimal_option(bill.pnl.as_deref());
            if balance_change.is_none() && fee_amount.is_none() && pnl_amount.is_none() {
                return None;
            }
            Some(ExchangeAccountBillSnapshotInput {
                external_bill_id,
                asset,
                balance_change,
                balance_change_usdt: None,
                balance_after: decimal_option(bill.balance_after.as_deref()),
                fee_amount,
                fee_usdt: None,
                pnl_amount,
                pnl_usdt: None,
                bill_type: trimmed_optional(bill.bill_type.as_deref()),
                bill_sub_type: trimmed_optional(bill.bill_sub_type.as_deref()),
                external_order_id: trimmed_optional(bill.order_id.as_deref()),
                external_trade_id: trimmed_optional(bill.trade_id.as_deref()),
                raw_payload_json: Some(
                    json!({
                        "source": "signed_read_only_account_snapshot",
                        "kind": "bill",
                        "exchange": bill.exchange.as_str(),
                        "symbol": bill.exchange_symbol,
                        "bill": bill.raw,
                    })
                    .to_string(),
                ),
                bill_at: bill
                    .timestamp
                    .and_then(timestamp_millis_to_naive_string)
                    .or_else(|| Some(snapshot_at.clone())),
            })
        })
        .collect();

    Ok(ExchangeAccountSnapshotReportRequest {
        combo_id: config.combo_id,
        buyer_email: config.buyer_email.clone(),
        exchange: config.exchange.as_str().to_string(),
        symbol: config.symbol.clone(),
        source_ref,
        snapshot_at: Some(snapshot_at),
        orders,
        trades,
        positions,
        balances,
        bills,
    })
}
