use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, warn};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite;
use tracing::{info, Level, span};
use crate::trading::okx::okx_websocket_client;
use crate::trading::okx::okx_websocket_client::ApiType;

use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};

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

pub async fn run_socket(inst_ids: Arc<Vec<&str>>, times: Arc<Vec<&str>>) {
    let span = span!(Level::DEBUG, "socket_logic");
    let _enter = span.enter();
    // 模拟盘的请求的header里面需要添加 "x-simulated-trading: 1"。
    let api_key = env::var("OKX_API_KEY").expect("");
    let api_secret = env::var("OKX_API_SECRET").expect("");
    let passphrase = env::var("OKX_PASSPHRASE").expect("");
    let mut okx_websocket_clinet = okx_websocket_client::OkxWebsocket::new(api_key, api_secret, passphrase);


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


    let mut public_candles_channels = Vec::new();
    for inst_id in inst_ids.iter() {
        for time in times.iter() {
            public_candles_channels.push(json!({
             "channel": format!("candle{}",time.clone()),
             "instId": inst_id.clone(),
        }));
        }
    }

    debug!("public_candles_channels-------------------: {:?}", public_candles_channels);


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


    // 订阅私有频道 账户频道
    let private_channels = vec![
        json!({
           "channel": "account",
            "ccy": "BTC-USDT-SWAP",
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
    // let private_task = okx_websocket_clinet.subscribe(ApiType::Private, private_channels);

    // 并行运行两个订阅任务
    if let (Err(public_err), Err(public_candles_err)) =
        tokio::join!(public_task,public_chanles_task) {
        // tokio::join!(public_task, private_task,public_chanles_task) {
        eprintln!("Failed to subscribe to public channels: {}", public_err);
        // eprintln!("Failed to subscribe to private channels: {}", private_err);
        eprintln!("Failed to subscribe to public candles channels: {}", public_candles_err);
    }
}
