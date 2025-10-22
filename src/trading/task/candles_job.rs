use crate::trading::model::asset::AssetModel;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::model::market::tickers::{TickersDataEntity, TicketsModel};
use crate::trading::strategy::redis_operations::{RedisCandle, RedisOperations};
use chrono::Utc;
use hmac::digest::typenum::op;
use okx::api::api_trait::OkxApiTrait;
use okx::api::market::OkxMarket;
use rbatis::rbatis_codegen::ops::AsProxy;
use rbatis::rbdc::datetime;

use std::thread;
use std::time::Duration;
use tokio::time::sleep;
use tracing::field::debug;
use tracing::{debug, error, info, warn};

fn get_period_back_test_candle_nums(period: &str) -> i32 {
    match period {
        // 24 * 60 * 20,
        "1m" => 28800,
        "5m" => 28800,
        "1H" => 28800,
        "4H" => 28800,
        "1D" => 28800,
        "1Dutc" => 28800,
        _ => 28800,
    }
}

//初始化创建表
pub async fn init_create_table(inst_ids: &Vec<String>, times: &Vec<String>) -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all(inst_ids).await.unwrap();
    //获取获取数据更旧的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in times {
            //获取当前数据最旧的数据
            let res = CandlesModel::new()
                .await
                .create_table(ticker.inst_id.as_str(), time)
                .await?;
            debug!("执行创建表语句 execResult{}", res);
        }
    }
    Ok(())
}

/** 同步所有更旧的蜡烛图**/
pub async fn init_all_candles(
    inst_ids: &Vec<String>,
    times: &Vec<String>,
) -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all(inst_ids).await?;
    //选择并发操作
    //获取获取数据更旧的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in times {
            //删除可能的异常数据(有可能中间某个未confirm==1)
            let res = CandlesModel::new()
                .await
                .get_older_un_confirm_data(ticker.inst_id.as_str(), time)
                .await?;
            if res.is_some() {
                //删除大于等于当前时间的所有数据
                let res = CandlesModel::new()
                    .await
                    .delete_lg_time(ticker.inst_id.as_str(), time, res.unwrap().ts)
                    .await?;
            }
            //判断是否达到最新的300000条
            let limit = get_period_back_test_candle_nums(time);
            let res = CandlesModel::new()
                .await
                .get_new_count(ticker.inst_id.as_str(), time, Some(limit))
                .await?;
            if (res > limit as u64) {
                debug!("达到最新的{}条,跳过", limit);
                continue;
            }
            //获取当前数据最旧的数据
            let res = CandlesModel::new()
                .await
                .get_oldest_data(ticker.inst_id.as_str(), time)
                .await?;
            debug!("res: {:?}", res);
            let mut after: i64 = 0;
            if res.is_none() {
                after = Utc::now().naive_utc().timestamp_millis();
            } else {
                after = res.unwrap().ts;
            }
            loop {
                sleep(Duration::from_millis(50)).await;
                info!("get after history_candles {},{}", &ticker.inst_id, time);
                //对下面进行的请求超时的时候进行重试
                let res = OkxMarket::from_env()?
                    .get_history_candles(
                        &ticker.inst_id,
                        time,
                        Some(&after.to_string()),
                        None,
                        None,
                    )
                    .await;
                if res.is_err() {
                    error!(
                        "get history_candles {} {} error{:?}",
                        &ticker.inst_id,
                        time,
                        res.err()
                    );
                    continue;
                }
                let res = res.unwrap();
                if res.is_empty() {
                    debug!("No old candles patch{},{}", ticker.inst_id, time);
                    break;
                    //插入数据
                }
                let res = CandlesModel::new()
                    .await
                    .add(res, ticker.inst_id.as_str(), time)
                    .await?;

                //判断是否达到最新的300000条
                let limit = get_period_back_test_candle_nums(time);
                let count = CandlesModel::new()
                    .await
                    .get_new_count(ticker.inst_id.as_str(), time, Some(limit))
                    .await?;
                if (count > limit as u64) {
                    info!("已达到所需数据的{}条,跳过", limit);
                    break;
                }

                let res = CandlesModel::new()
                    .await
                    .get_oldest_data(ticker.inst_id.as_str(), time)
                    .await?;
                after = res.unwrap().ts;
            }
        }
    }
    Ok(())
}

async fn get_sync_begin_with_end(
    inst_id: &str,
    period: &str,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let res = CandlesModel::new()
        .await
        .get_new_data(inst_id, period)
        .await?;
    match res {
        Some(t) => {
            let begin = t.ts;
            let end = crate::time_util::ts_add_n_period(t.ts, period, 100)?;
            Ok((Some(begin.to_string()), Some(end.to_string())))
        }
        None => Ok((None, None)),
    }
}
/** 同步所有更新的蜡烛图**/
pub async fn init_before_candles(
    inst_ids: &Vec<String>,
    times: &Vec<String>,
) -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all(inst_ids).await.unwrap();

    //获取获取数据更新的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in times {
            let res = CandlesModel::new()
                .await
                .get_new_data(ticker.inst_id.as_str(), time)
                .await?;
            debug!("res: {:?}", res);
            let mut before: i64 = 0;
            if res.is_none() {
                before = Utc::now().naive_utc().timestamp_millis();
            } else {
                before = res.unwrap().ts;
            }
            loop {
                sleep(Duration::from_millis(200)).await;
                info!("get before history_candles {},{}", &ticker.inst_id, time);
                //要计算出after_time
                let (begin, after) = get_sync_begin_with_end(ticker.inst_id.as_str(), time).await?;
                // info!("begin: {}, after: {}", begin.unwrap().clone(), after.unwrap().clone());
                let res = OkxMarket::from_env();
                if res.is_err() {
                    info!("OKX Market 初始化失败");
                    continue;
                }
                let res = res.unwrap();
                // println!("res: {:?}", res);
                let res = res
                    .get_history_candles(
                        &ticker.inst_id,
                        time,
                        Some(&after.unwrap()),
                        Some(&begin.unwrap()),
                        Some("300"),
                    )
                    .await?;
                if res.is_empty() {
                    debug!("No new candles patch{},{}", ticker.inst_id, time);
                    break;
                }
                let res = CandlesModel::new()
                    .await
                    .add(res, ticker.inst_id.as_str(), time)
                    .await?;
                let res = CandlesModel::new()
                    .await
                    .get_new_data(ticker.inst_id.as_str(), time)
                    .await?;
                before = res.unwrap().ts;
            }
        }
    }

    Ok(())
}

// /** 更新最新的蜡烛图**/
// pub async fn update_new_candles_to_redis(
//     mut redis: MultiplexedConnection,
//     inst_id: &str,
//     time: &str,
// ) -> anyhow::Result<()> {
//     //获取获取数据更新的数据
//     //获取当前交易产品的历史蜡烛图数据
//     //获取当前数据最旧的数据
//     let res = CandlesModel::new()
//         .await
//         .get_new_data(inst_id, time)
//         .await?;
//     debug!("res: {:?}", res);
//     let mut before: i64 = 0;
//     if res.is_none() {
//         before = Utc::now().naive_utc().timestamp_millis();
//     } else {
//         before = res.unwrap().ts;
//     }
//     let key = CandlesModel::get_tale_name(inst_id, time);
//     let res = OkxMarket::from_env()?
//         .get_candles(&inst_id, time, None, Some(&before.to_string()), Some("300"))
//         .await?;
//     if res.is_empty() {
//         debug!("No new candles patch{},{}", inst_id, time);
//         return Ok(());
//         //插入数据
//     } else {
//         let mut redis_conn = redis.clone();
//         let candle_structs: Vec<RedisCandle> = res
//             .iter()
//             .map(|c| RedisCandle {
//                 ts: c.ts.parse().unwrap(),
//                 c: c.c.clone(),
//             })
//             .collect();
//         let res =
//             RedisOperations::save_candles_to_redis(&mut redis_conn, key.as_str(), &candle_structs)
//                 .await?;
//     }
//     Ok(())
// }

// pub async fn update_new_candles_to_db(inst_id: &str, time: &str) -> anyhow::Result<()> {
//     //获取获取数据更新的数据
//     //获取当前交易产品的历史蜡烛图数据
//     //获取当前数据最旧的数据
//     let res = CandlesModel::new()
//         .await
//         .get_new_data(inst_id, time)
//         .await?;
//     debug!("res: {:?}", res);
//     let mut before: i64 = 0;
//     if res.is_none() {
//         before = Utc::now().naive_utc().timestamp_millis();
//     } else {
//         before = res.unwrap().ts;
//     }
//     loop {
//         let res = OkxMarket::from_env()?
//             .get_candles(&inst_id, time, None, Some(&before.to_string()), Some("300"))
//             .await?;
//         if res.is_empty() {
//             debug!("No new candles patch{},{}", inst_id, time);
//             break;
//             //插入数据
//         }
//         let res = CandlesModel::new().await.add(res, inst_id, time).await?;
//         let res = CandlesModel::new()
//             .await
//             .get_new_data(inst_id, time)
//             .await?;
//         before = res.unwrap().ts;
//     }

//     Ok(())
// }
