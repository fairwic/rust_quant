use anyhow::Result;
use dotenv::dotenv;
use log::error;
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
    let inst_id = Some(vec!["BTC-USDT-SWAP","ETH-USDT-SWAP","OM-USDT-SWAP"]);
    let period = Some(vec!["4H", "1H", "5m",  "1D"]);

    // let inst_id = Some(vec!["OM-USDT-SWAP"]);
    // let period = Some(vec![ "1m"]);

    // let inst_id = Some(vec!["BTC-USDT-SWAP", ]);
    // let period = Some(vec!["5m"]);

    let res = big_data_job::init_top_contract(inst_id.clone(), period.clone()).await?;

    //延迟100ms
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    // let period = Some(vec!["1D"]);

    let res = big_data_job::sync_top_contract(inst_id, period).await;
    if res.is_err() {
        error!("异常")
    }

    println!("take volume {:?}", res);
    Ok(())
}
