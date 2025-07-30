use dotenv::dotenv;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading;
use rust_quant::trading::indicator::predicting::predicting_multi_ema_indicator::PredictingMultiEmaIndicator;
use rust_quant::trading::model::entity::candles::enums::{SelectTime, TimeDirect};

#[tokio::test]
async fn test_predicting_ema() -> anyhow::Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let select_time = SelectTime {
        start_time: 1743508800000,
        end_time: None,
        direct: TimeDirect::BEFORE,
    };

    let mut predicting_ema = PredictingMultiEmaIndicator::new(&[12, 144, 169]).unwrap();
    let candles = trading::task::basic::get_candle_data_confirm(
        "BTC-USDT-SWAP",
        "1H",
        600,
        Some(select_time),
    )
    .await?;

    for candle in candles {
        predicting_ema.next(candle.c.parse::<f64>().unwrap());
    }
    let predictions = predicting_ema.get_predictions();
    println!("predictions0: {:?}", predictions);

    Ok(())
}
