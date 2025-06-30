use anyhow::Result;
use dotenv::dotenv;
use rust_quant::trading::indicator::is_big_kline::IsBigKLineIndicator;
use rust_quant::trading::indicator::vegas_indicator::VegasStrategy;
use rust_quant::trading::strategy::strategy_common;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};

  // 原有的异步测试，用于测试实时数据
#[tokio::test]
async fn test_big_k_line_real_data() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;


    let select_time: SelectTime = SelectTime {
        // point_time: 1741514400000,
        // point_time: 1740931200000,
        // point_time: 1740956400000,
        start_time: 1707476400000,

        direct: TimeDirect::BEFORE,
    };

    println!("\n===== RSI Real Data Test =====");
    
    let candles = trading::task::basic::get_candle_data("BTC-USDT-SWAP", "1H", 100, Some(select_time)).await?;

    let mut vega_indicator = VegasStrategy::default();
    let data_items = strategy_common::parse_candle_to_data_item(&candles.last().unwrap());
    let is_big_k_line = IsBigKLineIndicator::new(70.0).is_big_k_line(&data_items);

    println!("is_big_k_line: {:?}", is_big_k_line);
    Ok(())
}
