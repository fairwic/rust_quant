use crate::entities::{
    FundFlowAlert, MarketAnomaly, MarketRankEvent, MarketRankSnapshot, MarketVelocityEpisode,
    MarketVelocityEpisodeWrite,
};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
#[async_trait]
pub trait MarketAnomalyRepository: Send + Sync {
    async fn save(&self, anomaly: &MarketAnomaly) -> Result<i64>;
    async fn mark_exited(&self, symbol: &str) -> Result<()>;
    async fn get_latest_update_time(&self) -> Result<Option<DateTime<Utc>>>;
    async fn get_all_active(&self) -> Result<Vec<MarketAnomaly>>;
    async fn clear_stale_period_data(
        &self,
        clear_15m: bool,
        clear_4h: bool,
        clear_24h: bool,
    ) -> Result<()>;
    async fn save_rank_event(&self, event: &MarketRankEvent) -> Result<i64>;
    async fn upsert_market_velocity_episode(
        &self,
        episode: &MarketVelocityEpisode,
    ) -> Result<(i64, MarketVelocityEpisodeWrite)>;
    async fn attach_rank_event_to_market_velocity_episode(
        &self,
        episode_id: i64,
        rank_event_id: i64,
        escalated_at: DateTime<Utc>,
    ) -> Result<()>;
    async fn close_stale_market_velocity_episodes(
        &self,
        exchange: &str,
        stale_before: DateTime<Utc>,
    ) -> Result<u64>;
    async fn save_rank_snapshots(&self, snapshots: &[MarketRankSnapshot]) -> Result<()>;
    async fn load_recent_rank_snapshots(
        &self,
        exchange: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<MarketRankSnapshot>>;
    async fn delete_rank_snapshots_before(&self, before: DateTime<Utc>) -> Result<()>;
}
#[async_trait]
pub trait FundFlowAlertRepository: Send + Sync {
    /// 提供save的集中实现，避免配置运行时调用方重复处理相同细节。
    async fn save(&self, alert: &FundFlowAlert) -> Result<i64>;
}
