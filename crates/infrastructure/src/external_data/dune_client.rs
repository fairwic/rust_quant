use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const DEFAULT_DUNE_API_BASE_URL: &str = "https://api.dune.com/api/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuneExecutionState {
    Pending,
    Executing,
    Completed,
    CompletedPartial,
    Failed,
    Canceled,
    Expired,
    Unknown(String),
}

impl DuneExecutionState {
    fn from_api(value: &str) -> Self {
        match value {
            "QUERY_STATE_PENDING" => Self::Pending,
            "QUERY_STATE_EXECUTING" => Self::Executing,
            "QUERY_STATE_COMPLETED" => Self::Completed,
            "QUERY_STATE_COMPLETED_PARTIAL" => Self::CompletedPartial,
            "QUERY_STATE_FAILED" => Self::Failed,
            "QUERY_STATE_CANCELED" => Self::Canceled,
            "QUERY_STATE_EXPIRED" => Self::Expired,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::CompletedPartial
                | Self::Failed
                | Self::Canceled
                | Self::Expired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuneQueryPerformance {
    Medium,
    Large,
}

impl DuneQueryPerformance {
    fn as_str(self) -> &'static str {
        match self {
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuneExecutionResponse {
    pub execution_id: String,
    pub state: DuneExecutionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuneExecutionStatusResponse {
    pub execution_id: String,
    pub query_id: i64,
    pub is_execution_finished: bool,
    pub state: DuneExecutionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuneExecutionResultsResponse {
    pub execution_id: String,
    pub query_id: Option<i64>,
    pub is_execution_finished: bool,
    pub state: DuneExecutionState,
    pub rows: Vec<Value>,
}

pub struct DuneApiClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl DuneApiClient {
    pub fn new(api_key: String) -> Result<Self> {
        Self::with_base_url(api_key, DEFAULT_DUNE_API_BASE_URL.to_string())
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("DUNE_API_KEY")
            .map_err(|_| anyhow!("missing DUNE_API_KEY environment variable"))?;
        let base_url = std::env::var("DUNE_API_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_DUNE_API_BASE_URL.to_string());
        Self::with_base_url(api_key, base_url)
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            client,
            api_key,
            base_url,
        })
    }

    pub async fn execute_sql(
        &self,
        sql: &str,
        performance: DuneQueryPerformance,
    ) -> Result<DuneExecutionResponse> {
        let value = self
            .client
            .post(format!("{}/sql/execute", self.base_url))
            .header("X-Dune-Api-Key", &self.api_key)
            .json(&json!({
                "sql": sql,
                "performance": performance.as_str()
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        Self::parse_execute_sql_response(&value)
    }

    pub async fn get_execution_status(
        &self,
        execution_id: &str,
    ) -> Result<DuneExecutionStatusResponse> {
        let value = self
            .client
            .get(format!(
                "{}/execution/{}/status",
                self.base_url, execution_id
            ))
            .header("X-Dune-Api-Key", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        Self::parse_execution_status_response(&value)
    }

    pub async fn get_execution_results(
        &self,
        execution_id: &str,
        allow_partial_results: bool,
    ) -> Result<DuneExecutionResultsResponse> {
        let value = self
            .client
            .get(format!(
                "{}/execution/{}/results",
                self.base_url, execution_id
            ))
            .query(&[("allow_partial_results", allow_partial_results)])
            .header("X-Dune-Api-Key", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        Self::parse_execution_results_response(&value)
    }

    pub async fn run_sql(
        &self,
        sql: &str,
        performance: DuneQueryPerformance,
        poll_interval: Duration,
        max_polls: usize,
    ) -> Result<DuneExecutionResultsResponse> {
        let execution = self.execute_sql(sql, performance).await?;
        let execution_id = execution.execution_id;

        for _ in 0..max_polls {
            let status = self.get_execution_status(&execution_id).await?;
            if status.state.is_terminal() {
                if matches!(
                    status.state,
                    DuneExecutionState::Completed | DuneExecutionState::CompletedPartial
                ) {
                    return self.get_execution_results(&execution_id, true).await;
                }
                return Err(anyhow!(
                    "dune execution {} ended in state {:?}",
                    execution_id,
                    status.state
                ));
            }
            tokio::time::sleep(poll_interval).await;
        }

        Err(anyhow!(
            "dune execution {} did not finish in time",
            execution_id
        ))
    }

    pub fn parse_execute_sql_response(value: &Value) -> Result<DuneExecutionResponse> {
        Ok(DuneExecutionResponse {
            execution_id: value
                .get("execution_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing execution_id"))?
                .to_string(),
            state: DuneExecutionState::from_api(
                value
                    .get("state")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("missing state"))?,
            ),
        })
    }

    pub fn parse_execution_status_response(value: &Value) -> Result<DuneExecutionStatusResponse> {
        Ok(DuneExecutionStatusResponse {
            execution_id: value
                .get("execution_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing execution_id"))?
                .to_string(),
            query_id: value
                .get("query_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| anyhow!("missing query_id"))?,
            is_execution_finished: value
                .get("is_execution_finished")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow!("missing is_execution_finished"))?,
            state: DuneExecutionState::from_api(
                value
                    .get("state")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("missing state"))?,
            ),
        })
    }

    pub fn parse_execution_results_response(value: &Value) -> Result<DuneExecutionResultsResponse> {
        let rows = value
            .get("result")
            .and_then(|result| result.get("rows"))
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("missing result.rows"))?
            .to_vec();

        Ok(DuneExecutionResultsResponse {
            execution_id: value
                .get("execution_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing execution_id"))?
                .to_string(),
            query_id: value.get("query_id").and_then(Value::as_i64),
            is_execution_finished: value
                .get("is_execution_finished")
                .and_then(Value::as_bool)
                .ok_or_else(|| anyhow!("missing is_execution_finished"))?,
            state: DuneExecutionState::from_api(
                value
                    .get("state")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("missing state"))?,
            ),
            rows,
        })
    }
}
