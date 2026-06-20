#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ReconciliationSnapshotCheckConfig {
        ReconciliationSnapshotCheckConfig {
            buyer_email: "buyer@example.com".to_string(),
            exchange: ExchangeId::Okx,
            symbol: "BTC-USDT-SWAP".to_string(),
            combo_id: 85,
            task_id: 85,
            credential_ref: Some("manual".to_string()),
            report_reconciliation: false,
            include_fills: true,
            close_fill_writeback_apply: false,
            close_fill_writeback_intent: None,
        }
    }

    #[test]
    fn account_snapshot_report_uses_web_datetime_contract() {
        let request = build_exchange_account_snapshot_report_request(
            &test_config(),
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
        )
        .expect("account snapshot request");

        let snapshot_at = request.snapshot_at.expect("snapshot_at");
        assert!(
            chrono::NaiveDateTime::parse_from_str(&snapshot_at, "%Y-%m-%dT%H:%M:%S").is_ok(),
            "snapshot_at must use Web NaiveDateTime JSON contract, got {snapshot_at}"
        );
        assert!(
            !snapshot_at.contains(' '),
            "snapshot_at must not use chrono default space separator"
        );
    }

    #[test]
    fn exchange_timestamps_use_web_datetime_contract() {
        let formatted =
            timestamp_millis_to_naive_string(1_774_814_400_000).expect("valid timestamp millis");

        assert_eq!(formatted, "2026-03-29T20:00:00");
    }

    #[test]
    fn account_snapshot_report_includes_symbol_scoped_account_bills() {
        let bills = vec![
            AccountBill {
                exchange: ExchangeId::Okx,
                instrument: Some(crypto_exc_all::Instrument::perp("BTC", "USDT")),
                exchange_symbol: Some("BTC-USDT-SWAP".to_string()),
                bill_id: Some("okx-bill-1".to_string()),
                asset: Some("USDT".to_string()),
                balance_change: Some("9.7".to_string()),
                balance_after: Some("8211.49".to_string()),
                fee: Some("-0.3".to_string()),
                pnl: Some("10".to_string()),
                bill_type: Some("2".to_string()),
                bill_sub_type: Some("1".to_string()),
                order_id: Some("okx-order-1".to_string()),
                trade_id: Some("okx-fill-1".to_string()),
                timestamp: Some(1_774_814_400_000),
                raw: json!({"billId":"okx-bill-1"}),
            },
            AccountBill {
                exchange: ExchangeId::Okx,
                instrument: None,
                exchange_symbol: None,
                bill_id: Some("okx-transfer".to_string()),
                asset: Some("USDT".to_string()),
                balance_change: Some("100".to_string()),
                balance_after: None,
                fee: None,
                pnl: None,
                bill_type: Some("1".to_string()),
                bill_sub_type: None,
                order_id: None,
                trade_id: None,
                timestamp: Some(1_774_814_400_000),
                raw: json!({"billId":"okx-transfer"}),
            },
        ];
        let request = build_exchange_account_snapshot_report_request(
            &test_config(),
            &[],
            &[],
            &[],
            &[],
            &[],
            &bills,
        )
        .expect("account snapshot request");

        assert_eq!(request.bills.len(), 1);
        assert_eq!(request.bills[0].external_bill_id, "okx-bill-1");
        assert_eq!(request.bills[0].asset, "USDT");
        assert_eq!(request.bills[0].balance_change, Some(9.7));
        assert_eq!(request.bills[0].fee_amount, Some(-0.3));
        assert_eq!(request.bills[0].pnl_amount, Some(10.0));
    }
}
