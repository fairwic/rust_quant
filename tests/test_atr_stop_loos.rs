use dotenv::dotenv;
use rust_quant::app_config::db::init_db;
use rust_quant::trading::indicator::atr_stop_loos::ATRStopLoos;
use rust_quant::trading::indicator::bar::Bar;
use rust_quant::trading::model::entity::candles::entity::CandlesEntity;
use rust_quant::trading::model::entity::candles::enums::{SelectTime, TimeDirect};
use rust_quant::{time_util, trading};
use ta::indicators::AverageTrueRange;
use ta::Next;

/// 使用ta库的AverageTrueRange
#[tokio::test]
async fn test_atr_stop_loos() -> anyhow::Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置参数
    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "5m";
    let select_time = Some(SelectTime {
        direct: TimeDirect::BEFORE,
        start_time: 1760738100000,
        end_time: None,
    });

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data_confirm(inst_id, time, 1000, select_time).await?;
    println!("{:#?}", mysql_candles);

    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }

    let period = 14;
    let multi = 1.5;
    let mut atr = ATRStopLoos::new(period, multi).unwrap();
    for candle in mysql_candles.iter() {
        println!("candle: {:?}", candle);
        let (short_stop, long_stop, atr_value) = atr.next(
            candle.h.parse::<f64>()?,
            candle.l.parse::<f64>()?,
            candle.c.parse::<f64>()?,
        );

        let time_str = time_util::mill_time_to_datetime_shanghai(candle.ts).unwrap();
        println!("time_str: {:?}", time_str);
        println!(
            "short_stop:{}, long_stop:{}, atr_value:{}",
            short_stop, long_stop, atr_value
        );
    }

    Ok(())
}
