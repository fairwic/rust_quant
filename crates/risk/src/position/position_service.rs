use crate::legacy_signed_read_only::ensure_legacy_signed_read_only_allowed;
use anyhow::Result;
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::Position;
use okx::OkxAccount;
use serde_json::json;
use tracing::info;
pub struct PositionService {}
impl Default for PositionService {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    const TEST_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
    /// 封装环境变量lock，减少交易执行调用方重复实现相同细节。
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
    struct EnvSnapshot {
        /// 值；为空时表示该条件不启用。
        value: Option<String>,
    }
    impl EnvSnapshot {
        /// 提供capture的集中实现，避免交易执行调用方重复处理相同细节。
        fn capture() -> Self {
            Self {
                value: std::env::var(TEST_CONFIRM_ENV).ok(),
            }
        }
    }
    impl Drop for EnvSnapshot {
        /// 封装释放，减少交易执行调用方重复实现相同细节。
        fn drop(&mut self) {
            match &self.value {
                Some(value) => std::env::set_var(TEST_CONFIRM_ENV, value),
                None => std::env::remove_var(TEST_CONFIRM_ENV),
            }
        }
    }
    #[tokio::test]
    async fn legacy_position_read_requires_signed_read_only_confirmation_before_okx_client() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        let error = PositionService::new()
            .get_position_list()
            .await
            .expect_err("legacy position read must require explicit signed read-only confirmation");
        let message = error.to_string();
        assert!(
            message.contains(TEST_CONFIRM_ENV),
            "unexpected error: {message}"
        );
    }
}
impl PositionService {
    pub fn new() -> Self {
        Self {}
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    pub async fn get_position_list(&self) -> Result<Vec<Position>> {
        ensure_legacy_signed_read_only_allowed()?;
        let account = OkxAccount::from_env()?; //获取合约持仓信息
        let position_list = account
            .get_account_positions(Some("SWAP"), None, None)
            .await
            .map_err(|e| anyhow::anyhow!("OKX错误: {:?}", e))?;
        info!(
            "get current okx position_list: {:?}",
            json!(position_list).to_string()
        );
        Ok(position_list)
    }
}
