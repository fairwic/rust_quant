use std::thread;
use std::time::Duration;
use chrono::Utc;
use hmac::digest::typenum::op;
use rbatis::rbatis_codegen::ops::AsProxy;
use rbatis::rbdc::datetime;
use redis::aio::MultiplexedConnection;
use tokio::time::sleep;
use tracing::debug;
use tracing::field::debug;
use crate::trading::model::market::tickers::{TickersDataEntity, TicketsModel};
use crate::trading::model::asset::AssetModel;
use crate::trading::model::market::candles::{CandlesEntity, CandlesModel};
use crate::trading::okx::market::Market;
use crate::trading::strategy::redis_operations::{RedisCandle, RedisOperations};


pub async fn init_create_table(inst_ids: Option<&Vec<&str>>, times: Option<&Vec<&str>>) -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all(inst_ids).await.unwrap();
    //获取获取数据更旧的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in times.clone().unwrap() {
            //获取当前数据最旧的数据
            let res = CandlesModel::new().await.create_table(ticker.inst_id.as_str(), time).await?;
            debug!("执行创建表语句 execResult{}",res);
        }
    }
    Ok(())
}

/** 同步所有更旧的蜡烛图**/
pub async fn init_all_candles(inst_ids: Option<&Vec<&str>>, times: Option<&Vec<&str>>) -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all(inst_ids).await.unwrap();

    //获取获取数据更旧的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in times.clone().unwrap() {

            //删除可能的异常数据(有可能中间某个未confirm==1)
            let res = CandlesModel::new().await.get_older_un_confirm_data(ticker.inst_id.as_str(), time).await?;
            if res.is_some() {
                //删除大于等于当前时间的所有数据
                let res = CandlesModel::new().await.delete_lg_time(ticker.inst_id.as_str(), time, res.unwrap().ts).await?;
            }

            // //获取当前数据最旧的数据
            // let res = CandlesModel::new().await.get_oldest_data(ticker.inst_id.as_str(), time).await?;
            // debug!("res: {:?}", res);
            // let mut after: i64 = 0;
            // if res.is_none() {
            //     after = Utc::now().naive_utc().timestamp_millis();
            // } else {
            //     after = res.unwrap().ts;
            // }


            //判断是否达到最新的300000条
            let limit = 50000;
            let res = CandlesModel::new().await.get_new_count(ticker.inst_id.as_str(), time, Some(limit)).await?;
            if (res > limit as u64) {
                debug!("达到最新的{}条,跳过",limit);
                continue;
            }
            //获取当前数据最旧的数据
            let res = CandlesModel::new().await.get_oldest_data(ticker.inst_id.as_str(), time).await?;
            debug!("res: {:?}", res);
            let mut after: i64 = 0;
            if res.is_none() {
                after = Utc::now().naive_utc().timestamp_millis();
            } else {
                after = res.unwrap().ts;
            }
            //     after=
            //
            // }
            loop {
                let res = Market::new().get_history_candles(&ticker.inst_id, time, Some(&after.to_string()), None, None).await?;
                if res.is_empty() {
                    debug!("No old candles patch{},{}",ticker.inst_id, time);
                    break;
                    //插入数据
                }
                let res = CandlesModel::new().await.add(res, ticker.inst_id.as_str(), time).await?;
                let res = CandlesModel::new().await.get_oldest_data(ticker.inst_id.as_str(), time).await?;
                after = res.unwrap().ts;
                thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Ok(())
}

/** 同步所有更新的蜡烛图**/
pub async fn init_before_candles(inst_ids: Option<&Vec<&str>>, times: Option<Vec<&str>>) -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all(inst_ids).await.unwrap();

    //获取获取数据更新的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in times.clone().unwrap() {
            //获取当前数据最旧的数据
            let res = CandlesModel::new().await.get_new_data(ticker.inst_id.as_str(), time).await?;
            debug!("res: {:?}", res);
            let mut before: i64 = 0;
            if res.is_none() {
                before = Utc::now().naive_utc().timestamp_millis();
            } else {
                before = res.unwrap().ts;
            }
            loop {
                let res = Market::new().get_history_candles(&ticker.inst_id, time, None, Some(&before.to_string()), Some("300")).await?;
                if res.is_empty() {
                    debug!("No new candles patch{},{}",ticker.inst_id, time);
                    break;
                }
                let res = CandlesModel::new().await.add(res, ticker.inst_id.as_str(), time).await?;
                let res = CandlesModel::new().await.get_new_data(ticker.inst_id.as_str(), time).await?;
                before = res.unwrap().ts;
                thread::sleep(Duration::from_millis(300));
            }
        }
    }

    Ok(())
}

/** 更新最新的蜡烛图**/
pub async fn update_new_candles_to_redis(mut redis: MultiplexedConnection, inst_id: &str, time: &str) -> anyhow::Result<()> {
    //获取获取数据更新的数据
    //获取当前交易产品的历史蜡烛图数据
    //获取当前数据最旧的数据
    let res = CandlesModel::new().await.get_new_data(inst_id, time).await?;
    debug!("res: {:?}", res);
    let mut before: i64 = 0;
    if res.is_none() {
        before = Utc::now().naive_utc().timestamp_millis();
    } else {
        before = res.unwrap().ts;
    }
    let key = CandlesModel::get_tale_name(inst_id, time);
    let res = Market::new().get_candles(&inst_id, time, None, Some(&before.to_string()), Some("300")).await?;
    if res.is_empty() {
        debug!("No new candles patch{},{}",inst_id, time);
        return Ok(());
        //插入数据
    } else {
        let mut redis_conn = redis.clone();
        let candle_structs: Vec<RedisCandle> = res.iter().map(|c| RedisCandle { ts: c.ts.parse().unwrap(), c: c.c.clone() }).collect();
        let res = RedisOperations::save_candles_to_redis(&mut redis_conn, key.as_str(), &candle_structs).await?;
    }
    Ok(())
}

pub async fn update_new_candles_to_db(inst_id: &str, time: &str) -> anyhow::Result<()> {
    //获取获取数据更新的数据
    //获取当前交易产品的历史蜡烛图数据
    //获取当前数据最旧的数据
    let res = CandlesModel::new().await.get_new_data(inst_id, time).await?;
    debug!("res: {:?}", res);
    let mut before: i64 = 0;
    if res.is_none() {
        before = Utc::now().naive_utc().timestamp_millis();
    } else {
        before = res.unwrap().ts;
    }
    loop {
        let res = Market::new().get_candles(&inst_id, time, None, Some(&before.to_string()), Some("300")).await?;
        if res.is_empty() {
            debug!("No new candles patch{},{}",inst_id, time);
            break;
            //插入数据
        }
        let res = CandlesModel::new().await.add(res, inst_id, time).await?;
        let res = CandlesModel::new().await.get_new_data(inst_id, time).await?;
        before = res.unwrap().ts;
    }

    Ok(())
}


