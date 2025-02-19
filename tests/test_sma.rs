use anyhow::Result;
use dotenv::dotenv;
use rbatis::rbatis_codegen::ops::AsProxy;
use ta::indicators::SimpleMovingAverage;
use ta::Next;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::indicator::bar::Bar;
use rust_quant::trading::indicator::sma::Sma;
use rust_quant::trading::indicator::squeeze_momentum::service::get_last_squeeze_single;
use rust_quant::trading::indicator::squeeze_momentum::squeeze_config::SqueezeConfig;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};

// tests/squeeze_test.rs
#[tokio::test]
async fn test_sma() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let mut sma = Sma::new(2);

    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let candles =
        trading::task::get_candle_data("BTC-USDT-SWAP", "4H", 2, Some(select_time)).await?;

    let mut sma_value = 0.00;
    for candle in candles {
        sma_value = sma.next(candle.c.parse::<f64>().unwrap());
    }
    assert_eq!(format!("{:2}", sma_value), "97611.05");

    //测试2

    let mut sma = Sma::new(10);
    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let candles =
        trading::task::basic::get_candle_data("BTC-USDT-SWAP", "4H", 10, Some(select_time)).await?;

    let mut sma_value = 0.00;
    for candle in candles {
        sma_value = sma.next(candle.c.parse::<f64>().unwrap());
    }
    assert_eq!(format!("{:2}", sma_value), "98485.93");

    //测试3
    println!("测试3");
    let mut sma = SimpleMovingAverage::new(10).unwrap();
    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let candles =
        trading::task::basic::get_candle_data("BTC-USDT-SWAP", "4H", 10, Some(select_time)).await?;

    let mut sma_value = 0.00;
    for candle in candles {
        sma_value = sma.next(&Bar::new().close(candle.c.f64()));
        println!("3 sma_value{}",sma_value)
    }
    assert_eq!(format!("{:2}", sma_value), "98485.93");
    println!("测试通过-------");


    println!("测试通过-------");
    Ok(())
}
