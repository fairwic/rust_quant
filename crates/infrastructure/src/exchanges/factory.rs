//! 交易所工厂
//!
//! 提供统一的交易所客户端创建接口，支持配置化切换
//!
//! # 架构位置
//! 放在infrastructure层，避免core → infrastructure的循环依赖

use anyhow::{anyhow, Result};
use rust_quant_domain::traits::{
    ExchangeAccount, ExchangeContracts, ExchangeMarketData, ExchangePublicData,
};

use super::{OkxAccountAdapter, OkxContractsAdapter, OkxMarketDataAdapter, OkxPublicDataAdapter};

/// 交易所工厂
///
/// 根据配置创建交易所客户端，支持依赖注入
pub struct ExchangeFactory;

impl ExchangeFactory {
    /// 创建市场数据客户端
    ///
    /// # Arguments
    /// * `exchange_name` - 交易所名称（"okx", "binance", "bybit"）
    ///
    /// # Returns
    /// * 实现ExchangeMarketData接口的客户端
    pub fn create_market_data(exchange_name: &str) -> Result<Box<dyn ExchangeMarketData>> {
        match exchange_name.to_lowercase().as_str() {
            "okx" => Ok(Box::new(OkxMarketDataAdapter::new()?)),
            // 未来添加其他交易所：
            // "binance" => Ok(Box::new(BinanceMarketDataAdapter::new()?)),
            // "bybit" => Ok(Box::new(BybitMarketDataAdapter::new()?)),
            _ => Err(anyhow!("不支持的交易所: {}", exchange_name)),
        }
    }

    /// 从环境变量创建（读取 DEFAULT_EXCHANGE 或 EXCHANGE_NAME）
    pub fn create_default_market_data() -> Result<Box<dyn ExchangeMarketData>> {
        let exchange = std::env::var("DEFAULT_EXCHANGE")
            .or_else(|_| std::env::var("EXCHANGE_NAME"))
            .unwrap_or_else(|_| "okx".to_string());

        Self::create_market_data(&exchange)
    }

    /// 创建账户客户端
    pub fn create_account(exchange_name: &str) -> Result<Box<dyn ExchangeAccount>> {
        match exchange_name.to_lowercase().as_str() {
            "okx" => Ok(Box::new(OkxAccountAdapter::new()?)),
            // "binance" => Ok(Box::new(BinanceAccountAdapter::new()?)),
            _ => Err(anyhow!("不支持的交易所: {}", exchange_name)),
        }
    }

    /// 从环境变量创建账户客户端
    pub fn create_default_account() -> Result<Box<dyn ExchangeAccount>> {
        let exchange = std::env::var("DEFAULT_EXCHANGE").unwrap_or_else(|_| "okx".to_string());
        Self::create_account(&exchange)
    }

    /// 创建合约客户端
    pub fn create_contracts(exchange_name: &str) -> Result<Box<dyn ExchangeContracts>> {
        match exchange_name.to_lowercase().as_str() {
            "okx" => Ok(Box::new(OkxContractsAdapter::new()?)),
            _ => Err(anyhow!("不支持的交易所: {}", exchange_name)),
        }
    }

    /// 从环境变量创建合约客户端
    pub fn create_default_contracts() -> Result<Box<dyn ExchangeContracts>> {
        let exchange = std::env::var("DEFAULT_EXCHANGE").unwrap_or_else(|_| "okx".to_string());
        Self::create_contracts(&exchange)
    }

    /// 创建公共数据客户端
    pub fn create_public_data(exchange_name: &str) -> Result<Box<dyn ExchangePublicData>> {
        match exchange_name.to_lowercase().as_str() {
            "okx" => Ok(Box::new(OkxPublicDataAdapter::new()?)),
            _ => Err(anyhow!("不支持的交易所: {}", exchange_name)),
        }
    }

    /// 创建多个交易所的市场数据客户端（用于套利）
    ///
    /// # Arguments
    /// * `exchanges` - 交易所名称列表
    ///
    /// # Returns
    /// * 客户端列表
    pub fn create_multiple_market_data(exchanges: &[&str]) -> Vec<Box<dyn ExchangeMarketData>> {
        exchanges
            .iter()
            .filter_map(|name| Self::create_market_data(name).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creation() {
        // 测试工厂能够创建OKX客户端
        let result = ExchangeFactory::create_market_data("okx");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unsupported_exchange() {
        // 测试不支持的交易所返回错误
        let result = ExchangeFactory::create_market_data("unknown");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_exchange() {
        // 测试默认交易所创建
        std::env::set_var("DEFAULT_EXCHANGE", "okx");
        let result = ExchangeFactory::create_default_market_data();
        assert!(result.is_ok());
    }
}
