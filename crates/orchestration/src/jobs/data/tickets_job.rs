//! Ticker数据同步任务
//!
//! 从 src/trading/task/tickets_job.rs 迁移
//! 适配新的DDD架构：orchestration层只负责任务编排，业务逻辑在services层
//!
//! # 架构原则
//! - orchestration层：只做编排，不直接调用外部API
//! - services层：封装业务逻辑和外部API调用
//! - 依赖方向：orchestration → services → market/infrastructure

use anyhow::Result;
use rust_quant_services::market::TickerService;
use tracing::{error, info};

/// 同步Ticker数据
///
/// # Architecture
/// orchestration层的数据同步任务，只负责任务编排，业务逻辑在services层
///
/// # Migration Notes
/// - ✅ 从 src/trading/task/tickets_job.rs 迁移
/// - ✅ 业务逻辑已迁移到services层
/// - ✅ orchestration层只负责调用services层
///
/// # Arguments
/// * `inst_ids` - 交易对列表
///
/// # Example
/// ```rust,ignore
/// use rust_quant_orchestration::workflow::sync_tickers;
///
/// let inst_ids = vec!["BTC-USDT".to_string(), "ETH-USDT".to_string()];
/// sync_tickers(&inst_ids).await?;
/// ```
pub async fn sync_tickers(inst_ids: &[String]) -> Result<()> {
    info!("🎫 开始同步Ticker数据: {} 个交易对", inst_ids.len());

    let ticker_service = TickerService::new();

    for inst_id in inst_ids {
        match sync_single_ticker(inst_id, &ticker_service).await {
            Ok(_) => info!("✅ Ticker同步成功: {}", inst_id),
            Err(e) => error!("❌ Ticker同步失败: {} - {}", inst_id, e),
        }
    }

    info!("✅ 所有Ticker数据同步完成");
    Ok(())
}

/// 同步单个交易对的Ticker数据
///
/// 对应原始代码的 `get_ticket` 函数
/// orchestration层：只负责调用service层，不做业务逻辑判断，不直接调用外部API
async fn sync_single_ticker(inst_id: &str, ticker_service: &TickerService) -> Result<()> {
    // 调用service层完成完整的业务流程（从交易所获取 → 保存到数据库）
    match ticker_service.sync_ticker_from_exchange(inst_id).await? {
        Some(true) => info!("✅ Ticker同步成功（新插入）: {}", inst_id),
        Some(false) => info!("✅ Ticker同步成功（已更新）: {}", inst_id),
        None => {
            info!("⚠️  Ticker数据为空: {}", inst_id);
            return Ok(());
        }
    }

    Ok(())
}

/// 批量同步Ticker数据（并发）
///
/// # Arguments
/// * `inst_ids` - 交易对列表
/// * `concurrency` - 并发数量
pub async fn sync_tickers_concurrent(inst_ids: &[String], concurrency: usize) -> Result<()> {
    info!(
        "🎫 开始并发同步Ticker数据: {} 个交易对, 并发数: {}",
        inst_ids.len(),
        concurrency
    );

    let ticker_service = TickerService::new();
    use futures::stream::{self, StreamExt};

    let results: Vec<_> = stream::iter(inst_ids)
        .map(|inst_id| {
            let service = &ticker_service;
            async move { sync_single_ticker(inst_id, service).await }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let success_count = results.iter().filter(|r| r.is_ok()).count();
    let fail_count = results.len() - success_count;

    info!(
        "✅ Ticker同步完成: 成功 {}, 失败 {}",
        success_count, fail_count
    );

    Ok(())
}

/// 初始化所有Ticker数据
///
/// 对应原始代码的 `init_all_ticker` 函数
/// 批量获取SWAP类型的tickers并更新
///
/// # Architecture
/// orchestration层：只负责调用service层，不直接调用外部API
/// 完整的业务流程（从交易所获取 → 保存到数据库）在service层完成
///
/// # Arguments
/// * `inst_ids` - 需要同步的交易对列表
pub async fn init_all_ticker(inst_ids: &[String]) -> Result<()> {
    info!("开始同步ticker...");

    // 调用service层完成完整的业务流程（从交易所获取 → 批量保存到数据库）
    let ticker_service = TickerService::new();
    let ins_type = "SWAP";
    let count = ticker_service
        .sync_tickers_from_exchange(ins_type, inst_ids)
        .await?;

    info!("✅ 批量同步完成，处理了 {} 个ticker", count);
    Ok(())
}

/// 同步单个Ticker（兼容原始接口）
///
/// 对应原始代码的 `sync_ticker` 函数
pub async fn sync_ticker() -> Result<()> {
    let ticker_service = TickerService::new();
    sync_single_ticker("BTC-USDT-SWAP", &ticker_service).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_sync_single_ticker() {
        // 注意：需要在测试环境中配置OKX API密钥
        let ticker_service = TickerService::new();
        let result = sync_single_ticker("BTC-USDT", &ticker_service).await;
        assert!(result.is_ok());
    }
}
