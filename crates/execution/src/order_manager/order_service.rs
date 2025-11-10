use okx::api::api_trait::OkxApiTrait;
use okx::api::trade::OkxTrade;
use okx::dto::trade::trade_dto::OrderPendingRespDto;
use okx::dto::trade_dto::{OrdListReqDto, OrderDetailRespDto};
use okx::error::Error;
use rust_quant_common::AppError;
use rust_quant_risk::order::SwapOrdersDetailEntity;
use serde_json::json;
use tracing::{info, warn};

pub struct OrderService {}

impl OrderService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_pending_orders(
        &self,
        inst_id: Option<&str>,
    ) -> Result<Vec<OrderPendingRespDto>, AppError> {
        let trade_client = OkxTrade::from_env()
            .map_err(|e| AppError::OkxApiError(format!("OKX初始化失败: {:?}", e)))?;
        let position_list = trade_client
            .get_pending_orders(Some("SWAP"), None, None, None, None, None, None)
            .await
            .map_err(|e| AppError::OkxApiError(e.to_string()))?;
        info!("get pending orders: {:?}", json!(position_list).to_string());
        Ok(position_list)
    }

    pub async fn get_order_detail(
        &self,
        inst_id: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> Result<Vec<OrderDetailRespDto>, AppError> {
        let trade_client = OkxTrade::from_env()
            .map_err(|e| AppError::OkxApiError(format!("OKX初始化失败: {:?}", e)))?;
        let order_list = trade_client
            .get_order_details(inst_id, order_id, client_order_id)
            .await
            .map_err(|e| AppError::OkxApiError(e.to_string()))?;
        info!("get order detail: {:?}", json!(order_list).to_string());
        Ok(order_list)
    }
    ///
    pub async fn sync_order_detail(
        &self,
        inst_id: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> Result<(), AppError> {
        let detail = self
            .get_order_detail(inst_id, order_id, client_order_id)
            .await?;
        if detail.len() == 0 {
            warn!("get order detail is empty");
            return Ok(());
        }
        self.update_order_detail(detail[0].to_owned()).await?;
        Ok(())
    }

    pub async fn update_order_detail(
        &self,
        order_detail: OrderDetailRespDto,
    ) -> Result<(), AppError> {
        // TODO: 实现 OrderDetailRespDto 到 SwapOrdersDetailEntity 的转换
        // let entity = SwapOrdersDetailEntity::from(order_detail);
        // entity.insert().await?;
        warn!("update_order_detail 暂未实现");
        Ok(())
    }

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
                limit: limit,
                after: after.map(|s| s.to_string()),
                before: before.map(|s| s.to_string()),
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
        let model = OkxTrade::from_env().map_err(|e| AppError::OkxApiError(e.to_string()))?;
        let order_list = model
            .get_order_history_archive(OrdListReqDto {
                inst_type: inst_type.to_string(),
                inst_id: inst_id.map(|s| s.to_string()),
                ord_type: order_type.map(|s| s.to_string()),
                state: state.map(|s| s.to_string()),
                after: after.map(|s| s.to_string()),
                before: before.map(|s| s.to_string()),
                limit: limit,
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
