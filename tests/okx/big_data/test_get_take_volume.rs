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
    let inst_id = "BTC";
    let time = "4H";
    let period = 10;

    // let res = BigData::get_taker_volume(inst_id, "SPOT", None, None, None).await?;
    // println!("volume {:?}", res);
    //
    // let inst_id = "BTC-USDT-SWAP";
    //
    // let res = BigData::get_taker_volume_contract(inst_id, Some(time), Some("2"), None, None, None)
    //     .await?;
    // let contract = res;
    // println!("contract volume {:?}", contract);

    let res = big_data_job::run_take_volume_job(None, None).await?;
    println!("take volume {:?}", res);

    let res = BigDataOkxApi::get_taker_volume_contract(inst_id, Some(time), Some("2"), None, None, None)
        .await?;
    println!("take volume contract {:?}", res);
    Ok(())
}
