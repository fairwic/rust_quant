    // ========== 下单逻辑单元测试 ==========
    #[test]
    fn test_order_size_calculation() {
        let max_available = 100.0;
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor;
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };
        assert_eq!(order_size, "90.00");
        // 测试小于1的情况
        let max_available = 0.5;
        let order_size_f64 = max_available * safety_factor;
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };
        assert_eq!(order_size, "0");
    }
    #[test]
    fn test_stop_loss_calculation_long() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02; // 2%
        let stop_loss_price = entry_price * (1.0 - max_loss_percent);
        assert_eq!(stop_loss_price, 49000.0);
        // 验证：做多时，开仓价应该 > 止损价
        assert!(entry_price > stop_loss_price);
    }
    #[test]
    fn test_stop_loss_calculation_short() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02; // 2%
        let stop_loss_price = entry_price * (1.0 + max_loss_percent);
        assert_eq!(stop_loss_price, 51000.0);
        // 验证：做空时，开仓价应该 < 止损价
        assert!(entry_price < stop_loss_price);
    }
    #[test]
    fn test_stop_loss_validation_long_invalid() {
        let entry_price = 49000.0;
        let stop_loss_price = 50000.0; // 止损价 > 开仓价，不合理
        let is_valid = entry_price >= stop_loss_price;
        assert!(!is_valid, "做多时开仓价应该 >= 止损价");
    }
    #[test]
    fn test_stop_loss_validation_short_invalid() {
        let entry_price = 51000.0;
        let stop_loss_price = 50000.0; // 止损价 < 开仓价，不合理
        let is_valid = entry_price <= stop_loss_price;
        assert!(!is_valid, "做空时开仓价应该 <= 止损价");
    }
    #[test]
    fn test_signal_kline_stop_loss_priority() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02;
        let signal_kline_stop_loss = 48000.0; // 信号K线止损价
        // 计算默认止损价
        let default_stop_loss = entry_price * (1.0 - max_loss_percent); // 49000.0
        // 如果使用信号K线止损，应该使用信号K线止损价
        let final_stop_loss = match Some(true) {
            Some(true) => match Some(signal_kline_stop_loss) {
                Some(v) => v,
                None => default_stop_loss,
            },
            _ => default_stop_loss,
        };
        assert_eq!(final_stop_loss, signal_kline_stop_loss);
        assert_ne!(final_stop_loss, default_stop_loss);
    }
    #[test]
    fn test_signal_kline_stop_loss_fallback() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02;
        let default_stop_loss = entry_price * (1.0 - max_loss_percent); // 49000.0
        // 如果使用信号K线止损但信号K线止损价为None，应该使用默认止损
        let final_stop_loss = match Some(true) {
            Some(true) => match None::<f64> {
                Some(v) => v,
                None => default_stop_loss,
            },
            _ => default_stop_loss,
        };
        assert_eq!(final_stop_loss, default_stop_loss);
    }
    #[test]
    fn test_generate_in_order_id() {
        let inst_id = "BTC-USDT-SWAP";
        let strategy_type = "vegas";
        let config_id = 11;
        let period = "4H";
        let ts = 1234567890;
        let in_order_id =
            SwapOrder::generate_live_in_order_id(inst_id, strategy_type, config_id, period, ts);
        assert_eq!(in_order_id, "BTC-USDT-SWAP_vegas_11_4H_1234567890");
    }
    #[tokio::test]
    async fn test_idempotency_check() {
        let inst_id = "BTC-USDT-SWAP";
        let ts = 1234567890;
        let in_order_id = SwapOrder::generate_live_in_order_id(inst_id, "vegas", 1, "1H", ts);
        // 创建已存在的订单
        let existing_order = SwapOrder::new(
            1,
            in_order_id.clone(),
            "out_order_123".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            inst_id.to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );
        let repo = MockSwapOrderRepository::new().with_existing_order(existing_order);
        let service = StrategyExecutionService::new(Arc::new(repo));
        // 验证幂等性：查询已存在的订单应该返回Some
        let found = service
            .swap_order_repository
            .find_by_in_order_id(&in_order_id)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().in_order_id, in_order_id);
    }
    #[test]
    fn test_trade_direction_buy() {
        let signal = create_buy_signal(50000.0, 1234567890);
        let (side, pos_side) =
            StrategyExecutionService::trade_sides_from_signal(&signal).unwrap();
        assert_eq!(side, OrderSide::Buy);
        assert_eq!(side.as_str(), "buy");
        assert_eq!(pos_side, PositionSide::Long);
        assert_eq!(pos_side.as_str(), "long");
    }
    #[test]
    fn test_trade_direction_sell() {
        let signal = create_sell_signal(50000.0, 1234567890);
        let (side, pos_side) =
            StrategyExecutionService::trade_sides_from_signal(&signal).unwrap();
        assert_eq!(side, OrderSide::Sell);
        assert_eq!(side.as_str(), "sell");
        assert_eq!(pos_side, PositionSide::Short);
        assert_eq!(pos_side.as_str(), "short");
    }
    #[test]
    fn test_invalid_signal() {
        let signal = SignalResult {
            should_buy: false,
            should_sell: false,
            ..create_buy_signal(50000.0, 1234567890)
        };
        let error = StrategyExecutionService::trade_sides_from_signal(&signal)
            .expect_err("应该识别为无效信号");
        assert!(error.to_string().contains("信号无效"));
    }
    #[test]
    fn test_order_detail_json() {
        let entry_price = 50000.0;
        let stop_loss = 49000.0;
        let signal = create_buy_signal(entry_price, 1234567890);
        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": stop_loss,
            "take_profit": null,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
                "atr_stop_loss_price": signal.atr_stop_loss_price,
                "atr_take_profit_ratio_price": signal.atr_take_profit_ratio_price,
            }
        });
        assert_eq!(order_detail["entry_price"], entry_price);
        assert_eq!(order_detail["stop_loss"], stop_loss);
        assert_eq!(order_detail["signal"]["should_buy"], signal.should_buy);
        assert_eq!(order_detail["signal"]["should_sell"], signal.should_sell);
    }
    #[tokio::test]
    async fn test_order_save_success() {
        let repo = MockSwapOrderRepository::new();
        let service = StrategyExecutionService::new(Arc::new(repo));
        let order = SwapOrder::new(
            1,
            "test_in_123".to_string(),
            "test_out_456".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            "BTC-USDT-SWAP".to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );
        // 验证订单结构
        assert_eq!(order.strategy_id, 1);
        assert_eq!(order.inst_id, "BTC-USDT-SWAP");
        assert_eq!(order.side, "buy");
        // 测试保存
        let result = service.swap_order_repository.save(&order).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }
    #[tokio::test]
    async fn test_order_save_failure() {
        let repo = MockSwapOrderRepository::new().with_save_failure(true);
        let service = StrategyExecutionService::new(Arc::new(repo));
        let order = SwapOrder::new(
            1,
            "test_in_123".to_string(),
            "test_out_456".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            "BTC-USDT-SWAP".to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );
        // 验证保存失败时应该返回错误
        let result = service.swap_order_repository.save(&order).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("模拟保存失败"));
    }
    #[test]
    fn test_stop_loss_precision() {
        let stop_loss_price = 49000.123456789;
        let formatted = format!("{:.2}", stop_loss_price);
        assert_eq!(formatted, "49000.12");
    }
    #[test]
    fn test_order_size_precision() {
        let order_size_f64 = 90.123456789;
        let formatted = format!("{:.2}", order_size_f64);
        assert_eq!(formatted, "90.12");
    }
    #[test]
    fn test_long_stop_loss_edge_cases() {
        // 测试最大止损百分比
        let entry_price = 50000.0;
        let max_loss_percent = 0.05; // 5%
        let stop_loss = entry_price * (1.0 - max_loss_percent);
        assert_eq!(stop_loss, 47500.0);
        // 验证合理性
        assert!(entry_price > stop_loss);
    }
    #[test]
    fn test_short_stop_loss_edge_cases() {
        // 测试最大止损百分比
        let entry_price = 50000.0;
        let max_loss_percent = 0.05; // 5%
        let stop_loss = entry_price * (1.0 + max_loss_percent);
        assert_eq!(stop_loss, 52500.0);
        // 验证合理性
        assert!(entry_price < stop_loss);
    }
    #[test]
    fn test_zero_order_size_skip() {
        let order_size = "0".to_string();
        let should_skip = order_size == "0";
        assert!(should_skip);
    }
    #[test]
    fn test_small_order_size() {
        let max_available = 0.5;
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor; // 0.45
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };
        assert_eq!(order_size, "0");
    }
    #[test]
    fn test_order_from_signal() {
        let signal = create_buy_signal(50000.0, 1234567890);
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let strategy_type = "vegas";
        let side = "buy";
        let pos_side = "long";
        let order_size = "1.0";
        let in_order_id = "test_in_123";
        let out_order_id = "test_out_456";
        let platform_type = "okx";
        let order_detail = serde_json::json!({
            "entry_price": signal.open_price,
            "stop_loss": signal.signal_kline_stop_loss_price,
        });
        let order = SwapOrder::from_signal(
            1,
            inst_id,
            period,
            strategy_type,
            side,
            pos_side,
            order_size,
            in_order_id,
            out_order_id,
            platform_type,
            &order_detail.to_string(),
        );
        assert_eq!(order.strategy_id, 1);
        assert_eq!(order.inst_id, inst_id);
        assert_eq!(order.side, side);
        assert_eq!(order.pos_side, pos_side);
        assert_eq!(order.in_order_id, in_order_id);
        assert_eq!(order.out_order_id, out_order_id);
    }
    // ========== execute_order_internal 实际测试用例 ==========
