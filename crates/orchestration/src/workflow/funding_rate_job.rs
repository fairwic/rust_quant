use anyhow::Result;
use rust_quant_services::market::funding_rate_sync_service::FundingRateSyncService;
use tracing::{error, info};

/// 资金费率同步任务
///
/// # Architecture
/// orchestration层：只做编排，调用service层完成业务逻辑
pub struct FundingRateJob;

impl FundingRateJob {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FundingRateJob {
    fn default() -> Self {
        Self::new()
    }
}

impl FundingRateJob {
    /// 执行资金费率同步（增量 + 历史）
    ///
    /// # Arguments
    /// * `inst_ids` - 交易对列表
    pub async fn sync_funding_rates(inst_ids: &[String]) -> Result<()> {
        let service = FundingRateSyncService::new()?;

        info!("📈 开始同步资金费率: {} 个交易对", inst_ids.len());

        match service.sync_dynamic(inst_ids).await {
            Ok(_) => info!("✅ 资金费率同步任务完成"),
            Err(e) => error!("❌ 资金费率同步任务失败: {}", e),
        }

        Ok(())
    }
}
