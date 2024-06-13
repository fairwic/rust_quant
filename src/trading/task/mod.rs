use anyhow::anyhow;
use tracing::{Level, span};
use crate::trading;
use crate::trading::model::Db;
use crate::trading::model::market::candles;
use crate::trading::model::strategy::back_test_log;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::strategy::{StopLossStrategy, StrategyType};

pub mod tickets_job;
pub mod account_job;
pub mod asset_job;
pub(crate) mod candles_job;
pub mod trades_job;


/** 同步数据 任务**/
pub async fn run_sync_data_job() -> Result<(), anyhow::Error> {
    let span = span!(Level::DEBUG, "run_sync_data_job");
    let _enter = span.enter();
    let inst_ids = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP", "ADA-USDT-SWAP"];
    let tims = ["4H"];

    candles_job::init_create_table(Some(Vec::from(inst_ids)), Some(Vec::from(tims))).await.expect("init create_table errror");
    candles_job::init_all_candles(Some(Vec::from(inst_ids)), Some(Vec::from(tims))).await?;
    candles_job::init_before_candles(Some(Vec::from(inst_ids)), Some(Vec::from(tims))).await?;
    Ok(())
}

/** 执行策略 任务**/
pub async fn run_strategy_job() -> Result<(), anyhow::Error> {
    let span = span!(Level::DEBUG, "run_sync_strategy_job");
    let _enter = span.enter();
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");


    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);


    // let inst_ids = ["BTC-USDT-SWAP"];
    let inst_ids = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP", "ADA-USDT-SWAP"];
    let tims = ["4H"];
    for inst_id in inst_ids {
        for time in tims {
            let mysql_candles_5m = candles::CandlesModel::new().await.fetch_candles_from_mysql(inst_id, time).await?;
            if mysql_candles_5m.is_empty() {
                return Err(anyhow!("mysql candles 5m is empty"));
            }
            //macd
            let macd_fast_period = 12;
            let macd_slow_period = 26;
            let macd_signal_period = 9;

            //突破周期，确认周期，成交量比例
            let breakout_period = 10;
            let confirmation_period = 2;
            let volume_threshold = 1.1;
            let stop_loss_strategy = StopLossStrategy::Amount(5.00);

            // let res = startegy.breakout_strategy(&*mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
            // println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";
            //
            // // 解包 Result 类型
            // let (final_fund, win_rate) = res;
            // //把back test strategy结果写入数据
            // let back_test_log = BackTestLog {
            //     strategy_type: format!("{:?}", StrategyType::BreakoutUp),
            //     inst_type: inst_id.parse()?,
            //     time: time.parse()?,
            //     final_fund: final_fund.to_string(),
            //     win_rate: win_rate.to_string(),
            //     strategy_detail: Some(format!("macd_fast_period: {}, macd_slow_period: {}, macd_signal:{},breakout_period:{},\
            //                                   confirmation_period:{},volume_threshold:{},stop_loss_strategy:{:?}",
            //                                   macd_fast_period, macd_slow_period, macd_signal_period, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy
            //     )),
            // };
            // back_test_log::BackTestLogModel::new().await.add(back_test_log).await?;
            //

            let res = startegy.short_strategy(&*mysql_candles_5m, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy).await;
            println!("strategy{:#?}", res);    // let ins_id = "BTC-USDT-SWAP";

            // 解包 Result 类型
            let (final_fund, win_rate) = res;
            //把back test strategy结果写入数据
            let back_test_log = BackTestLog {
                strategy_type: format!("{:?}", StrategyType::BreakoutDown),
                inst_type: inst_id.parse()?,
                time: time.parse()?,
                final_fund: final_fund.to_string(),
                win_rate: win_rate.to_string(),
                strategy_detail: Some(format!("macd_fast_period: {}, macd_slow_period: {}, macd_signal:{},breakout_period:{},\
                                              confirmation_period:{},volume_threshold:{},stop_loss_strategy:{:?}",
                                              macd_fast_period, macd_slow_period, macd_signal_period, breakout_period, confirmation_period, volume_threshold, stop_loss_strategy
                )),
            };
            back_test_log::BackTestLogModel::new().await.add(back_test_log).await?
        }
    }


    Ok(())
}

