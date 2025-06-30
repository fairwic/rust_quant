use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::break_indicator::BreakIndicator;
use rust_quant::trading::indicator::vegas_indicator::VegasStrategy;
use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::trading::model::market::candles::SelectTime;
use rust_quant::trading::model::market::candles::TimeDirect;
use rust_quant::trading::strategy::strategy_common::get_multi_indicator_values;
use rust_quant::trading::strategy::strategy_common::parse_candle_to_data_item;
use rust_quant::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use rust_quant::{app_config::db::init_db, trading};
#[tokio::test]
async fn test_break_indicator() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "1H";
    let select_time: SelectTime = SelectTime {
        start_time: 1749456000000,
        direct: TimeDirect::BEFORE,
    };

    // 获取K线数据
    let candles_list: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data(inst_id, time, 8, Some(select_time)).await?;

    let mut break_indicator = BreakIndicator::new(5, 5, 0.8);
    println!("break_indicator: {:#?}", break_indicator);

    for (i, candle) in candles_list.iter().enumerate() {
        // 获取数据项
        let data_item = parse_candle_to_data_item(candle);
        let result = break_indicator.next(data_item.h(), data_item.l(), data_item.c());
        println!("--------------------------------");
        println!("data_item: {:#?}", data_item);
        println!("break_indicator: {:#?}", break_indicator);
        println!("result: {:?}", result);
    }

    Ok(())
}
