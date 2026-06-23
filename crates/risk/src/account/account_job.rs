use crate::legacy_signed_read_only::ensure_legacy_signed_read_only_allowed;
use okx::api::account::OkxAccount;
use okx::api::api_trait::OkxApiTrait;
/// 封装当前函数，减少风控调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
pub async fn get_account_balance() -> anyhow::Result<()> {
    ensure_legacy_signed_read_only_allowed()?;
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let balances = Account::get_balances(Some(&ccy)).await.map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
    let balances = OkxAccount::from_env()?
        .get_balance(None)
        .await
        .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
    println!("账户余额:{:#?}", balances);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    const TEST_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
    /// 封装环境变量lock，减少风控调用方重复实现相同细节。
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
    struct EnvSnapshot {
        /// 值；为空时表示该条件不启用。
        value: Option<String>,
    }
    impl EnvSnapshot {
        /// 提供capture的集中实现，避免风控调用方重复处理相同细节。
        fn capture() -> Self {
            Self {
                value: std::env::var(TEST_CONFIRM_ENV).ok(),
            }
        }
    }
    impl Drop for EnvSnapshot {
        /// 封装释放，减少风控调用方重复实现相同细节。
        fn drop(&mut self) {
            match &self.value {
                Some(value) => std::env::set_var(TEST_CONFIRM_ENV, value),
                None => std::env::remove_var(TEST_CONFIRM_ENV),
            }
        }
    }
    #[tokio::test]
    async fn legacy_account_balance_read_requires_signed_read_only_confirmation_before_okx_client()
    {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        let error = get_account_balance().await.expect_err(
            "legacy account balance read must require explicit signed read-only confirmation",
        );
        let message = error.to_string();
        assert!(
            message.contains(TEST_CONFIRM_ENV),
            "unexpected error: {message}"
        );
    }
}
