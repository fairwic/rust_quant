//! 策略执行上下文实现
//!
//! 实现 rust_quant_strategies 定义的 trait 接口，解决循环依赖问题
//!
//! 依赖关系：orchestration → strategies (单向)

use anyhow::Result;

use rust_quant_strategies::framework::execution_traits::{
    ExecutionStateManager, SignalLogger, StrategyExecutionContext, TimeChecker,
};
use rust_quant_strategies::strategy_common::SignalResult;
use rust_quant_strategies::StrategyType;

use crate::workflow::signal_logger::save_signal_log_async;
use crate::workflow::strategy_runner::StrategyExecutionStateManager as InternalStateManager;
use crate::workflow::time_checker::check_new_time as internal_check_new_time;

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
        old_time: i64,
        new_time: i64,
        period: &str,
        is_update: bool,
        force: bool,
    ) -> Result<bool> {
        // 使用独立的time_checker模块实现
        // 参数映射：is_update -> is_close_confirm, force -> just_check_confirm
        internal_check_new_time(old_time, new_time, period, is_update, force)
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
        // 使用独立的signal_logger模块实现异步保存
        save_signal_log_async(
            inst_id.to_string(),
            period.to_string(),
            self.strategy_type,
            signal.clone(),
        );
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
