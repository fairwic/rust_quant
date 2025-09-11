use env_logger::Builder;
use okx::api::api_trait::OkxApiTrait;
use okx::OkxAccount;
use rust_quant::app_init;
use rust_quant::error::app_error::AppError;
use rust_quant::trading::order::swap_order_service::SwapOrderService;
use rust_quant::trading::services::order_service::order_service::OrderService;
use rust_quant::trading::strategy::strategy_common::{BasicRiskStrategyConfig, SignalResult};
use rust_quant::trading::strategy::StrategyType;
use serde_json::json;
use tracing::error;

#[tokio::test]
async fn test_okx_order_detail() {
    // 启用详细日志
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    // let mut builder = Builder::from_default_env();
    // builder.target(Target::Stdout);
    // builder.filter_level(Level::Debug);
    // builder.filter_module("rust_quant", Level::Debug);
    // builder.default_format();
    // builder.init();

    app_init().await;
    let inst_id = "BTC-USDT-SWAP";
    let order_id = Some("2752618588464259072");
    let client_order_id = None;
    // let client_order_id = "btc1Hbs20250807110000";
    //1. 获取现有的仓位，判断是否有止损价格，没有需要告警，并自动设置最大止损价格
    let order_list = OrderService::new()
        .sync_order_detail(inst_id, order_id, client_order_id)
        .await;
    println!("order_list: {:?}", order_list);
}

// #[tokio::test]
// async fn test_get_position() -> Result<(), AppError> {
//     // 启用详细日志
//     std::env::set_var("RUST_LOG", "debug");
//     env_logger::init();
//     // let mut builder = Builder::from_default_env();
//     // builder.target(Target::Stdout);
//     // builder.filter_level(Level::Debug);
//     // builder.filter_module("rust_quant", Level::Debug);
//     // builder.default_format();
//     // builder.init();
//     app_init().await;
//     let inst_id = "BTC-USDT-SWAP";
//     let period = "1H";

//     // 获取当前仓位状态
//     let account = OkxAccount::from_env()?;
//     //todo 如有反向的仓位，应该开启异步去立即关闭
//     let position_list = account
//         .get_account_positions(Some("SWAP"), Some(inst_id), None)
//         .await?;
//     println!("position_list: {:?}", json!(position_list).to_string());
//     Ok(())
// }
