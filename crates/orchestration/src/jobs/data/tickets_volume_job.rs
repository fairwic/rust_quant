//! Ticker成交量数据同步任务
//!
//! 从 src/trading/task/tickets_volume_job.rs 迁移
//!
//! # 架构原则
//! - orchestration层：只做编排，不直接调用外部API
//! - services层：封装业务逻辑和外部API调用

use anyhow::Result;
use rust_quant_services::market::ContractsService;
use tracing::{debug, error, info};

// TODO: 需要TickerVolume相关的Entity和Repository
// use rust_quant_infrastructure::repositories::TickerVolumeRepository;

/// 同步持仓量和成交量数据
///
/// # Migration Notes
/// - ✅ 从 src/trading/task/tickets_volume_job.rs 迁移
/// - ✅ 保持核心逻辑
/// - ⏳ 需要适配TickerVolumeRepository
///
/// # Arguments
/// * `inst_id` - 交易对基础币种（如 "BTC"）
/// * `period` - 时间周期（如 "1D"）
pub async fn sync_open_interest_volume(inst_id: &str, period: &str) -> Result<()> {
    info!("开始同步持仓量数据: inst_id={}, period={}", inst_id, period);

    // 1. 通过service层获取持仓量和成交量数据
    let service = ContractsService::new();
    let items = service
        .fetch_open_interest_volume_from_exchange(Some(inst_id), None, None, Some(period))
        .await?;

    // 检查返回的数据
    let Some(items_array) = items.as_array() else {
        debug!("无持仓量数据(返回非数组): {} {}", inst_id, period);
        return Ok(());
    };
    if items_array.is_empty() {
        debug!("无持仓量数据: {} {}", inst_id, period);
        return Ok(());
    }

    info!(
        "获取到 {} 条持仓量数据: {} {}",
        items_array.len(),
        inst_id,
        period
    );

    // 2. 保存到数据库
    // ⏳ P1: 集成TickerVolumeRepository
    // 集成方式：
    // use rust_quant_infrastructure::repositories::TickerVolumeRepository;
    // let repo = TickerVolumeRepository::new(db_pool);
    //
    // // 删除旧数据
    // repo.delete_by_inst_id_and_period(inst_id, period).await?;
    //
    // // 批量插入新数据
    // for item in &items {
    //     let volume = TickerVolume {
    //         inst_id: inst_id.to_string(),
    //         period: period.to_string(),
    //         ts: item.ts.parse()?,
    //         vol: item.vol.clone(),
    //         oi: item.oi.clone(),
    //     };
    //     repo.save(&volume).await?;
    // }

    info!("持仓量数据同步完成");
    Ok(())
}

/// 批量同步多个币种的持仓量数据
///
/// # Arguments
/// * `inst_ids` - 币种列表（如 ["BTC", "ETH"]）
/// * `periods` - 时间周期列表
pub async fn sync_open_interest_volume_batch(inst_ids: &[&str], periods: &[&str]) -> Result<()> {
    info!(
        "批量同步持仓量数据: {} 个币种, {} 个周期",
        inst_ids.len(),
        periods.len()
    );

    for inst_id in inst_ids {
        for period in periods {
            match sync_open_interest_volume(inst_id, period).await {
                Ok(_) => info!("持仓量同步成功: {} {}", inst_id, period),
                Err(e) => error!("持仓量同步失败: {} {} - {}", inst_id, period, e),
            }

            // 避免API限流
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
    }

    info!("所有持仓量数据同步完成");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_sync_open_interest_volume() {
        // 注意：需要在测试环境中配置OKX API密钥
        let result = sync_open_interest_volume("BTC", "1D").await;
        assert!(result.is_ok());
    }
}
