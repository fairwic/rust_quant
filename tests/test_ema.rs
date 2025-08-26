use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};
use ta::indicators::ExponentialMovingAverage;
use ta::Next;

#[tokio::test]
async fn test_ema() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let select_time = SelectTime {
        start_time: 1742274000000,
        direct: TimeDirect::BEFORE,
    };

    let mut ema1 = ExponentialMovingAverage::new(12).unwrap();
    let mut ema2 = ExponentialMovingAverage::new(676).unwrap();
    let candles = trading::task::basic::get_candle_data_confirm(
        "BTC-USDT-SWAP",
        "1H",
        3200,
        Some(select_time),
    )
    .await?;

    let mut ema1_value = 0.00;
    let mut ema2_value = 0.00;
    for candle in candles {
        ema1_value = ema1.next(candle.c.parse::<f64>().unwrap());
        ema2_value = ema2.next(candle.c.parse::<f64>().unwrap());
        println!("ema1:{:?}", ema1_value);
        println!("ema2:{:?}", ema2_value);
    }
    assert_eq!(format!("{:.1}", ema1_value), "83444.4");
    assert_eq!(format!("{:.1}", ema2_value), "87491.3");
    println!("测试ema通过-------");

    Ok(())
}
