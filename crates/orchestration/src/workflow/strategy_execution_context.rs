//! 策略执行上下文实现
//! 
//! 实现 rust_quant_strategies 定义的 trait 接口，解决循环依赖问题
//! 
//! 依赖关系：orchestration → strategies (单向)

use anyhow::Result;
use tracing::{error, info};
use serde_json;

use rust_quant_strategies::framework::execution_traits::{
    ExecutionStateManager, TimeChecker, SignalLogger, StrategyExecutionContext,
};
use rust_quant_strategies::strategy_common::SignalResult;
use rust_quant_strategies::StrategyType;

use crate::workflow::strategy_runner::{
    StrategyExecutionStateManager as InternalStateManager,
    // check_new_time已移除，下面直接实现
};

// TODO: StrategyJobSignalLog 需要迁移到新的位置
// 暂时使用简化的日志记录
// use rust_quant_common::model::strategy::strategy_job_signal_log::{
//     StrategyJobSignalLog, StrategyJobSignalLogModel,
// };

/// orchestration 层的状态管理器实现
pub struct OrchestrationStateManager;

impl ExecutionStateManager for OrchestrationStateManager {
    fn try_mark_processing(&self, key: &str, timestamp: i64) -> bool {
        InternalStateManager::try_mark_processing(key, timestamp)
    }
    
    fn clear_processing(&self, key: &str) {
        // InternalStateManager 没有直接的清除方法，使用 mark_completed
        // 这里需要时间戳，但 trait 接口没有，暂时忽略
        // 在实际使用中，应该保存时间戳或者改进 trait 设计
        let _ = key; // 避免警告
    }
    
    fn is_processing(&self, key: &str) -> bool {
        // InternalStateManager 没有直接的检查方法
        // 可以通过尝试标记来检查（副作用：会插入记录）
        // 更好的方式是扩展 InternalStateManager
        let _ = key;
        false // 保守返回
    }
}

/// orchestration 层的时间检查器实现
pub struct OrchestrationTimeChecker;

impl TimeChecker for OrchestrationTimeChecker {
    fn check_new_time(
        &self,
        _old_time: i64,
        _new_time: i64,
        _period: &str,
        _is_update: bool,
        _force: bool,
    ) -> Result<bool> {
        // TODO: 实现check_new_time逻辑或从旧代码迁移
        // 暂时返回true，允许所有时间更新
        Ok(true)
    }
}

/// orchestration 层的信号日志记录器实现
pub struct OrchestrationSignalLogger {
    strategy_type: StrategyType,
}

impl OrchestrationSignalLogger {
    pub fn new(strategy_type: StrategyType) -> Self {
        Self { strategy_type }
    }
}

impl SignalLogger for OrchestrationSignalLogger {
    fn save_signal_log(&self, inst_id: &str, period: &str, signal: &SignalResult) {
        // TODO: 实现数据库持久化
        // 暂时只记录日志到 tracing
        let strategy_result_str = match serde_json::to_string(&signal) {
            Ok(s) => s,
            Err(e) => {
                error!("序列化 signal_result 失败: {}", e);
                format!("{:?}", signal)
            }
        };
        
        tracing::info!(
            strategy_type = self.strategy_type.as_str(),
            inst_id = inst_id,
            period = period,
            signal = %strategy_result_str,
            "策略信号记录"
        );
        
        // TODO: 实现异步保存到数据库
        // let signal_record = StrategyJobSignalLog {
        //     inst_id: inst_id.to_string(),
        //     time: period.to_string(),
        //     strategy_type: self.strategy_type.as_str().to_owned(),
        //     strategy_result: strategy_result_str,
        // };
        // 
        // tokio::spawn(async move {
        //     if let Err(e) = StrategyJobSignalLogModel::save_signal(&signal_record).await {
        //         error!("保存信号日志失败: {}", e);
        //     }
        // });
    }
}

/// orchestration 层的完整执行上下文
pub struct OrchestrationExecutionContext {
    state_manager: OrchestrationStateManager,
    time_checker: OrchestrationTimeChecker,
    signal_logger: OrchestrationSignalLogger,
}

impl OrchestrationExecutionContext {
    pub fn new(strategy_type: StrategyType) -> Self {
        Self {
            state_manager: OrchestrationStateManager,
            time_checker: OrchestrationTimeChecker,
            signal_logger: OrchestrationSignalLogger::new(strategy_type),
        }
    }
}

impl StrategyExecutionContext for OrchestrationExecutionContext {
    fn state_manager(&self) -> &dyn ExecutionStateManager {
        &self.state_manager
    }
    
    fn time_checker(&self) -> &dyn TimeChecker {
        &self.time_checker
    }
    
    fn signal_logger(&self) -> &dyn SignalLogger {
        &self.signal_logger
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_context_creation() {
        let context = OrchestrationExecutionContext::new(StrategyType::Vegas);
        assert!(context.state_manager().try_mark_processing("test:1H", 1000));
    }
}

