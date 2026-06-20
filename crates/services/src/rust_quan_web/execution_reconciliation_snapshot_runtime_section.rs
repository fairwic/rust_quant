pub async fn run_reconciliation_snapshot_check_from_env() -> Result<Value> {
    let config = ReconciliationSnapshotCheckConfig::from_env()?;
    run_reconciliation_snapshot_check(config).await
}

pub async fn run_account_snapshot_sync(config: AccountSnapshotSyncConfig) -> Result<Value> {
    run_reconciliation_snapshot_check(config.into_reconciliation_config()).await
}

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
    let order_history = gateway
        .order_history(
            config.exchange,
            OrderListQuery::for_instrument(instrument.clone()).with_limit(100),
        )
        .await
        .map_err(|error| {
            anyhow!(
                "signed read-only order-history snapshot failed: {}",
                redact_error_message(error.to_string())
            )
        })?;
    let fills = if config.include_fills {
        gateway
            .fills(
                config.exchange,
                FillListQuery::for_instrument(instrument.clone()).with_limit(20),
            )
            .await
            .map_err(|error| {
                anyhow!(
                    "signed read-only fills snapshot failed: {}",
                    redact_error_message(error.to_string())
                )
            })?
    } else {
        Vec::new()
    };
    let balances = gateway.balances(config.exchange).await.map_err(|error| {
        anyhow!(
            "signed read-only balance snapshot failed: {}",
            redact_error_message(error.to_string())
        )
    })?;
    let account_bills = if config.exchange == ExchangeId::Okx {
        let now = chrono::Utc::now();
        let start_time = (now - chrono::Duration::days(30)).timestamp_millis() as u64;
        gateway
            .account_bills(
                config.exchange,
                AccountBillQuery::for_instrument(instrument)
                    .with_start_time(start_time)
                    .with_end_time(now.timestamp_millis() as u64)
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
    )?;
    let account_snapshot_counts = json!({
        "orders": account_snapshot_request.orders.len(),
        "trades": account_snapshot_request.trades.len(),
        "positions": account_snapshot_request.positions.len(),
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
