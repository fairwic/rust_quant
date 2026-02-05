//! 交易所API配置实体

use serde::{Deserialize, Serialize};

/// 交易所API配置实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeApiConfig {
    /// 配置ID
    pub id: i32,

    /// 交易所名称（如 "okx"）
    pub exchange_name: String,

    /// API Key
    pub api_key: String,

    /// API Secret
    pub api_secret: String,

    /// Passphrase（OKX需要）
    pub passphrase: Option<String>,

    /// 是否沙箱环境
    pub is_sandbox: bool,

    /// 是否启用
    pub is_enabled: bool,

    /// 描述
    pub description: Option<String>,
}

impl ExchangeApiConfig {
    /// 创建新的API配置
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i32,
        exchange_name: String,
        api_key: String,
        api_secret: String,
        passphrase: Option<String>,
        is_sandbox: bool,
        is_enabled: bool,
        description: Option<String>,
    ) -> Self {
        Self {
            id,
            exchange_name,
            api_key,
            api_secret,
            passphrase,
            is_sandbox,
            is_enabled,
            description,
        }
    }

    /// 验证配置有效性
    pub fn validate(&self) -> Result<(), String> {
        if self.exchange_name.is_empty() {
            return Err("交易所名称不能为空".to_string());
        }
        if self.api_key.is_empty() {
            return Err("API Key不能为空".to_string());
        }
        if self.api_secret.is_empty() {
            return Err("API Secret不能为空".to_string());
        }
        // OKX需要passphrase
        if self.exchange_name.to_lowercase() == "okx" && self.passphrase.is_none() {
            return Err("OKX交易所需要Passphrase".to_string());
        }
        Ok(())
    }
}

/// 策略与API配置关联
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyApiConfig {
    /// 关联ID
    pub id: i32,

    /// 策略配置ID
    pub strategy_config_id: i32,

    /// API配置ID
    pub api_config_id: i32,

    /// 优先级（数字越小优先级越高）
    pub priority: i32,

    /// 是否启用
    pub is_enabled: bool,
}

impl StrategyApiConfig {
    /// 创建新的关联
    pub fn new(
        id: i32,
        strategy_config_id: i32,
        api_config_id: i32,
        priority: i32,
        is_enabled: bool,
    ) -> Self {
        Self {
            id,
            strategy_config_id,
            api_config_id,
            priority,
            is_enabled,
        }
    }
}
