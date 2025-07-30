use okx::api::api_trait::OkxApiTrait;
use okx::api::trade::OkxTrade;
use okx::dto::trade::trade_dto::Order;
use okx::error::Error;
use serde_json::json;
use tracing::info;

pub struct OrderService {}

impl OrderService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_pending_orders(&self, inst_id: Option<&str>) -> Result<Vec<Order>, Error> {
        let account = OkxTrade::from_env()?; //获取合约持仓信息
        let position_list = account
            .get_pending_orders(Some("SWAP"), None, None, None, None, None, None)
            .await?;
        info!("get pending orders: {:?}", json!(position_list).to_string());
        Ok(position_list)
    }
}
