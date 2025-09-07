// src/indicators/squeeze/calculator.rs
use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::squeeze_momentum::service::get_last_squeeze_single;
use rust_quant::trading::indicator::squeeze_momentum::squeeze_config::SqueezeConfig;
use rust_quant::trading::model::entity::candles::enums::{SelectTime, TimeDirect};

// tests/squeeze_test.rs
#[tokio::test]
async fn test_squeeze_strategy() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // //测试1
    // let config = SqueezeConfig {
    //     bb_length: 20,
    //     bb_multi: 2.0,
    //     kc_length: 20,
    //     kc_multi: 1.5,
    // };

    // let select_time = SelectTime {start_time:1737057600000,direct:TimeDirect::BEFORE, end_time: todo!() };

    // let result = get_last_squeeze_single(config, "BTC-USDT-SWAP", "4H", Some(select_time)).await?;

    // println!("{:#?}", result);
    // assert_eq!("101068.4", result.single_value.to_string());
    // assert_eq!("101041.66", format!("{:.2}", result.single_value));
    // assert_eq!("95006.07", format!("{:.2}", result.single_value));

    // assert_eq!("100333.00", format!("{:.2}", result.single_value));
    // assert_eq!("95714.73", format!("{:.2}", result.lower_kc));

    // println!("测试通过[1]----------");
    // //测试2
    // let config = SqueezeConfig {
    //     bb_length: 10,
    //     bb_multi: 3.0,
    //     kc_length: 20,
    //     kc_multi: 2.0,
    // };

    // let select_time = SelectTime {start_time:1732392000000,direct:TimeDirect::BEFORE, end_time: todo!() };

    // let result = get_last_squeeze_single(config, "BTC-USDT-SWAP", "4H", None).await?;

    // println!("{:#?}", result);
    // assert_eq!("97692.1", result.single_value.to_string());
    // assert_eq!("99451.04", format!("{:.2}", result.single_value));
    // assert_eq!("97520.82", format!("{:.2}", result.single_value));
    // assert_eq!("100422.91", format!("{:.2}", result.single_value));
    // assert_eq!("95067.94", format!("{:.2}", result.single_value));

    // println!("测试通过[2]----------");

    Ok(())
}
