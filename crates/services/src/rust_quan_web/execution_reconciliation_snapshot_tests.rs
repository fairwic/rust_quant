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
            credential_id: None,
            credential_ref: Some("manual".to_string()),
            report_reconciliation: false,
            include_fills: true,
            close_fill_writeback_apply: false,
            close_fill_writeback_intent: None,
        }
    }
    fn test_order(order_id: Option<&str>) -> Order {
        Order {
            exchange: ExchangeId::Okx,
            instrument: crypto_exc_all::Instrument::perp("BTC", "USDT"),
            exchange_symbol: "BTC-USDT-SWAP".to_string(),
            order_id: order_id.map(str::to_string),
            client_order_id: None,
            side: None,
            order_type: None,
            price: None,
            size: None,
            filled_size: None,
            average_price: None,
            status: None,
            created_at: None,
            updated_at: None,
            raw: json!({}),
        }
    }
    fn test_fill(trade_id: Option<&str>) -> Fill {
        Fill {
            exchange: ExchangeId::Okx,
            instrument: crypto_exc_all::Instrument::perp("BTC", "USDT"),
            exchange_symbol: "BTC-USDT-SWAP".to_string(),
            trade_id: trade_id.map(str::to_string),
            order_id: None,
            side: None,
            price: None,
            size: None,
            fee: None,
            fee_asset: None,
            role: None,
            timestamp: None,
            raw: json!({}),
        }
    }
    fn test_position_history(position_id: Option<&str>) -> crypto_exc_all::PositionHistory {
        crypto_exc_all::PositionHistory {
            exchange: ExchangeId::Okx,
            instrument: crypto_exc_all::Instrument::perp("BTC", "USDT"),
            exchange_symbol: "BTC-USDT-SWAP".to_string(),
            position_id: position_id.map(str::to_string),
            side: Some("long".to_string()),
            direction: Some("long".to_string()),
            leverage: Some("3".to_string()),
            margin_mode: Some("cross".to_string()),
            open_avg_price: Some("0.6208".to_string()),
            close_avg_price: Some("0.6047".to_string()),
            open_max_position: Some("1".to_string()),
            close_total_position: Some("1".to_string()),
            realized_pnl: Some("-0.01".to_string()),
            pnl: Some("-0.01".to_string()),
            pnl_ratio: Some("-0.0817".to_string()),
            fee: Some("-0.0002".to_string()),
            funding_fee: Some("0".to_string()),
            liquidation_penalty: Some("0".to_string()),
            close_type: Some("2".to_string()),
            open_time: Some(1_780_980_141_000),
            close_time: Some(1_781_122_152_000),
            raw: json!({"posId": position_id}),
        }
    }
    #[test]
    fn okx_order_history_pagination_uses_last_order_id_for_full_page() {
        let page = vec![
            test_order(Some("order-newer")),
            test_order(Some("order-older")),
        ];
        let cursor = next_okx_order_history_after_cursor(&page, 2);
        assert_eq!(cursor.as_deref(), Some("order-older"));
    }
    #[test]
    fn okx_order_history_pagination_stops_on_short_page_or_missing_cursor() {
        let short_page = vec![test_order(Some("order-only"))];
        let missing_cursor_page = vec![test_order(Some("order-newer")), test_order(None)];
        assert_eq!(next_okx_order_history_after_cursor(&short_page, 2), None);
        assert_eq!(
            next_okx_order_history_after_cursor(&missing_cursor_page, 2),
            None
        );
    }
    #[test]
    fn okx_fill_history_pagination_uses_last_trade_id_for_full_page() {
        let page = vec![test_fill(Some("fill-newer")), test_fill(Some("fill-older"))];
        let cursor = next_okx_fill_history_after_cursor(&page, 2);
        assert_eq!(cursor.as_deref(), Some("fill-older"));
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
            &[],
        )
        .expect("account snapshot request");
        assert_eq!(request.bills.len(), 1);
        assert_eq!(request.bills[0].external_bill_id, "okx-bill-1");
        assert_eq!(request.bills[0].asset, "USDT");
        assert_eq!(request.bills[0].balance_change, Some(9.7));
        assert_eq!(request.bills[0].fee_amount, Some(-0.3));
        assert_eq!(request.bills[0].pnl_amount, Some(10.0));
    }
    #[test]
    fn account_snapshot_report_includes_okx_position_history() {
        let position_history = vec![test_position_history(Some("okx-position-1"))];
        let request = build_exchange_account_snapshot_report_request(
            &test_config(),
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &position_history,
        )
        .expect("account snapshot request");
        assert_eq!(request.position_history.len(), 1);
        let item = &request.position_history[0];
        assert_eq!(item.external_position_id, "okx-position-1");
        assert_eq!(item.side.as_deref(), Some("long"));
        assert_eq!(item.direction.as_deref(), Some("long"));
        assert_eq!(item.leverage, Some(3.0));
        assert_eq!(item.margin_mode.as_deref(), Some("cross"));
        assert_eq!(item.open_avg_price, Some(0.6208));
        assert_eq!(item.close_avg_price, Some(0.6047));
        assert_eq!(item.open_max_position, Some(1.0));
        assert_eq!(item.close_total_position, Some(1.0));
        assert_eq!(item.realized_pnl_usdt, Some(-0.01));
        assert_eq!(item.pnl_usdt, Some(-0.01));
        assert_eq!(item.pnl_ratio, Some(-0.0817));
        assert_eq!(item.fee_usdt, Some(-0.0002));
        assert_eq!(item.close_type.as_deref(), Some("2"));
        assert_eq!(item.opened_at.as_deref(), Some("2026-06-09T04:42:21"));
        assert_eq!(item.closed_at.as_deref(), Some("2026-06-10T20:09:12"));
    }
}
