use crate::entities::{FundFlowAlert, MarketAnomaly, MarketRankEvent, MarketRankSnapshot};
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
    /// 追加市场排名事件流水
    async fn save_rank_event(&self, event: &MarketRankEvent) -> Result<i64>;
    /// 批量保存市场排名价格快照，用于重启后恢复历史排名和价格证据
    async fn save_rank_snapshots(&self, snapshots: &[MarketRankSnapshot]) -> Result<()>;
    /// 读取指定时间之后的市场排名价格快照
    async fn load_recent_rank_snapshots(
        &self,
        exchange: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<MarketRankSnapshot>>;
    /// 删除过期市场排名价格快照
    async fn delete_rank_snapshots_before(&self, before: DateTime<Utc>) -> Result<()>;
}

#[async_trait]
pub trait FundFlowAlertRepository: Send + Sync {
    async fn save(&self, alert: &FundFlowAlert) -> Result<i64>;
}
