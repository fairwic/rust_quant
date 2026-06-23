use super::*;

fn dec(raw: &str) -> Decimal {
    Decimal::from_str(raw).unwrap()
}

fn sample_filters() -> BinanceSymbolFilters {
    BinanceSymbolFilters {
        quantity_step: dec("0.001"),
        min_quantity: dec("0.001"),
        min_notional: dec("5"),
        price_tick: dec("0.01"),
    }
}

fn sample_symbol_config(leverage: &str) -> Value {
    json!([{"symbol": DEFAULT_EXCHANGE_SYMBOL, "leverage": leverage}])
}

fn sample_plan(position_mode: BinancePositionMode) -> BinanceEthMicroPreparedPlan {
    BinanceEthMicroPreparedPlan {
        exchange_symbol: DEFAULT_EXCHANGE_SYMBOL.to_string(),
        web_symbol: DEFAULT_WEB_SYMBOL.to_string(),
        qty: dec("0.010"),
        mark_price: dec("3500"),
        notional: dec("35"),
        stop_loss_price: dec("3430"),
        position_mode,
        available_usdt: dec("100"),
        filters: sample_filters(),
    }
}

#[test]
fn normalize_symbol_is_eth_only() {
    assert_eq!(normalize_eth_symbol("ETHUSDT").unwrap(), DEFAULT_WEB_SYMBOL);
    assert_eq!(
        normalize_eth_symbol("ETH-USDT-SWAP").unwrap(),
        DEFAULT_WEB_SYMBOL
    );
    assert!(normalize_eth_symbol("LINKUSDT").is_err());
}

#[test]
fn open_payload_carries_exact_credential_and_protective_stop() {
    let payload = build_open_task_payload(8801, &sample_plan(BinancePositionMode::OneWay));
    assert_eq!(payload["api_credential_id"], 8801);
    assert_eq!(payload["exchange"], "binance");
    assert_eq!(payload["symbol"], DEFAULT_WEB_SYMBOL);
    assert_eq!(payload["side"], "buy");
    assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    assert_eq!(payload["risk_plan"]["entry_price"], "3500");
    assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], "3430");
    assert_eq!(payload["risk_plan"]["direction"], "long");
    assert_eq!(payload["protective_stop_loss_required"], true);
}

#[test]
fn close_payload_is_reduce_only_in_one_way_mode() {
    let payload = build_close_task_payload(8801, 42, &sample_plan(BinancePositionMode::OneWay));
    assert_eq!(payload["api_credential_id"], 8801);
    assert_eq!(payload["source_open_task_id"], 42);
    assert_eq!(payload["close_order"]["side"], "sell");
    assert_eq!(payload["close_order"]["reduce_only"], true);
    assert!(payload["close_order"].get("position_side").is_none());
}

#[test]
fn close_payload_uses_position_side_in_hedge_mode() {
    let payload = build_close_task_payload(8801, 42, &sample_plan(BinancePositionMode::Hedge));
    assert_eq!(payload["close_order"]["position_side"], "LONG");
    assert!(payload["close_order"].get("reduce_only").is_none());
}

#[test]
fn worker_config_pins_single_live_task() {
    let config = build_worker_config(42, "execute_signal", "pending").unwrap();
    assert!(!config.dry_run);
    assert_eq!(config.default_exchange, ExchangeId::Binance);
    assert_eq!(config.lease_limit, 1);
    assert_eq!(config.target_task_ids, vec![42]);
    assert_eq!(config.task_types, vec!["execute_signal"]);
    assert_eq!(config.task_statuses, vec!["pending"]);
}

#[test]
fn worker_env_manifest_documents_live_scope() {
    let env = build_worker_env_manifest(42, "execute_signal", "pending").unwrap();
    assert_eq!(env["EXECUTION_WORKER_DRY_RUN"], "false");
    assert_eq!(env["EXECUTION_WORKER_TARGET_TASK_IDS"], "42");
    assert_eq!(env["EXECUTION_WORKER_LEASE_LIMIT"], "1");
    assert_eq!(
        env["EXECUTION_WORKER_LIVE_ORDER_CONFIRM"],
        EXECUTION_WORKER_CONFIRM_TOKEN
    );
}

#[test]
fn confirmation_token_is_exact() {
    let mut config = BinanceEthMicroLiveValidationConfig {
        web_database_url: "postgres://web".to_string(),
        quant_core_database_url: "postgres://core".to_string(),
        web_base_url: "http://127.0.0.1:8000".to_string(),
        internal_secret: "secret".to_string(),
        buyer_email: "buyer@example.com".to_string(),
        strategy_slug: "vegas".to_string(),
        strategy_key: "vegas_eth_micro_live_validation".to_string(),
        web_symbol: DEFAULT_WEB_SYMBOL.to_string(),
        qty: dec("0.010"),
        credential_id: Some(1),
        combo_id: Some(2),
        stop_loss_price: None,
        stop_loss_bps: DEFAULT_STOP_LOSS_BPS,
        margin_buffer_bps: DEFAULT_MARGIN_BUFFER_BPS,
        binance_fapi_base_url: DEFAULT_BINANCE_FAPI_BASE_URL.to_string(),
        proxy_url: None,
        apply: true,
        confirm: Some("wrong".to_string()),
    };
    assert!(ensure_live_apply_confirmation(&config).is_err());
    config.confirm = Some(BINANCE_ETH_MICRO_CONFIRM_TOKEN.to_string());
    assert!(ensure_live_apply_confirmation(&config).is_ok());
}

#[test]
fn config_accepts_socks_proxy_for_binance_http() {
    assert_eq!(
        normalize_proxy_url("BINANCE_PROXY_URL", "socks5h://127.0.0.1:7897".to_string()).unwrap(),
        "socks5h://127.0.0.1:7897"
    );
}

#[test]
fn config_rejects_unsupported_proxy_scheme() {
    let error = normalize_proxy_url("BINANCE_PROXY_URL", "ftp://127.0.0.1:7897".to_string())
        .expect_err("unsupported proxy scheme must fail closed");
    assert!(error.to_string().contains("BINANCE_PROXY_URL must start"));
}

#[test]
fn prepared_plan_refuses_existing_position_or_open_orders() {
    let config = BinanceEthMicroLiveValidationConfig {
        web_database_url: "postgres://web".to_string(),
        quant_core_database_url: "postgres://core".to_string(),
        web_base_url: "http://127.0.0.1:8000".to_string(),
        internal_secret: "secret".to_string(),
        buyer_email: "buyer@example.com".to_string(),
        strategy_slug: "vegas".to_string(),
        strategy_key: "vegas_eth_micro_live_validation".to_string(),
        web_symbol: DEFAULT_WEB_SYMBOL.to_string(),
        qty: dec("0.010"),
        credential_id: Some(1),
        combo_id: Some(2),
        stop_loss_price: None,
        stop_loss_bps: DEFAULT_STOP_LOSS_BPS,
        margin_buffer_bps: DEFAULT_MARGIN_BUFFER_BPS,
        binance_fapi_base_url: DEFAULT_BINANCE_FAPI_BASE_URL.to_string(),
        proxy_url: None,
        apply: false,
        confirm: None,
    };
    let account = json!({
        "positions": [{"symbol": "ETHUSDT", "positionAmt": "0.001"}],
        "assets": [{"asset": "USDT", "availableBalance": "100"}]
    });
    assert!(build_prepared_plan(
        &config,
        sample_filters(),
        dec("3500"),
        &account,
        &json!([]),
        &sample_symbol_config("30"),
        BinancePositionMode::OneWay,
    )
    .is_err());
    let flat_account = json!({
        "positions": [{"symbol": "ETHUSDT", "positionAmt": "0"}],
        "assets": [{"asset": "USDT", "availableBalance": "100"}]
    });
    assert!(build_prepared_plan(
        &config,
        sample_filters(),
        dec("3500"),
        &flat_account,
        &json!([{"orderId": 1}]),
        &sample_symbol_config("30"),
        BinancePositionMode::OneWay,
    )
    .is_err());
}

#[test]
fn prepared_plan_uses_futures_initial_margin_instead_of_full_notional() {
    let config = BinanceEthMicroLiveValidationConfig {
        web_database_url: "postgres://web".to_string(),
        quant_core_database_url: "postgres://core".to_string(),
        web_base_url: "http://127.0.0.1:8000".to_string(),
        internal_secret: "secret".to_string(),
        buyer_email: "buyer@example.com".to_string(),
        strategy_slug: "vegas".to_string(),
        strategy_key: "vegas_eth_micro_live_validation".to_string(),
        web_symbol: DEFAULT_WEB_SYMBOL.to_string(),
        qty: dec("0.012"),
        credential_id: Some(1),
        combo_id: Some(2),
        stop_loss_price: None,
        stop_loss_bps: DEFAULT_STOP_LOSS_BPS,
        margin_buffer_bps: DEFAULT_MARGIN_BUFFER_BPS,
        binance_fapi_base_url: DEFAULT_BINANCE_FAPI_BASE_URL.to_string(),
        proxy_url: None,
        apply: false,
        confirm: None,
    };
    let account = json!({
        "positions": [{"symbol": "ETHUSDT", "positionAmt": "0"}],
        "assets": [{"asset": "USDT", "availableBalance": "2.68"}]
    });
    let prepared = build_prepared_plan(
        &config,
        sample_filters(),
        dec("1727.06"),
        &account,
        &json!([]),
        &sample_symbol_config("30"),
        BinancePositionMode::OneWay,
    )
    .unwrap();

    assert_eq!(prepared.qty, dec("0.012"));
    assert_eq!(prepared.notional.round_dp(4), dec("20.7247"));
}
