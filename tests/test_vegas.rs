use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::signal_weight::SignalWeightsConfig;
use rust_quant::trading::indicator::vegas_indicator::VegasStrategy;
use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::trading::model::market::candles::SelectTime;
use rust_quant::trading::model::market::candles::TimeDirect;
use rust_quant::trading::strategy::profit_stop_loss::ProfitStopLoss;
use rust_quant::trading::strategy::strategy_common::get_multi_indivator_values;
use rust_quant::trading::strategy::strategy_common::parse_candle_to_data_item;
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
        point_time: 1743415200000,
        direct: TimeDirect::BEFORE,
    };

    // 获取K线数据
    let candles_list: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data(inst_id, time, 50, Some(select_time)).await?;

    let mut data_items = vec![];
    let mut strategy = VegasStrategy::default();
    let mut indicator_combine = strategy.get_indicator_combine();

    for (i, candle) in candles_list.iter().enumerate() {
        // 获取数据项
        let data_item = parse_candle_to_data_item(candle);

        println!("indicator_combine: {:#?}", indicator_combine);
        // 获取指标的值
        let mut multi_indicator_values =
            get_multi_indivator_values(&mut indicator_combine, &data_item);
        data_items.push(data_item);

        let signal_weights = strategy.signal_weights.as_ref().unwrap().clone();
        println!("signal_weights: {:#?}", signal_weights);
        println!("multi_indicator_values: {:#?}", multi_indicator_values);

        let result =
            strategy.get_trade_signal(&data_items, &mut multi_indicator_values, &signal_weights);
        println!("交易信号结果: {:?}", result);
    }

    Ok(())
}
