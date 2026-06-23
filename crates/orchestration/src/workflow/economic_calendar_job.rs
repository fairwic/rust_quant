//! 经济日历同步任务
//!
//! # Architecture
//! orchestration层：只做编排，调用service层完成业务逻辑
use anyhow::Result;
use rust_quant_services::market::economic_calendar_sync_service::EconomicCalendarSyncService;
use tracing::{error, info};
/// 经济日历同步任务
pub struct EconomicCalendarJob;
impl EconomicCalendarJob {
    pub fn new() -> Self {
        Self
    }
}
impl Default for EconomicCalendarJob {
    fn default() -> Self {
        Self::new()
    }
}
impl EconomicCalendarJob {
    /// 执行经济日历同步（增量 + 历史回填）
    /// 封装当前函数，减少量化核心调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    pub async fn sync_economic_calendar() -> Result<()> {
        let service = EconomicCalendarSyncService::new()?;
        info!("📅 开始同步经济日历数据");
        match service.sync_all().await {
            Ok(_) => info!("✅ 经济日历同步任务完成"),
            Err(e) => error!("❌ 经济日历同步任务失败: {}", e),
        }
        Ok(())
    }
    /// 仅同步增量数据（最新事件）
    pub async fn sync_incremental() -> Result<usize> {
        let service = EconomicCalendarSyncService::new()?;
        info!("⏩ 经济日历增量同步");
        match service.sync_incremental().await {
            Ok(count) => {
                info!("✅ 增量同步完成，新增 {} 条", count);
                Ok(count)
            }
            Err(e) => {
                error!("❌ 增量同步失败: {}", e);
                Err(e)
            }
        }
    }
}
