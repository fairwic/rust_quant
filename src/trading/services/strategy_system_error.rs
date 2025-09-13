//! 策略系统统一错误处理模块
//!
//! 提供统一的错误类型定义、错误处理逻辑和错误恢复机制，
//! 支持错误分类、日志记录和监控集成。

use thiserror::Error;
use tracing::{error, warn, info, debug};

use crate::trading::services::strategy_data_service::StrategyDataError;
use crate::trading::services::scheduler_service::SchedulerServiceError;

/// 策略系统统一错误类型
#[derive(Error, Debug)]
pub enum StrategySystemError {
    #[error("配置错误: {0}")]
    Config(#[from] StrategyConfigError),

    #[error("数据错误: {0}")]
    Data(#[from] StrategyDataError),

    #[error("调度器错误: {0}")]
    Scheduler(#[from] SchedulerServiceError),

    #[error("业务逻辑错误: {0}")]
    Business(#[from] BusinessLogicError),

    #[error("系统错误: {0}")]
    System(#[from] SystemError),
}

/// 策略配置错误
#[derive(Error, Debug)]
pub enum StrategyConfigError {
    #[error("配置不存在: {config_id}")]
    NotFound { config_id: i64 },

    #[error("配置解析失败: {field}")]
    ParseFailed { field: String },

    #[error("配置验证失败: {reason}")]
    ValidationFailed { reason: String },

    #[error("配置序列化失败: {reason}")]
    SerializationFailed { reason: String },
}

/// 业务逻辑错误
#[derive(Error, Debug)]
pub enum BusinessLogicError {
    #[error("策略已在运行: {strategy_key}")]
    StrategyAlreadyRunning { strategy_key: String },

    #[error("策略未运行: {strategy_key}")]
    StrategyNotRunning { strategy_key: String },

    #[error("策略未处于暂停状态: {strategy_key}")]
    StrategyNotPaused { strategy_key: String },

    #[error("不支持的策略类型: {strategy_type}")]
    UnsupportedStrategyType { strategy_type: String },

    #[error("策略状态转换失败: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },
}

/// 系统错误
#[derive(Error, Debug)]
pub enum SystemError {
    #[error("数据库操作失败: {operation}")]
    DatabaseError { operation: String },

    #[error("网络请求失败: {reason}")]
    NetworkError { reason: String },

    #[error("资源不足: {resource}")]
    ResourceExhausted { resource: String },

    #[error("超时: {operation}")]
    Timeout { operation: String },
}

/// 错误严重程度
#[derive(Debug, Clone, Copy)]
pub enum ErrorSeverity {
    /// 致命错误：需要立即停止系统
    Critical,
    /// 严重错误：需要人工介入
    High,
    /// 中等错误：系统可以继续运行但需要关注
    Medium,
    /// 轻微错误：记录日志即可
    Low,
}

/// 错误处理器
pub struct ErrorHandler;

impl ErrorHandler {
    /// 处理策略系统错误
    pub fn handle_error(error: &StrategySystemError, context: &str) -> ErrorSeverity {
        match error {
            StrategySystemError::Scheduler(scheduler_err) => {
                Self::handle_scheduler_error(scheduler_err, context)
            }
            StrategySystemError::Data(data_err) => {
                Self::handle_data_error(data_err, context)
            }
            StrategySystemError::Config(config_err) => {
                Self::handle_config_error(config_err, context)
            }
            StrategySystemError::Business(business_err) => {
                Self::handle_business_error(business_err, context)
            }
            StrategySystemError::System(system_err) => {
                Self::handle_system_error(system_err, context)
            }
        }
    }

    /// 处理调度器错误
    fn handle_scheduler_error(error: &SchedulerServiceError, context: &str) -> ErrorSeverity {
        match error {
            SchedulerServiceError::NotInitialized => {
                error!("调度器未初始化 - 上下文: {}", context);
                ErrorSeverity::Critical
            }
            SchedulerServiceError::OperationTimeout => {
                warn!("调度器操作超时，但系统可继续运行 - 上下文: {}", context);
                ErrorSeverity::Low
            }
            SchedulerServiceError::JobRemovalFailed { .. } => {
                warn!("任务移除失败，但不影响策略状态 - 上下文: {}", context);
                ErrorSeverity::Low
            }
            _ => {
                warn!("调度器操作失败 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
        }
    }

    /// 处理数据错误
    fn handle_data_error(error: &StrategyDataError, context: &str) -> ErrorSeverity {
        match error {
            StrategyDataError::DataFetchFailed { .. } => {
                error!("数据获取失败，策略无法启动 - 上下文: {}", context);
                ErrorSeverity::High
            }
            StrategyDataError::DataValidationFailed { .. } => {
                error!("数据验证失败，可能存在数据质量问题 - 上下文: {}", context);
                ErrorSeverity::High
            }
            StrategyDataError::ValidationError { .. } => {
                warn!("参数验证失败 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
            _ => {
                warn!("数据处理失败 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
        }
    }

    /// 处理配置错误
    fn handle_config_error(error: &StrategyConfigError, context: &str) -> ErrorSeverity {
        match error {
            StrategyConfigError::NotFound { .. } => {
                warn!("策略配置不存在 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
            StrategyConfigError::ParseFailed { .. } => {
                error!("配置解析失败，可能配置格式错误 - 上下文: {}", context);
                ErrorSeverity::High
            }
            _ => {
                warn!("配置处理失败 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
        }
    }

    /// 处理业务逻辑错误
    fn handle_business_error(error: &BusinessLogicError, context: &str) -> ErrorSeverity {
        match error {
            BusinessLogicError::StrategyAlreadyRunning { .. } => {
                info!("策略已在运行，跳过启动 - 上下文: {}", context);
                ErrorSeverity::Low
            }
            BusinessLogicError::StrategyNotRunning { .. } => {
                warn!("策略未运行，无法执行操作 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
            BusinessLogicError::UnsupportedStrategyType { .. } => {
                error!("不支持的策略类型 - 上下文: {}", context);
                ErrorSeverity::High
            }
            _ => {
                warn!("业务逻辑错误 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
        }
    }

    /// 处理系统错误
    fn handle_system_error(error: &SystemError, context: &str) -> ErrorSeverity {
        match error {
            SystemError::DatabaseError { .. } => {
                error!("数据库操作失败 - 上下文: {}", context);
                ErrorSeverity::High
            }
            SystemError::NetworkError { .. } => {
                warn!("网络请求失败 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
            SystemError::Timeout { .. } => {
                warn!("操作超时 - 上下文: {}", context);
                ErrorSeverity::Medium
            }
            SystemError::ResourceExhausted { .. } => {
                error!("资源不足 - 上下文: {}", context);
                ErrorSeverity::Critical
            }
        }
    }

    /// 根据错误严重程度执行相应的恢复策略
    pub async fn execute_recovery_strategy(
        severity: ErrorSeverity,
        error: &StrategySystemError,
        context: &str,
    ) {
        match severity {
            ErrorSeverity::Critical => {
                error!("致命错误，系统需要立即停止: {} - {}", error, context);
                // TODO: 实现系统安全停止逻辑
            }
            ErrorSeverity::High => {
                error!("严重错误，需要人工介入: {} - {}", error, context);
                // TODO: 发送告警通知
            }
            ErrorSeverity::Medium => {
                warn!("中等错误，系统继续运行但需关注: {} - {}", error, context);
                // TODO: 记录到监控系统
            }
            ErrorSeverity::Low => {
                debug!("轻微错误，已记录: {} - {}", error, context);
                // 仅记录日志
            }
        }
    }
}

/// 错误处理宏，简化错误处理代码
#[macro_export]
macro_rules! handle_strategy_error {
    ($error:expr, $context:expr) => {
        {
            let severity = crate::trading::services::strategy_system_error::ErrorHandler::handle_error(&$error, $context);
            crate::trading::services::strategy_system_error::ErrorHandler::execute_recovery_strategy(severity, &$error, $context).await;
            severity
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_severity_classification() {
        let scheduler_timeout = StrategySystemError::Scheduler(SchedulerServiceError::OperationTimeout);
        let severity = ErrorHandler::handle_error(&scheduler_timeout, "test");
        assert!(matches!(severity, ErrorSeverity::Low));

        let config_not_found = StrategySystemError::Config(StrategyConfigError::NotFound { config_id: 1 });
        let severity = ErrorHandler::handle_error(&config_not_found, "test");
        assert!(matches!(severity, ErrorSeverity::Medium));
    }
}
