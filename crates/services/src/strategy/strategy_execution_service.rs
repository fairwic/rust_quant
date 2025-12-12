//! 策略执行服务
//!
//! 协调策略分析、风控检查、订单创建的完整业务流程

use std::sync::Arc;

use anyhow::{anyhow, Result};
use tracing::{error, info, warn};

use rust_quant_domain::entities::SwapOrder;
use rust_quant_domain::traits::SwapOrderRepository;
use rust_quant_domain::StrategyConfig;
use rust_quant_market::models::CandlesEntity;
use rust_quant_strategies::strategy_common::SignalResult;

/// 策略执行服务
///
/// 职责：
/// 1. 协调策略分析流程
/// 2. 调用风控检查
/// 3. 协调订单创建
/// 4. 管理策略执行状态
///
/// 依赖：
/// - StrategyRegistry: 获取策略实现
/// - SwapOrderRepository: 订单持久化
/// - TradingService: 创建订单（待实现）
/// - RiskService: 风控检查（待实现）
pub struct StrategyExecutionService {
    /// 合约订单仓储（依赖注入）
    swap_order_repository: Arc<dyn SwapOrderRepository>,
}

impl StrategyExecutionService {
    /// 创建新的策略执行服务（依赖注入）
    pub fn new(swap_order_repository: Arc<dyn SwapOrderRepository>) -> Self {
        Self {
            swap_order_repository,
        }
    }

    /// 执行策略分析和交易流程
    ///
    /// 参考原始业务逻辑：src/trading/strategy/executor_common.rs::execute_order
    ///
    /// 完整业务流程：
    /// 1. 验证配置
    /// 2. 执行策略分析，获取信号
    /// 3. 检查信号有效性
    /// 4. 记录信号日志（异步，不阻塞）
    /// 5. 解析风险配置
    /// 6. 执行下单（待完整实现）
    pub async fn execute_strategy(
        &self,
        inst_id: &str,
        period: &str,
        config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<SignalResult> {
        info!(
            "开始执行策略: type={:?}, symbol={}, period={}",
            config.strategy_type, inst_id, period
        );

        // 1. 验证配置
        self.validate_config(config)?;

        // 2. 获取策略实现
        use rust_quant_strategies::strategy_registry::get_strategy_registry;

        let strategy_executor = get_strategy_registry()
            .detect_strategy(&config.parameters.to_string())
            .map_err(|e| anyhow!("策略类型检测失败: {}", e))?;

        info!("使用策略: {}", strategy_executor.name());

        // 3. 执行策略分析，获取交易信号
        let signal = strategy_executor
            .execute(inst_id, period, config, snap)
            .await
            .map_err(|e| {
                error!("策略执行失败: {}", e);
                anyhow!("策略分析失败: {}", e)
            })?;

        info!("策略分析完成");

        // 4. 检查信号有效性（参考：executor_common.rs:106-112）
        let has_signal = signal.should_buy || signal.should_sell;

        if !has_signal {
            info!(
                "无交易信号，跳过下单 - 策略类型：{:?}, 交易周期：{}",
                config.strategy_type, period
            );
            return Ok(signal);
        }

        // 5. 记录信号（参考：executor_common.rs:114-122）
        warn!(
            "{:?} 策略信号！inst_id={}, period={}, should_buy={:?}, should_sell={:?}, ts={:?}",
            config.strategy_type, inst_id, period, signal.should_buy, signal.should_sell, signal.ts
        );

        // 6. 异步记录信号日志（不阻塞下单）
        self.save_signal_log_async(inst_id, period, &signal, config);

        // 7. 解析风险配置
        let risk_config: rust_quant_domain::BasicRiskConfig =
            serde_json::from_value(config.risk_config.clone())
                .map_err(|e| anyhow!("解析风险配置失败: {}", e))?;

        info!("风险配置: risk_config:{:#?}", risk_config);

        // 8. 执行下单
        if let Err(e) = self
            .execute_order_internal(inst_id, period, &signal, &risk_config, config.id, config.strategy_type.as_str())
            .await
        {
            error!("❌ {:?} 策略下单失败: {}", config.strategy_type, e);
            return Err(e);
        }

        info!("✅ {:?} 策略执行完成", config.strategy_type);
        Ok(signal)
    }

    /// 批量执行多个策略
    pub async fn execute_multiple_strategies(
        &self,
        inst_id: &str,
        period: &str,
        configs: Vec<StrategyConfig>,
    ) -> Result<Vec<SignalResult>> {
        let total = configs.len();
        info!("批量执行 {} 个策略", total);

        let mut results = Vec::with_capacity(total);

        for config in configs {
            match self.execute_strategy(inst_id, period, &config, None).await {
                Ok(signal) => results.push(signal),
                Err(e) => {
                    error!("策略执行失败: config_id={}, error={}", config.id, e);
                    // 继续执行其他策略
                }
            }
        }

        info!("批量执行完成: 成功 {}/{}", results.len(), total);
        Ok(results)
    }

    /// 获取K线数据（内部辅助方法）
    /// TODO: 实现数据获取逻辑
    #[allow(dead_code)]
    async fn get_candles(
        &self,
        _inst_id: &str,
        _period: &str,
        _limit: usize,
    ) -> Result<Vec<rust_quant_domain::Candle>> {
        // TODO: 通过market服务获取数据
        // 暂时返回错误
        Err(anyhow!("get_candles 暂未实现"))
    }

    /// 异步记录信号日志（不阻塞主流程）
    ///
    /// 参考原始逻辑：src/trading/task/strategy_runner.rs::save_signal_log (641-669行)
    fn save_signal_log_async(
        &self,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        config: &StrategyConfig,
    ) {
        let signal_json = match serde_json::to_string(&signal) {
            Ok(s) => s,
            Err(e) => {
                error!("序列化信号失败: {}", e);
                format!("{:?}", signal)
            }
        };

        let inst_id = inst_id.to_string();
        let period = period.to_string();
        let strategy_type = config.strategy_type.as_str().to_string();

        // 异步记录，不阻塞下单流程
        tokio::spawn(async move {
            use rust_quant_infrastructure::SignalLogRepository;

            let repo = SignalLogRepository::new();

            match repo
                .save_signal_log(&inst_id, &period, &strategy_type, &signal_json)
                .await
            {
                Ok(_) => {
                    info!("✅ 信号日志已记录: inst_id={}, period={}", inst_id, period);
                }
                Err(e) => {
                    error!("❌ 写入信号日志失败: {}", e);
                }
            }
        });
    }

    /// 执行下单（内部方法）
    ///
    /// 参考原始逻辑：
    /// - src/trading/strategy/executor_common.rs::execute_order (99-153行)
    /// - src/trading/services/order_service/swap_order_service.rs::ready_to_order (197-560行)
    ///
    /// 完整业务流程：
    /// 1. 幂等性检查（避免重复下单）
    /// 2. 获取当前持仓和可用资金
    /// 3. 计算下单数量
    /// 4. 风控检查（止损止盈价格验证）
    /// 5. 实际下单到交易所
    async fn execute_order_internal(
        &self,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
        config_id: i64,
        strategy_type: &str,
    ) -> Result<()> {
        info!(
            "准备下单: inst_id={}, period={}, config_id={}",
            inst_id, period, config_id
        );

        // 0) 幂等性：同一策略配置 + 同一信号时间戳 + 同一方向，只允许下单一次
        // 说明：
        // - WS 重连/重复推送、系统重启、上游重复触发都会导致重复进入下单逻辑
        // - in_order_id 用作幂等键（同时可作为业务追踪ID）
        let in_order_id = SwapOrder::generate_in_order_id(inst_id, "strategy", signal.ts);
        match self.swap_order_repository.find_by_in_order_id(&in_order_id).await? {
            Some(existing) => {
                warn!(
                    "⚠️ 幂等命中，跳过重复下单: inst_id={}, period={}, config_id={}, in_order_id={}, out_order_id={:?}",
                    inst_id,
                    period,
                    config_id,
                    in_order_id,
                    existing.out_order_id
                );
                return Ok(());
            }
            None => {}
        }

        // 1. 确定交易方向
        let (side, pos_side) = if signal.should_buy {
            ("buy", "long")
        } else if signal.should_sell {
            ("sell", "short")
        } else {
            return Err(anyhow!("信号无效，无交易方向"));
        };

        info!("交易方向: side={}, pos_side={}", side, pos_side);

        // 3. 获取API配置（从Redis缓存或数据库）
        use crate::exchange::create_exchange_api_service;
        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| {
                error!("获取API配置失败: config_id={}, error={}", config_id, e);
                anyhow!("获取API配置失败: {}", e)
            })?;

        info!(
            "使用API配置: exchange={}, api_key={}...",
            api_config.exchange_name,
            &api_config.api_key[..api_config.api_key.len().min(8)]
        );

        // 4. 获取持仓和可用资金
        use crate::exchange::OkxOrderService;
        let okx_service = OkxOrderService;

        let (positions, max_size) = tokio::try_join!(
            okx_service.get_positions(&api_config, Some("SWAP"), Some(inst_id)),
            okx_service.get_max_available_size(&api_config, inst_id)
        )
        .map_err(|e| {
            error!("获取账户数据失败: {}", e);
            anyhow!("获取账户数据失败: {}", e)
        })?;

        info!(
            "当前持仓数量: {}, 最大可用数量: {}",
            positions.len(),
            max_size.max_buy
        );

        // 5. 计算下单数量（使用90%的安全系数）
        let safety_factor = 0.9;
        let max_buy = match max_size.max_buy.parse::<f64>() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "解析 max_buy 失败: inst_id={}, max_buy={}, error={}",
                    inst_id, max_size.max_buy, e
                );
                return Err(anyhow!("解析最大可用下单量失败"));
            }
        };
        let order_size_f64 = max_buy * safety_factor;
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        if order_size == "0" {
            info!("下单数量为0，跳过下单");
            return Ok(());
        }

        info!("计算的下单数量: {}", order_size);

        // 6. 计算止损止盈价格
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;

        let stop_loss_price = if side == "sell" {
            entry_price * (1.0 + max_loss_percent)
        } else {
            entry_price * (1.0 - max_loss_percent)
        };

        // 如果使用信号K线止损
        let final_stop_loss = if let Some(is_used_signal_k_line_stop_loss) =
            risk_config.is_used_signal_k_line_stop_loss
        {
            if is_used_signal_k_line_stop_loss {
                signal
                    .signal_kline_stop_loss_price
                    .unwrap_or(stop_loss_price)
            } else {
                stop_loss_price
            }
        } else {
            stop_loss_price
        };

        // 6. 验证止损价格合理性（参考：swap_order_service.rs:547-558）
        if pos_side == "short" && entry_price > final_stop_loss {
            error!(
                "做空开仓价 > 止损价，不下单: entry={}, stop_loss={}",
                entry_price, final_stop_loss
            );
            return Err(anyhow!("止损价格不合理"));
        }
        if pos_side == "long" && entry_price < final_stop_loss {
            error!(
                "做多开仓价 < 止损价，不下单: entry={}, stop_loss={}",
                entry_price, final_stop_loss
            );
            return Err(anyhow!("止损价格不合理"));
        }

        info!(
            "下单参数: entry_price={:.2}, stop_loss={:.2}",
            entry_price, final_stop_loss
        );

        // 7. 实际下单到交易所
        let order_result = okx_service
            .execute_order_from_signal(
                &api_config,
                inst_id,
                signal,
                order_size.clone(),
                Some(entry_price),
            )
            .await
            .map_err(|e| {
                error!("下单到交易所失败: {}", e);
                anyhow!("下单失败: {}", e)
            })?;

        // 获取交易所返回的订单ID
        let out_order_id = order_result
            .first()
            .map(|o| o.ord_id.clone())
            .unwrap_or_default();

        info!(
            "✅ 下单成功: inst_id={}, order_id={}, size={}",
            inst_id, out_order_id, order_size
        );

        // 8. 保存订单记录到数据库
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

        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            strategy_type,
            side,
            pos_side,
            &order_size,
            &in_order_id,
            &out_order_id,
            "okx",
            &order_detail.to_string(),
        );

        match self.swap_order_repository.save(&swap_order).await {
            Ok(order_id) => {
                info!("✅ 订单记录已保存: db_id={}, in_order_id={}", order_id, in_order_id);
            }
            Err(e) => {
                // 订单已提交到交易所,保存失败只记录警告,不返回错误
                error!("⚠️ 保存订单记录失败(订单已提交): {}", e);
            }
        }

        Ok(())
    }

    /// 验证策略配置
    fn validate_config(&self, config: &StrategyConfig) -> Result<()> {
        if !config.is_running() {
            return Err(anyhow!(
                "策略未运行: config_id={}, status={:?}",
                config.id,
                config.status
            ));
        }

        if config.parameters.is_null() {
            return Err(anyhow!("策略参数为空"));
        }

        Ok(())
    }

    /// 检查是否应该执行策略
    ///
    /// 考虑因素：
    /// - 策略状态
    /// - 时间窗口
    /// - 执行间隔
    pub fn should_execute(
        &self,
        config: &StrategyConfig,
        last_execution_time: Option<i64>,
        current_time: i64,
    ) -> bool {
        // 1. 检查策略状态
        if !config.is_running() {
            return false;
        }

        // 2. 检查执行间隔
        if let Some(last_time) = last_execution_time {
            let interval = current_time - last_time;
            let min_interval = self.get_min_execution_interval(&config.timeframe);

            if interval < min_interval {
                return false;
            }
        }

        true
    }

    /// 获取最小执行间隔（秒）
    fn get_min_execution_interval(&self, timeframe: &rust_quant_domain::Timeframe) -> i64 {
        use rust_quant_domain::Timeframe;

        match *timeframe {
            Timeframe::M1 => 60,
            Timeframe::M3 => 180,
            Timeframe::M5 => 300,
            Timeframe::M15 => 900,
            Timeframe::M30 => 1800,
            Timeframe::H1 => 3600,
            Timeframe::H2 => 7200,
            Timeframe::H4 => 14400,
            Timeframe::H6 => 21600,
            Timeframe::H12 => 43200,
            Timeframe::D1 => 86400,
            Timeframe::W1 => 604800,
            Timeframe::MN1 => 2592000, // 30天
        }
    }
}

// 注意：由于 StrategyExecutionService 需要依赖注入，不再实现 Default
// 调用方需要通过 new() 方法并提供 SwapOrderRepository 实例

// ============================================================================
// 辅助函数
// ============================================================================

// TODO: 数据转换逻辑待实现
// 当market包依赖稳定后，实现CandlesEntity到Candle的转换

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Mock SwapOrderRepository 用于测试
    struct MockSwapOrderRepository;

    #[async_trait]
    impl SwapOrderRepository for MockSwapOrderRepository {
        async fn find_by_id(&self, _id: i32) -> Result<Option<SwapOrder>> {
            Ok(None)
        }
        async fn find_by_in_order_id(&self, _in_order_id: &str) -> Result<Option<SwapOrder>> {
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
        async fn save(&self, _order: &SwapOrder) -> Result<i32> {
            Ok(1)
        }
        async fn update(&self, _order: &SwapOrder) -> Result<()> {
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
        StrategyExecutionService::new(Arc::new(MockSwapOrderRepository))
    }

    #[test]
    fn test_service_creation() {
        let _service = create_test_service();
        // 验证服务可以创建
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

        // 第一次执行（无上次执行时间）
        assert!(service.should_execute(&config, None, 1000));

        // 间隔太短
        assert!(!service.should_execute(&config, Some(1000), 1500));

        // 间隔足够
        assert!(service.should_execute(&config, Some(1000), 5000));
    }
}
