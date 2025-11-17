//! OKX交易所适配器
//!
//! 实现domain层定义的交易所接口，将OKX SDK适配为统一接口

use anyhow::Result;
use async_trait::async_trait;
use okx::api::account::{OkxAccount, OkxContracts};
use okx::api::api_trait::OkxApiTrait;
use okx::api::asset::OkxAsset;
use okx::api::market::OkxMarket;
use rust_quant_domain::traits::{
    ExchangeAccount, ExchangeContracts, ExchangeMarketData, ExchangePublicData,
};
use tracing::debug;

/// OKX市场数据适配器
pub struct OkxMarketDataAdapter {
    client: OkxMarket,
}

impl OkxMarketDataAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: OkxMarket::from_env()?,
        })
    }
}

#[async_trait]
impl ExchangeMarketData for OkxMarketDataAdapter {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value> {
        debug!("OKX: 获取Ticker - {}", symbol);
        let tickers = self.client.get_ticker(symbol).await?;
        Ok(serde_json::to_value(&tickers)?)
    }

    async fn fetch_tickers(&self, inst_type: &str) -> Result<Vec<serde_json::Value>> {
        debug!("OKX: 批量获取Ticker - {}", inst_type);
        let tickers = self.client.get_tickers(inst_type).await?;
        Ok(tickers
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect())
    }

    async fn fetch_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        debug!("OKX: 获取历史K线 - {} {}", symbol, timeframe);

        let after = start.map(|s| s.to_string());
        let before = end.map(|e| e.to_string());
        let limit_str = limit.map(|l| l.to_string());

        let candles = self
            .client
            .get_history_candles(
                symbol,
                timeframe,
                after.as_deref(),
                before.as_deref(),
                limit_str.as_deref(),
            )
            .await?;

        Ok(candles
            .into_iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect())
    }

    async fn fetch_latest_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>> {
        debug!("OKX: 获取最新K线 - {} {}", symbol, timeframe);

        let limit_str = limit.map(|l| l.to_string());

        let candles = self
            .client
            .get_candles(symbol, timeframe, None, None, limit_str.as_deref())
            .await?;

        Ok(candles
            .into_iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect())
    }
}

/// OKX账户适配器
pub struct OkxAccountAdapter {
    account_client: OkxAccount,
    asset_client: OkxAsset,
}

impl OkxAccountAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            account_client: OkxAccount::from_env()?,
            asset_client: OkxAsset::from_env()?,
        })
    }
}

#[async_trait]
impl ExchangeAccount for OkxAccountAdapter {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn fetch_balance(&self, currency: Option<&str>) -> Result<serde_json::Value> {
        debug!("OKX: 获取账户余额 - {:?}", currency);
        let balances = self.account_client.get_balance(currency).await?;
        Ok(serde_json::to_value(&balances)?)
    }

    async fn fetch_asset_balances(
        &self,
        currencies: Option<&[String]>,
    ) -> Result<serde_json::Value> {
        debug!("OKX: 获取资产余额 - {:?}", currencies);
        let currencies_vec: Option<Vec<String>> = currencies.map(|c| c.to_vec());
        let currencies_ref: Option<&Vec<String>> = currencies_vec.as_ref();
        let balances = self.asset_client.get_balances(currencies_ref).await?;
        Ok(serde_json::to_value(&balances)?)
    }
}

/// OKX合约适配器
pub struct OkxContractsAdapter {
    client: OkxContracts,
}

impl OkxContractsAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: OkxContracts::from_env()?,
        })
    }
}

#[async_trait]
impl ExchangeContracts for OkxContractsAdapter {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn fetch_open_interest_volume(
        &self,
        inst_id: Option<&str>,
        begin: Option<i64>,
        end: Option<i64>,
        period: Option<&str>,
    ) -> Result<serde_json::Value> {
        debug!("OKX: 获取持仓量数据 - {:?} {:?}", inst_id, period);
        let items = self
            .client
            .get_open_interest_volume(inst_id, begin, end, period)
            .await?;
        Ok(serde_json::to_value(&items)?)
    }
}

/// OKX公共数据适配器
pub struct OkxPublicDataAdapter;

impl OkxPublicDataAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

#[async_trait]
impl ExchangePublicData for OkxPublicDataAdapter {
    fn name(&self) -> &'static str {
        "okx"
    }

    async fn fetch_announcements(
        &self,
        _ann_type: Option<&str>,
        _page_size: Option<&str>,
    ) -> Result<Vec<String>> {
        debug!("OKX: 获取公告数据");
        // OKX SDK可能没有公告API，返回空
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_okx_market_adapter() {
        let adapter = OkxMarketDataAdapter::new().unwrap();
        assert_eq!(adapter.name(), "okx");

        let ticker = adapter.fetch_ticker("BTC-USDT").await;
        assert!(ticker.is_ok());
    }
}
