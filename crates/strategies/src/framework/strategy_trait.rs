//! 策略执行接口定义
//! 
//! 所有策略必须实现 StrategyExecutor trait，以便统一管理和调度

use async_trait::async_trait;
use anyhow::Result;
use std::collections::VecDeque;

use rust_quant_market::models::CandlesEntity;
use crate::order::strategy_config::StrategyConfig;
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use rust_quant_common::CandleItem;

/// 策略数据快照（通用）
#[derive(Debug, Clone)]
pub struct StrategyDataResult {
    pub hash_key: String,
    pub last_timestamp: i64,
}

/// 策略执行器接口
/// 
/// 所有策略必须实现此 trait，提供统一的执行接口
#[async_trait]
pub trait StrategyExecutor: Send + Sync {
    /// 策略名称（唯一标识，如 "Vegas", "Nwe"）
    fn name(&self) -> &'static str;
    
    /// 策略类型枚举
    fn strategy_type(&self) -> StrategyType;
    
    /// 检测配置是否为该策略类型
    /// 
    /// # 参数
    /// * `strategy_config` - JSON 格式的策略配置
    /// 
    /// # 返回
    /// * `true` - 能够解析该配置
    /// * `false` - 无法解析该配置
    fn can_handle(&self, strategy_config: &str) -> bool;
    
    /// 初始化策略数据
    /// 
    /// 在策略启动时调用一次，用于：
    /// - 加载历史K线数据
    /// - 初始化指标计算
    /// - 存储到缓存
    /// 
    /// # 参数
    /// * `strategy_config` - 策略配置
    /// * `inst_id` - 产品ID
    /// * `period` - 时间周期
    /// * `candles` - 历史K线数据
    /// 
    /// # 返回
    /// * `Ok(StrategyDataResult)` - 初始化成功，返回数据结果
    /// * `Err` - 初始化失败
    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandlesEntity>,
    ) -> Result<StrategyDataResult>;
    
    /// 执行策略（生成交易信号并下单）
    /// 
    /// 当K线确认时调用，用于：
    /// - 获取最新K线
    /// - 更新指标值
    /// - 生成交易信号
    /// - 执行下单（如有信号）
    /// 
    /// # 参数
    /// * `inst_id` - 产品ID
    /// * `period` - 时间周期
    /// * `strategy_config` - 策略配置
    /// * `snap` - 最新K线快照（可选）
    /// 
    /// # 返回
    /// * `Ok(())` - 执行成功
    /// * `Err` - 执行失败
    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<()>;
}

/// 策略执行器工厂
/// 
/// 用于创建策略执行器实例
pub trait StrategyExecutorFactory: Send + Sync {
    /// 创建策略执行器
    fn create(&self) -> Box<dyn StrategyExecutor>;
}

