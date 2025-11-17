//! 交易所抽象接口
//!
//! 定义交易所的统一接口，支持多交易所扩展
//! 遵循依赖倒置原则：services层依赖接口，infrastructure层实现接口

use anyhow::Result;
use async_trait::async_trait;

/// 交易所市场数据接口
///
/// 抽象所有交易所的市场数据访问，统一接口
#[async_trait]
pub trait ExchangeMarketData: Send + Sync {
    /// 获取交易所名称
    fn name(&self) -> &'static str;

    /// 获取单个Ticker数据
    ///
    /// # Arguments
    /// * `symbol` - 交易对（如"BTC-USDT"）
    ///
    /// # Returns
    /// * Ticker数据（JSON格式，便于适配不同交易所）
    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value>;

    /// 批量获取Ticker数据
    ///
    /// # Arguments
    /// * `inst_type` - 合约类型（如"SWAP"、"SPOT"）
    ///
    /// # Returns
    /// * Ticker数据列表
    async fn fetch_tickers(&self, inst_type: &str) -> Result<Vec<serde_json::Value>>;

    /// 获取历史K线数据
    ///
    /// # Arguments
    /// * `symbol` - 交易对
    /// * `timeframe` - 时间周期（如"1m"、"1H"）
    /// * `start` - 开始时间戳（毫秒）
    /// * `end` - 结束时间戳（毫秒）
    /// * `limit` - 数量限制
    async fn fetch_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>>;

    /// 获取最新K线数据
    ///
    /// # Arguments
    /// * `symbol` - 交易对
    /// * `timeframe` - 时间周期
    /// * `limit` - 数量限制
    async fn fetch_latest_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        limit: Option<usize>,
    ) -> Result<Vec<serde_json::Value>>;
}

/// 交易所账户接口
#[async_trait]
pub trait ExchangeAccount: Send + Sync {
    /// 获取交易所名称
    fn name(&self) -> &'static str;

    /// 获取账户余额
    ///
    /// # Arguments
    /// * `currency` - 币种（None表示所有币种）
    async fn fetch_balance(&self, currency: Option<&str>) -> Result<serde_json::Value>;

    /// 获取资产余额（资金账户）
    ///
    /// # Arguments
    /// * `currencies` - 币种列表
    async fn fetch_asset_balances(
        &self,
        currencies: Option<&[String]>,
    ) -> Result<serde_json::Value>;
}

/// 交易所合约接口
#[async_trait]
pub trait ExchangeContracts: Send + Sync {
    /// 获取交易所名称
    fn name(&self) -> &'static str;

    /// 获取持仓量和成交量数据
    ///
    /// # Arguments
    /// * `inst_id` - 交易对基础币种
    /// * `begin` - 开始时间
    /// * `end` - 结束时间
    /// * `period` - 周期
    async fn fetch_open_interest_volume(
        &self,
        inst_id: Option<&str>,
        begin: Option<i64>,
        end: Option<i64>,
        period: Option<&str>,
    ) -> Result<serde_json::Value>;
}

/// 交易所公共数据接口
#[async_trait]
pub trait ExchangePublicData: Send + Sync {
    /// 获取交易所名称
    fn name(&self) -> &'static str;

    /// 获取公告列表
    async fn fetch_announcements(
        &self,
        ann_type: Option<&str>,
        page_size: Option<&str>,
    ) -> Result<Vec<String>>;
}
