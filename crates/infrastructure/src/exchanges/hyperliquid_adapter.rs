use anyhow::{anyhow, Result};
use hyperliquid_rust_sdk::{AssetContext, BaseUrl, FundingHistoryResponse, InfoClient, Meta};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperliquidFundingHistoryPoint {
    pub coin: String,
    pub funding_rate: f64,
    pub premium: Option<f64>,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperliquidAssetContextSnapshot {
    pub coin: String,
    pub funding: Option<f64>,
    pub open_interest: Option<f64>,
    pub premium: Option<f64>,
    pub oracle_price: Option<f64>,
    pub mark_price: Option<f64>,
}

/// Hyperliquid 公共数据适配器
pub struct HyperliquidPublicAdapter;

impl HyperliquidPublicAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub async fn fetch_funding_history(
        &self,
        coin: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<HyperliquidFundingHistoryPoint>> {
        let client = Self::build_info_client().await?;
        let rows = client
            .funding_history(coin.to_string(), start_time as u64, Some(end_time as u64))
            .await?;

        Self::from_sdk_funding_history(rows)
    }

    pub async fn fetch_meta_and_asset_ctxs(
        &self,
        coin: &str,
    ) -> Result<HyperliquidAssetContextSnapshot> {
        let client = Self::build_info_client().await?;
        let (meta, contexts) = client.meta_and_asset_contexts().await?;

        Self::from_sdk_meta_and_asset_ctxs(&meta, &contexts, coin)
    }

    pub fn from_sdk_funding_history(
        rows: Vec<FundingHistoryResponse>,
    ) -> Result<Vec<HyperliquidFundingHistoryPoint>> {
        rows.iter()
            .map(|item| {
                Ok(HyperliquidFundingHistoryPoint {
                    coin: item.coin.clone(),
                    funding_rate: parse_required_f64(&item.funding_rate, "funding_rate")?,
                    premium: Some(parse_required_f64(&item.premium, "premium")?),
                    time: item.time as i64,
                })
            })
            .collect()
    }

    pub fn from_sdk_meta_and_asset_ctxs(
        meta: &Meta,
        contexts: &[AssetContext],
        coin: &str,
    ) -> Result<HyperliquidAssetContextSnapshot> {
        let index = meta
            .universe
            .iter()
            .position(|item| item.name == coin)
            .ok_or_else(|| anyhow!("coin {} not found in universe", coin))?;
        let ctx = contexts
            .get(index)
            .ok_or_else(|| anyhow!("context index {} missing", index))?;

        Ok(HyperliquidAssetContextSnapshot {
            coin: coin.to_string(),
            funding: Some(parse_required_f64(&ctx.funding, "funding")?),
            open_interest: Some(parse_required_f64(&ctx.open_interest, "open_interest")?),
            premium: parse_optional_f64_string(ctx.premium.as_deref(), "premium")?,
            oracle_price: Some(parse_required_f64(&ctx.oracle_px, "oracle_price")?),
            mark_price: Some(parse_required_f64(&ctx.mark_px, "mark_price")?),
        })
    }

    async fn build_info_client() -> Result<InfoClient> {
        Ok(InfoClient::new(None, Some(BaseUrl::Mainnet)).await?)
    }
}

fn parse_required_f64(value: &str, field: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .map_err(|e| anyhow!("failed to parse {}: {}", field, e))
}

fn parse_optional_f64_string(value: Option<&str>, field: &str) -> Result<Option<f64>> {
    match value {
        None => Ok(None),
        Some(raw) => raw
            .parse::<f64>()
            .map(Some)
            .map_err(|e| anyhow!("failed to parse {}: {}", field, e)),
    }
}
