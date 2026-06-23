use okx::api::api_trait::OkxApiTrait;
use okx::api::trade::OkxTrade;
use okx::dto::trade::trade_dto::OrderPendingRespDto;
use okx::dto::trade_dto::{OrdListReqDto, OrderDetailRespDto};
use rust_quant_common::AppError;
use serde_json::json;
use tracing::{info, warn};
pub struct OrderService {}
const LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV: &str = "LEGACY_SIGNED_READ_ONLY_CONFIRM";
const LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN: &str =
    "I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS";
impl OrderService {
    pub fn new() -> Self {
        Self {}
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
    async fn legacy_order_read_requires_signed_read_only_confirmation_before_okx_client() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _snapshot = EnvSnapshot::capture();
        std::env::remove_var(TEST_CONFIRM_ENV);
        let error = OrderService::new()
            .get_pending_orders(Some("ETH-USDT-SWAP"))
            .await
            .expect_err("legacy order read must require explicit signed read-only confirmation");
        let message = error.to_string();
        assert!(
            message.contains(TEST_CONFIRM_ENV),
            "unexpected error: {message}"
        );
    }
}
impl Default for OrderService {
    fn default() -> Self {
        Self::new()
    }
}
impl OrderService {
    /// 封装当前函数，减少交易执行调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    /// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
    /// 返回 Result 以便错误透明上抛，统一上层降级与重试策略。
    fn ensure_legacy_signed_read_only_allowed() -> Result<(), AppError> {
        let confirmation = std::env::var(LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV).ok();
        if confirmation.as_deref().map(str::trim) == Some(LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN) {
            return Ok(());
        }
        Err(AppError::Config(format!(
            "{}={} is required before using legacy rust_quant_execution signed read-only order queries; prefer the quant_web execution reconciliation path with exact credential_id and target task scope",
            LEGACY_SIGNED_READ_ONLY_CONFIRM_ENV,
            LEGACY_SIGNED_READ_ONLY_CONFIRM_TOKEN
        )))
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    pub async fn get_pending_orders(
        &self,
        _inst_id: Option<&str>,
    ) -> Result<Vec<OrderPendingRespDto>, AppError> {
        Self::ensure_legacy_signed_read_only_allowed()?;
        let trade_client = OkxTrade::from_env()
            .map_err(|e| AppError::OkxApiError(format!("OKX初始化失败: {:?}", e)))?;
        let position_list = trade_client
            .get_pending_orders(Some("SWAP"), None, None, None, None, None, None)
            .await
            .map_err(|e| AppError::OkxApiError(e.to_string()))?;
        info!("get pending orders: {:?}", json!(position_list).to_string());
        Ok(position_list)
    }
    /// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
    pub async fn get_order_detail(
        &self,
        inst_id: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> Result<Vec<OrderDetailRespDto>, AppError> {
        Self::ensure_legacy_signed_read_only_allowed()?;
        let trade_client = OkxTrade::from_env()
            .map_err(|e| AppError::OkxApiError(format!("OKX初始化失败: {:?}", e)))?;
        let order_list = trade_client
            .get_order_details(inst_id, order_id, client_order_id)
            .await
            .map_err(|e| AppError::OkxApiError(e.to_string()))?;
        info!("get order detail: {:?}", json!(order_list).to_string());
        Ok(order_list)
    }
    /// 同步订单详情
    pub async fn sync_order_detail(
        &self,
        inst_id: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> Result<(), AppError> {
        let detail = self
            .get_order_detail(inst_id, order_id, client_order_id)
            .await?;
        if detail.is_empty() {
            warn!("get order detail is empty");
            return Ok(());
        }
        self.update_order_detail(detail[0].to_owned()).await?;
        Ok(())
    }
    /// 更新 交易执行与风控 状态，并保留调用方需要的结果或错误信息。
    pub async fn update_order_detail(
        &self,
        _order_detail: OrderDetailRespDto,
    ) -> Result<(), AppError> {
        // TODO: 实现 OrderDetailRespDto 到 SwapOrdersDetailEntity 的转换
        // let entity = SwapOrdersDetailEntity::from(order_detail);
        // entity.insert().await?;
        warn!("update_order_detail 暂未实现");
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    /// 同步 交易执行与风控 数据，保证本地状态与外部事实源保持一致。
    pub async fn sync_order_history(
        &self,
        inst_type: &str,
        inst_id: Option<&str>,
        order_type: Option<&str>,
        state: Option<&str>,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<OrderDetailRespDto>, AppError> {
        Self::ensure_legacy_signed_read_only_allowed()?;
        //获取数据库中最新的更新的订单id
        // let last_update_info = SwapOrderDetailEntityModel::new()
        //     .await
        //     .get_new_update_order_id()
        //     .await?;
        // let mut before = before;
        // if last_update_info.is_some() && before.is_none() {
        //     before = Some(last_update_info.unwrap().update_at.unwrap().as_str());
        // }
        let model = OkxTrade::from_env().map_err(|e| AppError::OkxApiError(e.to_string()))?;
        let order_list = model
            .get_order_history(OrdListReqDto {
                inst_type: inst_type.to_string(),
                inst_id: inst_id.map(|s| s.to_string()),
                ord_type: order_type.map(|s| s.to_string()),
                state: state.map(|s| s.to_string()),
                // after: after.map(|s| s.to_string()),
                // before: before.map(|s| s.to_string()),
                limit,
                after: after.map(|s| s.to_string()),
                before: before.map(|s| s.to_string()),
                begin: None,
                end: None,
            })
            .await
            .map_err(|e| AppError::OkxApiError(e.to_string()))?;
        // for order in order_list {
        //     let order_detail = SwapOrderDetailEntity::from(order);
        //     let model = SwapOrderDetailEntityModel::new().await;
        //     model.update(&order_detail).await;
        // }
        Ok(order_list)
    }
    #[allow(clippy::too_many_arguments)]
    /// 同步 交易执行与风控 数据，保证本地状态与外部事实源保持一致。
    pub async fn sync_order_history_archive(
        &self,
        inst_type: &str,
        inst_id: Option<&str>,
        order_type: Option<&str>,
        state: Option<&str>,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<OrderDetailRespDto>, AppError> {
        Self::ensure_legacy_signed_read_only_allowed()?;
        let model = OkxTrade::from_env().map_err(|e| AppError::OkxApiError(e.to_string()))?;
        let order_list = model
            .get_order_history_archive(OrdListReqDto {
                inst_type: inst_type.to_string(),
                inst_id: inst_id.map(|s| s.to_string()),
                ord_type: order_type.map(|s| s.to_string()),
                state: state.map(|s| s.to_string()),
                after: after.map(|s| s.to_string()),
                before: before.map(|s| s.to_string()),
                begin: None,
                end: None,
                limit,
            })
            .await
            .map_err(|e| AppError::OkxApiError(e.to_string()))?;
        // for order in order_list {
        //     let order_detail = SwapOrderDetailEntity::from(order);
        //     let model = SwapOrderDetailEntityModel::new().await;
        //     model.update(&order_detail).await;
        // }
        Ok(order_list)
    }
}
