// 风险监控任务

use crate::order_manager::order_service::OrderService;
use okx::api::api_trait::OkxApiTrait;
use okx::dto::trade_dto::OrderDetailRespDto;
use rust_quant_common::AppError;
use tracing::info;

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

    #[tokio::test]
    async fn test_risk_job() {
        // 注意：此测试需要完整的应用环境初始化
        // 在实际测试中需要先初始化数据库连接等
        let inst_id = Some("BTC-USDT-SWAP");
        let _order_id = Some("2752618588464259072");
        let _client_order_id = Some("btc1Hbs20250807110000");

        // 测试代码已注释，需要完整环境才能运行
        // let risk_job = RiskOrderJob::new()
        //     .run(inst_id, order_id, client_order_id)
        //     .await;
    }

    #[tokio::test]
    async fn test_sync_order_list() -> Result<(), AppError> {
        // 注意：此测试需要完整的应用环境初始化
        // 在实际测试中需要先初始化数据库连接等
        let _inst_id = Some("BTC-USDT-SWAP");
        let _state: Option<&str> = None;
        let _after: Option<&'static str> = None;
        let _before: Option<&str> = None;
        let _limit: Option<u32> = None;
        let _order_type: Option<&str> = None;

        // 测试代码已注释，需要完整环境才能运行
        // let risk_job = RiskOrderJob::new()
        //     .sync_order_list("SWAP", inst_id, order_type, state, after, before, limit)
        //     .await?;
        Ok(())
    }
}
