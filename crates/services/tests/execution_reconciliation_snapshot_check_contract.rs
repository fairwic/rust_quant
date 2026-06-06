use crypto_exc_all::{ExchangeId, Fill, Instrument, Order, Position};
use rust_quant_services::rust_quan_web::{
    build_close_fill_writeback_candidates, build_close_fill_writeback_request_from_candidate,
    build_reconciliation_snapshot_requests, build_reconciliation_snapshot_task,
    ReconciliationSnapshotCheckConfig,
};
use serde_json::json;
use std::collections::HashMap;

fn config_from(values: &[(&str, &str)]) -> anyhow::Result<ReconciliationSnapshotCheckConfig> {
    let values: HashMap<String, String> = values
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    ReconciliationSnapshotCheckConfig::from_lookup(|key| values.get(key).cloned())
}

#[test]
fn reconciliation_snapshot_requires_explicit_signed_read_only_confirmation() {
    let err = config_from(&[
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ETHUSDT"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "7"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "42"),
    ])
    .unwrap_err();

    assert!(err.to_string().contains("RECONCILIATION_SNAPSHOT_CONFIRM"));
}

#[test]
fn reconciliation_snapshot_rejects_link_symbol() {
    let err = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "LINKUSDT"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "7"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "42"),
    ])
    .unwrap_err();

    assert!(err.to_string().contains("LINKUSDT"));
}

#[test]
fn reconciliation_snapshot_task_uses_secret_safe_v2_source_ref_context() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "Buyer@Example.COM"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "binance"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ETHUSDT"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "7"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "42"),
        ("RECONCILIATION_SNAPSHOT_CREDENTIAL_REF", "cred_live_001"),
    ])
    .unwrap();

    let task = build_reconciliation_snapshot_task(&config);
    let payload = task.request_payload_json;

    assert_eq!(task.id, 42);
    assert_eq!(task.combo_id, 7);
    assert_eq!(task.buyer_email, "Buyer@Example.COM");
    assert!(config.report_reconciliation);
    assert!(!config.include_fills);
    assert_eq!(payload["exchange"], "binance");
    assert_eq!(payload["symbol"], "ETHUSDT");
    assert_eq!(payload["credential_ref"], "cred_live_001");
    assert!(!payload.to_string().contains("Buyer@Example.COM"));

    let requests = build_reconciliation_snapshot_requests(
        &config,
        &[Position {
            exchange: ExchangeId::Binance,
            instrument: Instrument::perp("eth", "usdt").with_settlement("usdt"),
            exchange_symbol: "ETHUSDT".to_string(),
            side: Some("LONG".to_string()),
            size: "0.001".to_string(),
            entry_price: Some("2500".to_string()),
            mark_price: Some("2501".to_string()),
            unrealized_pnl: Some("0".to_string()),
            leverage: Some("1".to_string()),
            margin_mode: Some("isolated".to_string()),
            liquidation_price: None,
            raw: json!({}),
        }],
        &[],
    );
    let source_ref = requests[0].source_ref.as_deref().unwrap();

    assert!(source_ref.starts_with("rq:xrec:v2:ex=binance:"));
    assert!(source_ref.contains(":acct=email_sha256_"));
    assert!(source_ref.contains(":cred=cred_live_001:"));
    assert!(source_ref.contains(":combo=7:task=42:sym=ETHUSDT:"));
    assert!(!source_ref.contains("Buyer"));
    assert!(!source_ref.contains("@"));
}

#[test]
fn reconciliation_snapshot_builds_flat_position_sync_request() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "UNI-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "83"),
        ("RECONCILIATION_SNAPSHOT_CREDENTIAL_REF", "web-cred-85"),
    ])
    .unwrap();

    let requests = build_reconciliation_snapshot_requests(&config, &[], &[]);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].issue_type.as_str(), "exchange_position_flat");
    assert_eq!(requests[0].symbol, "UNI-USDT-SWAP");
    assert!(requests[0]
        .message
        .as_deref()
        .is_some_and(|message| message.contains("zero position")));
}

#[test]
fn reconciliation_snapshot_can_disable_web_report_for_read_only_evidence() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "UNI-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "83"),
        ("RECONCILIATION_SNAPSHOT_REPORT", "false"),
        ("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS", "true"),
    ])
    .unwrap();

    assert!(!config.report_reconciliation);
    assert!(config.include_fills);
    assert!(!config.close_fill_writeback_apply);
}

#[test]
fn reconciliation_snapshot_close_fill_writeback_apply_requires_confirmation_and_intent() {
    let err = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
        ("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS", "true"),
        ("RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY", "true"),
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM"));

    let err = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
        ("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS", "true"),
        ("RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY", "true"),
        (
            "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM",
            "I_UNDERSTAND_THIS_WRITES_EXCHANGE_CLOSE_FILL_TO_WEB",
        ),
        (
            "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT",
            "web-close-fill:combo=85:task=86:symbol=WRONG",
        ),
    ])
    .unwrap_err();

    assert!(err
        .to_string()
        .contains("RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT"));

    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
        ("RECONCILIATION_SNAPSHOT_INCLUDE_FILLS", "true"),
        ("RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY", "true"),
        (
            "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM",
            "I_UNDERSTAND_THIS_WRITES_EXCHANGE_CLOSE_FILL_TO_WEB",
        ),
        (
            "RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT",
            "web-close-fill:combo=85:task=86:symbol=ASTER-USDT-SWAP",
        ),
    ])
    .unwrap();

    assert!(config.close_fill_writeback_apply);
    assert_eq!(
        config.close_fill_writeback_intent.as_deref(),
        Some("web-close-fill:combo=85:task=86:symbol=ASTER-USDT-SWAP")
    );
}

#[test]
fn reconciliation_snapshot_builds_close_fill_writeback_candidate_when_flat_after_open() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
    ])
    .unwrap();
    let flat_position = Position {
        exchange: ExchangeId::Okx,
        instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
        exchange_symbol: "ASTER-USDT-SWAP".to_string(),
        side: Some("long".to_string()),
        size: "0".to_string(),
        entry_price: None,
        mark_price: None,
        unrealized_pnl: None,
        leverage: None,
        margin_mode: Some("isolated".to_string()),
        liquidation_price: None,
        raw: json!({}),
    };
    let fills = vec![
        Fill {
            exchange: ExchangeId::Okx,
            instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
            exchange_symbol: "ASTER-USDT-SWAP".to_string(),
            trade_id: Some("211850229".to_string()),
            order_id: Some("3631564680998985728".to_string()),
            side: Some("sell".to_string()),
            price: Some("0.6047".to_string()),
            size: Some("1".to_string()),
            fee: Some("-0.00030235".to_string()),
            fee_asset: Some("USDT".to_string()),
            role: Some("taker".to_string()),
            timestamp: Some(1_780_731_461_395),
            raw: json!({}),
        },
        Fill {
            exchange: ExchangeId::Okx,
            instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
            exchange_symbol: "ASTER-USDT-SWAP".to_string(),
            trade_id: Some("211849844".to_string()),
            order_id: Some("3631557801300238336".to_string()),
            side: Some("buy".to_string()),
            price: Some("0.607".to_string()),
            size: Some("1".to_string()),
            fee: Some("-0.0003035".to_string()),
            fee_asset: Some("USDT".to_string()),
            role: Some("taker".to_string()),
            timestamp: Some(1_780_731_256_364),
            raw: json!({}),
        },
    ];

    let candidates = build_close_fill_writeback_candidates(&config, &[flat_position], &[], &fills);

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0]["candidate_type"],
        "stop_loss_close_fill_observed"
    );
    assert_eq!(candidates[0]["writeback_mode"], "dry_run_plan_only");
    assert_eq!(candidates[0]["exchange"], "okx");
    assert_eq!(candidates[0]["symbol"], "ASTER-USDT-SWAP");
    assert_eq!(candidates[0]["task_id"], 86);
    assert_eq!(candidates[0]["combo_id"], 85);
    assert_eq!(candidates[0]["open_order_id"], "3631557801300238336");
    assert_eq!(candidates[0]["close_order_id"], "3631564680998985728");
    assert_eq!(candidates[0]["close_side"], "sell");
    assert_eq!(candidates[0]["close_size"], "1");
    assert_eq!(candidates[0]["close_price"], "0.6047");
    assert_eq!(candidates[0]["close_fee"], "-0.00030235");
    assert_eq!(candidates[0]["position_flat_confirmed"], true);
    assert_eq!(candidates[0]["active_open_order_count"], 0);
    assert_eq!(candidates[0]["web_writeback_allowed"], false);
    assert_eq!(candidates[0]["exchange_mutation_allowed"], false);
    assert!(candidates[0]["source_ref"]
        .as_str()
        .is_some_and(|value| value.contains("task=86:sym=ASTER-USDT-SWAP")));
    assert!(!candidates[0].to_string().contains("buyer@example.com"));
}

#[test]
fn reconciliation_snapshot_converts_close_fill_candidate_to_web_writeback_request() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
    ])
    .unwrap();
    let candidate = json!({
        "candidate_type": "stop_loss_close_fill_observed",
        "writeback_mode": "dry_run_plan_only",
        "exchange": "okx",
        "symbol": "ASTER-USDT-SWAP",
        "combo_id": 85,
        "task_id": 86,
        "open_order_id": "3631557801300238336",
        "open_trade_id": "211849844",
        "close_order_id": "3631564680998985728",
        "close_trade_id": "211850229",
        "close_side": "sell",
        "close_price": "0.6047",
        "close_size": "1",
        "close_fee": "-0.00030235",
        "close_timestamp": 1_780_731_461_395_i64,
        "quantity_match": true,
        "position_flat_confirmed": true,
        "active_open_order_count": 0,
        "web_writeback_allowed": false,
        "exchange_mutation_allowed": false,
        "report_result_allowed": false,
        "source_ref": "rq:xrec:v2:ex=okx:combo=85:task=86:sym=ASTER-USDT-SWAP"
    });

    let request = build_close_fill_writeback_request_from_candidate(&config, &candidate).unwrap();

    assert_eq!(request.task_id, 86);
    assert_eq!(request.combo_id, 85);
    assert_eq!(request.exchange, "okx");
    assert_eq!(request.symbol, "ASTER-USDT-SWAP");
    assert_eq!(
        request.open_order_id.as_deref(),
        Some("3631557801300238336")
    );
    assert_eq!(request.open_trade_id.as_deref(), Some("211849844"));
    assert_eq!(request.close_order_id, "3631564680998985728");
    assert_eq!(request.close_trade_id.as_deref(), Some("211850229"));
    assert_eq!(request.close_side, "sell");
    assert!((request.close_size - 1.0).abs() < f64::EPSILON);
    assert_eq!(request.close_price, Some(0.6047));
    assert_eq!(request.close_fee, Some(-0.00030235));
    assert_eq!(request.close_timestamp_ms, Some(1_780_731_461_395));
    assert!(request.position_flat_confirmed);
    assert_eq!(request.active_open_order_count, 0);
    assert!(request.quantity_match);
    assert!(request.writeback_authorized);
    assert!(!serde_json::to_string(&request)
        .unwrap()
        .contains("buyer@example.com"));
}

#[test]
fn reconciliation_snapshot_does_not_build_close_fill_candidate_when_position_not_flat() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
    ])
    .unwrap();
    let open_position = Position {
        exchange: ExchangeId::Okx,
        instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
        exchange_symbol: "ASTER-USDT-SWAP".to_string(),
        side: Some("long".to_string()),
        size: "1".to_string(),
        entry_price: Some("0.607".to_string()),
        mark_price: Some("0.606".to_string()),
        unrealized_pnl: None,
        leverage: Some("3".to_string()),
        margin_mode: Some("isolated".to_string()),
        liquidation_price: None,
        raw: json!({}),
    };
    let fills = vec![Fill {
        exchange: ExchangeId::Okx,
        instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
        exchange_symbol: "ASTER-USDT-SWAP".to_string(),
        trade_id: Some("211849844".to_string()),
        order_id: Some("3631557801300238336".to_string()),
        side: Some("buy".to_string()),
        price: Some("0.607".to_string()),
        size: Some("1".to_string()),
        fee: Some("-0.0003035".to_string()),
        fee_asset: Some("USDT".to_string()),
        role: Some("taker".to_string()),
        timestamp: Some(1_780_731_256_364),
        raw: json!({}),
    }];

    let candidates = build_close_fill_writeback_candidates(&config, &[open_position], &[], &fills);

    assert!(candidates.is_empty());
}

#[test]
fn reconciliation_snapshot_does_not_build_close_fill_candidate_with_active_open_order() {
    let config = config_from(&[
        (
            "RECONCILIATION_SNAPSHOT_CONFIRM",
            "I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION",
        ),
        ("RECONCILIATION_SNAPSHOT_BUYER_EMAIL", "buyer@example.com"),
        ("RECONCILIATION_SNAPSHOT_EXCHANGE", "okx"),
        ("RECONCILIATION_SNAPSHOT_SYMBOL", "ASTER-USDT-SWAP"),
        ("RECONCILIATION_SNAPSHOT_COMBO_ID", "85"),
        ("RECONCILIATION_SNAPSHOT_TASK_ID", "86"),
    ])
    .unwrap();
    let open_order = Order {
        exchange: ExchangeId::Okx,
        instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
        exchange_symbol: "ASTER-USDT-SWAP".to_string(),
        order_id: Some("protective-open".to_string()),
        client_order_id: None,
        side: Some("sell".to_string()),
        order_type: Some("stop_market".to_string()),
        price: None,
        size: Some("1".to_string()),
        filled_size: Some("0".to_string()),
        average_price: None,
        status: Some("live".to_string()),
        created_at: None,
        updated_at: None,
        raw: json!({}),
    };
    let fills = vec![
        Fill {
            exchange: ExchangeId::Okx,
            instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
            exchange_symbol: "ASTER-USDT-SWAP".to_string(),
            trade_id: Some("211849844".to_string()),
            order_id: Some("3631557801300238336".to_string()),
            side: Some("buy".to_string()),
            price: Some("0.607".to_string()),
            size: Some("1".to_string()),
            fee: Some("-0.0003035".to_string()),
            fee_asset: Some("USDT".to_string()),
            role: Some("taker".to_string()),
            timestamp: Some(1_780_731_256_364),
            raw: json!({}),
        },
        Fill {
            exchange: ExchangeId::Okx,
            instrument: Instrument::perp("aster", "usdt").with_settlement("usdt"),
            exchange_symbol: "ASTER-USDT-SWAP".to_string(),
            trade_id: Some("211850229".to_string()),
            order_id: Some("3631564680998985728".to_string()),
            side: Some("sell".to_string()),
            price: Some("0.6047".to_string()),
            size: Some("1".to_string()),
            fee: Some("-0.00030235".to_string()),
            fee_asset: Some("USDT".to_string()),
            role: Some("taker".to_string()),
            timestamp: Some(1_780_731_461_395),
            raw: json!({}),
        },
    ];

    let candidates = build_close_fill_writeback_candidates(&config, &[], &[open_order], &fills);

    assert!(candidates.is_empty());
}
