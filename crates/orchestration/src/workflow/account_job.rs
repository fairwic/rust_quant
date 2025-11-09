//! 账户数据同步任务
//! 
//! 从 src/trading/task/account_job.rs 迁移
//! 适配新的DDD架构

use anyhow::Result;
use tracing::info;
use okx::api::account::OkxAccount;
use okx::api::api_trait::OkxApiTrait;

/// 获取账户余额
/// 
/// # Architecture
/// orchestration层的任务调度功能
/// 
/// # Migration Notes
/// - ✅ 从 src/trading/task/account_job.rs 迁移
/// - ✅ 保持原有功能
/// - ⏳ 后续可集成AccountRepository持久化
/// 
/// # Example
/// ```rust,ignore
/// use rust_quant_orchestration::workflow::get_account_balance;
/// 
/// get_account_balance().await?;
/// ```
pub async fn get_account_balance() -> Result<()> {
    info!("🏦 开始获取账户余额...");
    
    // 使用OKX API获取余额
    let balances = OkxAccount::from_env()?
        .get_balance(None)
        .await?;
    
    info!("✅ 账户余额: {:#?}", balances);
    
    // ⏳ P1: 集成AccountRepository持久化
    // 集成示例：
    // use rust_quant_services::trading::AccountService;
    // let account_service = AccountService::new();
    // account_service.update_balance(&balances).await?;
    
    Ok(())
}

/// 获取指定币种的账户余额
/// 
/// # Arguments
/// * `currency` - 币种（如 "BTC", "USDT"）
pub async fn get_account_balance_by_currency(currency: Option<&str>) -> Result<()> {
    info!("🏦 获取指定币种余额: {:?}", currency);
    
    let balances = OkxAccount::from_env()?
        .get_balance(currency)
        .await?;
    
    info!("✅ 余额查询完成: {:#?}", balances);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_get_account_balance() {
        dotenv::dotenv().ok();
        let result = get_account_balance().await;
        assert!(result.is_ok());
    }
}
