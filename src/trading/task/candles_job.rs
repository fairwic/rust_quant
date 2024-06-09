use chrono::Utc;
use hmac::digest::typenum::op;
use rbatis::rbatis_codegen::ops::AsProxy;
use rbatis::rbdc::datetime;
use redis::aio::MultiplexedConnection;
use tracing::debug;
use tracing::field::debug;
use crate::trading::model::market::tickers::{TickersDataEntity, TicketsModel};
use crate::trading::model::asset::AssetModel;
use crate::trading::model::market::candles::{CandlesEntity, CandlesModel};
use crate::trading::okx::market::Market;
use crate::trading::strategy::redis_operations::{Candle, RedisOperations};


pub async fn init_create_table() -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all().await.unwrap();
    //获取获取数据更旧的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in ["1D"] {
            //获取当前数据最旧的数据
            let res = CandlesModel::new().await.create_table(ticker.inst_id.as_str(), time).await?;
            debug!("执行创建表语句 execResult{}",res);
        }
    }
    Ok(())
}

/** 同步所有更旧的蜡烛图**/
pub async fn init_all_candles() -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all().await.unwrap();

    //获取获取数据更旧的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in ["1D"] {
            //获取当前数据最旧的数据
            let res = CandlesModel::new().await.get_oldest_data(ticker.inst_id.as_str(), time).await?;
            println!("res: {:?}", res);
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
            }
        }
    }
    Ok(())
}

/** 同步所有更新的蜡烛图**/
pub async fn init_before_candles() -> anyhow::Result<()> {
    let res = TicketsModel::new().await;
    let res = res.get_all().await.unwrap();

    //获取获取数据更新的数据
    for ticker in res {
        //获取当前交易产品的历史蜡烛图数据
        for time in ["1D"] {
            //获取当前数据最旧的数据
            let res = CandlesModel::new().await.get_new_data(ticker.inst_id.as_str(), time).await?;
            println!("res: {:?}", res);
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
                    //插入数据
                }
                let res = CandlesModel::new().await.add(res, ticker.inst_id.as_str(), time).await?;
                let res = CandlesModel::new().await.get_new_data(ticker.inst_id.as_str(), time).await?;
                before = res.unwrap().ts;
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
    println!("res: {:?}", res);
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
        let candle_structs: Vec<Candle> = res.iter().map(|c| Candle { ts: c.ts.parse().unwrap(), c: c.c.clone() }).collect();
        let res = RedisOperations::save_candles_to_redis(&mut redis_conn, key.as_str(), &candle_structs).await?;
    }
    Ok(())
}

pub async fn update_new_candles_to_db(inst_id: &str, time: &str) -> anyhow::Result<()> {
    //获取获取数据更新的数据
    //获取当前交易产品的历史蜡烛图数据
    //获取当前数据最旧的数据
    let res = CandlesModel::new().await.get_new_data(inst_id, time).await?;
    println!("res: {:?}", res);
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


