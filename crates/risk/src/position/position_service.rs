use rust_quant_core::error::app_error::AppError;
use anyhow::Result;
use okx::api::api_trait::OkxApiTrait;
use okx::dto::account_dto::Position;
use okx::OkxAccount;
use serde_json::json;
use tracing::{error, info};

pub struct PositionService {}

impl PositionService {
    pub fn new() -> Self {
        Self {}
    }
    pub async fn get_position_list(&self) -> Result<Vec<Position>, AppError> {
        let account = OkxAccount::from_env()?; //获取合约持仓信息
        let position_list = account
            .get_account_positions(Some("SWAP"), None, None)
            .await?;
        info!(
            "get current okx position_list: {:?}",
            json!(position_list).to_string()
        );
        Ok(position_list)
    }
}
