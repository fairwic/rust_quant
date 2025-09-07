use crate::error::app_error::AppError;
use crate::trading::model::order::swap_orders_detail::{
    SwapOrderDetailEntity, SwapOrderDetailEntityModel,
};
use okx::api::api_trait::OkxApiTrait;
use okx::api::trade::OkxTrade;
use okx::dto::trade::trade_dto::OrderPendingRespDto;
use okx::dto::trade_dto::{OrdListReqDto, OrderDetailRespDto};
use okx::error::Error;
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
    ) -> Result<Vec<OrderPendingRespDto>, Error> {
        let trade_client = OkxTrade::from_env()?;
        let position_list = trade_client
            .get_pending_orders(Some("SWAP"), None, None, None, None, None, None)
            .await?;
        info!("get pending orders: {:?}", json!(position_list).to_string());
        Ok(position_list)
    }

    pub async fn get_order_detail(
        &self,
        inst_id: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> Result<Vec<OrderDetailRespDto>, Error> {
        let trade_client = OkxTrade::from_env()?;
        let order_list = trade_client
            .get_order_details(inst_id, order_id, client_order_id)
            .await?;
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

    pub async fn update_order_detail(&self, order_detail: OrderDetailRespDto) -> Result<(), Error> {
        let order_detail = SwapOrderDetailEntity::from(order_detail);
        let model = SwapOrderDetailEntityModel::new().await;
        model.add(&order_detail).await;
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
        let model = OkxTrade::from_env()?;
        let order_list = model
            .get_order_history(OrdListReqDto {
                inst_type: inst_type.to_string(),
                inst_id: inst_id.map(|s| s.to_string()),
                ord_type: order_type.map(|s| s.to_string()),
                state: state.map(|s| s.to_string()),
                after: after.map(|s| s.to_string()),
                before: before.map(|s| s.to_string()),
                limit: limit,
            })
            .await?;
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
        let model = OkxTrade::from_env()?;
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
            .await?;
        // for order in order_list {
        //     let order_detail = SwapOrderDetailEntity::from(order);
        //     let model = SwapOrderDetailEntityModel::new().await;
        //     model.update(&order_detail).await;
        // }
        Ok(order_list)
    }
}
