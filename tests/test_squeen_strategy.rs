// src/indicators/squeeze/calculator.rs
use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::squeeze_momentum::service::get_last_squeeze_single;
use rust_quant::trading::indicator::squeeze_momentum::types::SqueezeConfig;
use rust_quant::trading::model::market::candles::{SelectTime, TimeDirect};

// tests/squeeze_test.rs
#[tokio::test]
async fn test_squeeze_strategy() -> Result<()> {
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    //测试1
    let config = SqueezeConfig {
        bb_length: 20,
        bb_multi: 2.0,
        kc_length: 3,
        kc_multi: 1.5,
    };

    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let result = get_last_squeeze_single(config, "BTC-USDT-SWAP", "4H", Some(select_time)).await?;

    println!("{:#?}", result);
    assert_eq!("97692.1", result.close.to_string());
    assert_eq!("99945.88", format!("{:.2}", result.upper_bb));
    assert_eq!("95544.97", format!("{:.2}", result.lower_bb));
    assert_eq!("99131.92", format!("{:.2}", result.upper_kc));
    assert_eq!("96619.22", format!("{:.2}", result.lower_kc));
    assert_eq!("-432.94", format!("{:.2}", result.momentum));

    println!("测试通过[1]----------");
    //测试2
    let config = SqueezeConfig {
        bb_length: 10,
        bb_multi: 3.0,
        kc_length: 20,
        kc_multi: 2.0,
    };

    let select_time = SelectTime {
        point_time: 1732392000000,
        direct: TimeDirect::BEFORE,
    };

    let result = get_last_squeeze_single(config, "BTC-USDT-SWAP", "4H", None).await?;

    println!("{:#?}", result);
    assert_eq!("97692.1", result.close.to_string());
    assert_eq!("99451.04", format!("{:.2}", result.upper_bb));
    assert_eq!("97520.82", format!("{:.2}", result.lower_bb));
    assert_eq!("100422.91", format!("{:.2}", result.upper_kc));
    assert_eq!("95067.94", format!("{:.2}", result.lower_kc));
    assert_eq!("2235.00", format!("{:.2}", result.momentum));

    println!("测试通过[2]----------");

    Ok(())
}