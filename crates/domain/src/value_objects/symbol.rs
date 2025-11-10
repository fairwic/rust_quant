//! 交易对符号值对象

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SymbolError {
    #[error("交易对格式无效: {0}")]
    InvalidFormat(String),

    #[error("交易对为空")]
    Empty,

    #[error("交易对包含非法字符: {0}")]
    IllegalCharacter(String),
}

/// 交易对符号值对象
///
/// 业务规则:
/// - 格式: BASE-QUOTE (如 "BTC-USDT")
/// - 大小写: 统一转为大写
/// - 不允许空格和特殊字符
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol(String);

impl Symbol {
    /// 创建交易对符号 (带验证)
    pub fn new(value: impl Into<String>) -> Result<Self, SymbolError> {
        let value = value.into();

        if value.is_empty() {
            return Err(SymbolError::Empty);
        }

        // 转为大写
        let value = value.to_uppercase();

        // 验证格式: 应该包含一个连字符
        if !value.contains('-') {
            return Err(SymbolError::InvalidFormat(format!(
                "交易对应该包含'-'分隔符，例如: BTC-USDT, 实际: {}",
                value
            )));
        }

        // 验证字符: 只允许字母、数字、连字符
        if !value.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Err(SymbolError::IllegalCharacter(value));
        }

        // 分割并验证
        let parts: Vec<&str> = value.split('-').collect();
        if parts.len() != 2 {
            return Err(SymbolError::InvalidFormat(format!(
                "交易对格式应该是 BASE-QUOTE, 实际: {}",
                value
            )));
        }

        if parts[0].is_empty() || parts[1].is_empty() {
            return Err(SymbolError::InvalidFormat(format!(
                "交易对的基础货币和计价货币不能为空, 实际: {}",
                value
            )));
        }

        Ok(Self(value))
    }

    /// 获取符号字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 获取基础货币 (如 BTC-USDT -> BTC)
    pub fn base_currency(&self) -> &str {
        self.0.split('-').next().unwrap()
    }

    /// 获取计价货币 (如 BTC-USDT -> USDT)
    pub fn quote_currency(&self) -> &str {
        self.0.split('-').nth(1).unwrap()
    }

    /// 转为OKX格式 (BTC-USDT-SWAP)
    pub fn to_okx_swap(&self) -> String {
        format!("{}-SWAP", self.0)
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Symbol> for String {
    fn from(symbol: Symbol) -> Self {
        symbol.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_symbol() {
        let symbol = Symbol::new("BTC-USDT").unwrap();
        assert_eq!(symbol.as_str(), "BTC-USDT");
        assert_eq!(symbol.base_currency(), "BTC");
        assert_eq!(symbol.quote_currency(), "USDT");
    }

    #[test]
    fn test_lowercase_converted() {
        let symbol = Symbol::new("btc-usdt").unwrap();
        assert_eq!(symbol.as_str(), "BTC-USDT");
    }

    #[test]
    fn test_invalid_format() {
        // 缺少连字符
        assert!(Symbol::new("BTCUSDT").is_err());

        // 多个连字符
        assert!(Symbol::new("BTC-USDT-SWAP").is_err());

        // 空字符串
        assert!(Symbol::new("").is_err());

        // 包含空格
        assert!(Symbol::new("BTC USDT").is_err());
    }

    #[test]
    fn test_okx_format() {
        let symbol = Symbol::new("BTC-USDT").unwrap();
        assert_eq!(symbol.to_okx_swap(), "BTC-USDT-SWAP");
    }
}
