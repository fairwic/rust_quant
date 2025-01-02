use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::okx::big_data::BigDataOkxApi;

#[tokio::test]
async fn test_get_support_coin() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置日志
    println!("init log config");
    setup_logging().await?;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";
    let period = 10;

    let res = BigDataOkxApi::get_support_coin()
        .await
        .expect("TODO: panic message");
    let contract = res.data.contract;

    if contract.contains(&"om".to_string().to_uppercase()) {
        println!("包含")
    } else {
        println!("不包含")
    }

    Ok(())
}
