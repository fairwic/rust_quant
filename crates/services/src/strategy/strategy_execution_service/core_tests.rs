    use super::*;
    use async_trait::async_trait;
    use rust_quant_core::cache::init_redis_pool;

    use rust_quant_strategies::TradePosition;
    use std::sync::Mutex;

    /// Mock SwapOrderRepository - 支持自定义行为
    struct MockSwapOrderRepository {
        /// 模拟已存在的订单（用于幂等性测试）
        existing_order: Option<SwapOrder>,
        /// 保存订单时是否返回错误
        save_should_fail: bool,
        /// 保存的订单记录
        saved_orders: Arc<Mutex<Vec<SwapOrder>>>,
}
    impl MockSwapOrderRepository {
        fn new() -> Self {
            Self {
                existing_order: None,
                save_should_fail: false,
                saved_orders: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_existing_order(mut self, order: SwapOrder) -> Self {
            self.existing_order = Some(order);
            self
        }

        fn with_save_failure(mut self, should_fail: bool) -> Self {
            self.save_should_fail = should_fail;
            self
        }

        #[allow(dead_code)]
        fn get_saved_orders(&self) -> Vec<SwapOrder> {
            self.saved_orders.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SwapOrderRepository for MockSwapOrderRepository {
        async fn find_by_id(&self, _id: i32) -> Result<Option<SwapOrder>> {
            Ok(None)
        }

        async fn find_by_in_order_id(&self, in_order_id: &str) -> Result<Option<SwapOrder>> {
            if let Some(ref order) = self.existing_order {
                if order.in_order_id == in_order_id {
                    return Ok(Some(order.clone()));
                }
            }
            Ok(None)
        }

        async fn find_by_out_order_id(&self, _out_order_id: &str) -> Result<Option<SwapOrder>> {
            Ok(None)
        }

        async fn find_by_inst_id(
            &self,
            _inst_id: &str,
            _limit: Option<i32>,
        ) -> Result<Vec<SwapOrder>> {
            Ok(vec![])
        }

        async fn find_pending_order(
            &self,
            _inst_id: &str,
            _period: &str,
            _side: &str,
            _pos_side: &str,
        ) -> Result<Vec<SwapOrder>> {
            Ok(vec![])
        }

        async fn find_latest_by_strategy_inst_period_pos_side(
            &self,
            strategy_id: i32,
            inst_id: &str,
            period: &str,
            pos_side: &str,
        ) -> Result<Option<SwapOrder>> {
            if let Some(ref order) = self.existing_order {
                if order.strategy_id == strategy_id
                    && order.inst_id == inst_id
                    && order.period == period
                    && order.pos_side == pos_side
                {
                    return Ok(Some(order.clone()));
                }
            }

            let orders = self.saved_orders.lock().unwrap();
            let mut candidates: Vec<SwapOrder> = orders
                .iter()
                .filter(|order| {
                    order.strategy_id == strategy_id
                        && order.inst_id == inst_id
                        && order.period == period
                        && order.pos_side == pos_side
                })
                .cloned()
                .collect();

            candidates.sort_by_key(|order| order.created_at);
            Ok(candidates.pop())
        }

        async fn save(&self, order: &SwapOrder) -> Result<i32> {
            if self.save_should_fail {
                return Err(anyhow!("模拟保存失败"));
            }
            self.saved_orders.lock().unwrap().push(order.clone());
            Ok(1)
        }

        async fn update(&self, order: &SwapOrder) -> Result<()> {
            let mut orders = self.saved_orders.lock().unwrap();
            if let Some(existing) = orders.iter_mut().find(|o| {
                (order.id.is_some() && o.id == order.id) || o.in_order_id == order.in_order_id
            }) {
                *existing = order.clone();
            }
            Ok(())
        }

        async fn find_by_strategy_and_time(
            &self,
            _strategy_id: i32,
            _start_time: i64,
            _end_time: i64,
        ) -> Result<Vec<SwapOrder>> {
            Ok(vec![])
        }
    }

    fn create_test_service() -> StrategyExecutionService {
        StrategyExecutionService::new(Arc::new(MockSwapOrderRepository::new()))
    }

    /// 创建测试用的SignalResult - 买入信号
    fn create_buy_signal(open_price: f64, ts: i64) -> SignalResult {
        SignalResult {
            should_buy: true,
            should_sell: false,
            open_price,
            signal_kline_stop_loss_price: Some(open_price * 0.98), // 2%止损
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            stop_loss_source: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Long,
        }
    }

    /// 创建测试用的SignalResult - 卖出信号
    fn create_sell_signal(open_price: f64, ts: i64) -> SignalResult {
        SignalResult {
            should_buy: false,
            should_sell: true,
            open_price,
            signal_kline_stop_loss_price: Some(open_price * 1.02), // 2%止损
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            stop_loss_source: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Short,
        }
    }

    fn create_trigger_candle(close: f64, ts: i64) -> CandleItem {
        CandleItem {
            o: close * 0.99,
            h: close * 1.01,
            l: close * 0.98,
            c: close,
            v: 100.0,
            ts,
            confirm: 1,
        }
    }

    #[test]
    fn smoke_force_signal_buy_uses_trigger_candle() {
        let mut signal = SignalResult {
            should_buy: false,
            should_sell: false,
            open_price: 0.0,
            ts: 0,
            ..create_sell_signal(100.0, 1)
        };
        let trigger_candle = create_trigger_candle(3420.5, 1_714_000_000_000);

        let applied = StrategyExecutionService::apply_smoke_forced_signal(
            &mut signal,
            &trigger_candle,
            Some("buy"),
        )
        .unwrap();

        assert!(applied);
        assert!(signal.should_buy);
        assert!(!signal.should_sell);
        assert_eq!(signal.open_price, 3420.5);
        assert_eq!(signal.ts, 1_714_000_000_000);
        assert_eq!(signal.signal_kline_stop_loss_price, Some(3420.5 * 0.98));
        assert_eq!(signal.direction, rust_quant_domain::SignalDirection::Long);
    }

    #[test]
    fn smoke_force_signal_rejects_invalid_side() {
        let mut signal = create_buy_signal(100.0, 1);
        let trigger_candle = create_trigger_candle(101.0, 2);

        let error = StrategyExecutionService::apply_smoke_forced_signal(
            &mut signal,
            &trigger_candle,
            Some("flat"),
        )
        .unwrap_err();

        assert!(error.to_string().contains("RUST_QUANT_SMOKE_FORCE_SIGNAL"));
    }

    #[test]
    fn web_dispatch_mode_skips_local_close_algo_management() {
        assert!(
            !StrategyExecutionService::should_manage_local_close_algos_after_open_from_env(
                Some("web"),
                Some("http://127.0.0.1:8000"),
                None,
            )
        );
        assert!(
            StrategyExecutionService::should_manage_local_close_algos_after_open_from_env(
                Some("legacy"),
                Some("http://127.0.0.1:8000"),
                None,
            )
        );
    }

    #[test]
    fn builds_quant_web_strategy_signal_request_from_live_entry_signal() {
        let config = StrategyConfig::new(
            42,
            rust_quant_domain::StrategyType::Vegas,
            "ETH-USDT-SWAP".to_string(),
            rust_quant_domain::Timeframe::H4,
            serde_json::json!({"window": 144}),
            serde_json::json!({"max_loss_percent": 0.02}),
        );
        let signal = create_buy_signal(3500.0, 1704067200000);
        let risk_config = create_test_risk_config(0.02, Some(true));

        let request = StrategyExecutionService::build_strategy_signal_submit_request(
            "ETH-USDT-SWAP",
            "4H",
            &signal,
            &risk_config,
            42,
            config.strategy_type.as_str(),
            Some("binance"),
            "buy",
            "long",
            "rq421704067200000",
        )
        .unwrap();

        assert_eq!(request.source, "rust_quant");
        assert_eq!(
            request.external_id,
            "rust_quant:vegas:42:ETH-USDT-SWAP:4H:1704067200000"
        );
        assert_eq!(request.strategy_slug, "vegas");
        assert_eq!(request.strategy_key, "vegas:ETH-USDT-SWAP:4H:42");
        assert_eq!(request.symbol, "ETH-USDT-SWAP");
        assert_eq!(request.signal_type, "entry");
        assert_eq!(request.direction, "long");
        assert_eq!(
            request.generated_at.as_deref(),
            Some("2024-01-01T00:00:00Z")
        );
        assert!(request.title.contains("Vegas"));
        assert!(request.title.contains("long"));

        let payload: serde_json::Value = serde_json::from_str(&request.payload_json).unwrap();
        assert_eq!(payload["source"], "rust_quant");
        assert_eq!(payload["config_id"], 42);
        assert_eq!(payload["strategy_type"], "vegas");
        assert_eq!(payload["period"], "4H");
        assert_eq!(payload["symbol"], "ETH-USDT-SWAP");
        assert_eq!(payload["exchange"], "binance");
        assert_eq!(payload["side"], "buy");
        assert_eq!(payload["position_side"], "long");
        assert_eq!(payload["order_type"], "market");
        assert_eq!(payload["client_order_id"], "rq421704067200000");
        assert_eq!(payload["signal"]["open_price"], 3500.0);
        assert_eq!(payload["risk_plan"]["entry_price"], 3500.0);
        assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3430.0);
        assert_eq!(payload["risk_plan"]["direction"], "long");
        assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
        assert!(payload.get("size").is_none());
    }

    #[test]
    fn builds_quant_web_strategy_signal_request_with_short_risk_plan() {
        let signal = create_sell_signal(3500.0, 1704067200000);
        let risk_config = create_test_risk_config(0.02, Some(true));

        let request = StrategyExecutionService::build_strategy_signal_submit_request(
            "ETH-USDT-SWAP",
            "4H",
            &signal,
            &risk_config,
            42,
            "vegas",
            Some("binance"),
            "sell",
            "short",
            "rq421704067200000",
        )
        .unwrap();

        let payload: serde_json::Value = serde_json::from_str(&request.payload_json).unwrap();
        assert_eq!(request.direction, "short");
        assert_eq!(payload["risk_plan"]["entry_price"], 3500.0);
        assert_eq!(payload["risk_plan"]["selected_stop_loss_price"], 3570.0);
        assert_eq!(payload["risk_plan"]["direction"], "short");
        assert_eq!(payload["risk_plan"]["protective_stop_loss_required"], true);
    }

    #[test]
    fn strategy_signal_external_id_appends_smoke_suffix_when_present() {
        let external_id = StrategyExecutionService::build_strategy_signal_external_id(
            "vegas",
            42,
            "ETH-USDT-SWAP",
            "4H",
            1704067200000,
            Some("run-20260424"),
        );

        assert_eq!(
            external_id,
            "rust_quant:vegas:42:ETH-USDT-SWAP:4H:1704067200000:run-20260424"
        );
    }

    #[test]
    fn test_service_creation() {
        let _service = create_test_service();
    }

    #[test]
    fn test_close_algo_detail_roundtrip() {
        let detail = serde_json::json!({
            "entry_price": 100.0,
            "stop_loss": 95.0,
        })
        .to_string();
        let algo_ids = vec!["a1".to_string(), "a2".to_string()];
        let updated = StrategyExecutionService::upsert_close_algo_detail(
            &detail,
            &algo_ids,
            "rq-1",
            Some(95.0),
            Some(110.0),
        );

        let extracted = StrategyExecutionService::extract_close_algo_ids(&updated);
        assert_eq!(extracted, algo_ids);

        let cleared = StrategyExecutionService::remove_close_algo_detail(&updated);
        let extracted_after_clear = StrategyExecutionService::extract_close_algo_ids(&cleared);
        assert!(extracted_after_clear.is_empty());
    }

    #[test]
    fn test_min_execution_interval() {
        use rust_quant_domain::Timeframe;

        let service = create_test_service();

        assert_eq!(service.get_min_execution_interval(&Timeframe::M1), 60);
        assert_eq!(service.get_min_execution_interval(&Timeframe::M5), 300);
        assert_eq!(service.get_min_execution_interval(&Timeframe::H1), 3600);
        assert_eq!(service.get_min_execution_interval(&Timeframe::D1), 86400);
    }

    #[tokio::test]
    async fn test_should_execute() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};

        let service = create_test_service();

        let config = StrategyConfig {
            id: 1,
            strategy_type: StrategyType::Vegas,
            exchange: None,
            symbol: "BTC-USDT".to_string(),
            timeframe: Timeframe::H1,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        assert!(service.should_execute(&config, None, 1000));
        assert!(!service.should_execute(&config, Some(1000), 1500));
        assert!(service.should_execute(&config, Some(1000), 5000));
    }

    #[tokio::test]
    async fn execution_respects_filter_block() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};
        use rust_quant_strategies::framework::backtest::BasicRiskStrategyConfig;

        let repo = Arc::new(MockSwapOrderRepository::new());
        let service = StrategyExecutionService::new(repo.clone());

        let config = StrategyConfig {
            id: 42,
            strategy_type: StrategyType::Vegas,
            exchange: None,
            symbol: "BTC-USDT".to_string(),
            timeframe: Timeframe::H1,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({ "max_loss_percent": 0.02 }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        let mut signal = create_buy_signal(100.0, 1);
        signal
            .filter_reasons
            .push("FIB_STRICT_MAJOR_BEAR_BLOCK_LONG".to_string());

        let candle = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 99.0,
            c: 100.0,
            v: 1.0,
            ts: 1,
            confirm: 1,
        };

        let decision_risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };
        let order_risk = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };

        let outcome = service
            .handle_live_decision(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                &mut signal,
                &candle,
                decision_risk,
                &order_risk,
            )
            .await
            .expect("handle_live_decision should succeed");

        assert!(outcome.opened_side.is_none());
        assert!(repo.get_saved_orders().is_empty());
    }

    #[tokio::test]
    async fn handle_live_decision_rolls_back_state_when_open_fails() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};
        use rust_quant_strategies::framework::backtest::BasicRiskStrategyConfig;

        let repo = Arc::new(MockSwapOrderRepository::new());
        let service = StrategyExecutionService::new(repo);
        service.configure_open_failure_for_test(true);

        let config = StrategyConfig {
            id: 4200,
            strategy_type: StrategyType::Vegas,
            exchange: None,
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::H4,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({ "max_loss_percent": 0.02 }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        let mut signal = create_buy_signal(100.0, 1_700_000_000_000);
        let candle = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 99.0,
            c: 100.0,
            v: 1.0,
            ts: 1_700_000_000_000,
            confirm: 1,
        };
        let decision_risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };
        let order_risk = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };

        let err = service
            .handle_live_decision(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                &mut signal,
                &candle,
                decision_risk,
                &order_risk,
            )
            .await
            .expect_err("mock open failure should make order placement fail");
        assert!(err.to_string().contains("mock open failed"));

        let reloaded = service
            .live_states
            .get(&config.id)
            .map(|v| v.clone())
            .unwrap_or_default();
        assert!(reloaded.trade_position.is_none());
    }

    #[tokio::test]
    async fn confirm_external_flat_close_requires_second_observation_without_inspection() {
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        let _ = init_redis_pool().await;

        let service = create_test_service();
        let config_id = 5200;
        let inst_id = "ETH-USDT-SWAP";
        let period = "4H";

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");

        let first = service
            .confirm_external_flat_close(config_id, inst_id, period, false)
            .await
            .expect("first observation should succeed");
        assert!(matches!(first, ExternalFlatDecision::Skip));

        let second = service
            .confirm_external_flat_close(config_id, inst_id, period, false)
            .await
            .expect("second observation should succeed");
        assert!(matches!(second, ExternalFlatDecision::Confirmed));

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");
    }

    #[tokio::test]
    async fn confirm_external_flat_close_confirms_immediately_with_inspection() {
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        let _ = init_redis_pool().await;

        let service = create_test_service();
        let config_id = 5300;
        let inst_id = "ETH-USDT-SWAP";
        let period = "4H";

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");

        let decision = service
            .confirm_external_flat_close(config_id, inst_id, period, true)
            .await
            .expect("inspection-backed observation should succeed");
        assert!(matches!(decision, ExternalFlatDecision::Confirmed));

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");
    }

    #[tokio::test]
    async fn opened_sync_failure_forces_close_when_compensation_cannot_restore_tpsl() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};

        let service = create_test_service();
        let config = StrategyConfig {
            id: 999,
            strategy_type: StrategyType::Vegas,
            exchange: None,
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::H4,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({"max_loss_percent": 0.02}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        let state = TradingState {
            trade_position: Some(TradePosition {
                trade_side: TradeSide::Long,
                position_nums: 1.0,
                open_price: 100.0,
                open_position_time: "2026-01-01 00:00:00".to_string(),
                signal_high_low_diff: 1.0,
                ..Default::default()
            }),
            ..TradingState::default()
        };
        service.live_states.insert(config.id, state);
        service.configure_guard_test_state(true, false, false);

        let err = service
            .enforce_opened_position_guard(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                TradeSide::Long,
                1_738_454_400_000,
            )
            .await
            .expect_err("guard should force close when no tp/sl can be restored");
        assert!(err
            .to_string()
            .contains("开仓后止盈止损同步失败，补偿未成功，已触发主动平仓"));

        let (compensate_calls, close_calls) = service.guard_test_calls();
        assert_eq!(compensate_calls, 1);
        assert_eq!(close_calls, 1);

        let reloaded = service
            .live_states
            .get(&config.id)
            .map(|v| v.clone())
            .unwrap_or_default();
        assert!(reloaded.trade_position.is_none());
        assert!(!service.has_live_algo_for_side(config.id, TradeSide::Long));
    }

    #[tokio::test]
    async fn opened_sync_failure_keeps_position_when_compensation_restores_tpsl() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};

        let service = create_test_service();
        let config = StrategyConfig {
            id: 1000,
            strategy_type: StrategyType::Vegas,
            exchange: None,
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::H4,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({"max_loss_percent": 0.02}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };
        service.configure_guard_test_state(false, true, false);

        let result = service
            .enforce_opened_position_guard(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                TradeSide::Long,
                1_738_454_400_000,
            )
            .await;
        assert!(result.is_ok());

        let (compensate_calls, close_calls) = service.guard_test_calls();
        assert_eq!(compensate_calls, 1);
        assert_eq!(close_calls, 0);
        assert!(service.has_live_algo_for_side(config.id, TradeSide::Long));
    }
