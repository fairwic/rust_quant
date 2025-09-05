use anyhow::Result;
use dotenv::dotenv;
use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::indicator::vegas_indicator::VegasStrategy;
use rust_quant::trading::strategy::order::vagas_order::{StrategyConfig, StrategyOrder};
use rust_quant::trading::strategy::strategy_common::BasicRiskStrategyConfig;
use rust_quant::trading::task::basic;

#[tokio::test]
async fn test_run_strategy_job() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    setup_logging().await?;
    init_db().await;

    let mut strategy = VegasStrategy::default();

    // 设置布林带参数
    strategy.bolling_signal.as_mut().unwrap().multiplier = 2.0;
    strategy.bolling_signal.as_mut().unwrap().period = 10;
    strategy
        .bolling_signal
        .as_mut()
        .unwrap()
        .consecutive_touch_times = 4;
    //rsi
    strategy.rsi_signal.as_mut().unwrap().rsi_length = 20;
    strategy.rsi_signal.as_mut().unwrap().rsi_overbought = 90.0;
    strategy.rsi_signal.as_mut().unwrap().rsi_oversold = 20.0;
    //hammer
    strategy
        .kline_hammer_signal
        .as_mut()
        .unwrap()
        .up_shadow_ratio = 0.9;
    strategy
        .kline_hammer_signal
        .as_mut()
        .unwrap()
        .down_shadow_ratio = 0.9;
    //volume
    strategy
        .volume_signal
        .as_mut()
        .unwrap()
        .volume_increase_ratio = 2.0;
    strategy
        .volume_signal
        .as_mut()
        .unwrap()
        .volume_decrease_ratio = 2.0;
    strategy.volume_signal.as_mut().unwrap().volume_bar_num = 6;
    //engulfing
    strategy.engulfing_signal.as_mut().unwrap().body_ratio = 0.4;

    println!("strategy: {:#?}", strategy);

    let risk_config: BasicRiskStrategyConfig = BasicRiskStrategyConfig {
        is_one_k_line_diff_stop_loss: true,
        ..Default::default()
    };
    let strategy_config = StrategyConfig {
        strategy_config: strategy,
        risk_config: risk_config,
        strategy_config_id: 5,
    };
    let inst_id = "BTC-USDT-SWAP";
    let time = "1Dutc";
    //初始化数据与获取初始化的指标值
    let result = StrategyOrder::initialize_strategy_data(
        &strategy_config,
        &*"BTC-USDT-SWAP".to_string(),
        &*"1Dutc".to_string(),
    )
    .await;
    println!("result: {:?}", result);
    //执行一次策略
    let manager = basic::run_ready_to_order_with_manager(&*inst_id, &*time, &strategy_config).await;
    println!("result: {:?}", result);
    Ok(())
}
