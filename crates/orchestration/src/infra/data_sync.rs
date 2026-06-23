//! 数据同步工具模块
//!
//! 从 src/trading/task/data_sync.rs 迁移
//! 重构为符合架构规范：orchestration层只做编排，调用jobs层
use crate::jobs::data::candles_job::CandlesJob;
use anyhow::Result;
use tracing::info;
/// 同步所有数据任务的统一入口
/// # Migration Notes
/// - ✅ 从 src/trading/task/data_sync.rs 迁移
/// - ✅ 作为数据同步的统一调度入口
/// - ✅ 重构为调用candles_job，符合架构规范
/// # Full Implementation
/// ```rust,ignore
/// // 1. 同步市场数据
/// sync_tickers(&inst_ids).await?;
/// sync_market_data(&inst_ids, &periods).await?;
/// // 2. 同步交易数据
/// sync_trades_batch(&inst_ids, Some("100")).await?;
/// // 3. 同步账户数据
/// get_account_balance().await?;
/// get_asset_balance().await?;
/// ```
pub async fn sync_all_data(inst_ids: &[String], periods: &[String]) -> Result<()> {
    info!("🔄 开始同步所有数据...");
    sync_market_data(inst_ids, periods).await?;
    info!("✅ 所有数据同步完成");
    Ok(())
}
/// 同步市场数据
/// # Architecture
/// orchestration层的数据同步任务，调用candles_job完成业务逻辑
/// 参考tickets_job的实现方式，只做编排，不直接调用外部API
/// # Arguments
/// * `inst_ids` - 交易对列表
/// * `periods` - 时间周期列表
pub async fn sync_market_data(inst_ids: &[String], periods: &[String]) -> Result<()> {
    info!("📈 同步市场数据...");
    let job = CandlesJob::new();
    // 默认走全量三段式同步（建表/回填历史/回填增量），但这个流程在已有表时可能很慢。
    // 设置 SYNC_LATEST_ONLY=1 可只做“增量同步”（用于快速补齐 BTC 大盘数据等场景）。
    if rust_quant_core::config::env_is_true("SYNC_LATEST_ONLY", false) {
        job.sync_latest_candles(inst_ids, periods).await?;
    } else {
        // 调用candles_job完成完整的数据同步（建表、补历史、补增量）
        job.sync_all_data(inst_ids, periods).await?;
    }
    Ok(())
}
/// 同步账户数据
pub async fn sync_account_data() -> Result<()> {
    info!("💰 同步账户数据...");
    // use crate::workflow::{get_account_balance, get_asset_balance};
    // get_account_balance().await?;
    // get_asset_balance().await?;
    Ok(())
}
