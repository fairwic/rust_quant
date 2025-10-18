use dotenv::dotenv;
use rust_quant::app_config::db::init_db;
use rust_quant::trading::indicator::nwe_indicator::NweIndicator;
use rust_quant::trading::model::entity::candles::entity::CandlesEntity;
use rust_quant::trading::model::entity::candles::enums::{SelectTime, TimeDirect};
use rust_quant::{time_util, trading};
use ta::indicators::AverageTrueRange;
use ta::Next;

/// 使用ta库的AverageTrueRange
#[tokio::test]
async fn test_ta_atr() -> anyhow::Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "5m";
    let select_time = Some(SelectTime {direct:TimeDirect::BEFORE, start_time:1760738100000, end_time: None});


    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data_confirm(inst_id, time, 501, select_time).await?;
    println!("{:#?}", mysql_candles);

    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }
    let mut nwe = NweIndicator::new(8.0, 3.0, 500); // let mut sma = SMA::new(2); // 设置周期为15

    // 计算并显示结果
    for candle in mysql_candles.iter() {
        // 解析价格数据
        let high = candle.h.parse::<f64>()?;
        let low = candle.l.parse::<f64>()?;
        let close = candle.c.parse::<f64>()?;
        // 计算NWE
        let nwe_value = nwe.next(close);
        // 时间格式化
        let time_str = time_util::mill_time_to_datetime_shanghai(candle.ts).unwrap();
        println!("time_str: {:?}", time_str);
        println!("nwe_value: {:?}", nwe_value);
        // 输出结果
    }
    Ok(())
}
