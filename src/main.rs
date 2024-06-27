#[macro_use]
extern crate rbatis;

use std::env;
use std::time::Duration;
// use anyhow::{Error, Result};
use base64;
use chrono::{DateTime, format, Timelike, Utc};
use hmac::Mac;
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, interval, sleep_until};


mod trading;
mod job;
mod time_util;
mod socket;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use trading::okx::okx_client;
use trading::model::biz_activity_model::BizActivityModel;
use clap::Parser;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::okx::market::Market;
use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::okx::okx_websocket_client;
use crate::trading::task::asset_job;
use crate::trading::task::candles_job;

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

use std::{
    collections::HashMap,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use anyhow::anyhow;
use dotenv::dotenv;

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, SinkExt, stream::TryStreamExt, StreamExt};
use futures_util::future::join_all;
use tracing::{error};
use redis::streams::StreamClaimOptions;
use serde_json::json;

use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{tungstenite};
// use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};
use tracing::{debug, info, Level, span};
use tracing_subscriber::{EnvFilter, fmt, FmtSubscriber};
use crate::trading::model::Db;
use crate::trading::okx::okx_websocket_client::ApiType;
use trading::strategy::StopLossStrategy;
use crate::job::task_scheduler::TaskScheduler;
use crate::trading::model::market::candles;
use crate::trading::okx::public_data::OkxPublicData;
use crate::trading::task::{account_job, tickets_job};

use crate::trading::model::strategy::back_test_log;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::okx::trade;
use crate::trading::okx::trade::{AttachAlgoOrd, OrderRequest, Side};
use crate::trading::strategy::StrategyType;
use tracing_subscriber::prelude::*;
use crate::trading::{order, task};  // 导入所有必要的扩展 trait

async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
    if let Err(e) = handle_connection(peer, stream).await {
        match e {
            tungstenite::Error::ConnectionClosed | tungstenite::Error::Protocol(_) | tungstenite::Error::Utf8 => (),
            err => error!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream) -> Result<()> {
    let mut ws_stream = accept_async(stream).await.expect("Failed to accept");

    info!("New WebSocket connection: {}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        info!("New Message : {}", msg);
        if msg.is_text() || msg.is_binary() {
            let response = "hhhh";
            ws_stream.send(Message::from(response)).await?;
        }
    }

    Ok(())
}


async fn validate_system_time() {
    let time_str = OkxPublicData::get_time().await;
    debug!("获取okx系统时间: {:?}", time_str);
    // 将字符串转换为DateTime<Utc>
    let time = time_str.unwrap().parse::<i64>().unwrap();
    let time = DateTime::<Utc>::from_utc(
        chrono::NaiveDateTime::from_timestamp(time / 1000, ((time % 1000) * 1_000_000) as u32),
        Utc,
    );

    // 获取本地时间
    let now = Utc::now().timestamp_millis();
    let okx_time = time.timestamp_millis();

    // 判断获取到时间是否与本地时间相差不超过100ms
    let time_diff = (now - okx_time).abs();
    if time_diff < 20000 {
        info!("时间间隔相差值: {} 毫秒", time_diff);
    } else {
        info!("时间未同步，时间间隔相差值: {} 毫秒", time_diff);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    //env init
    dotenv().ok();

    if env::var("APP_ENV").expect("app_env config is none") == "LOCAL" {
        // a builder for `FmtSubscriber`.
        let subscriber = FmtSubscriber::builder()
            // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
            // will be written to stdout.
            .with_max_level(Level::INFO)
            // completes the builder.
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    } else {
        // 设置日志轮换配置
        let debug_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "debug.log");
        let info_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "info.log");
        let error_file = RollingFileAppender::new(Rotation::DAILY, "log_files", "error.log");

        // 创建非阻塞的日志记录器
        // let (debug_non_blocking, _debug_guard) = tracing_appender::non_blocking(debug_file);
        let (info_non_blocking, _info_guard) = tracing_appender::non_blocking(info_file);
        let (error_non_blocking, _error_guard) = tracing_appender::non_blocking(error_file);
        // 初始化 tracing 订阅器
        tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .with_writer(info_non_blocking)
                    .with_filter(EnvFilter::new("info"))
                    .with_filter(tracing_subscriber::filter::LevelFilter::INFO)
            )
            .with(
                fmt::layer()
                    .with_writer(error_non_blocking)
                    .with_filter(EnvFilter::new("error"))
                    .with_filter(tracing_subscriber::filter::LevelFilter::ERROR)
            )
            .init();
    }


    //模拟交易
    // 模拟盘的请求的header里面需要添加 "x-simulated-trading: 1"。
    let api_key = env::var("OKX_API_KEY").expect("");
    let api_secret = env::var("OKX_API_SECRET").expect("");
    let passphrase = env::var("OKX_PASSPHRASE").expect("");
    let okx_websocket_clinet = okx_websocket_client::OkxWebsocket::new(api_key, api_secret, passphrase);

    let ins_type = "SWAP";
    let ticker = Market::get_tickers(&ins_type, None, None).await;
    println!("全部tickets: {:?}", ticker);
    // //
    // if let Ok(ticker_list) = ticker {
    //     let res = TicketsModel::new().await;
    //     let res = res.add(ticker_list).await;
    //     println!("插入数据库结果: {:?}", res);
    // }
    //2
    // let ins_type = "BTC-USDT-SWAP";
    // let ticker = Market::get_ticker(&ins_type).await;
    // println!("单个ticket: {:?}", ticker);
    // //
    // if let Ok(ticker_list) = ticker {
    //     let res = TicketsModel::new().await;
    //     let res = res.update(ticker_list.get(0).unwrap()).await;
    //     println!("插入数据库结果: {:?}", res);
    // }
    //3
    // let res = TicketsModel::new().await;
    // let res = res.get_all().await;
    // println!("全部结果: {:?}", res);

    // let ins_id = "BTC-USDT-SWAP";
    // let ins_id = "BTC-USDT";
    // let bar = "1D";
    // let ticker = Market::get_candles(&ins_id, bar, None, None, None).await;
    // println!("获取数据: {:?}", ticker);
    // if let Ok(ticket_list) = ticker {
    //     let res = CandlesModel::new().await;
    //     let res = res.add(ticket_list, "btc", "1D").await;
    //     println!("全部结果: {:?}", res);
    // }
    // let ins_id = "btc";
    // let bar = "1D";
    // let res = CandlesModel::new().await;
    // let res = res.get_all(ins_id, bar).await;
    // println!("全部结果: {:?}", res);

    //创建蜡烛图表
    // let res = CandlesModel::new().await;
    // let res = res.create_table(ins_id, bar).await;

    // let symbol = "BTC-USDT";
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


    // 初始化 Redis
    let client = redis::Client::open(env::var("REDIS_HOST").unwrap()).expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");


    //验证当前系统时间
    if env::var("APP_ENV").unwrap() != "LOCAL" {
        validate_system_time().await;
    }

    //----- 同步所有tickets
    // tickets_job::init_all_ticker().await?;
    //----- 1.定义想要交易的产品及周期
    // let inst_ids = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP", "ADA-USDT-SWAP"];
    // let tims = ["1H", "5m", "1D", "4H"];
    // let inst_ids = vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP"];
    // let inst_ids = vec!["SOL-USDT-SWAP"];

    let inst_ids = Arc::new(vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP"]);
    let times = Arc::new(vec!["1H", "4H", "1D"]);

    //------2. 初始化需要同步数据产品数据
    if env::var("IS_RUN_SYNC_DATA_JOB").unwrap() == "true" {
        task::run_sync_data_job(&inst_ids, &times).await?;
    }

    let mut scheduler = TaskScheduler::new();
    // //周期性任务
    // scheduler.add_periodic_task("periodic_task_1".to_string(), 500, || async {
    //     info!("Periodic job executed at {:?}", tokio::time::Instant::now());
    //     //同步单个交易产品
    //     tickets_job::sync_ticker().await;
    // });
    // // 周期性任务
    // scheduler.add_periodic_task("periodic_task_2".to_string(), 500, || async {
    //     println!("Periodic job executed at {:?}", tokio::time::Instant::now());
    //     //获取账户交易余额
    //     account_job::get_account_balance().await.expect("获取同步账户余额异常");
    // });
    // 周期性任务
    // scheduler.add_periodic_task("periodic_task_3".to_string(), 500, || async {
    //     println!("Periodic job executed at {:?}", tokio::time::Instant::now());
    //     获取账户交易余额
    // asset_job::get_balance().await.expect("获取资金账户余额异常");
    // });


    // ---------执行回测任务
    // let mut tasks = Vec::new();
    // for inst_id in inst_ids.iter() {
    //     for time in times.iter() {
    //         let inst_id = inst_id.to_string();
    //         let time = time.to_string();
    //         tasks.push(tokio::spawn(async move {
    //             let res = task::run_ut_boot_run_test(&inst_id, &time).await;
    //         }));
    //     }
    // }
    // 并发执行所有任务
    // join_all(tasks).await;


    // ------ 执行下单逻辑

    //test执行一次
    // let res = task::run_ut_boot_strategy_job(inst_ids, times).await;
    // match res {
    //     Ok(()) => {
    //         info!("run strategy success:");
    //     }
    //     Err(error) => {
    //         error!("run strategy error: {}", error);
    //     }
    // }

    // let inst_ids_clone = Arc::clone(&inst_ids);
    // let times_clone = Arc::clone(&times);

    //--- 重复运行
    scheduler.add_periodic_task("run_ut_boot_strategy_job".to_string(), 30000, move || {
        let inst_ids_inner = vec!["SOL-USDT-SWAP", "ETH-USDT-SWAP"];
        let times_inner = vec!["1D", "4H", "1H"];
        async move {
            let res = task::run_ut_boot_strategy_job(inst_ids_inner, times_inner).await;
            match res {
                Ok(()) => {
                    info!("run strategy success:");
                }
                Err(error) => {
                    error!("run strategy error: {}", error);
                }
            }
        }
    });


    // // 添加一个定时任务 删除3天前或者更早的信号日志，
    let target_time = Utc::now() + chrono::Duration::hours(12);
    scheduler.add_scheduled_task("scheduled_task_1".to_string(), target_time, || async {
        println!("Scheduled job executed at {:?}", tokio::time::Instant::now());
    });

    if env::var("IS_OPEN_SOCKET").unwrap() == "true" {
        // ---------3.运行websocket,实时同步数据
        socket::run_socket(inst_ids.clone(), times.clone()).await;
    }


    // 捕捉Ctrl+C信号以平滑关闭
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received, shutting down.");
        }
    }


    scheduler.shutdown().await;
    // 模拟运行一段时间后关闭调度器
    Ok(())
}
