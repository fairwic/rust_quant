use crate::entities::{ExchangeSymbol, ExchangeSymbolListingEvent};
use anyhow::Result;
use async_trait::async_trait;
#[async_trait]
pub trait ExchangeSymbolRepository: Send + Sync {
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn upsert_many(&self, symbols: Vec<ExchangeSymbol>) -> Result<u64>;
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn find_by_exchange(
        &self,
        exchange: &str,
        status: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ExchangeSymbol>>;
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn find_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbol>>;
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn record_first_seen_many(
        &self,
        symbols: &[ExchangeSymbol],
    ) -> Result<Vec<ExchangeSymbolListingEvent>>;
    /// 封装当前函数，减少配置运行时调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn find_listing_events_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbolListingEvent>>;
}
