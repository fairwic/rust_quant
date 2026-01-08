use crate::entities::funding_rate::FundingRate;
use anyhow::Result;
use async_trait::async_trait;

/// 资金费率仓储接口
#[async_trait]
pub trait FundingRateRepository: Send + Sync {
    /// 保存资金费率 (如果存在则更新)
    async fn save(&self, funding_rate: FundingRate) -> Result<()>;

    /// 批量保存资金费率
    async fn save_batch(&self, funding_rates: Vec<FundingRate>) -> Result<()>;

    /// 获取最新的资金费率
    async fn find_latest(&self, inst_id: &str) -> Result<Option<FundingRate>>;

    /// 获取资金费率历史
    /// start_time: 开始时间戳 (毫秒)
    /// end_time: 结束时间戳 (毫秒)
    async fn find_history(
        &self,
        inst_id: &str,
        start_time: i64,
        end_time: i64,
        limit: Option<i64>,
    ) -> Result<Vec<FundingRate>>;

    /// 获取最早的资金费率 (用于历史回填)
    async fn find_oldest(&self, inst_id: &str) -> Result<Option<FundingRate>>;
}
