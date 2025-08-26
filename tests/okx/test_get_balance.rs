use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::task::{account_job, asset_job};

#[tokio::test]
async fn test_atr_calculation() -> Result<()> {
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

    asset_job::get_balance().await.expect("TODO: panic message");

    Ok(())
}
