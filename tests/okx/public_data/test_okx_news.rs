use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::task::{account_job, asset_job};

#[tokio::test]
async fn test_get_economic_calendar() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;
    // 设置日志
    println!("init log config");
    setup_logging().await?;

    for i in 0..10 {
        let res = rust_quant::trading::okx::public_data::economic_calendar::EconomicCalendar::get_economic_calendar(None, Some("3"), None, None, Some(100)).await.expect("TODO: panic message");
        println!("111111111");
        println!("res: {:?}", serde_json::to_string(&res).unwrap());
    }

    Ok(())
}
