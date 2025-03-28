use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::signal_weight::SignalWeightsConfig;
use rust_quant::trading::indicator::vegas_indicator::VegasIndicator;
use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::trading::model::market::candles::SelectTime;
use rust_quant::trading::model::market::candles::TimeDirect;
use rust_quant::trading::strategy::profit_stop_loss::ProfitStopLoss;
use rust_quant::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use rust_quant::trading::task::basic;
use rust_quant::{app_config::db::init_db, trading};
use tracing::error;
#[tokio::test]
async fn test_vegas() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "1H";
    let select_time: SelectTime = SelectTime {
        point_time: 1742274000000,
        direct: TimeDirect::BEFORE,
    };

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data(inst_id, time, 3400, Some(select_time)).await?;

    if true {
        //取出最新的一条数据，判断时间是否==当前时间的H,如果不是跳过
        //验证最新数据准确性
        let is_valid = basic::valid_candles_data(&mysql_candles, time);
        if is_valid.is_err() {
            error!("校验数据失败{}", is_valid.err().unwrap());
            return Ok(());
        }
    }

    let mut strategy = VegasIndicator::default();

    let result = strategy.run_test(
        &mysql_candles,
        BasicRiskStrategyConfig::default(),
    );
    println!("交易信号结果: {:?}", result);

    Ok(())
}
