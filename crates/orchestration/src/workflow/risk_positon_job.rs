// 风险监控任务

use rust_quant_execution::order_manager::order_service::OrderService;
use rust_quant_risk::position::position_service::PositionService;
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info};
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::SetLeverageRequest;
use okx::dto::asset_dto::{AssetBalance, TransferOkxReqDto};
use okx::dto::trade_dto::TdModeEnum;
use okx::dto::PositionSide;
use okx::enums::account_enums::AccountType;
use okx::{OkxAccount, OkxAsset};
use std::str::FromStr;
use tracing::{span, Level};
use serde_json::json;

// 常量定义

/// 风险管理任务，负责仓位风险的检查
pub struct RiskPositionJob {}

impl RiskPositionJob {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(&self) -> Result<()> {
        //1. 获取现有的仓位，判断是否有止损价格，没有需要告警，并自动设置最大止损价格
        let position_list = PositionService::new().get_position_list().await?;
        println!("position_list: {:#?}", position_list);
        println!("position_list_json: {:#?}", json!(position_list).to_string());
        // //获取仓位的未成交订单(限价止盈，限价开多，限价开空)
        // for position in position_list {
        //     let inst_id = position.inst_id;
        //     let pending_orders = OrderService::new()
        //         .get_pending_orders(Some(&inst_id))
        //         .await?;
        //     println!("pending_orders: {:?}", pending_orders);
        // }
        Ok(())
    }
}

/// 测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_init;

    #[tokio::test]
    async fn test_risk_job() {
        // 设置日志
        env_logger::init();
        app_init().await;
        let risk_job = RiskPositionJob::new().run().await;
        println!("risk_job: {:?}", risk_job);
    }
}
