//! 账户服务
//!
//! 封装交易所Account API调用，提供账户查询业务逻辑
//!
//! # 架构
//! - 依赖domain::traits::ExchangeAccount接口
//! - 支持多交易所扩展

use anyhow::Result;
use rust_quant_infrastructure::ExchangeFactory;
use tracing::info;

/// 账户服务
///
/// 职责：封装交易所账户API调用，支持多交易所
pub struct AccountService;

impl AccountService {
    pub fn new() -> Self {
        Self
    }

    /// 从交易所获取账户余额
    ///
    /// # Arguments
    /// * `currency` - 币种（None表示获取所有币种）
    ///
    /// # Returns
    /// * 账户余额（JSON格式）
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE）
    pub async fn fetch_balance_from_exchange(
        &self,
        currency: Option<&str>,
    ) -> Result<serde_json::Value> {
        let exchange = ExchangeFactory::create_default_account()?;
        let balances = exchange.fetch_balance(currency).await?;

        info!("✅ 从交易所 {} 获取了账户余额", exchange.name());
        Ok(balances)
    }

    /// 获取所有币种余额
    pub async fn fetch_all_balances(&self) -> Result<serde_json::Value> {
        self.fetch_balance_from_exchange(None).await
    }

    /// 获取指定币种余额
    pub async fn fetch_currency_balance(&self, currency: &str) -> Result<serde_json::Value> {
        self.fetch_balance_from_exchange(Some(currency)).await
    }
}

impl Default for AccountService {
    fn default() -> Self {
        Self::new()
    }
}
