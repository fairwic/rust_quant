// 风险监控任务

use crate::order_manager::order_service::OrderService;
use anyhow::anyhow;
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::SetLeverageRequest;
use okx::dto::asset_dto::{AssetBalance, TransferOkxReqDto};
use okx::dto::trade_dto::{OrderDetailRespDto, TdModeEnum};
use okx::dto::PositionSide;
use okx::enums::account_enums::AccountType;
use okx::{OkxAccount, OkxAsset};
use rust_quant_common::AppError;
use rust_quant_risk::position::position_service::PositionService;
use std::str::FromStr;
use tracing::{debug, error, info};
use tracing::{span, Level};

// 常量定义

/// 风险管理任务，负责仓位风险的检查
pub struct RiskOrderJob {}

impl RiskOrderJob {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(
        &self,
        inst_id: Option<&str>,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> Result<(), AppError> {
        //1. 获取未成交的订单
        let pending_orders = OrderService::new().get_pending_orders(inst_id).await?;
        if pending_orders.len() == 0 {
            info!("获取未成交订单为空");
            return Ok(());
        }
        for order in pending_orders {
            //获取订单详情
            OrderService::new()
                .sync_order_detail(order.inst_id.as_str(), order_id, client_order_id)
                .await?;
        }
        //更新订单详情到数据库中去
        Ok(())
    }

    ///同步订单列表
    pub async fn sync_order_list(
        &self,
        inst_type: &str,
        inst_id: Option<&str>,
        order_type: Option<&str>,
        state: Option<&str>,
        after: Option<&str>,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<OrderDetailRespDto>, AppError> {
        let order_list = OrderService::new()
            .sync_order_history(inst_type, inst_id, order_type, state, after, before, limit)
            .await?;
        if order_list.len() == 0 {
            info!("获取历史已完成的订单列表为空");
            return Ok(vec![]);
        }

        Ok(order_list)
    }
}

/// 测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_init;
    use serde_json::json;

    #[tokio::test]
    async fn test_risk_job() {
        // 设置日志
        env_logger::init();
        app_init().await;
        let inst_id = Some("BTC-USDT-SWAP");
        let order_id = Some("2752618588464259072");
        let client_order_id = Some("btc1Hbs20250807110000");
        // let risk_job = RiskOrderJob::new()
        //     .sync_order_list("SWAP", inst_id, None, None, None, None, None, Some(10))
        //     .await;
        // println!("risk_job: {:?}", risk_job);
    }

    #[tokio::test]
    async fn test_sync_order_list() -> Result<(), AppError> {
        // 设置日志
        env_logger::init();
        app_init().await;
        let inst_id = Some("BTC-USDT-SWAP");
        let state = None;
        let after: Option<&str> = None;
        let before = None;
        let limit = None;
        let order_type = None;
        let risk_job = RiskOrderJob::new()
            .sync_order_list("SWAP", inst_id, order_type, state, after, before, limit)
            .await?;
        println!("risk_job: {:?}", risk_job);
        println!("risk_job_json: {:?}", json!(risk_job).to_string());
        Ok(())
    }
}
