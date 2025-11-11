use env_logger::Builder;
use okx::api::api_trait::OkxApiTrait;
use okx::OkxAccount;
use rust_quant::app_init;
use rust_quant::error::app_error::AppError;
use rust_quant::trading::services::order_service::swap_order_service::SwapOrderService;
use rust_quant::trading::strategy::strategy_common::{BasicRiskStrategyConfig, SignalResult};
use rust_quant::trading::strategy::StrategyType;
use serde_json::json;
use tracing::error;

#[tokio::test]
async fn test_okx_order() {
    // 启用详细日志
    std::env::set_var("RUST_LOG", "debug");
    app_init().await;
    // let mut builder = Builder::from_default_env();
    // builder.target(Target::Stdout);
    // builder.filter_level(Level::Debug);
    // builder.filter_module("rust_quant", Level::Debug);
    // builder.default_format();
    // builder.init();
    let inst_id = "ETH-USDT-SWAP";
    let period = "4H";

    println!("🧪 开始测试OKX订单功能");
    println!("📋 测试参数:");
    println!("   - 合约: {}", inst_id);
    println!("   - 周期: {}", period);

    let signal_result = SignalResult {
        should_buy: false,
        should_sell: true,
        open_price: 3581.0,
        signal_kline_stop_loss_price: Some(3700.0),
        best_open_price: None,
        best_take_profit_price: Some(3500.2),
        ts: 0,
        single_value: None,
        single_result: None,
    };

    println!("📊 交易信号: {:#?}", signal_result);

    //执行交易
    let risk_config = BasicRiskStrategyConfig::default();
    println!("⚡ 开始执行订单...");

    let order = SwapOrderService::new()
        .ready_to_order(
            &StrategyType::Vegas,
            inst_id,
            period,
            &signal_result,
            &risk_config,
            5,
        )
        .await;
    if let Err(e) = order {
        error!("order error: {:?}", e);
    } else {
        println!("order success: {:?}", order);
    }
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
