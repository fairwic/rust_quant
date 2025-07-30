use anyhow::Result;
use dotenv::dotenv;

use okx::api::big_data::OkxBigData;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;

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

    // let res = OkxBigData::from_env()
    //     .unwrap()
    //     .get_support_coin()
    //     .await
    //     .expect("TODO: panic message");
    // let contract = res.data.contract;

    // if contract.contains(&"om".to_string().to_uppercase()) {
    //     println!("包含")
    // } else {
    //     println!("不包含")
    // }

    Ok(())
}
