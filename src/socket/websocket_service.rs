use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
// use log::{debug, error, warn};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite;
use tracing::{info, Level, span};
use okx::websocket::OkxWebsocketClient;
use okx::websocket::ChannelType;

use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};
use okx::config::Credentials;
use okx::websocket::Args;
use tracing::debug;
use tracing::error;
use okx::dto::market_dto::TickerOkxResDto;
use crate::trading::task::tickets_job::update_ticker;
use okx::dto::TickerOkxResWsDto;
use okx::dto::CandleOkxWsResDto;
use okx::dto::CommonOkxWsResDto;
use serde::de;
use crate::trading::services::candle_service::candle_service::CandleService;
use okx::dto::market_dto::CandleOkxRespDto;
use okx::api::api_trait::OkxApiTrait;
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


pub async fn run_socket(inst_ids: Vec<&str>, times: Vec<&str>) {
    let span = span!(Level::DEBUG, "socket_logic");
    let _enter = span.enter();
    // 模拟盘的请求的header里面需要添加 "x-simulated-trading: 1"。
    let api_key = env::var("OKX_API_KEY").expect("");
    let api_secret = env::var("OKX_API_SECRET").expect("");
    let passphrase = env::var("OKX_PASSPHRASE").expect("");
    let sim_trading = env::var("OKX_SIM_TRADING").expect("");

    let mut okx_websocket_clinet = OkxWebsocketClient::new_public();
    let mut rx_public = okx_websocket_clinet.connect().await.unwrap();

    let mut okx_websocket_clinet_private = OkxWebsocketClient::new_private(Credentials::new(api_key, api_secret, passphrase,sim_trading));
    let mut rx_private = okx_websocket_clinet_private.connect().await.unwrap();


    // 订阅多个k线频道
    let mut candle_tasks = Vec::new();
    for inst_id in inst_ids.iter() {
        for time in times.iter() {
            let mut args = Args::new()
                .with_inst_id(inst_id.to_string())
                .with_param("period".to_string(), time.to_string());
            // 用私有client订阅k线频道
            let task = okx_websocket_clinet_private.subscribe(ChannelType::Candle(time.to_string()), args.clone());
            candle_tasks.push(task);
        }
    }

    // 订阅多个tickers频道
    let mut ticker_tasks = Vec::new();
    for inst_id in inst_ids.iter() {
        let mut args = Args::new().with_inst_id(inst_id.to_string());
        // 用公有client订阅tickers频道
        let task = okx_websocket_clinet.subscribe(ChannelType::Tickers, args.clone());
        ticker_tasks.push(task);
    }

    // 并行等待所有订阅
    let (candle_results, ticker_results): (Vec<_>, Vec<_>) = tokio::join!(
        futures::future::join_all(candle_tasks),
        futures::future::join_all(ticker_tasks)
    );

    // 错误处理
    for res in candle_results {
        if let Err(e) = res {
            error!("订阅k线频道失败: {:?}", e);
        }
    }
    for res in ticker_results {
        if let Err(e) = res {
            error!("订阅tickers频道失败: {:?}", e);
        }
    }

    // 持续监听并处理 websocket 消息
    tokio::spawn(async move {
        while let Some(msg) = rx_public.recv().await {
            debug!("收到公共频道消息: {:?}", msg);
            // Object {"arg": Object {"channel": String("tickers"), "instId": String("BTC-USDT")}, "data": Array [Object {"askPx": String("103808"), "askSz": String("0.42913987"), "bidPx": String("103807.9"), "bidSz": String("0.75111858"), "high24h": String("104651.8"), "instId": String("BTC-USDT"), "instType": String("SPOT"), "last": String("103807.9"), "lastSz": String("0.00015066"), "low24h": String("100733"), "open24h": String("104016.9"), "sodUtc0": String("102790.1"), "sodUtc8": String("102520.1"), "ts": String("1747136969082"), "vol24h": String("8547.16177946"), "volCcy24h": String("878595784.826748153")}]}
            // 这里可以根据业务需求进一步处理消息
            //  todo 更新tickets
            let msg_str = msg.to_string();
            debug!("msg_str: {:?}", msg_str);
            let res = serde_json::from_str::<TickerOkxResWsDto>(&msg_str);
            if res.is_ok() {
                let ticker = res.unwrap();
                info!("ticketOkxResWsDto数据: {:?}", ticker);
                let res=update_ticker(ticker.data,Some(vec![&ticker.arg.inst_id.as_str()])).await;
                if res.is_ok() {
                    info!("更新ticker成功: {:?}", res.unwrap());
                }else {
                    error!("更新ticker失败: {:?}", res.err());
                }
            }else  {
                let res = serde_json::from_str::<CommonOkxWsResDto>(&msg_str);
                if res.is_ok() {
                    let dto = res.unwrap();
                    if dto.code == "0" {
                        info!("get a message from common okx ws : {:?}", dto);
                    }else {
                        error!("get a message from common okx ws error : {:?}", dto);
                    }
                }
            }
            // println!("ticker.data: {:?}", ticker.data);
        }
    });
    tokio::spawn(async move {
        while let Some(msg) = rx_private.recv().await {
            debug!("收到私有频道消息: {:?}", msg);
            // Object {"arg": Object {"channel": String("candle1D"), "instId": String("BTC-USDT")}, "data": Array [Array [String("1747065600000"), String("102520.1"), String("103834.1"),
            // String("100733"), String("103807.9"), String("5492.76982429"), String("562452565.494063325"), String("562452565.494063325"), String("0")]]}
            // 这里可以根据业务需求进一步处理消息
            // let candle = serde_json::from_str::<CandleOkxWsResDto>(msg.as_str().unwrap()).unwrap();
            // println!("candle.data: {:?}", candle.data);
            let msg_str = msg.to_string();
            let res = serde_json::from_str::<CandleOkxWsResDto>(&msg_str);
            if res.is_ok() {
                let candle = res.unwrap();
                debug!("candleOkxResWsDto数据: {:?}", candle);
                //candle2h 处理成 2h
                let period =candle.arg.channel.as_str().replace("candle", "");
                // 更新candle
                let candle_data =candle.data.iter().map(|v| CandleOkxRespDto::from_vec(v.clone())).collect();
                let res=CandleService::new().update_candle(candle_data,candle.arg.inst_id.as_str(),period.as_str()).await;
                if res.is_ok() {
                    info!("更新candle成功: {:?}", res.unwrap());
                }else {
                    error!("更新candle失败: {:?}", res.err());
                }
            }else  {
                let res = serde_json::from_str::<CommonOkxWsResDto>(&msg_str);
                if res.is_ok() {
                    let dto = res.unwrap();
                    if dto.code == "0" {
                        info!("get a message from common okx ws : {:?}", dto);
                    }else {
                        error!("get a message from common okx ws error : {:?}", dto);
                    }
                }
            }
        }
    });
}
