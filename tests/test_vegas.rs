use anyhow::Result;
use dotenv::dotenv;

use rust_quant::{
    app_config::db::init_db, trading,
    trading::model::market::candles::CandlesEntity,
};
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::vegas_indicator::VegasIndicator;

#[tokio::test]
async fn test_vegas() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> = trading::task::basic::get_candle_data(inst_id, time, 200, None).await?;
    println!("{:#?}", mysql_candles);
    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }
    let mut stategy = VegasIndicator::new(12, 144, 169);
    let result = stategy.get_trade_signal(&mysql_candles);

    println!("result:{:?}", result);


    Ok(())
}
