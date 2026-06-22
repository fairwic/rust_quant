pub async fn run_reconciliation_snapshot_check_from_env() -> Result<Value> {
    let config = ReconciliationSnapshotCheckConfig::from_env()?;
    run_reconciliation_snapshot_check(config).await
}

pub async fn run_account_snapshot_sync(config: AccountSnapshotSyncConfig) -> Result<Value> {
    if config.account_wide {
        return run_account_wide_snapshot_sync(config).await;
    }
    run_reconciliation_snapshot_check(config.into_reconciliation_config()).await
}

const OKX_HISTORY_PAGE_LIMIT: u32 = 100;
const OKX_HISTORY_MAX_PAGES: usize = 20;

async fn run_reconciliation_snapshot_check(
    config: ReconciliationSnapshotCheckConfig,
) -> Result<Value> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_default();
    let has_internal_secret = !internal_secret.trim().is_empty();
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })?;

    let instrument = parse_instrument(&config.symbol)?;
    let user_config = client
        .resolve_user_exchange_config(&config.buyer_email, config.exchange.as_str())
        .await?;
    let gateway = CryptoExcAllGateway::from_single_exchange_credentials(
        config.exchange,
        user_config.api_key,
        user_config.api_secret,
        user_config.passphrase,
        user_config.simulated,
    )?;
    let okx_history_window = if config.exchange == ExchangeId::Okx {
        let now = chrono::Utc::now();
        Some((
            (now - chrono::Duration::days(90)).timestamp_millis() as u64,
            now.timestamp_millis() as u64,
        ))
    } else {
        None
    };

    let positions = gateway
        .positions(config.exchange, Some(&instrument))
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only position snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let open_orders = gateway
        .open_orders(
            config.exchange,
            OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
        )
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only open-orders snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let order_history = match okx_history_window {
        Some((start_time, end_time)) => {
            fetch_okx_order_history_pages(
                &gateway,
                config.exchange,
                Some(&instrument),
                start_time,
                end_time,
            )
            .await
        }
        None => gateway
            .order_history(
                config.exchange,
                OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
            )
            .await
            .map_err(anyhow::Error::from),
    }
    .map_err(|error| {
        anyhow!(
            "signed read-only order-history snapshot failed: {}",
            redact_error_message(error.to_string())
        )
    })?;
    let fills = if config.include_fills {
        match okx_history_window {
            Some((start_time, end_time)) => {
                fetch_okx_fill_history_pages(
                    &gateway,
                    config.exchange,
                    Some(&instrument),
                    start_time,
                    end_time,
                )
                .await
            }
            None => gateway
                .fills(
                    config.exchange,
                    FillListQuery::for_instrument(instrument.clone()).with_limit(20),
                )
                .await
                .map_err(anyhow::Error::from),
        }
        .map_err(|error| {
            anyhow!(
                "signed read-only fills snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?
    } else {
        Vec::new()
    };
    let position_history = match okx_history_window {
        Some((start_time, end_time)) => {
            fetch_okx_position_history_pages(
                &gateway,
                config.exchange,
                Some(&instrument),
                start_time,
                end_time,
            )
            .await
        }
        None => gateway
            .position_history(
                config.exchange,
                PositionHistoryQuery::for_instrument(instrument.clone()).with_limit(100),
            )
            .await
            .map_err(anyhow::Error::from),
    }
    .map_err(|error| {
        anyhow!(
            "signed read-only position-history snapshot failed: {}",
            redact_error_message(error.to_string())
        )
    })?;
    let balances = gateway.balances(config.exchange).await.map_err(|error| {
        anyhow!(
            "signed read-only balance snapshot failed: {}",
            redact_error_message(error.to_string())
        )
    })?;
    let account_bills = if config.exchange == ExchangeId::Okx {
        let now = chrono::Utc::now();
        let start_time = okx_history_window
            .map(|(start_time, _)| start_time)
            .unwrap_or_else(|| (now - chrono::Duration::days(90)).timestamp_millis() as u64);
        let end_time = okx_history_window
            .map(|(_, end_time)| end_time)
            .unwrap_or_else(|| now.timestamp_millis() as u64);
        gateway
            .account_bills(
                config.exchange,
                AccountBillQuery::for_instrument(instrument)
                    .with_start_time(start_time)
                    .with_end_time(end_time)
                    .with_limit(100)
                    .with_archive(true),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "signed read-only account bills snapshot failed: {}",
                    redact_error_message(error.to_string())
                )
            })?
    } else {
        Vec::new()
    };

    let requests = build_reconciliation_snapshot_requests(&config, &positions, &open_orders);
    let close_fill_writeback_candidates =
        build_close_fill_writeback_candidates(&config, &positions, &open_orders, &fills);
    let account_snapshot_request = build_exchange_account_snapshot_report_request(
        &config,
        &positions,
        &open_orders,
        &order_history,
        &fills,
        &balances,
        &account_bills,
        &position_history,
    )?;
    let account_snapshot_counts = json!({
        "orders": account_snapshot_request.orders.len(),
        "trades": account_snapshot_request.trades.len(),
        "positions": account_snapshot_request.positions.len(),
        "position_history": account_snapshot_request.position_history.len(),
        "balances": account_snapshot_request.balances.len(),
        "bills": account_snapshot_request.bills.len(),
        "source_ref": account_snapshot_request.source_ref,
    });
    let account_snapshot_response = if has_internal_secret {
        Some(report_exchange_account_snapshot(&client, account_snapshot_request).await?)
    } else {
        None
    };
    let mut close_fill_writeback_responses = Vec::new();
    if config.close_fill_writeback_apply {
        if !has_internal_secret {
            bail!(
                "EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required before close-fill writeback apply"
            );
        }
        if close_fill_writeback_candidates.len() != 1 {
            bail!(
                "close-fill writeback apply requires exactly one candidate, found {}",
                close_fill_writeback_candidates.len()
            );
        }
        let request = build_close_fill_writeback_request_from_candidate(
            &config,
            &close_fill_writeback_candidates[0],
        )?;
        let response = apply_close_fill_writeback(&client, request).await?;
        close_fill_writeback_responses.push(json!({
            "order_result_id": response.order_result.id,
            "external_order_id": response.order_result.external_order_id,
            "order_side": response.order_result.order_side,
            "order_status": response.order_result.order_status,
            "trade_record": response.trade_record,
            "position_snapshot_cleared": response.position_snapshot_cleared,
            "exchange_mutation_allowed": false,
        }));
    }
    let mut report_responses = Vec::new();
    if config.report_reconciliation {
        for request in &requests {
            report_responses.push(report_reconciliation(&client, request).await?);
        }
    }

    Ok(json!({
        "exchange": config.exchange.as_str(),
        "symbol": config.symbol,
        "combo_id": config.combo_id,
        "task_id": config.task_id,
        "position_count": positions.len(),
        "position_history_count": position_history.len(),
        "non_zero_position_count": non_zero_position_count(&positions),
        "open_order_count": open_orders.len(),
        "order_history_count": order_history.len(),
        "active_open_order_count": active_open_order_count(&open_orders),
        "issue_count": requests.len(),
        "reconciliation_report_enabled": config.report_reconciliation,
        "reported_issue_count": report_responses.len(),
        "position_summaries": position_summaries(&positions),
        "open_order_summaries": open_order_summaries(&open_orders),
        "order_history_summaries": open_order_summaries(&order_history),
        "fill_snapshot_enabled": config.include_fills,
        "fill_count": fills.len(),
        "fill_summaries": fill_summaries(&fills),
        "close_fill_writeback_candidates": close_fill_writeback_candidates,
        "close_fill_writeback_apply_enabled": config.close_fill_writeback_apply,
        "close_fill_writeback_apply_count": close_fill_writeback_responses.len(),
        "close_fill_writeback_responses": close_fill_writeback_responses,
        "account_snapshot_counts": account_snapshot_counts,
        "account_snapshot_writeback_enabled": has_internal_secret,
        "account_snapshot_writeback_response": account_snapshot_response,
        "source_refs": requests
            .iter()
            .filter_map(|request| request.source_ref.clone())
            .collect::<Vec<_>>(),
        "place_order_allowed": false,
        "mutation_allowed": false,
        "report_result_allowed": false,
    }))
}

async fn run_account_wide_snapshot_sync(config: AccountSnapshotSyncConfig) -> Result<Value> {
    if config.exchange != ExchangeId::Okx {
        bail!("account-wide exchange account snapshot sync currently supports OKX only");
    }

    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_default();
    let has_internal_secret = !internal_secret.trim().is_empty();
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })?;

    let user_config = client
        .resolve_user_exchange_config(&config.buyer_email, config.exchange.as_str())
        .await?;
    let gateway = CryptoExcAllGateway::from_single_exchange_credentials(
        config.exchange,
        user_config.api_key,
        user_config.api_secret,
        user_config.passphrase,
        user_config.simulated,
    )?;

    let now = chrono::Utc::now();
    let start_time = (now - chrono::Duration::days(90)).timestamp_millis() as u64;
    let end_time = now.timestamp_millis() as u64;
    let positions = gateway
        .positions(config.exchange, None)
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only account-wide position snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let open_orders = gateway
        .open_orders(config.exchange, OrderListQuery::new().with_limit(100))
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only account-wide open-orders snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let order_history =
        fetch_okx_order_history_pages(&gateway, config.exchange, None, start_time, end_time)
            .await
            .map_err(|error| {
                anyhow!(
                    "signed read-only account-wide order-history snapshot failed: {}",
                    redact_error_message(error.to_string())
                )
            })?;
    let fills = if config.include_fills {
        fetch_okx_fill_history_pages(&gateway, config.exchange, None, start_time, end_time)
            .await
            .map_err(|error| {
                anyhow!(
                    "signed read-only account-wide fills snapshot failed: {}",
                    redact_error_message(error.to_string())
                )
            })?
    } else {
        Vec::new()
    };
    let position_history =
        fetch_okx_position_history_pages(&gateway, config.exchange, None, start_time, end_time)
            .await
            .map_err(|error| {
                anyhow!(
                    "signed read-only account-wide position-history snapshot failed: {}",
                    redact_error_message(error.to_string())
                )
            })?;
    let balances = gateway.balances(config.exchange).await.map_err(|error| {
        anyhow!(
            "signed read-only account-wide balance snapshot failed: {}",
            redact_error_message(error.to_string())
        )
    })?;
    let account_bills = gateway
        .account_bills(
            config.exchange,
            AccountBillQuery::new()
                .with_inst_type("SWAP")
                .with_start_time(start_time)
                .with_end_time(end_time)
                .with_limit(100)
                .with_archive(true),
        )
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only account-wide account bills snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;

    let symbols = account_snapshot_symbols(
        &positions,
        &open_orders,
        &order_history,
        &fills,
        &account_bills,
        &position_history,
    );
    let empty_balances = Vec::new();
    let mut account_snapshot_counts = Vec::new();
    let mut account_snapshot_responses = Vec::new();
    for (index, symbol) in symbols.iter().enumerate() {
        let symbol_config = ReconciliationSnapshotCheckConfig {
            buyer_email: config.buyer_email.clone(),
            exchange: config.exchange,
            symbol: symbol.clone(),
            combo_id: 0,
            task_id: 0,
            credential_ref: config.credential_ref.clone(),
            report_reconciliation: false,
            include_fills: config.include_fills,
            close_fill_writeback_apply: false,
            close_fill_writeback_intent: None,
        };
        let balances_for_symbol = if index == 0 {
            balances.as_slice()
        } else {
            empty_balances.as_slice()
        };
        let account_snapshot_request = build_exchange_account_snapshot_report_request(
            &symbol_config,
            &positions,
            &open_orders,
            &order_history,
            &fills,
            balances_for_symbol,
            &account_bills,
            &position_history,
        )?;
        account_snapshot_counts.push(json!({
            "symbol": symbol,
            "orders": account_snapshot_request.orders.len(),
            "trades": account_snapshot_request.trades.len(),
            "positions": account_snapshot_request.positions.len(),
            "position_history": account_snapshot_request.position_history.len(),
            "balances": account_snapshot_request.balances.len(),
            "bills": account_snapshot_request.bills.len(),
            "source_ref": account_snapshot_request.source_ref,
        }));
        if has_internal_secret {
            account_snapshot_responses
                .push(report_exchange_account_snapshot(&client, account_snapshot_request).await?);
        }
    }

    Ok(json!({
        "exchange": config.exchange.as_str(),
        "account_wide": true,
        "symbol": config.symbol,
        "combo_id": 0,
        "task_id": 0,
        "symbol_count": symbols.len(),
        "symbols": symbols,
        "position_count": positions.len(),
        "position_history_count": position_history.len(),
        "non_zero_position_count": non_zero_position_count(&positions),
        "open_order_count": open_orders.len(),
        "order_history_count": order_history.len(),
        "active_open_order_count": active_open_order_count(&open_orders),
        "fill_snapshot_enabled": config.include_fills,
        "fill_count": fills.len(),
        "account_bill_count": account_bills.len(),
        "account_snapshot_counts": account_snapshot_counts,
        "account_snapshot_writeback_enabled": has_internal_secret,
        "account_snapshot_writeback_responses": account_snapshot_responses,
        "place_order_allowed": false,
        "mutation_allowed": false,
        "report_result_allowed": false,
    }))
}

async fn fetch_okx_order_history_pages(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    instrument: Option<&crypto_exc_all::Instrument>,
    start_time: u64,
    end_time: u64,
) -> Result<Vec<Order>> {
    let mut orders = Vec::new();
    let mut after = None;
    for _ in 0..OKX_HISTORY_MAX_PAGES {
        let mut query = instrument
            .cloned()
            .map(OrderListQuery::for_instrument)
            .unwrap_or_else(OrderListQuery::new)
            .with_start_time(start_time)
            .with_end_time(end_time)
            .with_limit(OKX_HISTORY_PAGE_LIMIT);
        if let Some(cursor) = after {
            query = query.with_after(cursor);
        }
        let page = gateway.order_history(exchange, query).await?;
        let next_after = next_okx_order_history_after_cursor(&page, OKX_HISTORY_PAGE_LIMIT);
        orders.extend(page);
        let Some(cursor) = next_after else {
            return Ok(orders);
        };
        after = Some(cursor);
    }
    bail!("OKX order history pagination exceeded {OKX_HISTORY_MAX_PAGES} pages");
}

async fn fetch_okx_fill_history_pages(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    instrument: Option<&crypto_exc_all::Instrument>,
    start_time: u64,
    end_time: u64,
) -> Result<Vec<Fill>> {
    let mut fills = Vec::new();
    let mut after = None;
    for _ in 0..OKX_HISTORY_MAX_PAGES {
        let mut query = instrument
            .cloned()
            .map(FillListQuery::for_instrument)
            .unwrap_or_else(FillListQuery::new)
            .with_start_time(start_time)
            .with_end_time(end_time)
            .with_limit(OKX_HISTORY_PAGE_LIMIT);
        if let Some(cursor) = after {
            query = query.with_after(cursor);
        }
        let page = gateway.fills(exchange, query).await?;
        let next_after = next_okx_fill_history_after_cursor(&page, OKX_HISTORY_PAGE_LIMIT);
        fills.extend(page);
        let Some(cursor) = next_after else {
            return Ok(fills);
        };
        after = Some(cursor);
    }
    bail!("OKX fill history pagination exceeded {OKX_HISTORY_MAX_PAGES} pages");
}

async fn fetch_okx_position_history_pages(
    gateway: &CryptoExcAllGateway,
    exchange: ExchangeId,
    instrument: Option<&crypto_exc_all::Instrument>,
    start_time: u64,
    end_time: u64,
) -> Result<Vec<PositionHistory>> {
    let mut positions = Vec::new();
    let mut after = Some(end_time.to_string());
    let before = start_time.to_string();
    for _ in 0..OKX_HISTORY_MAX_PAGES {
        let mut query = instrument
            .cloned()
            .map(PositionHistoryQuery::for_instrument)
            .unwrap_or_else(PositionHistoryQuery::new)
            .with_instrument_type("SWAP")
            .with_before(before.clone())
            .with_limit(OKX_HISTORY_PAGE_LIMIT);
        if let Some(cursor) = after {
            query = query.with_after(cursor);
        }
        let page = gateway.position_history(exchange, query).await?;
        let next_after = next_okx_position_history_after_cursor(&page, OKX_HISTORY_PAGE_LIMIT);
        positions.extend(page);
        let Some(cursor) = next_after else {
            return Ok(positions);
        };
        after = Some(cursor);
    }
    bail!("OKX position history pagination exceeded {OKX_HISTORY_MAX_PAGES} pages");
}

fn account_snapshot_symbols(
    positions: &[Position],
    open_orders: &[Order],
    order_history: &[Order],
    fills: &[Fill],
    account_bills: &[AccountBill],
    position_history: &[PositionHistory],
) -> Vec<String> {
    let mut symbols = BTreeSet::new();
    for position in positions {
        insert_account_snapshot_symbol(&mut symbols, &position.exchange_symbol);
    }
    for order in open_orders.iter().chain(order_history.iter()) {
        insert_account_snapshot_symbol(&mut symbols, &order.exchange_symbol);
    }
    for fill in fills {
        insert_account_snapshot_symbol(&mut symbols, &fill.exchange_symbol);
    }
    for bill in account_bills {
        if let Some(symbol) = bill.exchange_symbol.as_deref() {
            insert_account_snapshot_symbol(&mut symbols, symbol);
        }
    }
    for position in position_history {
        insert_account_snapshot_symbol(&mut symbols, &position.exchange_symbol);
    }
    if symbols.is_empty() {
        symbols.insert("ACCOUNT-WIDE".to_string());
    }
    symbols.into_iter().collect()
}

fn insert_account_snapshot_symbol(symbols: &mut BTreeSet<String>, symbol: &str) {
    let symbol = symbol.trim();
    if !symbol.is_empty() {
        symbols.insert(symbol.to_ascii_uppercase());
    }
}

fn next_okx_order_history_after_cursor(page: &[Order], limit: u32) -> Option<String> {
    if page.len() < limit as usize {
        return None;
    }
    page.last()?.order_id.clone()
}

fn next_okx_fill_history_after_cursor(page: &[Fill], limit: u32) -> Option<String> {
    if page.len() < limit as usize {
        return None;
    }
    page.last()?.trade_id.clone()
}

fn next_okx_position_history_after_cursor(
    page: &[PositionHistory],
    limit: u32,
) -> Option<String> {
    if page.len() < limit as usize {
        return None;
    }
    page.last()?.close_time.map(|value| value.to_string())
}

async fn report_exchange_account_snapshot(
    client: &ExecutionTaskClient,
    request: ExchangeAccountSnapshotReportRequest,
) -> Result<ExchangeAccountSnapshotReportResponse> {
    client
        .report_exchange_account_snapshot(request)
        .await
        .map_err(|error| {
            anyhow!(
                "report exchange account snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })
}

async fn apply_close_fill_writeback(
    client: &ExecutionTaskClient,
    request: ExchangeCloseFillWritebackRequest,
) -> Result<ExchangeCloseFillWritebackResponse> {
    client
        .apply_exchange_close_fill_writeback(request)
        .await
        .map_err(|error| {
            anyhow!(
                "apply exchange close-fill writeback failed: {}",
                redact_error_message(error.to_string())
            )
        })
}

async fn report_reconciliation(
    client: &ExecutionTaskClient,
    request: &ExchangeReconciliationReportRequest,
) -> Result<Value> {
    let response = client
        .report_exchange_reconciliation(request.clone())
        .await
        .map_err(|error| {
            anyhow!(
                "report exchange reconciliation failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    Ok(json!({
        "combo_id": response.combo_id,
        "symbol": response.symbol,
        "signal_id": response.signal_id,
        "issue_type": response.issue_type,
        "api_execution_status": response.api_execution_status,
    }))
}
