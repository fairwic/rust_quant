use anyhow::Result;
use dotenv::dotenv;
use ta::Next;

use rust_quant::{
    app_config::db::init_db, time_util, trading,
    trading::model::market::candles::CandlesEntity,
};
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::bar::Bar;
use rust_quant::trading::strategy::ut_boot_strategy::UtBootStrategy;

#[tokio::test]
async fn test_atr() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";
    let period = 2;

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> = trading::task::basic::get_candle_data_confirm(inst_id, time, 100, None).await?;
    println!("{:#?}", mysql_candles);

    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }

    let mut stategy = UtBootStrategy::new(2.0, 1, 10, false);
    let result = stategy.get_trade_signal(&mysql_candles);

    println!("result:{:?}",result);


    Ok(())
}
