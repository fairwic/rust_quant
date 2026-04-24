use anyhow::{anyhow, Result};
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ExecutionTaskConfig {
    pub base_url: String,
    pub internal_secret: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionTaskClient {
    client: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLeaseRequest {
    pub worker_id: String,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskLease {
    pub tasks: Vec<ExecutionTask>,
}

impl<'de> Deserialize<'de> for ExecutionTaskLease {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawLease {
            #[serde(default)]
            tasks: Vec<ExecutionTask>,
            #[serde(default)]
            items: Vec<RawLeaseItem>,
        }

        #[derive(Deserialize)]
        struct RawLeaseItem {
            task: ExecutionTask,
        }

        let raw = RawLease::deserialize(deserializer)?;
        let tasks = if raw.tasks.is_empty() {
            raw.items.into_iter().map(|item| item.task).collect()
        } else {
            raw.tasks
        };

        Ok(Self { tasks })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTask {
    pub id: i64,
    #[serde(default)]
    pub news_signal_id: Option<i64>,
    #[serde(default)]
    pub strategy_signal_id: Option<i64>,
    pub combo_id: i64,
    pub buyer_email: String,
    pub strategy_slug: String,
    pub symbol: String,
    pub task_type: String,
    pub task_status: String,
    pub priority: i32,
    pub lease_owner: Option<String>,
    pub lease_until: Option<String>,
    pub scheduled_at: String,
    #[serde(deserialize_with = "deserialize_json_value_from_string")]
    pub request_payload_json: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskReportRequest {
    pub task_id: i64,
    pub execution_status: String,
    pub exchange: String,
    pub external_order_id: String,
    pub order_side: String,
    pub order_status: String,
    pub filled_qty: Option<f64>,
    pub filled_quote: Option<f64>,
    pub fee_amount: Option<f64>,
    pub profit_usdt: Option<f64>,
    pub executed_at: Option<String>,
    pub error_message: Option<String>,
    pub raw_payload_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionTaskReportResponse {
    pub task: ExecutionTask,
    pub attempt: Value,
    pub order_result: Option<Value>,
    pub trade_record: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalSubmitRequest {
    pub source: String,
    pub external_id: String,
    pub strategy_slug: String,
    pub strategy_key: String,
    pub symbol: String,
    pub signal_type: String,
    pub direction: String,
    pub title: String,
    pub summary: Option<String>,
    pub confidence: Option<f64>,
    pub payload_json: String,
    pub generated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalInbox {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    pub strategy_slug: String,
    pub strategy_key: String,
    pub symbol: String,
    pub signal_type: String,
    pub direction: String,
    pub title: String,
    pub summary: Option<String>,
    pub confidence: Option<f64>,
    pub payload_json: String,
    pub generated_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StrategySignalDispatchResponse {
    pub inbox: StrategySignalInbox,
    pub generated_tasks: Vec<ExecutionTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct UserExchangeConfig {
    pub buyer_email: String,
    pub exchange: String,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: Option<String>,
    #[serde(default)]
    pub simulated: bool,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    success: bool,
    data: T,
}

const LEASE_TASKS_PATH: &str = "/api/commerce/internal/execution-tasks/lease";
const REPORT_RESULT_PATH: &str = "/api/commerce/internal/execution-results";
const STRATEGY_SIGNAL_PATH: &str = "/api/commerce/internal/strategy-signals";
const USER_EXCHANGE_CONFIG_PATH: &str = "/api/commerce/internal/api-credentials/resolve";
const INTERNAL_SECRET_HEADER: &str = "x-alpha-execution-secret";

impl ExecutionTaskClient {
    pub fn new(config: ExecutionTaskConfig) -> Result<Self> {
        let base_url = config.base_url.trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(anyhow!("execution task base_url is empty"));
        }

        let client = reqwest::Client::builder().build()?;
        let internal_secret = {
            let secret = config.internal_secret.trim().to_string();
            (!secret.is_empty()).then_some(secret)
        };

        Ok(Self {
            client,
            base_url,
            internal_secret,
        })
    }

    pub async fn lease_tasks(
        &self,
        request: ExecutionTaskLeaseRequest,
    ) -> Result<ExecutionTaskLease> {
        self.get_json(&self.lease_url(request.limit)).await
    }

    pub async fn report_result(
        &self,
        request: ExecutionTaskReportRequest,
    ) -> Result<ExecutionTaskReportResponse> {
        self.post_json(REPORT_RESULT_PATH, &request).await
    }

    pub async fn submit_strategy_signal(
        &self,
        request: StrategySignalSubmitRequest,
    ) -> Result<StrategySignalDispatchResponse> {
        self.post_json(STRATEGY_SIGNAL_PATH, &request).await
    }

    pub async fn resolve_user_exchange_config(
        &self,
        buyer_email: &str,
        exchange: &str,
    ) -> Result<UserExchangeConfig> {
        let mut url = reqwest::Url::parse(&self.url(USER_EXCHANGE_CONFIG_PATH))?;
        url.query_pairs_mut()
            .append_pair("buyer_email", buyer_email)
            .append_pair("exchange", exchange);
        self.get_json(url.as_str()).await
    }

    pub fn lease_url(&self, limit: u32) -> String {
        format!(
            "{}?limit={}",
            self.url(LEASE_TASKS_PATH),
            limit.clamp(1, 100)
        )
    }

    pub fn strategy_signal_url(&self) -> String {
        self.url(STRATEGY_SIGNAL_PATH)
    }

    pub fn parse_envelope<R>(body: &str) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let envelope = serde_json::from_str::<ApiEnvelope<R>>(body)
            .map_err(|e| anyhow!("parse quant_web envelope failed: {}; body={}", e, body))?;
        if !envelope.success {
            return Err(anyhow!("quant_web envelope success=false"));
        }
        Ok(envelope.data)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get_json<R>(&self, url: &str) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let mut request = self.client.get(url);
        if let Some(secret) = self.internal_secret.as_deref() {
            request = request.header(INTERNAL_SECRET_HEADER, secret);
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("GET {} returned {}: {}", url, status, body));
        }

        Self::parse_envelope(&body)
    }

    async fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let mut request = self
            .client
            .post(self.url(path))
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .json(body);

        if let Some(secret) = self.internal_secret.as_deref() {
            request = request.header(INTERNAL_SECRET_HEADER, secret);
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("{} returned {}: {}", path, status, body));
        }

        Self::parse_envelope(&body)
    }
}

impl ExecutionTaskReportRequest {
    pub fn success(
        task_id: i64,
        exchange: impl Into<String>,
        external_order_id: impl Into<String>,
        order_side: impl Into<String>,
        order_status: impl Into<String>,
        raw_payload: Value,
    ) -> Self {
        Self {
            task_id,
            execution_status: "completed".to_string(),
            exchange: exchange.into(),
            external_order_id: external_order_id.into(),
            order_side: order_side.into(),
            order_status: order_status.into(),
            filled_qty: None,
            filled_quote: None,
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: None,
            raw_payload_json: Some(raw_payload.to_string()),
        }
    }

    pub fn failed(
        task_id: i64,
        exchange: impl Into<String>,
        order_side: impl Into<String>,
        message: impl Into<String>,
        raw_payload: Value,
    ) -> Self {
        Self {
            task_id,
            execution_status: "failed".to_string(),
            exchange: exchange.into(),
            external_order_id: format!("failed-task-{task_id}"),
            order_side: order_side.into(),
            order_status: "failed".to_string(),
            filled_qty: None,
            filled_quote: None,
            fee_amount: None,
            profit_usdt: None,
            executed_at: None,
            error_message: Some(message.into()),
            raw_payload_json: Some(raw_payload.to_string()),
        }
    }
}

fn deserialize_json_value_from_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Value, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(raw) => serde_json::from_str(&raw).map_err(serde::de::Error::custom),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_lease_request_without_extra_noise() {
        let request = ExecutionTaskLeaseRequest {
            worker_id: "worker-a".to_string(),
            limit: 10,
            task_types: vec![],
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["worker_id"], "worker-a");
        assert_eq!(value["limit"], 10);
        assert!(value.get("task_types").is_none());
    }

    #[test]
    fn lease_url_matches_quant_web_internal_contract() {
        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();

        assert_eq!(
            client.lease_url(25),
            "https://quant-web.example/api/commerce/internal/execution-tasks/lease?limit=25"
        );
    }

    #[test]
    fn parses_execution_task_envelope_from_quant_web() {
        let body = r#"{
            "success": true,
            "data": {
                "tasks": [{
                    "id": 42,
                    "news_signal_id": 7,
                    "combo_id": 9,
                    "buyer_email": "buyer@example.com",
                    "strategy_slug": "news_momentum",
                    "symbol": "BTC-USDT-SWAP",
                    "task_type": "execute_signal",
                    "task_status": "pending",
                    "priority": 3,
                    "lease_owner": null,
                    "lease_until": null,
                    "scheduled_at": "2026-04-23T12:00:00",
                    "request_payload_json": "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}",
                    "created_at": "2026-04-23T12:00:00",
                    "updated_at": "2026-04-23T12:00:00"
                }]
            }
        }"#;

        let parsed: ExecutionTaskLease = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(parsed.tasks[0].id, 42);
        assert_eq!(parsed.tasks[0].buyer_email, "buyer@example.com");
        assert_eq!(parsed.tasks[0].request_payload_json["size"], "0.01");
    }

    #[test]
    fn parses_strategy_execution_task_with_nullable_news_signal_id() {
        let body = r#"{
            "success": true,
            "data": {
                "items": [{
                    "task": {
                        "id": 43,
                        "news_signal_id": null,
                        "strategy_signal_id": 11,
                        "combo_id": 9,
                        "buyer_email": "buyer@example.com",
                        "strategy_slug": "vegas",
                        "symbol": "BTC-USDT-SWAP",
                        "task_type": "execute_signal",
                        "task_status": "leased",
                        "priority": 3,
                        "lease_owner": "rust_quant",
                        "lease_until": "2026-04-23T12:02:00",
                        "scheduled_at": "2026-04-23T12:00:00",
                        "request_payload_json": "{\"source_signal_type\":\"technical_strategy\",\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}",
                        "created_at": "2026-04-23T12:00:00",
                        "updated_at": "2026-04-23T12:00:00"
                    },
                    "api_credentials": []
                }]
            }
        }"#;

        let parsed: ExecutionTaskLease = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(parsed.tasks[0].id, 43);
        assert_eq!(parsed.tasks[0].news_signal_id, None);
        assert_eq!(parsed.tasks[0].strategy_signal_id, Some(11));
        assert_eq!(
            parsed.tasks[0].request_payload_json["source_signal_type"],
            "technical_strategy"
        );
    }

    #[test]
    fn strategy_signal_request_matches_quant_web_contract() {
        let request = StrategySignalSubmitRequest {
            source: "rust_quant".to_string(),
            external_id: "vegas-BTC-1713864000".to_string(),
            strategy_slug: "vegas".to_string(),
            strategy_key: "vegas_1h".to_string(),
            symbol: "BTC-USDT-SWAP".to_string(),
            signal_type: "entry".to_string(),
            direction: "long".to_string(),
            title: "Vegas long signal".to_string(),
            summary: Some("EMA alignment confirmed".to_string()),
            confidence: Some(0.82),
            payload_json: "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}".to_string(),
            generated_at: Some("2026-04-23T12:00:00Z".to_string()),
        };
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["source"], "rust_quant");
        assert_eq!(value["strategy_slug"], "vegas");
        assert_eq!(value["strategy_key"], "vegas_1h");
        assert_eq!(value["direction"], "long");
        assert_eq!(
            value["payload_json"],
            "{\"exchange\":\"okx\",\"side\":\"buy\",\"size\":\"0.01\"}"
        );

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "https://quant-web.example/".to_string(),
            internal_secret: "secret".to_string(),
        })
        .unwrap();
        assert_eq!(
            client.strategy_signal_url(),
            "https://quant-web.example/api/commerce/internal/strategy-signals"
        );
    }

    #[test]
    fn parses_execution_task_items_envelope_from_quant_web() {
        let body = r#"{
            "success": true,
            "data": {
                "items": [{
                    "task": {
                        "id": 42,
                        "news_signal_id": 7,
                        "combo_id": 9,
                        "buyer_email": "buyer@example.com",
                        "strategy_slug": "news_momentum",
                        "symbol": "BTC-USDT-SWAP",
                        "task_type": "execute_signal",
                        "task_status": "leased",
                        "priority": 3,
                        "lease_owner": "rust_quant",
                        "lease_until": "2026-04-23T12:02:00",
                        "scheduled_at": "2026-04-23T12:00:00",
                        "request_payload_json": "{\"signal_type\":\"buy\",\"payload_json\":\"{\\\"exchange\\\":\\\"okx\\\",\\\"side\\\":\\\"buy\\\",\\\"size\\\":\\\"0.01\\\"}\"}",
                        "created_at": "2026-04-23T12:00:00",
                        "updated_at": "2026-04-23T12:00:00"
                    },
                    "api_credentials": []
                }]
            }
        }"#;

        let parsed: ExecutionTaskLease = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(parsed.tasks.len(), 1);
        assert_eq!(parsed.tasks[0].id, 42);
        assert_eq!(parsed.tasks[0].task_status, "leased");
    }

    #[test]
    fn report_request_matches_quant_web_order_result_contract() {
        let request = ExecutionTaskReportRequest::success(
            42,
            "okx",
            "order-1",
            "buy",
            "filled",
            serde_json::json!({"dry_run": true}),
        );
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["task_id"], 42);
        assert_eq!(value["execution_status"], "completed");
        assert_eq!(value["external_order_id"], "order-1");
        assert_eq!(value["raw_payload_json"], "{\"dry_run\":true}");
        assert!(value.get("worker_id").is_none());
    }

    #[test]
    fn parses_user_exchange_config_envelope_without_persisting_secret() {
        let body = r#"{
            "success": true,
            "data": {
                "buyer_email": "buyer@example.com",
                "exchange": "OKX",
                "api_key": "api-key",
                "api_secret": "api-secret",
                "passphrase": "passphrase",
                "simulated": true
            }
        }"#;

        let config: UserExchangeConfig = ExecutionTaskClient::parse_envelope(body).unwrap();

        assert_eq!(config.exchange, "OKX");
        assert_eq!(config.api_key, "api-key");
        assert_eq!(config.passphrase.as_deref(), Some("passphrase"));
        assert!(config.simulated);
    }

    #[tokio::test]
    async fn resolve_user_exchange_config_uses_internal_get_contract() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            tx.send(request).unwrap();

            let body = r#"{"success":true,"data":{"buyer_email":"buyer@example.com","exchange":"OKX","api_key":"plain-api-key","api_secret":"plain-api-secret","passphrase":"plain-passphrase","simulated":false}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let client = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: format!("http://{}", addr),
            internal_secret: "dev-secret".to_string(),
        })
        .unwrap();
        let config = client
            .resolve_user_exchange_config("buyer@example.com", "OKX")
            .await
            .unwrap();

        server.await.unwrap();
        let request = rx.recv().unwrap();

        assert!(request.starts_with(
            "GET /api/commerce/internal/api-credentials/resolve?buyer_email=buyer%40example.com&exchange=OKX HTTP/1.1"
        ));
        assert!(request.contains("x-alpha-execution-secret: dev-secret"));
        assert_eq!(config.buyer_email, "buyer@example.com");
        assert_eq!(config.api_key, "plain-api-key");
        assert_eq!(config.api_secret, "plain-api-secret");
        assert_eq!(config.passphrase.as_deref(), Some("plain-passphrase"));
        assert!(!config.simulated);
    }

    #[test]
    fn validates_base_url() {
        let err = ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: "   ".to_string(),
            internal_secret: "secret".to_string(),
        })
        .expect_err("empty base_url must fail");

        assert!(err.to_string().contains("base_url"));
    }
}
