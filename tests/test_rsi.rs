use anyhow::Result;
use dotenv::dotenv;
use ta::Next;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::indicator::rma::Rma;
use rust_quant::trading::indicator::rsi_indicator::RsiIndicator;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};
use rust_quant::trading::indicator::rsi_rma_indicator::RsiIndicator;

  // 原有的异步测试，用于测试实时数据
#[tokio::test]
async fn test_rsi_real_data() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let mut rsi = RsiIndicator::new(12);

    let select_time = SelectTime {
        start_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    println!("\n===== RSI Real Data Test =====");
    
    let candles = trading::task::basic::get_candle_data_confirm("BTC-USDT-SWAP", "1H", 100, None).await?;

    for candle in candles {
        let price = candle.c.parse::<f64>().unwrap();
        let rsi_value = rsi.next(price);
        println!("Time: {}, Price: {:.2}, RSI: {:.2}", 
                 candle.ts, price, rsi_value);
    }

    Ok(())
}
