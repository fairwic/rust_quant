//! 资产服务
//!
//! 封装交易所Asset API调用，提供资产查询业务逻辑
//!
//! # 架构
//! - 依赖domain::traits::ExchangeAccount接口
//! - 支持多交易所扩展

use anyhow::Result;
use okx::dto::asset_dto::AssetBalance;
use rust_quant_infrastructure::ExchangeFactory;
use tracing::info;

/// 资产服务
///
/// 职责：封装交易所资产API调用，支持多交易所
pub struct AssetService;

impl AssetService {
    pub fn new() -> Self {
        Self
    }

    /// 从交易所获取指定币种余额
    ///
    /// # Arguments
    /// * `currencies` - 币种列表，None表示获取所有币种
    ///
    /// # Returns
    /// * 余额列表
    ///
    /// # Note
    /// 使用默认交易所（从环境变量 DEFAULT_EXCHANGE）
    pub async fn fetch_balances_from_exchange(
        &self,
        currencies: Option<&[String]>,
    ) -> Result<Vec<AssetBalance>> {
        let exchange = ExchangeFactory::create_default_account()?;
        let balances_json = exchange.fetch_asset_balances(currencies).await?;

        // 转换为AssetBalance（保持向后兼容）
        let balances: Vec<AssetBalance> = serde_json::from_value(balances_json)?;

        info!(
            "✅ 从交易所 {} 获取了 {} 个币种余额",
            exchange.name(),
            balances.len()
        );
        Ok(balances)
    }

    /// 获取USDT余额
    pub async fn fetch_usdt_balance(&self) -> Result<Vec<AssetBalance>> {
        let ccy = vec!["USDT".to_string()];
        self.fetch_balances_from_exchange(Some(&ccy)).await
    }

    /// 获取所有币种余额
    pub async fn fetch_all_balances(&self) -> Result<Vec<AssetBalance>> {
        self.fetch_balances_from_exchange(None).await
    }

    /// 获取指定币种余额
    pub async fn fetch_specific_balances(
        &self,
        currencies: Vec<String>,
    ) -> Result<Vec<AssetBalance>> {
        self.fetch_balances_from_exchange(Some(&currencies)).await
    }
}

impl Default for AssetService {
    fn default() -> Self {
        Self::new()
    }
}
