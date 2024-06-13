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
mod Job;
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
use log::{error};
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
use crate::Job::task_scheduler::TaskScheduler;
use crate::trading::model::market::candles;
use crate::trading::okx::public_data::public_data;
use crate::trading::task::{account_job, tickets_job};

use crate::trading::model::strategy::back_test_log;
use crate::trading::model::strategy::back_test_log::BackTestLog;
use crate::trading::okx::trade;
use crate::trading::okx::trade::{AttachAlgoOrd, OrderRequest};
use crate::trading::strategy::StrategyType;
use tracing_subscriber::prelude::*;
use crate::trading::task;  // 导入所有必要的扩展 trait

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
    let time_str = public_data::get_time().await;
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
            .with_max_level(Level::DEBUG)
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
        let (debug_non_blocking, _debug_guard) = tracing_appender::non_blocking(debug_file);
        let (info_non_blocking, _info_guard) = tracing_appender::non_blocking(info_file);
        let (error_non_blocking, _error_guard) = tracing_appender::non_blocking(error_file);


        // 初始化 tracing 订阅器
        tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .with_writer(debug_non_blocking)
                    .with_filter(EnvFilter::new("debug"))
                    .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG)
            )
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

    //
    // // 订阅公共频道
    // let public_channels = vec![
    //     json!({
    //         "channel": "tickers",
    //         "instId": "LTC-USDT"
    //     }),
    //     json!({
    //         "channel":"tickers",
    //         "instId":"ETH-USDT"
    //     }),
    // ];
    // // 订阅私有频道
    // let private_channels = vec![
    //     json!({
    //        "channel": "account",
    //         "ccy": "BTC-USDT_SWAP",
    //         "extraParams": "
    //     {
    //       \"updateInterval\": \"0\"
    //     }
    //   "
    //     }),
    // ];
    // // 创建并行任务
    // let public_task = okx_websocket_clinet.subscribe(ApiType::Public, public_channels);
    // let private_task = okx_websocket_clinet.subscribe(ApiType::Private, private_channels);
    //
    // // 并行运行两个订阅任务
    // if let (Err(public_err), Err(private_err)) = tokio::join!(public_task, private_task) {
    //     eprintln!("Failed to subscribe to public channels: {}", public_err);
    //     eprintln!("Failed to subscribe to private channels: {}", private_err);
    // }


    // let ins_type = "SWAP";
    // let ticker = Market::get_tickers(&ins_type, None, None).await;
    // println!("全部tickets: {:?}", ticker);
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
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").expect("get redis client error");
    let mut con = client.get_multiplexed_async_connection().await.expect("get multi redis connection error");


    //验证当前系统时间
    validate_system_time().await;

    //初始化数据
    task::run_sync_data_job().await;

    //执行策略
    task::run_strategy_job().await?;


    //初始化可以交易产品
    // tickets_job::init_all_ticker().await;
    // let inst_ids = ["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"];


    // let order_params = OrderRequest {
    //     inst_id: "BTC-USDT".to_string(),
    //     td_mode: "isolated".to_string(),
    //     ccy: None,
    //     cl_ord_id: Some("custom_order_id".to_string()),
    //     tag: Some("order_tag".to_string()),
    //     side: "buy".to_string(),
    //     pos_side: Some("long".to_string()),
    //     ord_type: "limit".to_string(),
    //     sz: "1".to_string(),
    //     px: Some("30000".to_string()),
    //     px_usd: None,
    //     px_vol: None,
    //     reduce_only: Some(false),
    //     tgt_ccy: Some("quote_ccy".to_string()),
    //     ban_amend: Some(false),
    //     quick_mgn_type: None,
    //     stp_id: None,
    //     stp_mode: Some("cancel_maker".to_string()),
    //     attach_algo_ords: Some(vec![
    //         AttachAlgoOrd {
    //             attach_algo_cl_ord_id: Some("algo_order_id".to_string()),
    //             tp_trigger_px: Some("35000".to_string()),
    //             tp_ord_px: Some("34900".to_string()),
    //             tp_ord_kind: Some("limit".to_string()),
    //             sl_trigger_px: Some("29000".to_string()),
    //             sl_ord_px: Some("28900".to_string()),
    //             tp_trigger_px_type: Some("last".to_string()),
    //             sl_trigger_px_type: Some("last".to_string()),
    //             sz: Some("1".to_string()),
    //             amend_px_on_trigger_type: Some(0),
    //         }
    //     ]),
    // };
    //
    // //下单
    // let result = trade::Trade::order(order_params).await?;
    // println!("Order result: {}", result);
    //


    // let bar = "1D";
    // candles_job::update_new_candles_to_redis(con, ins_id, bar).await?;

    // let ins_id = "BTC-USDT-SWAP";
    // let bar = "1D";
    // candles_job::update_new_candles_to_redis(con, ins_id, bar).await?;

    // let result = db.add().await?;


    let mut scheduler = TaskScheduler::new();

    //周期性任务
    scheduler.add_periodic_task("periodic_task_1".to_string(), 500, || async {
        info!("Periodic Job executed at {:?}", tokio::time::Instant::now());
        //同步单个交易产品
        tickets_job::sync_ticker().await;
    });
    // // 周期性任务
    // scheduler.add_periodic_task("periodic_task_2".to_string(), 500, || async {
    //     println!("Periodic Job executed at {:?}", tokio::time::Instant::now());
    //     //获取账户交易余额
    //     account_job::get_account_balance().await.expect("获取同步账户余额异常");
    // });
    // 周期性任务
    // scheduler.add_periodic_task("periodic_task_3".to_string(), 500, || async {
    //     // println!("Periodic Job executed at {:?}", tokio::time::Instant::now());
    //     //获取账户交易余额
    //     asset_job::get_balance().await.expect("获取资金账户余额异常");
    // });


    // // 添加一个定时任务
    // let target_time = Utc::now() + chrono::Duration::seconds(30);
    // scheduler.add_scheduled_task("scheduled_task_1".to_string(), target_time, || async {
    //     println!("Scheduled Job executed at {:?}", tokio::time::Instant::now());
    // });

    //运行websocket,实时同步数据
    socket::run_socket().await;
    // 捕捉Ctrl+C信号以平滑关闭
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Ctrl+C received, shutting down.");
        }
    }


    scheduler.shutdown().await;
    // 模拟运行一段时间后关闭调度器
    // tokio::time::sleep(Duration::from_secs(60)).await;
    // scheduler.shutdown().await;
    Ok(())
}
