// 风险监控任务

use crate::trading::services::order_service::order_service::OrderService;
use crate::trading::services::position_service::position_service::PositionService;
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
    ) -> Result<()> {
        //1. 获取现有的仓位，判断是否有止损价格，没有需要告警，并自动设置最大止损价格
        let pending_orders = OrderService::new().get_pending_orders(inst_id).await?;
        for order in pending_orders {
            OrderService::new()
                .sync_order_detail(order.inst_id.as_str(), order_id, client_order_id)
                .await?;
        }
        //更新订单详情到数据库中去
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
        let inst_id = "BTC-USDT-SWAP";
        let order_id = Some("2752618588464259072");
        let client_order_id = Some("btc1Hbs20250807110000");
        let risk_job = RiskOrderJob::new()
            .run(Some(inst_id), order_id, client_order_id)
            .await;
        println!("risk_job: {:?}", risk_job);
    }
}
