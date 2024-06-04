use std::env;
use std::io::{stdin, stdout};
use base64::encode;
use chrono::Utc;

use futures_util::{future, pin_mut, SinkExt, StreamExt};
use hmac_sha256::HMAC;
use log::{info, error};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

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
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();
                        println!("Received: {}", text);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}