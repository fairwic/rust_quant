use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::log::setup_logging;
use rust_quant::time_util;
use rust_quant::trading::indicator::vegas_indicator::VegasStrategy;
use rust_quant::trading::model::entity::candles::entity::CandlesEntity;
use rust_quant::trading::model::entity::candles::enums::SelectTime;
use rust_quant::trading::model::entity::candles::enums::TimeDirect;
use rust_quant::trading::strategy::nwe_strategy::NweSignalValues;
use rust_quant::trading::strategy::nwe_strategy::NweStrategy;
use rust_quant::trading::strategy::nwe_strategy::NweStrategyConfig;
use rust_quant::trading::strategy::strategy_common::get_multi_indicator_values;
use rust_quant::trading::strategy::strategy_common::parse_candle_to_data_item;
use rust_quant::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use rust_quant::{app_config::db::init_db, trading};
#[tokio::test]
async fn test_nwe_strategy() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    // let inst_id = "BTC-USDT-SWAP";
    // let time = "1H";
    let time = "5m";
    // let time = "1Dutc";
    let select_time: SelectTime = SelectTime {
        start_time: 1760689200000,
        direct: TimeDirect::BEFORE,
        end_time: None,
    };
    print!("11111111");

    // 获取K线数据
    let candles_list: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data_confirm(inst_id, time, 501, Some(select_time))
            .await?;

    let mut data_items = vec![];
    // let mut strategy = VegasStrategy::new(time.to_string());
    let mut strategy = NweStrategy::new(NweStrategyConfig::default());

    let mut indicator_combine = strategy.get_indicator_combine();

    let mut nwe_signal_values = NweSignalValues::default();
    for (i, candle) in candles_list.iter().enumerate() {
        // 获取数据项
        let data_item = parse_candle_to_data_item(candle);

        // 获取指标的值
        data_items.push(data_item.clone());

        let risk_strategy_config = BasicRiskStrategyConfig::default();
        indicator_combine.get_indicator_values(&mut nwe_signal_values, &data_item);
        if data_items.len() < 500 {
            continue;
        }
        println!("ts: {:#?}", time_util::mill_time_to_datetime_shanghai(data_item.clone().ts()).unwrap());
        println!("candle: {:#?}", candle.clone());
        println!("nwe_signal_values: {:#?}", nwe_signal_values);
        let result = strategy.get_trade_signal(&data_items, &nwe_signal_values);
        if i == (candles_list.len() - 1) {
            println!("交易信号结果: {:#?}", result);
        }
    }
    Ok(())
}
