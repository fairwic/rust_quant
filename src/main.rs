#[macro_use]
extern crate rbatis;

use std::time::Duration;
use anyhow::Result;
use base64;
use chrono::{DateTime, Timelike, Utc};
use hmac::Mac;
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, interval, sleep_until};


mod trading;

use trading::okx::okx_client;
use trading::okx::model::biz_activity_model::BizActivityModel;
use clap::Parser;
use crate::trading::okx::market::Market;
use crate::trading::okx::model::market::tickers::TicketsModel;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    name: String,

    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,
}

#[tokio::main]
async fn main() {
    // let ins_type = "SWAP";
    // let ticker = Market::get_tickers(&ins_type, None, None).await;
    // println!("全部tickets: {:?}", ticker);
    // //
    // if let Ok(ticker_list) = ticker {
    //     let res = TicketsModel::new().await;
    //     let res = res.add(ticker_list).await;
    //     println!("插入数据库结果: {:?}", res);
    // }
    let ins_type = "BTC-USDT-SWAP";
    let ticker = Market::get_ticker(&ins_type).await;
    println!("单个ticket: {:?}", ticker);
    //
    if let Ok(ticker_list) = ticker {
        let res = TicketsModel::new().await;
        let res = res.update(ticker_list.get(0).unwrap()).await;
        println!("插入数据库结果: {:?}", res);
    }


    // let symbol = "BTC-USDT";
    //
    // 获取交易产品的k线数据
    // let symbol = "BTC-USDT";
    // let bar = "1m";
    // let candles = okx_client.get_candles(&symbol, &bar, None, None, None).await?;
    // println!("K线数据:");
    // for candle in &candles.data {
    //     println!("时间戳: {}, 开盘价: {}, 最高价: {}, 最低价: {}, 收盘价: {}, 交易量(张): {}, 交易量(币): {}, 交易量(计价货币): {}, K线状态: {}",
    //              candle.ts, candle.o, candle.h, candle.l, candle.c, candle.vol, candle.vol_ccy, candle.vol_ccy_quote, candle.confirm);
    // }
    //
    // //获取可以交易的产品信息 BTC-USDT-SWAP
    // let symbol = "SWAP";
    // let res = public_data::get_instruments(&symbol, None, None, None).await?;
    // println!("交易产品信息:{:#?}", res);
    // //
    // //
    // // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // // let balances = okx_client.get_balances(&ccy).await?;
    // // println!("账户余额:");
    // // for balance in balances {
    // //     println!("{}: {}", balance.ccy, balance.bal);
    // // }
    // let ccy = vec!["BTC".to_string(), "USDT".to_string(), "ETH".to_string()];
    // let balances = Account::get_balances(&ccy).await?;
    // println!("账户余额:{:#?}", balances);

    //获取系统时间
    // let time = public_data::get_time().await?;
    // println!("系统时间:{:#?}", time);

    // let db = BizActivityModel::new().await;
    // let result = db.add().await?;
    //


    // let every_n_seconds = Duration::from_secs(10); // 每隔10秒执行一次
    // let mut interval_timer = interval(every_n_seconds);
    //
    // // 指定时间
    // let now: DateTime<Utc> = Utc::now();
    // let target_time = now
    //     .with_hour(23).unwrap() // 指定小时
    //     .with_minute(30).unwrap() // 指定分钟
    //     .with_second(0).unwrap() // 指定秒
    //     .with_nanosecond(0).unwrap(); // 指定纳秒
    //
    // let duration_until_target = (target_time - now).to_std().unwrap();
    // let mut target_instant = Instant::now() + duration_until_target;
    //
    // println!("Current time: {:?}", now);
    // println!("Target time: {:?}", target_time);
    //
    //
    //   let args = Args::parse();
    //
    // for _ in 0..args.count {
    //     println!("Hello {}!", args.name)
    // }
    //
    // // 任务调度
    // loop {
    //     tokio::select! {
    //         _ = interval_timer.tick() => {
    //             println!("Periodic task executed at {:?}", tokio::time::Instant::now());
    //         }
    //         _ = sleep_until(target_instant) => {
    //             println!("Scheduled task executed at {:?}", tokio::time::Instant::now());
    //             // 重新计算下一次指定时间的间隔，如果你想要重复执行这个指定时间任务
    //             let next_target_time = target_time + chrono::Duration::days(1);
    //             let duration_until_next_target = (next_target_time - Utc::now()).to_std().unwrap();
    //             let next_target_instant = Instant::now() + duration_until_next_target;
    //             target_instant = next_target_instant; // 更新目标时间
    //         }
    //     }
    // }


    // Ok(())
}