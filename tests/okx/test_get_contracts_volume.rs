use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::okx::public_data::contracts::Contracts;
use rust_quant::trading::task::tickets_volume_job;
use rust_quant::trading::task::{account_job, asset_job};

#[tokio::test]
async fn test_contracts_volume() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置日志
    println!("init log config");
    setup_logging().await?;

    // 设置参数
    let inst_id = "BTC";
    let time = "4H";
    let period = 10;

    let res = Contracts::get_open_interest_volume(Some("BTC"), None, None, Some("1D"))
        .await
        .unwrap();
    println!("res volumes{:?}", res);
    tickets_volume_job::init_all_ticker_volume("BTC", "1D")
        .await
        .unwrap();
    Ok(())
}
