use crypto_exc_all::{ExchangeId, Instrument, Position};
use rust_quant_services::rust_quan_web::{
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
