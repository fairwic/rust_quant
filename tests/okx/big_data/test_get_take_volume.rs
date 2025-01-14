use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::okx::big_data::BigDataOkxApi;
use rust_quant::trading::task::big_data_job;

#[tokio::test]
async fn test_get_take_volume() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    // 设置日志
    println!("init log config");
    setup_logging().await?;
    init_db().await;
    // 设置参数
    let period = 10;
    // let res = BigData::get_taker_volume(inst_id, "SPOT", None, None, None).await?;
    // println!("volume {:?}", res);
    let inst_id = Some(vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP"]);
    let period = Some(vec!["4H"]);
    //
    // let res = BigData::get_taker_volume_contract(inst_id, Some(time), Some("2"), None, None, None)
    //     .await?;
    // let contract = res;
    // println!("contract volume {:?}", contract);

    let res = big_data_job::run_take_volume_job(inst_id, period).await?;
    println!("take volume {:?}", res);
    Ok(())
}
