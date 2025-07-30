use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};
use ta::indicators::BollingerBands;
use ta::Next;

// tests/squeeze_test.rs
#[tokio::test]
async fn test_bolling_bands() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let select_time = SelectTime {
        start_time: 1742274000000,
        direct: TimeDirect::BEFORE,
    };

    let candles =  trading::task::basic::get_candle_data_confirm("BTC-USDT-SWAP", "1H", 1200, Some(select_time)).await?;

    let mut boll = BollingerBands::new(9, 3.6).unwrap();

    for candle in candles {
        let boll_value = boll.next(candle.c.parse::<f64>().unwrap());
        println!("boll_value:{:?}", boll_value);
    }
    Ok(())
}
