use std::env;
use std::io::{stdin, stdout};
use base64::encode;
use chrono::Utc;

use futures_util::{future, pin_mut, SinkExt, StreamExt};
use hmac_sha256::HMAC;
use tracing::{info, error, error_span};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::debug;
use crate::trading::model::market::candles::{CandlesEntity, CandlesModel};
use crate::trading::okx::public_data::CandleData;
// use crate::trading::okx::trade::CandleData;

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
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventMessage {
    //事件类型
    event: String,
    arg: Arg,
    //连接id
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

    fn generate_sign(&self, timestamp: &str, method: &str, request_path: &str, secret_key: String) -> String {
        let prehash = format!("{}{}{}", timestamp, method, request_path);
        let hash = HMAC::mac(prehash.as_bytes(), secret_key.as_bytes());
        encode(hash)
    }

    fn get_ws_url(&self, api_type: &ApiType) -> Result<String, Box<dyn std::error::Error>> {
        let env_var = match api_type {
            ApiType::Public => "WS_PUBLIC_URL",
            ApiType::Private => "WS_PRIVATE_URL",
            ApiType::Business => "WS_BUSINESS_URL",
        };
        println!("{}", env::var(env_var).unwrap());
        env::var(env_var).map_err(|_| format!("Environment variable {} not set", env_var).into())
    }

    async fn connect_and_login(&self, api_type: &ApiType) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Box<dyn std::error::Error>> {
        let ws_url = self.get_ws_url(api_type)?;
        let (ws_stream, _) = connect_async(&ws_url).await?;
        println!("WebSocket handshake has been successfully completed");

        if matches!(api_type, ApiType::Private) {
            let (mut write, mut read) = ws_stream.split();
            let timestamp = Utc::now().timestamp().to_string();
            let sign = self.generate_sign(&timestamp, "GET", "/users/self/verify", self.api_secret.clone());

            let login_msg = json!({
                "op": "login",
                "args": [{
                    "apiKey": self.api_key,
                    "passphrase": self.passphrase,
                    "timestamp": timestamp,
                    "sign": sign,
                }]
            });

            write.send(Message::Text(login_msg.to_string())).await.expect("Failed to send login message");

            while let Some(msg) = read.next().await {
                match msg {
                    Ok(msg) => {
                        if msg.is_text() {
                            let text = msg.to_text().unwrap();
                            if text.contains("\"event\":\"login\"") && text.contains("\"code\":\"0\"") {
                                println!("Login successful: {}", text);
                                return Ok(write.reunite(read).unwrap());
                            } else {
                                println!("Private received: {}", text);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        break;
                    }
                }
            }
            Err("Login failed".into())
        } else {
            Ok(ws_stream)
        }
    }

    // pub async fn subscribe(&self, api_type: ApiType, channels: Vec<serde_json::Value>) -> Result<(), Box<dyn std::error::Error>> {
    //     let ws_stream = self.connect_and_login(&api_type).await?;
    //     let (mut write, mut read) = ws_stream.split();
    //
    //     let subscribe_msg = json!({
    //         "op": "subscribe",
    //         "args": channels
    //     });
    //
    //     write.send(Message::Text(subscribe_msg.to_string())).await.expect("Failed to send subscribe message");
    //
    //     while let Some(msg) = read.next().await {
    //         match msg {
    //             Ok(msg) => {
    //                 if msg.is_text() {
    //                     let text = msg.to_text().unwrap();
    //                     println!("Received: {}", text);
    //                 }
    //             }
    //             Err(e) => {
    //                 eprintln!("Error: {}", e);
    //                 break;
    //             }
    //         }
    //     }
    //
    //     Ok(())
    // }


    async fn parse_and_log_message(&self, text: &str) -> anyhow::Result<()> {
        // 记录收到的原始消息
        info!("Received: {}", text);

        let message = serde_json::from_str::<WebSocketMessage>(text).unwrap();
        // 解析消息
        match message {
            WebSocketMessage::Candle(message_data) => {
                self.deal_candles_socker_messge(message_data).await?
            }
            WebSocketMessage::Tickers(message_data) => {
                info!("解析websocket tickers")
            }
            WebSocketMessage::Event(mesage_data) => {
                info!("解析websocket evene")
            }
        }
        Ok(())
    }

    pub async fn deal_candles_socker_messge(&self, message_data: CandleMessage) -> anyhow::Result<()> {
        info!("Parsed message: {:?}", message_data);
        let arg = &message_data.arg;
        let inst_id = &arg.inst_id; //ETH-USDT-SWAP
        let channel = &arg.channel; //candle1m
        //对字符串进行截取，从弟6个字符开始
        let time = &channel[6..];
        info!("substr time: {}", time);
        // 解析具体的数据

        // 解析具体的数据
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
            info!("Candle data: {:?}", candle_data);
            //把当前数据写入到数据库中
            CandlesModel::new().await.update_or_create(&candle_data, inst_id, time).await?;
        }
        Ok(())
    }


    pub async fn subscribe(&self, api_type: ApiType, channels: Vec<serde_json::Value>) -> Result<(), Box<dyn std::error::Error>> {
        let ws_stream = self.connect_and_login(&api_type).await?;
        let (mut write, mut read) = ws_stream.split();

        let subscribe_msg = json!({
                         "op": "subscribe",
                         "args": channels
          });
        write.send(Message::Text(subscribe_msg.to_string())).await.expect("Failed to send subscribe message");

        while let Some(msg) = read.next().await {
            match msg {
                Ok(msg) => {
                    debug!("okx websocket Received a new message: {}", msg);
                    debug!("is_text: {}", msg.is_text());
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();
                        debug!("to_text: {}", text);
                        self.parse_and_log_message(text).await?
                    }
                }
                Err(e) => {
                    error!("Error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}