//! 头部合约数据同步任务
//!
//! 从 src/trading/task/top_contract_job.rs 迁移
//! 同步交易量最大的合约数据
//!
//! # 架构原则
//! - orchestration层：只做编排，不直接调用外部API
//! - services层：封装业务逻辑和外部API调用

use anyhow::Result;
use rust_quant_services::market::TickerService;
use tracing::{debug, error, info};

// TODO: 需要TopContract相关的Entity和Repository
// use rust_quant_infrastructure::repositories::TopContractRepository;

/// 同步头部合约数据
///
/// # Migration Notes
/// - ✅ 从 src/trading/task/top_contract_job.rs 迁移
/// - ✅ 保持核心逻辑
/// - ⏳ 需要适配TopContractRepository
///
/// # Responsibilities
/// 1. 获取指定类型的所有Ticker
/// 2. 按交易量排序
/// 3. 筛选头部合约（交易量最大的N个）
/// 4. 保存到数据库
pub async fn sync_top_contracts(inst_type: &str, top_n: usize) -> Result<()> {
    info!(
        "🏆 开始同步头部合约: inst_type={}, top_n={}",
        inst_type, top_n
    );

    // 1. 通过service层获取头部合约（已按交易量排序）
    let service = TickerService::new();
    let tickers = service
        .fetch_top_contracts_by_volume(inst_type, top_n)
        .await?;

    if tickers.is_empty() {
        debug!("无Ticker数据: {}", inst_type);
        return Ok(());
    }

    info!("📊 获取到 {} 个头部合约", tickers.len());

    // 2. 按交易量排序（需要解析vol字段）
    // ⏳ P1: 实现排序逻辑
    // let mut sorted_tickers = tickers;
    // sorted_tickers.sort_by(|a, b| {
    //     let vol_a: f64 = a.vol24h.parse().unwrap_or(0.0);
    //     let vol_b: f64 = b.vol24h.parse().unwrap_or(0.0);
    //     vol_b.partial_cmp(&vol_a).unwrap_or(std::cmp::Ordering::Equal)
    // });

    // 3. 取前top_n个
    // let top_contracts = &sorted_tickers[..top_n.min(sorted_tickers.len())];

    // 4. 保存到数据库
    // ⏳ P1: 集成TopContractRepository
    // use rust_quant_infrastructure::repositories::TopContractRepository;
    // let repo = TopContractRepository::new(db_pool);
    // repo.update_top_contracts(inst_type, top_contracts).await?;

    info!("✅ 头部合约数据同步完成（框架实现）");
    Ok(())
}

/// 同步SWAP类型的头部合约
pub async fn sync_top_swap_contracts(top_n: usize) -> Result<()> {
    sync_top_contracts("SWAP", top_n).await
}

/// 同步SPOT类型的头部合约
pub async fn sync_top_spot_contracts(top_n: usize) -> Result<()> {
    sync_top_contracts("SPOT", top_n).await
}

/// 同步所有类型的头部合约
///
/// # Arguments
/// * `swap_top_n` - SWAP合约数量
/// * `spot_top_n` - SPOT合约数量
pub async fn sync_all_top_contracts(swap_top_n: usize, spot_top_n: usize) -> Result<()> {
    info!("🏆 同步所有头部合约...");

    // 同步SWAP
    if let Err(e) = sync_top_swap_contracts(swap_top_n).await {
        error!("❌ SWAP头部合约同步失败: {}", e);
    }

    // 避免API限流
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 同步SPOT
    if let Err(e) = sync_top_spot_contracts(spot_top_n).await {
        error!("❌ SPOT头部合约同步失败: {}", e);
    }

    info!("✅ 所有头部合约同步完成");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_sync_top_contracts() {
        dotenv::dotenv().ok();
        let result = sync_top_swap_contracts(10).await;
        assert!(result.is_ok());
    }
}
