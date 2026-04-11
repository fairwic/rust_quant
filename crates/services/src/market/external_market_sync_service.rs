use anyhow::Result;
use async_trait::async_trait;
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use rust_quant_infrastructure::{
    exchanges::{
        HyperliquidAssetContextSnapshot, HyperliquidFundingHistoryPoint, HyperliquidPublicAdapter,
    },
    repositories::SqlxExternalMarketSnapshotRepository,
};
use serde_json::json;
use std::sync::Arc;

const HYPERLIQUID_SOURCE: &str = "hyperliquid";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalMarketSource {
    Hyperliquid,
    Okx,
    Binance,
}

impl ExternalMarketSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hyperliquid => "hyperliquid",
            Self::Okx => "okx",
            Self::Binance => "binance",
        }
    }
}

pub fn normalize_external_market_symbol(symbol: &str) -> String {
    let normalized = symbol.trim().to_uppercase();
    if let Some((base, _)) = normalized.split_once("-USDT") {
        return base.to_string();
    }
    if let Some(base) = normalized.strip_suffix("USDT") {
        return base.to_string();
    }
    normalized
}

#[async_trait]
pub trait ExternalMarketDataProvider: Send + Sync {
    async fn fetch_hyperliquid_funding_history(
        &self,
        coin: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<HyperliquidFundingHistoryPoint>>;

    async fn fetch_hyperliquid_meta_and_asset_ctxs(
        &self,
        coin: &str,
    ) -> Result<HyperliquidAssetContextSnapshot>;
}

pub struct HyperliquidExternalMarketDataProvider {
    adapter: HyperliquidPublicAdapter,
}

impl HyperliquidExternalMarketDataProvider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            adapter: HyperliquidPublicAdapter::new()?,
        })
    }
}

#[async_trait]
impl ExternalMarketDataProvider for HyperliquidExternalMarketDataProvider {
    async fn fetch_hyperliquid_funding_history(
        &self,
        coin: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<HyperliquidFundingHistoryPoint>> {
        self.adapter
            .fetch_funding_history(coin, start_time, end_time)
            .await
    }

    async fn fetch_hyperliquid_meta_and_asset_ctxs(
        &self,
        coin: &str,
    ) -> Result<HyperliquidAssetContextSnapshot> {
        self.adapter.fetch_meta_and_asset_ctxs(coin).await
    }
}

pub struct ExternalMarketSyncService {
    repo: Arc<dyn ExternalMarketSnapshotRepository>,
    provider: Arc<dyn ExternalMarketDataProvider>,
}

impl ExternalMarketSyncService {
    pub fn new() -> Result<Self> {
        let pool = get_db_pool().clone();
        let repo = Arc::new(SqlxExternalMarketSnapshotRepository::new(pool));
        let provider = Arc::new(HyperliquidExternalMarketDataProvider::new()?);
        Ok(Self { repo, provider })
    }

    pub fn with_repo_and_provider(
        repo: Arc<dyn ExternalMarketSnapshotRepository>,
        provider: Arc<dyn ExternalMarketDataProvider>,
    ) -> Self {
        Self { repo, provider }
    }

    pub async fn sync_hyperliquid_coin(
        &self,
        coin: &str,
        start_time: i64,
        end_time: i64,
        snapshot_time: i64,
    ) -> Result<usize> {
        let funding_rows = self
            .provider
            .fetch_hyperliquid_funding_history(coin, start_time, end_time)
            .await?;
        let asset_ctx = self
            .provider
            .fetch_hyperliquid_meta_and_asset_ctxs(coin)
            .await?;

        let mut snapshots: Vec<ExternalMarketSnapshot> = funding_rows
            .into_iter()
            .map(Self::hyperliquid_funding_point_to_snapshot)
            .collect();
        snapshots.push(Self::hyperliquid_asset_context_to_snapshot(
            asset_ctx,
            snapshot_time,
        ));

        let count = snapshots.len();
        self.repo.save_batch(snapshots).await?;
        Ok(count)
    }

    pub fn hyperliquid_funding_point_to_snapshot(
        point: HyperliquidFundingHistoryPoint,
    ) -> ExternalMarketSnapshot {
        let mut snapshot = ExternalMarketSnapshot::new(
            ExternalMarketSource::Hyperliquid.as_str().to_string(),
            normalize_external_market_symbol(&point.coin),
            "funding".to_string(),
            point.time,
        );
        snapshot.funding_rate = Some(point.funding_rate);
        snapshot.premium = point.premium;
        snapshot.raw_payload = Some(json!({
            "coin": point.coin,
            "funding_rate": point.funding_rate,
            "premium": point.premium,
            "time": point.time
        }));
        snapshot
    }

    pub fn hyperliquid_asset_context_to_snapshot(
        ctx: HyperliquidAssetContextSnapshot,
        metric_time: i64,
    ) -> ExternalMarketSnapshot {
        let mut snapshot = ExternalMarketSnapshot::new(
            ExternalMarketSource::Hyperliquid.as_str().to_string(),
            normalize_external_market_symbol(&ctx.coin),
            "meta".to_string(),
            metric_time,
        );
        snapshot.funding_rate = ctx.funding;
        snapshot.premium = ctx.premium;
        snapshot.open_interest = ctx.open_interest;
        snapshot.oracle_price = ctx.oracle_price;
        snapshot.mark_price = ctx.mark_price;
        snapshot.raw_payload = Some(json!({
            "coin": ctx.coin,
            "funding": ctx.funding,
            "open_interest": ctx.open_interest,
            "premium": ctx.premium,
            "oracle_price": ctx.oracle_price,
            "mark_price": ctx.mark_price
        }));
        snapshot
    }
}
