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
const LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
const LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS";
/// 封装当前函数，减少配置运行时调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
fn ensure_legacy_signed_read_only_allowed() -> Result<()> {
    let confirmation = std::env::var(LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV).ok();
    if confirmation.as_deref().map(str::trim) == Some(LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN) {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "{}={} is required before using legacy rust_quant_infrastructure OKX account adapter signed read-only queries; prefer the quant_web execution reconciliation path with exact credential_id and target task scope",
        LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV,
        LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN
    ))
}
/// OKX市场数据适配器
pub struct OkxMarketDataAdapter {
    /// 外部服务客户端。
    client: OkxMarket,
}
impl OkxMarketDataAdapter {
    /// 构建 配置、基础设施和运行时 所需实例，并集中初始化依赖和默认状态。
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_ticker(&self, symbol: &str) -> Result<serde_json::Value> {
        debug!("OKX: 获取Ticker - {}", symbol);
        let tickers = self.client.get_ticker(symbol).await?;
        Ok(serde_json::to_value(&tickers)?)
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_tickers(&self, inst_type: &str) -> Result<Vec<serde_json::Value>> {
        debug!("OKX: 批量获取Ticker - {}", inst_type);
        let tickers = self.client.get_tickers(inst_type).await?;
        Ok(tickers
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect())
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 账户客户端，用于运行时配置或基础设施依赖。
    account_client: OkxAccount,
    /// 资产客户端，用于运行时配置或基础设施依赖。
    asset_client: OkxAsset,
}
impl OkxAccountAdapter {
    /// 构建 配置、基础设施和运行时 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Result<Self> {
        ensure_legacy_signed_read_only_allowed()?;
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    async fn fetch_balance(&self, currency: Option<&str>) -> Result<serde_json::Value> {
        debug!("OKX: 获取账户余额 - {:?}", currency);
        let balances = self.account_client.get_balance(currency).await?;
        Ok(serde_json::to_value(&balances)?)
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 外部服务客户端。
    client: OkxContracts,
}
impl OkxContractsAdapter {
    /// 构建 配置、基础设施和运行时 所需实例，并集中初始化依赖和默认状态。
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
    use std::sync::{Mutex, OnceLock};
    const TEST_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
    /// 封装环境变量lock，减少配置运行时调用方重复实现相同细节。
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
    struct EnvSnapshot {
        /// 值；为空时表示该条件不启用。
        value: Option<String>,
    }
    impl EnvSnapshot {
        /// 提供capture的集中实现，避免配置运行时调用方重复处理相同细节。
        fn capture() -> Self {
            Self {
                value: std::env::var(TEST_CONFIRM_ENV).ok(),
            }
        }
    }
    impl Drop for EnvSnapshot {
        /// 封装释放，减少配置运行时调用方重复实现相同细节。
        fn drop(&mut self) {
            match &self.value {
                Some(value) => std::env::set_var(TEST_CONFIRM_ENV, value),
                None => std::env::remove_var(TEST_CONFIRM_ENV),
            }
        }
    }
    #[test]
    fn legacy_okx_account_adapter_requires_signed_read_only_confirmation_before_okx_client() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        let error = match OkxAccountAdapter::new() {
            Ok(_) => panic!(
                "legacy OKX account adapter must require explicit signed read-only confirmation"
            ),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(
            message.contains(TEST_CONFIRM_ENV),
            "unexpected error: {message}"
        );
    }
    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_okx_market_adapter() {
        let adapter = OkxMarketDataAdapter::new().unwrap();
        assert_eq!(adapter.name(), "okx");
        let ticker = adapter.fetch_ticker("BTC-USDT").await;
        assert!(ticker.is_ok());
    }
}
