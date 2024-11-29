// src/indicators/squeeze/calculator.rs
use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::model::market::tickers::TicketsModel;

// tests/squeeze_test.rs
#[tokio::test]
async fn test_classification() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let model = TicketsModel::new().await;

    let mut ints_ids= Vec::new();
    ints_ids.push("BTC-USDT-SWAP");
    ints_ids.push("ETH-USDT-SWAP");
    // 获取所有交易对的每日交易量
    let tickers_data = model.get_daily_volumes(Some(ints_ids)).await?;
    println!("tickers_data{:?}",tickers_data);

    // 计算过去7天的平均交易量
    let avg_volumes = model.calculate_7_day_avg_volume(tickers_data.clone());
    println!("avg_volumes{:?}",avg_volumes);
    // 设置一个阈值，判断交易量超过平均值的板块
    let lifted_assets = model.check_for_possible_lift(tickers_data, avg_volumes, 1.5);
    println!("lifted_assets{:?}",lifted_assets);

    // 输出可能出现拉升的板块
    println!("Possible lifted assets: {:?}", lifted_assets);
    Ok(())
}
