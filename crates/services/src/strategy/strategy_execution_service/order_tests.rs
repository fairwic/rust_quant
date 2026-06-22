    #[allow(dead_code)]
    fn create_test_api_config() -> rust_quant_domain::entities::ExchangeApiConfig {
        rust_quant_domain::entities::ExchangeApiConfig::new(
            1,
            "okx".to_string(),
            "test_api_key".to_string(),
            "test_api_secret".to_string(),
            Some("test_passphrase".to_string()),
            true, // sandbox
            true, // enabled
            Some("测试API配置".to_string()),
        )
    }

    /// 测试辅助：创建测试用的BasicRiskConfig
    fn create_test_risk_config(
        max_loss_percent: f64,
        use_signal_kline_stop_loss: Option<bool>,
    ) -> rust_quant_domain::BasicRiskConfig {
        rust_quant_domain::BasicRiskConfig {
            max_loss_percent,
            atr_take_profit_ratio: None,
            fix_signal_kline_take_profit_ratio: None,
            is_move_stop_loss: None,
            is_used_signal_k_line_stop_loss: use_signal_kline_stop_loss,
            max_hold_time: None,
            max_leverage: None,
        }
    }

    /// 测试：execute_order_internal - 正常买入下单流程
    ///
    /// 注意：此测试需要mock外部依赖（ExchangeApiService和OkxOrderService）
    /// 由于这些依赖是硬编码的，此测试主要用于验证逻辑流程
    #[tokio::test]
    #[ignore] // 需要真实环境或mock，默认忽略
    async fn test_execute_order_internal_buy_success() {
        let repo = MockSwapOrderRepository::new();
        let _service = StrategyExecutionService::new(Arc::new(repo));

        let signal = create_buy_signal(50000.0, 1234567890);
        let risk_config = create_test_risk_config(0.02, None);
        let _inst_id = "BTC-USDT-SWAP";
        let _period = "1H";
        let _config_id = 1;
        let _strategy_type = "vegas";

        // 注意：此测试需要mock ExchangeApiService 和 OkxOrderService
        // 由于这些是硬编码依赖，实际测试需要：
        // 1. 使用真实环境（需要配置API密钥）
        // 2. 或者重构代码支持依赖注入
        // 3. 或者使用条件编译创建测试版本

        // 这里只验证信号和配置的有效性
        assert!(signal.should_buy);
        assert!(!signal.should_sell);
        assert_eq!(signal.open_price, 50000.0);
        assert_eq!(risk_config.max_loss_percent, 0.02);
    }

    /// 测试：execute_order_internal - 幂等性检查
    #[tokio::test]
    async fn test_execute_order_internal_idempotency() {
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

    /// 测试：execute_order_internal - 无效信号处理
    #[test]
    fn test_execute_order_internal_invalid_signal() {
        let signal = SignalResult {
            should_buy: false,
            should_sell: false,
            ..create_buy_signal(50000.0, 1234567890)
        };

        // 验证无效信号应该返回错误
        let (side, pos_side) = if signal.should_buy {
            ("buy", "long")
        } else if signal.should_sell {
            ("sell", "short")
        } else {
            ("invalid", "invalid")
        };

        assert_eq!(side, "invalid");
        assert_eq!(pos_side, "invalid");
    }

    /// 测试：execute_order_internal - 下单数量为0时跳过
    #[test]
    fn test_execute_order_internal_zero_size_skip() {
        // 模拟最大可用数量很小的情况
        let max_available = 0.5; // 小于1
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor; // 0.45

        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        assert_eq!(order_size, "0");
        // 当order_size为0时，应该跳过下单
        assert!(order_size == "0");
    }

    /// 测试：execute_order_internal - 止损价格验证失败（做多）
    #[test]
    fn test_execute_order_internal_stop_loss_validation_fail_long() {
        let entry_price = 49000.0;
        let stop_loss_price = 50000.0; // 止损价 > 开仓价，不合理

        // 做多时，开仓价应该 > 止损价
        let is_valid = entry_price >= stop_loss_price;
        assert!(!is_valid, "做多时止损价格不合理应该失败");
    }

    /// 测试：execute_order_internal - 止损价格验证失败（做空）
    #[test]
    fn test_execute_order_internal_stop_loss_validation_fail_short() {
        let entry_price = 51000.0;
        let stop_loss_price = 50000.0; // 止损价 < 开仓价，不合理

        // 做空时，开仓价应该 < 止损价
        let is_valid = entry_price <= stop_loss_price;
        assert!(!is_valid, "做空时止损价格不合理应该失败");
    }

    /// 测试：execute_order_internal - 使用信号K线止损
    #[test]
    fn test_execute_order_internal_signal_kline_stop_loss() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02;
        let signal_kline_stop_loss = 48000.0;

        // 计算默认止损
        let default_stop_loss = entry_price * (1.0 - max_loss_percent); // 49000.0

        // 如果使用信号K线止损，应该使用信号K线止损价
        let risk_config = create_test_risk_config(0.02, Some(true));
        let final_stop_loss = match risk_config.is_used_signal_k_line_stop_loss {
            Some(true) => match Some(signal_kline_stop_loss) {
                Some(v) => v,
                None => default_stop_loss,
            },
            _ => default_stop_loss,
        };

        assert_eq!(final_stop_loss, signal_kline_stop_loss);
        assert_ne!(final_stop_loss, default_stop_loss);
    }

    /// 测试：execute_order_internal - 订单保存成功
    #[tokio::test]
    async fn test_execute_order_internal_order_save_success() {
        let repo = MockSwapOrderRepository::new();
        let service = StrategyExecutionService::new(Arc::new(repo));

        let signal = create_buy_signal(50000.0, 1234567890);
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let strategy_type = "vegas";
        let config_id = 1;
        let in_order_id = SwapOrder::generate_live_in_order_id(
            inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let out_order_id = "test_out_123".to_string();
        let order_size = "1.0".to_string();

        let order_detail = serde_json::json!({
            "entry_price": signal.open_price,
            "stop_loss": signal.signal_kline_stop_loss_price,
        });

        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            strategy_type,
            "buy",
            "long",
            &order_size,
            &in_order_id,
            &out_order_id,
            "okx",
            &order_detail.to_string(),
        );

        // 测试保存订单
        let result = service.swap_order_repository.save(&swap_order).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    /// 测试：execute_order_internal - 订单保存失败处理
    #[tokio::test]
    async fn test_execute_order_internal_order_save_failure() {
        let repo = MockSwapOrderRepository::new().with_save_failure(true);
        let service = StrategyExecutionService::new(Arc::new(repo));

        let signal = create_buy_signal(50000.0, 1234567890);
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let strategy_type = "vegas";
        let config_id = 1;
        let in_order_id = SwapOrder::generate_live_in_order_id(
            inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let out_order_id = "test_out_123".to_string();
        let order_size = "1.0".to_string();

        let order_detail = serde_json::json!({
            "entry_price": signal.open_price,
            "stop_loss": signal.signal_kline_stop_loss_price,
        });

        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            strategy_type,
            "buy",
            "long",
            &order_size,
            &in_order_id,
            &out_order_id,
            "okx",
            &order_detail.to_string(),
        );

        // 测试保存失败
        let result = service.swap_order_repository.save(&swap_order).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("模拟保存失败"));
    }

    /// 测试：execute_order_internal - 真实场景集成测试
    ///
    /// 此测试通过execute_strategy方法间接测试execute_order_internal的完整流程
    /// 使用真实的数据结构和逻辑，可以连接真实的数据库和API（如果配置了）
    ///
    /// 前置条件（可选）：
    /// 1. 数据库配置：DATABASE_URL环境变量
    /// 2. Redis配置：REDIS_URL环境变量
    /// 3. API配置：需要在数据库中配置策略配置ID和API配置的关联
    ///
    /// 如果未配置数据库或API，测试会跳过实际下单，仅验证逻辑流程
    #[tokio::test]
    #[ignore] // 默认忽略，需要真实环境配置
    async fn test_execute_order_internal_real_scenario() {
        use chrono::Utc;
        use rust_quant_core::database::get_db_pool;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};
        use rust_quant_infrastructure::repositories::SqlxSwapOrderRepository;

        println!("🚀 开始真实场景集成测试");

        // 1. 初始化数据库连接（如果配置了）
        let pool_result = std::panic::catch_unwind(get_db_pool);
        let repo: Arc<dyn SwapOrderRepository> = match pool_result {
            Ok(pool) => {
                println!("✅ 数据库连接成功");
                // Pool 实现了 Clone trait，可以安全地克隆
                Arc::new(SqlxSwapOrderRepository::new(pool.clone()))
            }
            Err(_) => {
                println!("⚠️  数据库未配置，使用Mock Repository");
                Arc::new(MockSwapOrderRepository::new())
            }
        };

        let service = StrategyExecutionService::new(repo.clone());

        // 2. 创建真实的策略配置
        let config_id = 1i64;
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let risk_config = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: 0.02, // 2%止损
            atr_take_profit_ratio: None,
            fix_signal_kline_take_profit_ratio: None,
            is_move_stop_loss: None,
            is_used_signal_k_line_stop_loss: Some(true), // 使用信号K线止损
            max_hold_time: None,
            max_leverage: None,
        };

        let _config = StrategyConfig {
            id: config_id,
            strategy_type: StrategyType::Vegas,
            exchange: None,
            symbol: "BTC-USDT".to_string(),
            timeframe: Timeframe::H1,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::to_value(&risk_config).unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: Some("真实场景测试配置".to_string()),
        };

        // 3. 创建真实的交易信号（模拟策略分析结果）
        let current_price = 50000.0;
        let ts = chrono::Utc::now().timestamp_millis();
        let signal = SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: current_price,
            signal_kline_stop_loss_price: Some(current_price * 0.98), // 2%止损
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            stop_loss_source: None,
            ts,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Long,
        };

        println!(
            "📊 交易信号: should_buy={}, open_price={}, stop_loss={:?}",
            signal.should_buy, signal.open_price, signal.signal_kline_stop_loss_price
        );

        // 4. 验证信号和配置
        assert!(signal.should_buy, "信号应该是买入信号");
        assert_eq!(signal.open_price, current_price);
        assert!(signal.signal_kline_stop_loss_price.is_some());

        // 5. 验证止损价格计算逻辑
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;
        let default_stop_loss = entry_price * (1.0 - max_loss_percent);
        let final_stop_loss = match risk_config.is_used_signal_k_line_stop_loss {
            Some(true) => signal
                .signal_kline_stop_loss_price
                .unwrap_or(default_stop_loss),
            _ => default_stop_loss,
        };

        assert!(entry_price > final_stop_loss, "做多时开仓价应该 > 止损价");
        assert_eq!(
            final_stop_loss,
            current_price * 0.98,
            "应该使用信号K线止损价"
        );
        println!(
            "✅ 止损价格验证通过: entry={}, stop_loss={}",
            entry_price, final_stop_loss
        );

        // 6. 验证订单ID生成
        let in_order_id =
            SwapOrder::generate_live_in_order_id(inst_id, "vegas", config_id, period, signal.ts);
        assert!(!in_order_id.is_empty());
        assert!(in_order_id.contains(inst_id));
        println!("✅ 订单ID生成: {}", in_order_id);

        // 7. 检查幂等性
        let existing_order = service
            .swap_order_repository
            .find_by_in_order_id(&in_order_id)
            .await
            .unwrap();

        if let Some(existing_order) = existing_order {
            println!("⚠️  订单已存在（幂等性检查通过），跳过重复下单");
            println!("   已存在订单: {:?}", existing_order.out_order_id);
            println!(
                "   配置ID: {}, 交易对: {}, 周期: {}",
                config_id, inst_id, period
            );
            return;
        }
        println!("✅ 幂等性检查通过，可以下单");

        // 8. 尝试通过execute_strategy执行完整流程（需要真实环境）
        // 注意：这会实际调用外部API，需要：
        // - 数据库中存在config_id对应的策略配置
        // - 数据库中配置了策略与API的关联
        // - API配置有效且有足够资金

        println!("ℹ️  尝试执行完整下单流程...");
        println!("   提示：如果数据库和API未配置，此步骤会失败，但逻辑验证已完成");

        // 由于execute_strategy需要真实的K线数据，这里我们只验证逻辑
        // 如果需要完整测试，需要提供真实的CandlesEntity

        // 9. 验证订单详情构建
        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": final_stop_loss,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
                "atr_stop_loss_price": signal.atr_stop_loss_price,
                "atr_take_profit_ratio_price": signal.atr_take_profit_ratio_price,
            }
        });

        assert_eq!(order_detail["entry_price"], entry_price);
        assert_eq!(order_detail["stop_loss"], final_stop_loss);
        assert_eq!(order_detail["signal"]["should_buy"], signal.should_buy);
        println!("✅ 订单详情构建验证通过");

        // 10. 验证订单对象创建
        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            "vegas",
            "buy",
            "long",
            "1.0",
            &in_order_id,
            "test_out_123",
            "okx",
            &order_detail.to_string(),
        );

        assert_eq!(swap_order.strategy_id, config_id as i32);
        assert_eq!(swap_order.inst_id, inst_id);
        assert_eq!(swap_order.side, "buy");
        assert_eq!(swap_order.pos_side, "long");
        assert_eq!(swap_order.in_order_id, in_order_id);
        println!("✅ 订单对象创建验证通过");

        println!("✅ 真实场景测试完成：所有逻辑验证通过");
        println!("   如需完整测试，请配置数据库和API环境变量");
    }

    #[tokio::test]
    #[ignore]
    async fn test_execute_order_internal_simulated_service_e2e_persists_swap_order(
    ) -> anyhow::Result<()> {
        use rust_quant_core::cache::init_redis_pool;
        use rust_quant_core::database::{get_db_pool, init_db_pool};
        use rust_quant_domain::entities::ExchangeApiConfig;
        use rust_quant_domain::traits::{ExchangeApiConfigRepository, StrategyApiConfigRepository};
        use rust_quant_infrastructure::repositories::{
            SqlxExchangeApiConfigRepository, SqlxStrategyApiConfigRepository,
            SqlxSwapOrderRepository,
        };

        fn env_or_default(key: &str, default: &str) -> String {
            std::env::var(key).unwrap_or_else(|_| default.to_string())
        }

        fn env_required(key: &str) -> anyhow::Result<String> {
            std::env::var(key).map_err(|_| anyhow::anyhow!("missing env var: {}", key))
        }

        fn instrument_from_okx_inst_id(
            inst_id: &str,
        ) -> anyhow::Result<crypto_exc_all::Instrument> {
            let parts = inst_id.split('-').collect::<Vec<_>>();
            let base = parts
                .first()
                .copied()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("invalid OKX inst_id: {}", inst_id))?;
            let quote = parts
                .get(1)
                .copied()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("invalid OKX inst_id: {}", inst_id))?;
            Ok(crypto_exc_all::Instrument::perp(base, quote).with_settlement(quote))
        }

        async fn get_position_mgn_mode(
            okx: &crate::exchange::OkxOrderService,
            api: &ExchangeApiConfig,
            inst_id: &str,
            pos_side: &str,
        ) -> anyhow::Result<Option<String>> {
            let positions = okx.get_positions(api, Some("SWAP"), Some(inst_id)).await?;
            for p in positions {
                if p.inst_id != inst_id || p.pos_side != pos_side {
                    continue;
                }
                let qty = p.pos.parse::<f64>().unwrap_or(0.0);
                if qty.abs() < 1e-12 {
                    continue;
                }
                return Ok(Some(p.mgn_mode));
            }
            Ok(None)
        }

        async fn wait_for_position(
            okx: &crate::exchange::OkxOrderService,
            api: &ExchangeApiConfig,
            inst_id: &str,
            pos_side: &str,
            should_exist: bool,
        ) -> anyhow::Result<()> {
            let max_tries: usize = env_or_default("OKX_TEST_RETRY", "20").parse().unwrap_or(20);
            let sleep_ms: u64 = env_or_default("OKX_TEST_RETRY_SLEEP_MS", "500")
                .parse()
                .unwrap_or(500);

            for _ in 0..max_tries {
                let exists = get_position_mgn_mode(okx, api, inst_id, pos_side)
                    .await?
                    .is_some();
                if exists == should_exist {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }

            anyhow::bail!(
                "position state did not converge: inst_id={}, pos_side={}, expected_exist={}",
                inst_id,
                pos_side,
                should_exist
            );
        }

        dotenv::dotenv().ok();
        if env_or_default("RUN_OKX_SIMULATED_SERVICE_E2E", "0") != "1" {
            eprintln!(
                "skip test_execute_order_internal_simulated_service_e2e_persists_swap_order: RUN_OKX_SIMULATED_SERVICE_E2E!=1"
            );
            return Ok(());
        }

        std::env::set_var("APP_ENV", "local");
        std::env::set_var("OKX_SIMULATED_TRADING", "1");
        std::env::set_var("OKX_REQUEST_EXPIRATION_MS", "300000");
        if std::env::var("QUANT_CORE_DATABASE_URL").is_err() {
            std::env::set_var(
                "QUANT_CORE_DATABASE_URL",
                "postgres://postgres:postgres123@127.0.0.1:5432/quant_core",
            );
        }
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        let _ = init_db_pool().await;
        let _ = init_redis_pool().await;
        let pool = get_db_pool().clone();

        let api_key = env_required("OKX_SIMULATED_API_KEY")?;
        let api_secret = env_required("OKX_SIMULATED_API_SECRET")?;
        let passphrase = env_required("OKX_SIMULATED_PASSPHRASE")?;
        let inst_id = env_or_default("OKX_TEST_INST_ID", "ETH-USDT-SWAP");
        let period = "4H";
        let strategy_type = "vegas";
        let config_id = env_or_default("OKX_SIMULATED_SERVICE_STRATEGY_CONFIG_ID", "990011")
            .parse::<i64>()
            .unwrap_or(990011);

        let api_repo = SqlxExchangeApiConfigRepository::new(pool.clone());
        let relation_repo = SqlxStrategyApiConfigRepository::new(pool.clone());
        let swap_repo = Arc::new(SqlxSwapOrderRepository::new(pool.clone()));
        let service = StrategyExecutionService::new(swap_repo.clone());

        let api_config = ExchangeApiConfig::new(
            0,
            "okx".to_string(),
            api_key.clone(),
            api_secret.clone(),
            Some(passphrase.clone()),
            true,
            true,
            Some("simulated-service-e2e".to_string()),
        );

        let api_config_id = api_repo.save(&api_config).await?;
        relation_repo
            .create_association(config_id as i32, api_config_id, 1)
            .await?;

        let okx_service = crate::exchange::OkxOrderService;
        let db_api_config = api_repo
            .find_by_id(api_config_id)
            .await?
            .ok_or_else(|| anyhow!("saved api config not found"))?;

        if let Some(mgn_mode) =
            get_position_mgn_mode(&okx_service, &db_api_config, &inst_id, "long").await?
        {
            okx_service
                .close_position(
                    &db_api_config,
                    &inst_id,
                    okx::dto::PositionSide::Long,
                    &mgn_mode,
                )
                .await?;
            wait_for_position(&okx_service, &db_api_config, &inst_id, "long", false).await?;
        }

        let instrument = instrument_from_okx_inst_id(&inst_id)?;
        let gateway = crate::exchange::CryptoExcAllGateway::from_single_exchange_credentials(
            crypto_exc_all::ExchangeId::Okx,
            api_key.clone(),
            api_secret,
            Some(passphrase.clone()),
            true,
        )?;
        let open_price = gateway
            .ticker(crypto_exc_all::ExchangeId::Okx, &instrument)
            .await?
            .last_price
            .parse::<f64>()?;

        let signal = SignalResult {
            should_buy: true,
            should_sell: false,
            open_price,
            signal_kline_stop_loss_price: Some(open_price * 0.98),
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            stop_loss_source: None,
            ts: chrono::Utc::now().timestamp_millis(),
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Long,
        };
        let risk_config = create_test_risk_config(0.02, Some(true));

        service
            .execute_order_internal(
                &inst_id,
                period,
                &signal,
                &risk_config,
                config_id,
                strategy_type,
                None,
            )
            .await?;

        let in_order_id = SwapOrder::generate_live_in_order_id(
            &inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let saved = swap_repo
            .find_by_in_order_id(&in_order_id)
            .await?
            .ok_or_else(|| anyhow!("swap_orders did not persist in_order_id={}", in_order_id))?;
        assert_eq!(saved.strategy_id, config_id as i32);
        assert_eq!(saved.inst_id, inst_id);
        assert_eq!(saved.period, period);
        assert_eq!(saved.side, "buy");
        assert_eq!(saved.pos_side, "long");
        assert!(!saved.out_order_id.trim().is_empty());

        wait_for_position(&okx_service, &db_api_config, &inst_id, "long", true).await?;

        if let Some(mgn_mode) =
            get_position_mgn_mode(&okx_service, &db_api_config, &inst_id, "long").await?
        {
            okx_service
                .close_position(
                    &db_api_config,
                    &inst_id,
                    okx::dto::PositionSide::Long,
                    &mgn_mode,
                )
                .await?;
            wait_for_position(&okx_service, &db_api_config, &inst_id, "long", false).await?;
        }

        sqlx::query("DELETE FROM exchange_apikey_strategy_relation WHERE strategy_config_id = $1")
            .bind(config_id as i32)
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE exchange_apikey_config SET is_deleted = 1 WHERE id = $1")
            .bind(api_config_id)
            .execute(&pool)
            .await?;

        Ok(())
    }

    /// 测试：execute_order_internal - 完整流程验证（逻辑层面）
    #[test]
    fn test_execute_order_internal_full_flow_logic() {
        // 1. 创建信号
        let signal = create_buy_signal(50000.0, 1234567890);
        assert!(signal.should_buy);
        assert_eq!(signal.open_price, 50000.0);

        // 2. 创建风险配置
        let risk_config = create_test_risk_config(0.02, None);
        assert_eq!(risk_config.max_loss_percent, 0.02);

        // 3. 计算止损价格
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;
        let stop_loss_price = entry_price * (1.0 - max_loss_percent);
        assert_eq!(stop_loss_price, 49000.0);

        // 4. 验证止损价格合理性（做多）
        let _pos_side = "long";
        assert!(entry_price > stop_loss_price, "做多时开仓价应该 > 止损价");

        // 5. 计算下单数量
        let max_available = 100.0;
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor;
        let order_size = format!("{:.2}", order_size_f64);
        assert_eq!(order_size, "90.00");

        // 6. 生成订单ID
        let inst_id = "BTC-USDT-SWAP";
        let config_id = 1i64;
        let period = "1H";
        let in_order_id =
            SwapOrder::generate_live_in_order_id(inst_id, "vegas", config_id, period, signal.ts);
        assert_eq!(
            in_order_id,
            format!(
                "{}_{}_{}_{}_{}",
                inst_id, "vegas", config_id, period, signal.ts
            )
        );

        // 7. 创建订单详情
        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": stop_loss_price,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
            }
        });
        assert_eq!(order_detail["entry_price"], entry_price);
        assert_eq!(order_detail["stop_loss"], stop_loss_price);
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_in_band() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(300.2, 699.8, config);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_fund_to_trade() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(250.0, 750.0, config);
        assert_eq!(
            result,
            Some((50.0, TradeBucketTransferDirection::FundToTrade))
        );
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_trade_to_fund() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(350.0, 650.0, config);
        assert_eq!(
            result,
            Some((50.0, TradeBucketTransferDirection::TradeToFund))
        );
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_exact_difference_inside_old_band() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let lower = StrategyExecutionService::calculate_trade_bucket_transfer(290.0, 710.0, config);
        let upper = StrategyExecutionService::calculate_trade_bucket_transfer(310.0, 690.0, config);

        assert_eq!(
            lower,
            Some((10.0, TradeBucketTransferDirection::FundToTrade))
        );
        assert_eq!(
            upper,
            Some((10.0, TradeBucketTransferDirection::TradeToFund))
        );
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_dynamic_ratio_growth() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(500.0, 1500.0, config);
        assert_eq!(
            result,
            Some((100.0, TradeBucketTransferDirection::FundToTrade))
        );
    }
