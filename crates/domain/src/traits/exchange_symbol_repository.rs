use crate::entities::{ExchangeSymbol, ExchangeSymbolListingEvent};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait ExchangeSymbolRepository: Send + Sync {
    async fn upsert_many(&self, symbols: Vec<ExchangeSymbol>) -> Result<u64>;

    async fn find_by_exchange(
        &self,
        exchange: &str,
        status: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ExchangeSymbol>>;

    async fn find_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbol>>;

    async fn record_first_seen_many(
        &self,
        symbols: &[ExchangeSymbol],
    ) -> Result<Vec<ExchangeSymbolListingEvent>>;

    async fn find_listing_events_by_asset(
        &self,
        base_asset: &str,
        quote_asset: &str,
        market_type: &str,
    ) -> Result<Vec<ExchangeSymbolListingEvent>>;
}
