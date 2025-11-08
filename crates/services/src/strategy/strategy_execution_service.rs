//! 策略执行服务
//! 
//! 协调策略分析、风控检查、订单创建的完整业务流程

use anyhow::{anyhow, Result};
use tracing::{info, warn, error};

use rust_quant_domain::{StrategyConfig, SignalResult};
use rust_quant_market::models::CandlesEntity;

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
/// - TradingService: 创建订单（待实现）
/// - RiskService: 风控检查（待实现）
pub struct StrategyExecutionService {
    // 策略注册表暂时不存储，每次使用时通过get_strategy_registry()获取
}

impl StrategyExecutionService {
    /// 创建新的策略执行服务
    pub fn new() -> Self {
        Self {}
    }
    
    /// 执行策略分析和交易流程
    /// 
    /// 这是策略执行的主入口，协调整个业务流程：
    /// 1. 获取策略实现
    /// 2. 获取市场数据
    /// 3. 执行策略分析
    /// 4. 风控检查（TODO）
    /// 5. 创建订单（TODO）
    /// 6. 记录日志
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
        
        // 3. 执行策略分析（通过策略注册表）
        // 这里委托给策略层执行，strategies负责信号生成
        strategy_executor
            .execute(inst_id, period, config, snap)
            .await
            .map_err(|e| {
                error!("策略执行失败: {}", e);
                anyhow!("策略分析失败: {}", e)
            })?;
        
        // TODO: 策略执行后需要返回信号
        // 目前暂时返回空信号
        let signal = SignalResult::empty();
        
        info!("策略分析完成");
        
        // 4. 风控检查
        // TODO: 调用 RiskManagementService
        // if signal.can_open {
        //     let can_trade = self.risk_service.check_can_open(inst_id, &signal).await?;
        //     if !can_trade {
        //         warn!("风控检查未通过，跳过开仓");
        //         return Ok(signal);
        //     }
        // }
        
        // 5. 创建订单
        // TODO: 调用 TradingService
        // if signal.can_open {
        //     self.trading_service.create_order_from_signal(inst_id, &signal, config).await?;
        //     info!("订单创建成功");
        // }
        
        // 6. 平仓处理
        // if signal.should_close {
        //     self.trading_service.close_positions(inst_id, &signal).await?;
        //     info!("平仓完成");
        // }
        
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
    
    /// 验证策略配置
    fn validate_config(&self, config: &StrategyConfig) -> Result<()> {
        if !config.is_running() {
            return Err(anyhow!("策略未运行: config_id={}, status={:?}", config.id, config.status));
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

impl Default for StrategyExecutionService {
    fn default() -> Self {
        Self::new()
    }
}

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
    
    #[test]
    fn test_service_creation() {
        let service = StrategyExecutionService::new();
        // 验证服务可以创建
    }
    
    #[test]
    fn test_min_execution_interval() {
        use rust_quant_domain::Timeframe;
        
        let service = StrategyExecutionService::new();
        
        assert_eq!(service.get_min_execution_interval(&Timeframe::M1), 60);
        assert_eq!(service.get_min_execution_interval(&Timeframe::M5), 300);
        assert_eq!(service.get_min_execution_interval(&Timeframe::H1), 3600);
        assert_eq!(service.get_min_execution_interval(&Timeframe::D1), 86400);
    }
    
    #[tokio::test]
    async fn test_should_execute() {
        use rust_quant_domain::{StrategyType, StrategyStatus, Timeframe};
        use chrono::Utc;
        
        let service = StrategyExecutionService::new();
        
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

