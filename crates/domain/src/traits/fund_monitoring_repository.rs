use crate::entities::{FundFlowAlert, MarketAnomaly};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait MarketAnomalyRepository: Send + Sync {
    /// UPSERT: 按 symbol 更新或插入
    async fn save(&self, anomaly: &MarketAnomaly) -> Result<i64>;
    /// 标记跌出 Top 150 的币种
    async fn mark_exited(&self, symbol: &str) -> Result<()>;
}

#[async_trait]
pub trait FundFlowAlertRepository: Send + Sync {
    async fn save(&self, alert: &FundFlowAlert) -> Result<i64>;
    // async fn find_recent_by_symbol(&self, symbol: &str, limit: i64) -> Result<Vec<FundFlowAlert>>;
}
