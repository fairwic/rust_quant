use anyhow::Result;
use dotenv::dotenv;

use rust_quant::trading::task::basic;
use rust_quant::{
    app_config::db::init_db, trading,
    trading::model::market::candles::CandlesEntity,
};
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::vegas_indicator::VegasIndicator;
use tracing::error;
use rust_quant::trading::indicator::signal_weight::SignalWeights;

#[tokio::test]
async fn test_vegas() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "OM-USDT-SWAP";
    let time = "1H";

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> = trading::task::basic::get_candle_data(inst_id, time, 600, None).await?;
    println!("{:#?}", mysql_candles);
    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }

    if true {
        //取出最新的一条数据，判断时间是否==当前时间的H,如果不是跳过
        //验证最新数据准确性
        let is_valid = basic::valid_candles_data(&mysql_candles, time);
        if is_valid.is_err() {
            error!("校验数据失败{}",is_valid.err().unwrap());
            return Ok(());
        }
    }

    let mut strategy = VegasIndicator::new(12, 144, 169, 576, 676);
    
    // 打印更详细的数据信息
    println!("数据总量: {}", mysql_candles.len());
    println!("最新K线价格: {}", mysql_candles.last().unwrap().c);
    println!("所需最小数据量: EMA1={}, EMA2={}, EMA3={}", 
        strategy.ema1_length, 
        strategy.ema2_length, 
        strategy.ema3_length);
    
    // 获取最近几根K线的收盘价，用于验证
    let last_prices: Vec<f64> = mysql_candles.iter()
        .rev()
        .take(5)
        .map(|c| c.c.parse::<f64>().unwrap())
        .collect();
    println!("最近5根K线收盘价: {:?}", last_prices);
    
    let result = strategy.get_trade_signal(&mysql_candles, &SignalWeights::default());
    println!("交易信号结果: {:?}", result);
    
    if let Some(detail) = &result.single_detail {
        println!("信号详情: {}", detail);
    }

    Ok(())
}
