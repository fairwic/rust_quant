use anyhow::Result;
use dotenv::dotenv;

use rust_quant::trading::services::big_data::big_data_top_contract_service::BigDataTopContractService;
use rust_quant::trading::services::big_data::big_data_top_position_service::BigDataTopPositionService;
use rust_quant::trading::strategy::profit_stop_loss::ProfitStopLoss;
use rust_quant::trading::strategy::top_contract_strategy::{TopContractData, TopContractStrategy};
use rust_quant::trading::task::top_contract_job::TopContractJob;
use rust_quant::{
    app_config::db::init_db, trading, trading::model::market::candles::CandlesEntity,
};

#[tokio::test]
async fn test_top_contract() -> Result<()> {
    // 初始化环境和数据库连接
    dotenv().ok();
    init_db().await;

    // 设置参数
    let inst_id = "BTC-USDT-SWAP";
    let time = "4H";
    let period = 2;

    // 获取K线数据
    let mysql_candles: Vec<CandlesEntity> =
        trading::task::basic::get_candle_data(inst_id, time, 1000, None).await?;
    // println!("{:#?}", mysql_candles);

    // 确保有数据
    if mysql_candles.is_empty() {
        println!("警告: 未获取到K线数据");
        return Ok(());
    }
    //获取到精英交易员仓位和人数的比例
    let account_ratio_list =
        BigDataTopContractService::get_list_by_time(inst_id, time, 1000, None).await?;

    let position_ratio_list =
        BigDataTopPositionService::get_list_by_time(inst_id, time, 1000, None).await?;
    println!("position res{:?}", position_ratio_list);
    let res = TopContractData {
        candle_list: mysql_candles,
        account_ratio: account_ratio_list,
        position_ratio: position_ratio_list,
    };

    let stra = TopContractStrategy {
        data: res,
        key_value: 0.0,
        atr_period: 0,
        heikin_ashi: false,
    };
    let fibonacci_level = ProfitStopLoss::get_fibonacci_level(inst_id, time);

    let res = stra.run_test(&fibonacci_level, 10.00, false, true, false, false)
        .await;

    print!("res{:?}", res);
    Ok(())
}
