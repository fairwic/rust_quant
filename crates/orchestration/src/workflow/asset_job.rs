//! 资金账户余额同步任务
//! 
//! 从 src/trading/task/asset_job.rs 迁移

use anyhow::Result;
use tracing::info;
use okx::api::api_trait::OkxApiTrait;
use okx::api::asset::OkxAsset;

/// 获取资金账户余额
/// 
/// # Migration Notes
/// - ✅ 从 src/trading/task/asset_job.rs 迁移
/// - ✅ 保持OKX Asset API调用
/// - ⏳ 可集成AccountRepository持久化
pub async fn get_asset_balance() -> Result<()> {
    info!("💰 开始获取资金账户余额...");
    
    // 查询USDT余额
    let ccy = vec!["USDT".to_string()];
    let balances = OkxAsset::from_env()?
        .get_balances(Some(&ccy))
        .await?;
    
    info!("✅ 资金账户余额: {:#?}", balances);
    
    // ⏳ P1: 保存到数据库
    // use rust_quant_infrastructure::repositories::AssetRepository;
    // let repo = AssetRepository::new(db_pool);
    // repo.save_balances(&balances).await?;
    
    Ok(())
}

/// 获取所有币种余额
pub async fn get_all_asset_balances() -> Result<()> {
    info!("💰 获取所有资金账户余额...");
    
    let balances = OkxAsset::from_env()?
        .get_balances(None)
        .await?;
    
    info!("✅ 所有余额: {:#?}", balances);
    Ok(())
}

/// 获取指定币种余额
pub async fn get_asset_balance_by_currencies(currencies: Vec<String>) -> Result<()> {
    info!("💰 获取指定币种余额: {:?}", currencies);
    
    let balances = OkxAsset::from_env()?
        .get_balances(Some(&currencies))
        .await?;
    
    info!("✅ 余额: {:#?}", balances);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_get_asset_balance() {
        dotenv::dotenv().ok();
        let result = get_asset_balance().await;
        assert!(result.is_ok());
    }
}
