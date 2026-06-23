//! 策略执行接口定义
//!
//! 所有策略必须实现 StrategyExecutor trait，以便统一管理和调度
use crate::framework::config::strategy_config::StrategyConfig;
use crate::strategy_common::SignalResult;
use crate::StrategyType;
use anyhow::Result;
use async_trait::async_trait;
use rust_quant_common::CandleItem;
/// 策略数据快照（通用）
#[derive(Debug, Clone)]
pub struct StrategyDataResult {
    /// 策略数据缓存键。
    pub hash_key: String,
    /// 时间戳。
    pub last_timestamp: i64,
}
/// 策略执行器接口
///
/// 所有策略必须实现此 trait，提供统一的执行接口
#[async_trait]
pub trait StrategyExecutor: Send + Sync {
    fn name(&self) -> &'static str;
    fn strategy_type(&self) -> StrategyType;
    fn can_handle(&self, strategy_config: &str) -> bool;
    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandleItem>,
    ) -> Result<StrategyDataResult>;
    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandleItem>,
    ) -> Result<SignalResult>;
}
/// 策略执行器工厂
///
/// 用于创建策略执行器实例
pub trait StrategyExecutorFactory: Send + Sync {
    /// 创建策略执行器
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 以结构体实例状态为输入，避免重复传参并保证接口一致性。
    fn create(&self) -> Box<dyn StrategyExecutor>;
}
