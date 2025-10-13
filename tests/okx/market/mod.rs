
use anyhow::Result;
use chrono::Utc;
use dotenv::dotenv;
use okx::api::api_trait::OkxApiTrait;

use okx::OkxMarket;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::time_util;
use rust_quant::trading::task::{account_job, asset_job};

#[tokio::test]
async fn test_get_candle() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置日志
    println!("init log config");
    setup_logging().await?;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";
    let after=time_util::get_period_start_timestamp(time).to_string();


    let res = OkxMarket::from_env().unwrap().get_candles(inst_id, time, Some(&after), None, Some("1")).await.unwrap();
    print!("res:{:?}",res);
    Ok(())
}
