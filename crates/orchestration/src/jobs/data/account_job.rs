//! 账户数据同步任务
//!
//! 从 src/trading/task/account_job.rs 迁移
//! 适配新的DDD架构
//!
//! # 架构原则
//! - orchestration层：只做编排，不直接调用外部API
//! - services层：封装业务逻辑和外部API调用

use anyhow::Result;
use rust_quant_services::market::AccountService;
use tracing::info;

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

    let service = AccountService::new();
    let balances = service.fetch_all_balances().await?;

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

    let service = AccountService::new();
    let balances = service.fetch_balance_from_exchange(currency).await?;

    info!("✅ 余额查询完成: {:#?}", balances);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 需要OKX API配置
    async fn test_get_account_balance() {
        // 注意：需要在测试环境中配置OKX API密钥
        let result = get_account_balance().await;
        assert!(result.is_ok());
    }
}
