use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::vegas_indicator::VegasStrategy;
use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::trading::model::market::candles::SelectTime;
use rust_quant::trading::model::market::candles::TimeDirect;
use rust_quant::trading::strategy::strategy_common::get_multi_indicator_values;
use rust_quant::trading::strategy::strategy_common::parse_candle_to_data_item;
use rust_quant::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use rust_quant::{app_config::db::init_db, trading};
#[tokio::test]
async fn test_vegas() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    // let time = "1H";
    let time = "1Dutc";
    let select_time: SelectTime = SelectTime {
        start_time:1736726400000,
        direct: TimeDirect::BEFORE,
        end_time: None,
    };

    // 获取K线数据
    let candles_list: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data_confirm(inst_id, time, 7000, Some(select_time)).await?;

    let mut data_items = vec![];
    let mut  strategy = VegasStrategy::default();

    // 设置布林带参数
    strategy.bolling_signal.as_mut().unwrap().multiplier=2.0;
    strategy.bolling_signal.as_mut().unwrap().period=10;
    strategy.bolling_signal.as_mut().unwrap().consecutive_touch_times=4;
    //rsi
    strategy.rsi_signal.as_mut().unwrap().rsi_length=10;
    strategy.rsi_signal.as_mut().unwrap().rsi_overbought=90.0;
    strategy.rsi_signal.as_mut().unwrap().rsi_oversold=10.0;
    //hammer
    strategy.kline_hammer_signal.as_mut().unwrap().up_shadow_ratio=0.9;
    strategy.kline_hammer_signal.as_mut().unwrap().down_shadow_ratio=0.9;
    //volume
    strategy.volume_signal.as_mut().unwrap().volume_increase_ratio=2.5;
    strategy.volume_signal.as_mut().unwrap().volume_decrease_ratio=2.0;
    strategy.volume_signal.as_mut().unwrap().volume_bar_num=4;
    //engulfing
    strategy.engulfing_signal.as_mut().unwrap().body_ratio=0.4;

    println!("strategy: {:#?}", strategy);
    let mut indicator_combine = strategy.get_indicator_combine();

    for (i, candle) in candles_list.iter().enumerate() {
        // 获取数据项
        let data_item = parse_candle_to_data_item(candle);

        // 获取指标的值
        let mut multi_indicator_values =
            get_multi_indicator_values(&mut indicator_combine, &data_item);
        data_items.push(data_item);

        let signal_weights = strategy.signal_weights.as_ref().unwrap().clone();
        if i == (candles_list.len() - 1) {
            println!("final multi_indicator_values: {:#?}", multi_indicator_values);
        }

        let risk_strategy_config = BasicRiskStrategyConfig {
            is_move_stop_loss: true,
            ..Default::default()
        };

        let result =
            strategy.get_trade_signal(&data_items, &mut multi_indicator_values, &signal_weights, &risk_strategy_config);
        if i == (candles_list.len() - 1) {
            println!("交易信号结果: {:#?}", result);
        }
    }

    Ok(())
}
