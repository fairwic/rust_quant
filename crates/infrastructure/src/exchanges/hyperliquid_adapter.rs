use anyhow::{anyhow, Result};
use hyperliquid_rust_sdk::{AssetContext, BaseUrl, FundingHistoryResponse, InfoClient, Meta};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperliquidFundingHistoryPoint {
    /// coin，用于运行时配置或基础设施依赖。
    pub coin: String,
    /// 资金费率。
    pub funding_rate: f64,
    /// 溢价率；为空时表示交易所未返回该指标。
    pub premium: Option<f64>,
    /// 时间字段。
    pub time: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperliquidAssetContextSnapshot {
    /// 币种标识。
    pub coin: String,
    /// 资金费率；为空时使用默认值或表示不限制。
    pub funding: Option<f64>,
    /// 未平仓量；为空时表示交易所未返回该指标。
    pub open_interest: Option<f64>,
    /// 溢价率；为空时表示交易所未返回该指标。
    pub premium: Option<f64>,
    /// 价格数值。
    pub oracle_price: Option<f64>,
    /// 价格数值。
    pub mark_price: Option<f64>,
}
/// Hyperliquid 公共数据适配器
pub struct HyperliquidPublicAdapter;
impl HyperliquidPublicAdapter {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
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
    /// 加载 配置、基础设施和运行时 运行所需数据，并把缺失或异常交给调用方处理。
    pub async fn fetch_meta_and_asset_ctxs(
        &self,
        coin: &str,
    ) -> Result<HyperliquidAssetContextSnapshot> {
        let client = Self::build_info_client().await?;
        let (meta, contexts) = client.meta_and_asset_contexts().await?;
        Self::from_sdk_meta_and_asset_ctxs(&meta, &contexts, coin)
    }
    /// 从外部输入转换为内部模型，隔离 配置、基础设施和运行时 的字段适配细节。
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
    /// 从外部输入转换为内部模型，隔离 配置、基础设施和运行时 的字段适配细节。
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
/// 封装当前函数，减少配置运行时调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
fn parse_required_f64(value: &str, field: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .map_err(|e| anyhow!("failed to parse {}: {}", field, e))
}
/// 解析输入参数并收敛为 配置、基础设施和运行时 可使用的结构化值。
fn parse_optional_f64_string(value: Option<&str>, field: &str) -> Result<Option<f64>> {
    match value {
        None => Ok(None),
        Some(raw) => raw
            .parse::<f64>()
            .map(Some)
            .map_err(|e| anyhow!("failed to parse {}: {}", field, e)),
    }
}
