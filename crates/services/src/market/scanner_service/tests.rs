#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_domain::entities::{
        MarketRankEventType, MarketRankSnapshot, MarketRankTechnicalSnapshot,
    };
    #[test]
    fn build_rank_velocity_event_uses_scanner_product_contract() {
        let detected_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let event = build_rank_velocity_event(
            "ETH-USDT-SWAP",
            "15分钟",
            Some(42),
            18,
            Some(24),
            None,
            Some(Decimal::new(2200, 0)),
            Some(Decimal::new(2000, 0)),
            detected_at,
            MarketRankTechnicalCapture::not_requested(),
        );
        assert_eq!(event.exchange, "okx");
        assert_eq!(event.symbol, "ETH-USDT-SWAP");
        assert_eq!(event.event_type, MarketRankEventType::RankVelocity);
        assert_eq!(event.timeframe.as_deref(), Some("15分钟"));
        assert_eq!(event.old_rank, Some(42));
        assert_eq!(event.new_rank, Some(18));
        assert_eq!(event.delta_rank, Some(24));
        assert_eq!(event.current_price, Some(Decimal::new(2200, 0)));
        assert_eq!(event.previous_price, Some(Decimal::new(2000, 0)));
        assert_eq!(event.price_change_pct, Some(Decimal::new(100, 1)));
        assert_eq!(event.price_direction, "up");
        assert_eq!(event.source, "scanner_service");
        assert_eq!(event.notification_state, "pending");
    }
    #[test]
    fn build_kline_15m_rank_velocity_event_uses_handoff_compatible_contract() {
        let candle_open_ts_ms = 1_777_824_000_000;
        let event = build_kline_15m_rank_velocity_event(
            "ETH-USDT-SWAP",
            candle_open_ts_ms,
            Decimal::new(100, 0),
            Decimal::new(104, 0),
        )
        .expect("valid 15m candle should build an event");
        assert_eq!(event.exchange, "okx");
        assert_eq!(event.symbol, "ETH-USDT-SWAP");
        assert_eq!(event.event_type, MarketRankEventType::RankVelocity);
        assert_eq!(event.timeframe.as_deref(), Some("15分钟"));
        assert_eq!(event.old_rank, None);
        assert_eq!(event.new_rank, Some(0));
        assert_eq!(event.delta_rank, Some(0));
        assert_eq!(event.current_price, Some(Decimal::new(104, 0)));
        assert_eq!(event.previous_price, Some(Decimal::new(100, 0)));
        assert_eq!(event.price_change_pct, Some(Decimal::new(4, 0)));
        assert_eq!(event.price_direction, "up");
        assert_eq!(event.technical_snapshot_status, "not_requested");
        assert!(event.technical_snapshot.is_none());
        assert_eq!(
            event.detected_at,
            DateTime::from_timestamp_millis(candle_open_ts_ms + 15 * 60 * 1000)
                .expect("valid detected_at")
        );
        assert_eq!(event.source, "kline_15m_scanner");
        assert_eq!(event.notification_state, "pending");
    }
    #[test]
    fn build_kline_15m_rank_velocity_event_rejects_invalid_open_price() {
        let error = build_kline_15m_rank_velocity_event(
            "ETH-USDT-SWAP",
            1_777_824_000_000,
            Decimal::ZERO,
            Decimal::new(104, 0),
        )
        .expect_err("zero open price should be rejected");
        assert!(
            error.to_string().contains("15m candle open price"),
            "unexpected error: {error}"
        );
    }
    #[test]
    fn build_top_list_event_uses_entry_and_exit_contract() {
        let detected_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let entry = build_top_list_event(
            "SOL-USDT-SWAP",
            true,
            Some(55),
            Some(40),
            None,
            Some(Decimal::new(180, 1)),
            None,
            detected_at,
            MarketRankTechnicalCapture::not_requested(),
        );
        assert_eq!(entry.exchange, "okx");
        assert_eq!(entry.event_type, MarketRankEventType::TopEntry);
        assert_eq!(entry.old_rank, Some(55));
        assert_eq!(entry.new_rank, Some(40));
        assert_eq!(entry.delta_rank, Some(15));
        assert_eq!(entry.current_price, Some(Decimal::new(180, 1)));
        assert_eq!(entry.price_direction, "unknown");
        assert_eq!(entry.source, "scanner_service");
        let exit = build_top_list_event(
            "DOGE-USDT-SWAP",
            false,
            Some(45),
            Some(62),
            None,
            Some(Decimal::new(12, 2)),
            Some(Decimal::new(15, 2)),
            detected_at,
            MarketRankTechnicalCapture::not_requested(),
        );
        assert_eq!(exit.event_type, MarketRankEventType::TopExit);
        assert_eq!(exit.symbol, "DOGE-USDT-SWAP");
        assert_eq!(exit.old_rank, Some(45));
        assert_eq!(exit.new_rank, Some(62));
        assert_eq!(exit.delta_rank, Some(-17));
        assert_eq!(exit.price_change_pct, Some(Decimal::new(-200, 1)));
        assert_eq!(exit.price_direction, "down");
        assert_eq!(exit.notification_state, "pending");
    }
    #[test]
    fn build_market_rank_technical_snapshot_detects_4h_ma_and_ema_breakout() {
        let snapshot_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let mut closes = vec![100.0; 20];
        closes.push(120.0);
        let snapshot: MarketRankTechnicalSnapshot =
            build_market_rank_technical_snapshot_from_closes("4h", 20, &closes, snapshot_at)
                .expect("enough candles should build technical snapshot");
        assert_eq!(snapshot.timeframe, "4h");
        assert_eq!(snapshot.period, 20);
        assert_eq!(snapshot.close_price, Decimal::new(120, 0));
        assert_eq!(snapshot.ma_value, Decimal::new(101, 0));
        assert_eq!(snapshot.ma_state, "breakout_up");
        assert_eq!(snapshot.ema_state, "breakout_up");
        assert_eq!(snapshot.candle_count, 21);
        assert_eq!(snapshot.snapshot_at, snapshot_at);
        assert!(snapshot.ma_distance_pct > Decimal::ZERO);
        assert!(snapshot.ema_distance_pct > Decimal::ZERO);
    }
    #[test]
    fn build_market_rank_technical_snapshot_requires_enough_closes() {
        let snapshot_at = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let snapshot =
            build_market_rank_technical_snapshot_from_closes("4h", 20, &[100.0; 19], snapshot_at);
        assert!(snapshot.is_none());
    }
    #[test]
    fn rank_history_from_persisted_snapshots_restores_prices_by_scan_time() {
        let first_scan = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        let second_scan = DateTime::from_timestamp(1_774_815_300, 0).expect("valid test timestamp");
        let rows = vec![
            MarketRankSnapshot {
                id: Some(1),
                exchange: "okx".to_string(),
                symbol: "XLM-USDT-SWAP".to_string(),
                rank: 107,
                price: Decimal::new(105, 3),
                volume_24h_quote: Decimal::new(42_000_000, 0),
                captured_at: first_scan,
                created_at: first_scan,
            },
            MarketRankSnapshot {
                id: Some(2),
                exchange: "okx".to_string(),
                symbol: "XLM-USDT-SWAP".to_string(),
                rank: 23,
                price: Decimal::new(126, 3),
                volume_24h_quote: Decimal::new(112_000_000, 0),
                captured_at: second_scan,
                created_at: second_scan,
            },
        ];
        let history = rank_history_from_persisted_snapshots(rows);
        assert_eq!(history.len(), 2);
        assert_eq!(
            history[0].prices.get("XLM-USDT-SWAP"),
            Some(&Decimal::new(105, 3))
        );
        assert_eq!(
            history[1].prices.get("XLM-USDT-SWAP"),
            Some(&Decimal::new(126, 3))
        );
    }
    #[test]
    fn market_rank_history_restore_targets_only_cover_required_horizons() {
        let now = DateTime::from_timestamp(1_774_900_000, 0).expect("valid test timestamp");
        assert_eq!(
            market_rank_history_restore_targets(now),
            [
                now - Duration::hours(24),
                now - Duration::hours(4),
                now - Duration::minutes(15),
                now,
            ]
        );
    }
    #[test]
    fn market_rank_snapshot_persistence_is_limited_to_once_per_minute() {
        let first = DateTime::from_timestamp(1_774_900_000, 0).expect("valid test timestamp");
        assert!(market_rank_snapshot_persistence_is_due(None, first));
        assert!(!market_rank_snapshot_persistence_is_due(
            Some(first),
            first + Duration::seconds(59)
        ));
        assert!(market_rank_snapshot_persistence_is_due(
            Some(first),
            first + Duration::seconds(60)
        ));
    }
    #[test]
    fn market_velocity_episode_stale_cutoff_uses_rank_history_window() {
        let now = DateTime::from_timestamp(1_774_814_400, 0).expect("valid test timestamp");
        assert_eq!(
            market_velocity_episode_stale_before(now),
            now - Duration::hours(MARKET_RANK_HISTORY_RETENTION_HOURS)
        );
    }
    #[test]
    fn market_rank_snapshot_prune_is_not_a_scanner_side_effect() {
        let service_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/market/scanner_service.rs");
        let source = std::fs::read_to_string(&service_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", service_path.display(), error));
        assert!(
            !source.contains("delete_rank_snapshots_before"),
            "scanner service should only persist rank snapshots; pruning belongs to the maintenance scheduler"
        );
        assert!(
            !source.contains("last_rank_snapshot_pruned_at"),
            "scanner service should not own maintenance job state"
        );
    }
}
