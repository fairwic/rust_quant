use crate::entities::{FundFlowAlert, MarketAnomaly};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait MarketAnomalyRepository: Send + Sync {
    /// UPSERT: 按 symbol 更新或插入
    async fn save(&self, anomaly: &MarketAnomaly) -> Result<i64>;
    /// 标记跌出 Top 150 的币种
    async fn mark_exited(&self, symbol: &str) -> Result<()>;
    /// 获取最新的更新时间 (用于重启恢复)
    async fn get_latest_update_time(&self) -> Result<Option<DateTime<Utc>>>;
    /// 获取所有 ACTIVE 状态的记录 (用于重启恢复)
    async fn get_all_active(&self) -> Result<Vec<MarketAnomaly>>;
    /// 清除过期的周期数据 (重启时使用)
    async fn clear_stale_period_data(
        &self,
        clear_15m: bool,
        clear_4h: bool,
        clear_24h: bool,
    ) -> Result<()>;
}

#[async_trait]
pub trait FundFlowAlertRepository: Send + Sync {
    async fn save(&self, alert: &FundFlowAlert) -> Result<i64>;
}
