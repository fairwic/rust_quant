use crate::entities::external_market_snapshot::ExternalMarketSnapshot;
use anyhow::Result;
use async_trait::async_trait;
/// 外部市场快照仓储接口
#[async_trait]
pub trait ExternalMarketSnapshotRepository: Send + Sync {
    /// 保存单条快照（存在则更新）
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn save(&self, snapshot: ExternalMarketSnapshot) -> Result<()>;
    /// 批量保存快照
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    async fn save_batch(&self, snapshots: Vec<ExternalMarketSnapshot>) -> Result<()>;
    /// 按来源/标的/指标类型查询时间范围
    /// 封装当前函数，减少行情数据调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn find_range(
        &self,
        source: &str,
        symbol: &str,
        metric_type: &str,
        start_time: i64,
        end_time: i64,
        limit: Option<i64>,
    ) -> Result<Vec<ExternalMarketSnapshot>>;
}
