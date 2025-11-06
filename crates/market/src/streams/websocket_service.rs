use futures_util::{SinkExt, StreamExt};
use okx::websocket::auto_reconnect_client::AutoReconnectWebsocketClient;
use okx::websocket::auto_reconnect_client::ReconnectConfig;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
// use log::{debug, error, warn};
use okx::websocket::ChannelType;
use okx::websocket::OkxWebsocketClient;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tracing::{info, span, Level};

use crate::trading::cache::latest_candle_cache::default_provider;
use crate::trading::services::candle_service::candle_service::CandleService;
use crate::trading::services::candle_service::persist_worker::{CandlePersistWorker, PersistTask};
use crate::trading::task::tickets_job::update_ticker;
use okx::api::api_trait::OkxApiTrait;
use okx::config::Credentials;
use okx::dto::market_dto::CandleOkxRespDto;
use okx::dto::market_dto::TickerOkxResDto;
use okx::dto::CandleOkxWsResDto;
use okx::dto::CommonOkxWsResDto;
use okx::dto::TickerOkxResWsDto;
use okx::websocket::Args;
use serde::de;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Error, Result},
};
use tracing::debug;
use tracing::error;
async fn accept_connection(peer: SocketAddr, stream: TcpStream) {
    if let Err(e) = handle_connection(peer, stream).await {
        match e {
            tungstenite::Error::ConnectionClosed
            | tungstenite::Error::Protocol(_)
            | tungstenite::Error::Utf8 => (),
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

pub async fn run_socket(inst_ids: &Vec<String>, times: &Vec<String>) {
    let span = span!(Level::DEBUG, "socket_logic");
    let _enter = span.enter();
    // 模拟盘的请求的header里面需要添加 "x-simulated-trading: 1"。
    let api_key = env::var("OKX_API_KEY").expect("未配置OKX_API_KEY");
    let api_secret = env::var("OKX_API_SECRET").expect("未配置OKX_API_SECRET");
    let passphrase = env::var("OKX_PASSPHRASE").expect("未配置OKX_PASSPHRASE");
    let sim_trading = env::var("OKX_SIMULATED_TRADING").expect("未配置OKX_SIMULATED_TRADING");

    // 🚀 [已优化] 创建批处理Worker
    info!("🚀 初始化批处理Worker...");
    let (persist_tx, persist_rx) = mpsc::unbounded_channel::<PersistTask>();
    let worker = CandlePersistWorker::new(persist_rx)
        .with_config(100, std::time::Duration::from_millis(500));
    
    // 启动Worker
    tokio::spawn(async move {
        worker.run().await;
    });

    // 🚀 [已优化] 创建共享的CandleService实例
    let candle_service = Arc::new(
        CandleService::new_with_persist_worker(default_provider(), persist_tx)
    );
    info!("✅ CandleService实例已创建并启用批处理");

    // 创建自动重连客户端
    info!("📡 创建自动重连客户端...");
    let public_client = AutoReconnectWebsocketClient::new_public();

    let mut public_receiver = match public_client.start().await {
        Ok(rx) => {
            info!("✅ okx public websocket启动成功");
            rx
        }
        Err(e) => {
            error!("❌ okx public websocket启动失败: {}", e);
            return;
        }
    };
    let credentials = Credentials::new(api_key, api_secret, passphrase, sim_trading);

    let mut okx_websocket_client_business = AutoReconnectWebsocketClient::new_business(credentials);

    let mut private_message_receiver = match okx_websocket_client_business.start().await {
        Ok(rx) => {
            info!("✅ okx private websocket启动成功");
            rx
        }
        Err(e) => {
            error!("❌ okx private websocket启动失败: {}", e);
            return;
        }
    };

    // 订阅多个k线频道
    for inst_id in inst_ids.iter() {
        for time in times.iter() {
            let mut args = Args::new()
                .with_inst_id(inst_id.to_string())
                .with_param("period".to_string(), time.to_string());
            // 用私有client订阅k线频道
            let task = okx_websocket_client_business
                .subscribe(ChannelType::Candle(time.to_string()), args.clone())
                .await;
            match task {
                Ok(_) => {
                    info!("订阅k线频道成功: {:?},{:?}", inst_id, time);
                }
                Err(e) => {
                    error!("订阅k线频道失败: {:?}", e);
                }
            }
        }
    }

    // 订阅多个tickers频道
    for inst_id in inst_ids.iter() {
        let args = Args::new().with_inst_id(inst_id.to_string());
        // 用公有client订阅tickers频道
        let task = public_client
            .subscribe(ChannelType::Tickers, args.clone())
            .await;
        match task {
            Ok(_) => {
                info!("订阅tickers频道成功: {:?}", inst_id);
            }
            Err(e) => {
                error!("订阅tickers频道失败: {:?}", e);
            }
        }
    }

    // 持续监听并处理 websocket 消息
    tokio::spawn(async move {
        while let Some(msg) = public_receiver.recv().await {
            // info!("收到公共频道消息: {:?}", msg);
            // Object {"arg": Object {"channel": String("tickers"), "instId": String("BTC-USDT")}, "data": Array [Object {"askPx": String("103808"), "askSz": String("0.42913987"), "bidPx": String("103807.9"), "bidSz": String("0.75111858"), "high24h": String("104651.8"), "instId": String("BTC-USDT"), "instType": String("SPOT"), "last": String("103807.9"), "lastSz": String("0.00015066"), "low24h": String("100733"), "open24h": String("104016.9"), "sodUtc0": String("102790.1"), "sodUtc8": String("102520.1"), "ts": String("1747136969082"), "vol24h": String("8547.16177946"), "volCcy24h": String("878595784.826748153")}]}
            // 这里可以根据业务需求进一步处理消息
            //  todo 更新tickets
            let msg_str = msg.to_string();
            debug!("msg_str: {:?}", msg_str);
            let res = serde_json::from_str::<TickerOkxResWsDto>(&msg_str);
            if res.is_ok() {
                let ticker = res.unwrap();
                // info!("ticketOkxResWsDto数据: {:?}", ticker);
                let res =
                    update_ticker(ticker.data, &vec![ticker.arg.inst_id]).await;
                if res.is_ok() {
                    // info!("更新ticker成功: {:?}", res.unwrap());
                } else {
                    error!("更新ticker失败: {:?}", res.err());
                }
            } else {
                let res = serde_json::from_str::<CommonOkxWsResDto>(&msg_str);
                if res.is_ok() {
                    let dto = res.unwrap();
                    if dto.code == "0" {
                        debug!("get a message from common okx ws : {:?}", dto);
                    } else {
                        error!("get a message from common okx ws error : {:?}", dto);
                    }
                }
            }
            // println!("ticker.data: {:?}", ticker.data);
        }
    });
    // 🚀 [已优化] 复用service实例 + 消除二次序列化
    let candle_service_clone = Arc::clone(&candle_service);
    tokio::spawn(async move {
        while let Some(msg) = private_message_receiver.recv().await {
            // 🚀 [已优化] 直接从 Value 解析，避免 to_string() 序列化
            if let Ok(candle) = serde_json::from_value::<CandleOkxWsResDto>(msg.clone()) {
                debug!("收到K线数据: inst_id={}, channel={}", 
                    candle.arg.inst_id, candle.arg.channel);
                
                // 提取周期：candle2h -> 2h
                let period = candle.arg.channel.replace("candle", "");
                
                // 🚀 [已优化] 处理全部数据（而非只取last），使用into_iter避免clone
                let candle_data: Vec<CandleOkxRespDto> = candle
                    .data
                    .into_iter()
                    .map(CandleOkxRespDto::from_vec)
                    .collect();
                
                // 🚀 [已优化] 使用共享实例，批量处理
                if let Err(e) = candle_service_clone
                    .update_candles_batch(candle_data, &candle.arg.inst_id, &period)
                    .await
                {
                    error!("批量更新K线失败: inst_id={}, period={}, error={:?}", 
                        candle.arg.inst_id, period, e);
                }
            } else if let Ok(dto) = serde_json::from_value::<CommonOkxWsResDto>(msg) {
                if dto.code != "0" {
                    error!("收到错误消息: code={}, msg={}", dto.code, dto.msg);
                } else {
                    debug!("收到确认消息: {:?}", dto);
                }
            }
        }
    });
}
