//! 策略执行相关接口定义
//!
//! 通过 trait 解耦 strategies 和 orchestration 的循环依赖
//!
//! 依赖关系：
//! - strategies 定义 trait（本文件）
//! - orchestration 实现 trait
//! - strategies 依赖 trait 而非具体实现

use anyhow::Result;
use async_trait::async_trait;

use crate::strategy_common::SignalResult;

/// 策略执行状态管理器接口
///
/// 负责去重、状态跟踪等功能
#[async_trait]
pub trait ExecutionStateManager: Send + Sync {
    /// 尝试标记为处理中（去重检查）
    ///
    /// # 参数
    /// - `key`: 唯一标识符（通常是 inst_id:period）
    /// - `timestamp`: K线时间戳
    ///
    /// # 返回
    /// - `true`: 可以执行（未重复）
    /// - `false`: 重复执行，应跳过
    fn try_mark_processing(&self, key: &str, timestamp: i64) -> bool;

    /// 清除执行状态
    fn clear_processing(&self, key: &str);

    /// 检查是否正在处理
    fn is_processing(&self, key: &str) -> bool;
}

/// 时间检查器接口
///
/// 负责验证时间戳是否应该触发策略执行
#[async_trait]
pub trait TimeChecker: Send + Sync {
    /// 检查是否是新的时间
    ///
    /// # 参数
    /// - `old_time`: 上一次执行的时间戳
    /// - `new_time`: 当前K线时间戳
    /// - `period`: 时间周期（如 "1H", "4H"）
    /// - `is_update`: 是否是更新模式
    /// - `force`: 是否强制检查
    ///
    /// # 返回
    /// - `Ok(true)`: 是新时间，应该执行
    /// - `Ok(false)`: 不是新时间，跳过
    /// - `Err`: 检查失败
    fn check_new_time(
        &self,
        old_time: i64,
        new_time: i64,
        period: &str,
        is_update: bool,
        force: bool,
    ) -> Result<bool>;
}

/// 信号日志记录器接口
///
/// 负责记录策略产生的交易信号
#[async_trait]
pub trait SignalLogger: Send + Sync {
    /// 保存信号日志
    ///
    /// # 参数
    /// - `inst_id`: 交易对标识
    /// - `period`: 时间周期
    /// - `signal`: 信号结果
    fn save_signal_log(&self, inst_id: &str, period: &str, signal: &SignalResult);

    /// 查询最近的信号日志（可选）
    async fn get_recent_signals(
        &self,
        inst_id: &str,
        period: &str,
        limit: usize,
    ) -> Result<Vec<SignalResult>> {
        // 默认实现：不支持
        let _ = (inst_id, period, limit);
        Ok(Vec::new())
    }
}

/// 策略执行上下文接口
///
/// 组合所有必需的执行依赖
#[async_trait]
pub trait StrategyExecutionContext: Send + Sync {
    /// 获取状态管理器
    fn state_manager(&self) -> &dyn ExecutionStateManager;

    /// 获取时间检查器
    fn time_checker(&self) -> &dyn TimeChecker;

    /// 获取信号日志记录器
    fn signal_logger(&self) -> &dyn SignalLogger;
}

/// 空实现（用于测试或不需要这些功能的场景）
pub struct NoOpStateManager;

impl ExecutionStateManager for NoOpStateManager {
    fn try_mark_processing(&self, _key: &str, _timestamp: i64) -> bool {
        true // 总是允许执行
    }

    fn clear_processing(&self, _key: &str) {}

    fn is_processing(&self, _key: &str) -> bool {
        false
    }
}

pub struct NoOpTimeChecker;

impl TimeChecker for NoOpTimeChecker {
    fn check_new_time(
        &self,
        old_time: i64,
        new_time: i64,
        _period: &str,
        _is_update: bool,
        _force: bool,
    ) -> Result<bool> {
        Ok(new_time > old_time)
    }
}

pub struct NoOpSignalLogger;

impl SignalLogger for NoOpSignalLogger {
    fn save_signal_log(&self, _inst_id: &str, _period: &str, _signal: &SignalResult) {
        // 什么都不做
    }
}

/// 默认执行上下文（使用 NoOp 实现）
pub struct DefaultExecutionContext {
    state_manager: NoOpStateManager,
    time_checker: NoOpTimeChecker,
    signal_logger: NoOpSignalLogger,
}

impl DefaultExecutionContext {
    pub fn new() -> Self {
        Self {
            state_manager: NoOpStateManager,
            time_checker: NoOpTimeChecker,
            signal_logger: NoOpSignalLogger,
        }
    }
}

impl Default for DefaultExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyExecutionContext for DefaultExecutionContext {
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
