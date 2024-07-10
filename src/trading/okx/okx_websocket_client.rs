use tokio_retry::strategy::{ExponentialBackoff, jitter};
use tokio_retry::Retry;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, MaybeTlsStream};
use futures_util::{SinkExt, StreamExt};
use tokio::time::{self, Duration};
use tracing::{debug, error, info};
use std::env;
use base64::encode;
use chrono::Utc;
use hmac_sha256::HMAC;
use serde::{Deserialize, Serialize};
use serde_json::json;
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::okx::public_data::CandleData;

lazy_static! {
    static ref CONNECTION_COUNT: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    static ref MAX_CONNECTIONS: u32 = 20;
    static ref REQUEST_COUNT: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    static ref MAX_REQUESTS: u32 = 480;
}

pub(crate) struct OkxWebsocket {
    api_key: String,
    api_secret: String,
    passphrase: String,
}

#[derive(Debug)]
pub enum ApiType {
    Public,
    Private,
    Business,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum WebSocketMessage {
    Tickers(TickersMessage),
    Candle(CandleMessage),
    Event(EventMessage),
    Pong,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventMessage {
    event: String,
    arg: Arg,
    conn_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TickersMessage {
    arg: Arg,
    data: Vec<TickerData>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TickerData {
    instType: String,
    instId: String,
    last: String,
    lastSz: String,
    askPx: String,
    askSz: String,
    bidPx: String,
    bidSz: String,
    open24h: String,
    high24h: String,
    low24h: String,
    sodUtc0: String,
    sodUtc8: String,
    volCcy24h: String,
    vol24h: String,
    ts: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CandleMessage {
    arg: Arg,
    data: Vec<[String; 9]>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Arg {
    channel: String,
    inst_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Data {
    ts: String,
    o: String,
    h: String,
    l: String,
    c: String,
    vol: String,
    volCcy: String,
    volCcyQuote: String,
    confirm: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CandleMessageData {
    arg: Arg,
    data: Vec<Data>,
}

impl OkxWebsocket {
    pub fn new(api_key: String, api_secret: String, passphrase: String) -> Self {
        OkxWebsocket {
            api_key,
            api_secret,
            passphrase,
        }
    }

    fn generate_sign(&self, timestamp: &str, method: &str, request_path: &str) -> String {
        let prehash = format!("{}{}{}", timestamp, method, request_path);
        let hash = HMAC::mac(prehash.as_bytes(), self.api_secret.as_bytes());
        encode(hash)
    }

    fn get_ws_url(&self, api_type: &ApiType) -> Result<String> {
        let env_var = match api_type {
            ApiType::Public => "WS_PUBLIC_URL",
            ApiType::Private => "WS_PRIVATE_URL",
            ApiType::Business => "WS_BUSINESS_URL",
        };
        env::var(env_var).map_err(|_| anyhow!("Environment variable {} not set", env_var))
    }

    async fn login(&self, ws_stream: tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>) -> Result<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>> {
        let (mut write, mut read) = ws_stream.split();
        let timestamp = Utc::now().timestamp().to_string();
        let sign = self.generate_sign(&timestamp, "GET", "/users/self/verify");

        let login_msg = json!({
        "op": "login",
        "args": [{
            "apiKey": self.api_key,
            "passphrase": self.passphrase,
            "timestamp": timestamp,
            "sign": sign,
        }]
    });

        self.send_request(&mut write, &login_msg).await?;

        while let Some(msg) = read.next().await {
            match msg {
                Ok(msg) => {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();
                        if text.contains("\"event\":\"login\"") && text.contains("\"code\":\"0\"") {
                            info!("登录成功: {}", text);
                            return Ok(write.reunite(read).expect("Failed to reunite the stream"));
                        } else {
                            info!("私有接收到: {}", text);
                        }
                    }
                }
                Err(e) => {
                    error!("错误: {}", e);
                    break;
                }
            }
        }
        Err(anyhow!("登录失败"))
    }

    async fn connect_and_login(&self, api_type: &ApiType) -> Result<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>> {
        {
            let mut count = CONNECTION_COUNT.lock().unwrap();
            if *count >= *MAX_CONNECTIONS {
                error!("连接数过多");
                return Err(anyhow!("连接数过多"));
            }
            *count += 1;
        }

        let ws_url = self.get_ws_url(api_type)?;
        let (ws_stream, _) = connect_async(&ws_url).await?;
        info!("WebSocket握手已成功完成");

        if let ApiType::Private = api_type {
            self.login(ws_stream).await
        } else {
            Ok(ws_stream)
        }
    }

    async fn subscribe_channels(&self, write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>, channels: &[serde_json::Value]) -> Result<()> {
        let subscribe_msg = json!({
            "op": "subscribe",
            "args": channels
        });
        self.send_request(write, &subscribe_msg).await?;
        Ok(())
    }

    async fn send_request(&self, write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>, request: &serde_json::Value) -> Result<()> {
        let mut request_count = REQUEST_COUNT.lock().unwrap();
        if *request_count >= *MAX_REQUESTS {
            error!("Too many requests");
            return Err(anyhow!("Too many requests"));
        }

        write.send(Message::Text(request.to_string())).await.map_err(|e| anyhow!(e))?;
        *request_count += 1;

        // Reset request count every hour
        tokio::spawn({
            let request_count = REQUEST_COUNT.clone();
            async move {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                *request_count.lock().unwrap() = 0;
            }
        });

        Ok(())
    }

    async fn handle_reconnect(&self, api_type: &ApiType, channels: &[serde_json::Value]) -> Result<()> {
        let retry_strategy = ExponentialBackoff::from_millis(10)
            .map(jitter)
            .take(5);

        Retry::spawn(retry_strategy, || async {
            time::sleep(Duration::from_secs(1)).await; // 限制每秒最多3次连接尝试
            match self.connect_and_subscribe(api_type, channels).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("Retry failed with error: {}", e);
                    Err(e)
                }
            }
        }).await?;
        Ok(())
    }

    async fn connect_and_subscribe(&self, api_type: &ApiType, channels: &[serde_json::Value]) -> Result<()> {
        let mut ws_stream = self.connect_and_login(api_type).await?;
        let (mut write, mut read) = ws_stream.split();
        self.subscribe_channels(&mut write, channels).await?;

        self.start_heartbeat(&mut write, &mut read).await?;

        {
            let mut count = CONNECTION_COUNT.lock().unwrap();
            *count -= 1;
        }

        Ok(())
    }

    async fn start_heartbeat(&self, write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>, read: &mut futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>) -> Result<()> {
        let heartbeat_interval = Duration::from_secs(25);
        let mut heartbeat_timer = time::interval(heartbeat_interval);

        loop {
            tokio::select! {
                Some(msg) = read.next() => {
                    match msg {
                        Ok(msg) => {
                            info!("Received a new message: {}", msg);
                            if msg.is_text() {
                                let text = msg.to_text().unwrap();
                                if let Err(e) = self.parse_and_deal_socket_message(&text).await {
                                    error!("parse and deal socket message: {}", e);
                                }
                                // 重置心跳计时器
                                heartbeat_timer.reset();
                            }
                        }
                        Err(e) => {
                            error!("Error: {}", e);
                            return Err(anyhow!(e));
                        }
                    }
                },
                _ = heartbeat_timer.tick() => {
                    // 在指定时间间隔内未收到消息时发送ping消息
                    info!("Sending ping");
                    write.send(Message::Text("ping".into())).await.map_err(|e| anyhow!(e))?;
                }
            }
        }
    }

    async fn parse_and_deal_socket_message(&self, text: &str) -> Result<()> {
        let message: WebSocketMessage = serde_json::from_str(text)?;
        match message {
            WebSocketMessage::Candle(message_data) => self.deal_candles_socket_message(message_data).await?,
            WebSocketMessage::Tickers(_) => info!("Parsing websocket tickers"),
            WebSocketMessage::Event(_) => info!("Parsing websocket event"),
            WebSocketMessage::Pong => info!("Received pong"),
        }
        debug!("Parsed socket message successfully");
        Ok(())
    }

    pub async fn deal_candles_socket_message(&self, message_data: CandleMessage) -> Result<()> {
        let arg = &message_data.arg;
        let inst_id = &arg.inst_id;
        let channel = &arg.channel;
        let time = &channel[6..];

        for data in message_data.data {
            let candle_data = CandleData {
                ts: data[0].clone(),
                o: data[1].clone(),
                h: data[2].clone(),
                l: data[3].clone(),
                c: data[4].clone(),
                vol: data[5].clone(),
                vol_ccy: data[6].clone(),
                vol_ccy_quote: data[7].clone(),
                confirm: data[8].clone(),
            };
            CandlesModel::new().await.update_or_create(&candle_data, inst_id, time).await?;
        }
        Ok(())
    }

    pub async fn subscribe(&self, api_type: ApiType, channels: Vec<serde_json::Value>) -> Result<()> {
        self.handle_reconnect(&api_type, &channels).await?;
        Ok(())
    }
}
