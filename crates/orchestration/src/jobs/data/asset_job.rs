//! 资金账户余额同步任务
//!
//! 从 src/trading/task/asset_job.rs 迁移
//!
//! # 架构原则
//! - orchestration层：只做编排，不直接调用外部API
//! - services层：封装业务逻辑和外部API调用
use anyhow::Result;
use rust_quant_services::market::AssetService;
use tracing::info;
/// 获取资金账户余额
/// # Migration Notes
/// - ✅ 从 src/trading/task/asset_job.rs 迁移
/// - ✅ 已重构：通过services层调用
/// - ⏳ 可集成AccountRepository持久化
/// # Architecture
/// orchestration层：只做编排，通过AssetService调用外部API
pub async fn get_asset_balance() -> Result<()> {
    info!("💰 开始获取资金账户余额...");
    let service = AssetService::new();
    let balances = service.fetch_usdt_balance().await?;
    info!("✅ 资金账户余额: {:#?}", balances);
    // ⏳ P1: 保存到数据库
    // use rust_quant_infrastructure::repositories::AssetRepository;
    // let repo = AssetRepository::new(db_pool);
    // repo.save_balances(&balances).await?;
    Ok(())
}
/// 获取所有币种余额
/// # Architecture
/// orchestration层：只做编排，通过AssetService调用外部API
pub async fn get_all_asset_balances() -> Result<()> {
    info!("💰 获取所有资金账户余额...");
    let service = AssetService::new();
    let balances = service.fetch_all_balances().await?;
    info!("✅ 所有余额: {:#?}", balances);
    Ok(())
}
/// 获取指定币种余额
/// # Architecture
/// orchestration层：只做编排，通过AssetService调用外部API
pub async fn get_asset_balance_by_currencies(currencies: Vec<String>) -> Result<()> {
    info!("💰 获取指定币种余额: {:?}", currencies);
    let service = AssetService::new();
    let balances = service.fetch_specific_balances(currencies).await?;
    info!("✅ 余额: {:#?}", balances);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_get_asset_balance() {
        // 注意：需要在测试环境中配置OKX API密钥
        let result = get_asset_balance().await;
        assert!(result.is_ok());
    }
}
