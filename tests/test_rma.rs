use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::indicator::rma::Rma;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};

#[tokio::test]
async fn test_rma() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let mut rma = Rma::new(2);

    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let candles =
        trading::task::basic::get_candle_data("BTC-USDT-SWAP", "4H", 3, Some(select_time)).await?;

    let mut rma_value = 0.00;
    for candle in candles {
        rma_value = rma.next(candle.c.parse::<f64>().unwrap());
        println!("rma:{:?}", rma);
        println!("ram_value:{}", rma_value);
    }
    assert_eq!(format!("{:2}", rma_value), "97692.10");

    println!("测试2");
    let mut rma = Rma::new(10);
    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let candles =
        trading::task::basic::get_candle_data("BTC-USDT-SWAP", "4H", 10, Some(select_time)).await?;

    let mut rma_value = 0.00;
    for candle in candles {
        rma_value = rma.next(candle.c.parse::<f64>().unwrap());
        println!("rma:{:?}", rma)
    }
    assert_eq!(format!("{:2}", rma_value), "98485.93");
    println!("测试rma通过-------");
    Ok(())
}
