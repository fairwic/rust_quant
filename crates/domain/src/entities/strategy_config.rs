//! 策略配置实体 (Strategy Config Aggregate Root)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::enums::{StrategyStatus, StrategyType, Timeframe};

/// 策略配置实体 - 聚合根
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// 配置ID
    pub id: i64,

    /// 策略类型
    pub strategy_type: StrategyType,

    /// 交易对符号
    pub symbol: String,

    /// 时间周期
    pub timeframe: Timeframe,

    /// 策略参数 (JSON格式)
    pub parameters: JsonValue,

    /// 风险配置 (JSON格式)
    pub risk_config: JsonValue,

    /// 策略状态
    pub status: StrategyStatus,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub updated_at: DateTime<Utc>,

    /// 回测开始时间 (可选)
    pub backtest_start: Option<i64>,

    /// 回测结束时间 (可选)
    pub backtest_end: Option<i64>,

    /// 描述
    pub description: Option<String>,
}

impl StrategyConfig {
    /// 创建新的策略配置
    pub fn new(
        id: i64,
        strategy_type: StrategyType,
        symbol: String,
        timeframe: Timeframe,
        parameters: JsonValue,
        risk_config: JsonValue,
    ) -> Self {
        let now = Utc::now();

        Self {
            id,
            strategy_type,
            symbol,
            timeframe,
            parameters,
            risk_config,
            status: StrategyStatus::Stopped,
            created_at: now,
            updated_at: now,
            backtest_start: None,
            backtest_end: None,
            description: None,
        }
    }

    /// 启动策略
    pub fn start(&mut self) {
        self.status = StrategyStatus::Running;
        self.updated_at = Utc::now();
    }

    /// 停止策略
    pub fn stop(&mut self) {
        self.status = StrategyStatus::Stopped;
        self.updated_at = Utc::now();
    }

    /// 暂停策略
    pub fn pause(&mut self) {
        self.status = StrategyStatus::Paused;
        self.updated_at = Utc::now();
    }

    /// 标记为错误状态
    pub fn mark_error(&mut self) {
        self.status = StrategyStatus::Error;
        self.updated_at = Utc::now();
    }

    /// 更新参数
    pub fn update_parameters(&mut self, parameters: JsonValue) {
        self.parameters = parameters;
        self.updated_at = Utc::now();
    }

    /// 更新风险配置
    pub fn update_risk_config(&mut self, risk_config: JsonValue) {
        self.risk_config = risk_config;
        self.updated_at = Utc::now();
    }

    /// 设置回测时间范围
    pub fn set_backtest_range(&mut self, start: i64, end: i64) {
        self.backtest_start = Some(start);
        self.backtest_end = Some(end);
        self.updated_at = Utc::now();
    }

    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        self.status == StrategyStatus::Running
    }

    /// 是否可以启动
    pub fn can_start(&self) -> bool {
        matches!(
            self.status,
            StrategyStatus::Stopped | StrategyStatus::Paused
        )
    }
}

/// 基础风险策略配置 (通用的风险管理参数)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicRiskConfig {
    /// 最大损失百分比
    pub max_loss_percent: f64,

    /// 止盈比率
    pub take_profit_ratio: f64,

    /// 是否启用移动止损
    pub is_move_stop_loss: bool,

    /// 是否使用信号K线作为止损
    pub is_used_signal_k_line_stop_loss: bool,

    /// 最大持仓时间 (秒，可选)
    pub max_hold_time: Option<i64>,

    /// 最大杠杆倍数 (可选)
    pub max_leverage: Option<f64>,
}

impl Default for BasicRiskConfig {
    fn default() -> Self {
        Self {
            max_loss_percent: 0.02, // 默认2%止损
            take_profit_ratio: 1.5, // 默认1.5倍止盈
            is_move_stop_loss: false,
            is_used_signal_k_line_stop_loss: true,
            max_hold_time: None,
            max_leverage: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_strategy_config_creation() {
        let config = StrategyConfig::new(
            1,
            StrategyType::Vegas,
            "BTC-USDT".to_string(),
            Timeframe::H1,
            json!({"param1": "value1"}),
            json!({"max_loss": 0.02}),
        );

        assert_eq!(config.strategy_type, StrategyType::Vegas);
        assert_eq!(config.status, StrategyStatus::Stopped);
    }

    #[test]
    fn test_strategy_lifecycle() {
        let mut config = StrategyConfig::new(
            1,
            StrategyType::Vegas,
            "BTC-USDT".to_string(),
            Timeframe::H1,
            json!({}),
            json!({}),
        );

        assert!(!config.is_running());
        assert!(config.can_start());

        config.start();
        assert!(config.is_running());

        config.pause();
        assert!(!config.is_running());
        assert!(config.can_start());

        config.stop();
        assert_eq!(config.status, StrategyStatus::Stopped);
    }
}
