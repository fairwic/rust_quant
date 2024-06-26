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
use crate::trading::model::market::candles::CandlesModel;
use crate::trading::okx::public_data::CandleData;

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

    async fn connect_and_login(&self, api_type: &ApiType) -> Result<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>> {
        let ws_url = self.get_ws_url(api_type)?;
        let (mut ws_stream, _) = connect_async(&ws_url).await?;
        info!("WebSocket handshake has been successfully completed");

        if let ApiType::Private = api_type {
            self.login(&mut ws_stream).await?;
        }

        Ok(ws_stream)
    }

    async fn login(&self, ws_stream: &mut tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>)
                   -> Result<()> {
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

        write.send(Message::Text(login_msg.to_string())).await.map_err(|e| anyhow!(e))?;

        while let Some(msg) = read.next().await {
            match msg {
                Ok(msg) => {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();
                        if text.contains("\"event\":\"login\"") && text.contains("\"code\":\"0\"") {
                            info!("Login successful: {}", text);
                            return Ok(());
                        } else {
                            info!("Private received: {}", text);
                        }
                    }
                }
                Err(e) => {
                    error!("Error: {}", e);
                    break;
                }
            }
        }
        Err(anyhow!("Login failed"))
    }

    async fn subscribe_channels(&self, write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>, channels: &[serde_json::Value]) -> Result<()> {
        let subscribe_msg = json!({
            "op": "subscribe",
            "args": channels
        });
        write.send(Message::Text(subscribe_msg.to_string())).await.map_err(|e| anyhow!(e))?;
        Ok(())
    }

    async fn handle_reconnect(&self, api_type: &ApiType, channels: &[serde_json::Value]) -> Result<()> {
        let retry_strategy = ExponentialBackoff::from_millis(10)
            .map(jitter)
            .take(5);

        Retry::spawn(retry_strategy, || async {
            self.connect_and_subscribe(api_type, channels).await
        }).await?;
        Ok(())
    }

    async fn connect_and_subscribe(&self, api_type: &ApiType, channels: &[serde_json::Value]) -> Result<()> {
        let mut ws_stream = self.connect_and_login(api_type).await?;
        let (mut write, mut read) = ws_stream.split();
        self.subscribe_channels(&mut write, channels).await?;

        self.start_heartbeat(&mut write, &mut read).await?;
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
                            info!("okx websocket Received a new message: {}", msg);
                            if msg.is_text() {
                                let text = msg.to_text().unwrap();
                                if let Err(e) = self.parse_and_deal_socket_message(&text).await {
                                    error!("parse and deal socket message: {}", e);
                                }
                                // Reset the timer on any received message
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
                    // Send a ping message if no messages were received within the interval
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
