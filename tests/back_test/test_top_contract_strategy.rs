use std::sync::Arc;

use anyhow::Result;
use dotenv::dotenv;

use rust_quant::app_config::db::init_db;
use rust_quant::app_config::log::setup_logging;
use rust_quant::trading::strategy::profit_stop_loss::ProfitStopLoss;
use rust_quant::trading::strategy::top_contract_strategy::TopContractStrategy;

#[tokio::test]
async fn test_top_contract() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    // 设置日志
    println!("init log config");
    setup_logging().await?;

    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "1D";
    let period = 2;

    let strate = TopContractStrategy::new(inst_id, time).await?;

    let stra = TopContractStrategy {
        data: Arc::new(strate),
        key_value: 1.1,
        atr_period: 0,
        heikin_ashi: false,
    };
    let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);

    let res = stra
        .run_test(&fibonacci_level, 10.00, false, true, true, false)
        .await;

    print!("res{:?}", res);
    Ok(())
}
