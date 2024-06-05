#[macro_use]
extern crate rbatis;

use std::env;
use std::time::Duration;
// use anyhow::{Error, Result};
use base64;
use chrono::{DateTime, Timelike, Utc};
use hmac::Mac;
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, interval, sleep_until};


mod trading;

use trading::okx::okx_client;
use trading::model::biz_activity_model::BizActivityModel;
use clap::Parser;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::okx::market::Market;
use crate::trading::model::market::tickers::TicketsModel;
use crate::trading::okx::okx_websocket_client;

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
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use crate::trading::model::Db;
use crate::trading::okx::okx_websocket_client::ApiType;
use trading::strategy::StopLossStrategy;

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


#[tokio::main]
async fn main() {
    //env init
    dotenv().ok();
    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(Level::TRACE)
        // completes the builder.
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");
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


    // let res = okx_websocket_clinet.socket_connect().await;
    // println!("!!!!!!!");
    // let res = okx_websocket_clinet.private_subscribe("tickers", "LTC-USDT").await;


    // let addr = "127.0.0.1:9002";
    // let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    // info!("Listening on: {}", addr);
    //
    // while let Ok((stream, _)) = listener.accept().await {
    //     let peer = stream.peer_addr().expect("connected streams should have a peer address");
    //     info!("Peer address: {}", peer);
    //
    //     let res = tokio::spawn(accept_connection(peer, stream));
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
    // 初始化 Redis
    let client = redis::Client::open("redis://:pxb7_redis@127.0.0.1:26379/").unwrap();
    let mut con = client.get_multiplexed_async_connection().await.unwrap();

    // let db = BizActivityModel::new().await;
    let mut startegy = trading::strategy::Strategy::new(Db::get_db_client().await, con);
    let res = startegy.main("btc", "1D", 12, 26, 9,StopLossStrategy::Amount(3.00)).await;
    println!("strategy{:#?}", res)

    // let result = db.add().await?;
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