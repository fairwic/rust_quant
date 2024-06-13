use std::env;
use log::{debug, error, warn};
use serde_json::json;
use tokio::net::TcpListener;
use tracing::{info, Level, span};
use crate::accept_connection;
use crate::trading::okx::okx_websocket_client;
use crate::trading::okx::okx_websocket_client::ApiType;

pub async fn run_socket() {
    let span = span!(Level::DEBUG, "socket_logic");
    let _enter = span.enter();
    // 模拟盘的请求的header里面需要添加 "x-simulated-trading: 1"。
    let api_key = env::var("OKX_API_KEY").expect("");
    let api_secret = env::var("OKX_API_SECRET").expect("");
    let passphrase = env::var("OKX_PASSPHRASE").expect("");
    let okx_websocket_clinet = okx_websocket_client::OkxWebsocket::new(api_key, api_secret, passphrase);


    // 订阅行情频道
    let public_channels = vec![
        json!({
            "channel": "tickers",
            "instId": "LTC-USDT"
        }),
        json!({
            "channel":"tickers",
            "instId":"ETH-USDT"
        }),
    ];


    // 订阅k线频道
    // 从数据库中获取需要订阅的产品
    let inst_ids = vec!["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP", "SUSHI-USDT-SWAP", "ADA-USDT-SWAP"];
    let times = vec!["4H"];

    let mut public_candles_channels = Vec::new();
    for inst_id in &inst_ids {
        for time in &times {
            // public_candles_channels.push(json!({
            //  "channel": format!("candle{}",time.clone()),
            // "instId": inst_id.clone(),

            public_candles_channels.push(json!({
             "channel": format!("candle{}",time.clone()),
            "instId": inst_id.clone(),
        }));
        }
    }
    let public_candles_channels = vec![
        json!({
            "channel": "candle1D",
            "instId": "BTC-USDT"
        }),
    ];

    println!("public_channels-------------------: {:?}", public_candles_channels);
    println!("public_channels-----------: {:?}", public_candles_channels);


    // 订阅私有频道
    let public_channels = vec![
        json!({
            "channel": "tickers",
            "instId": "LTC-USDT"
        }),
        json!({
            "channel":"tickers",
            "instId":"ETH-USDT"
        }),
    ];


    // 订阅私有频道
    let private_channels = vec![
        json!({
           "channel": "account",
            "ccy": "BTC-USDT_SWAP",
            "extraParams": "
        {
          \"updateInterval\": \"0\"
        }
      "
        }),
    ];

    // 创建并行任务
    let public_chanles_task = okx_websocket_clinet.subscribe(ApiType::Business, public_candles_channels);
    let public_task = okx_websocket_clinet.subscribe(ApiType::Public, public_channels);
    let private_task = okx_websocket_clinet.subscribe(ApiType::Private, private_channels);

    // 并行运行两个订阅任务
    if let (Err(public_err), Err(private_err), Err(public_candles_err)) =
        tokio::join!(public_task, private_task,public_chanles_task) {
        eprintln!("Failed to subscribe to public channels: {}", public_err);
        eprintln!("Failed to subscribe to private channels: {}", private_err);
        eprintln!("Failed to subscribe to public candles channels: {}", public_candles_err);
    }


    // let res = okx_websocket_clinet.socket_connect().await;
    // println!("!!!!!!!");
    // let res = okx_websocket_clinet.private_subscribe("tickers", "LTC-USDT").await;

    //
    // let addr = "127.0.0.1:9002";
    // let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    // info!("Listening on: {}", addr);
    //
    // while let Ok((stream, _)) = listener.accept().await {
    //     let peer = stream.peer_addr().expect("connected streams should have a peer address");
    //     info!("Peer address: {}", peer);
    //     let res = tokio::spawn(accept_connection(peer, stream));
    // }
}
