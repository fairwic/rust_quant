use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use rust_quant_core::database::get_db_pool;
use rust_quant_domain::entities::ExternalMarketSnapshot;
use rust_quant_domain::traits::ExternalMarketSnapshotRepository;
use rust_quant_infrastructure::{
    external_data::{DuneApiClient, DuneQueryPerformance},
    repositories::ShardedExternalMarketSnapshotRepository,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
const DUNE_SOURCE: &str = "dune";
#[async_trait]
pub trait DuneSqlRunner: Send + Sync {
    /// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn run_sql(&self, sql: &str, performance: DuneQueryPerformance) -> Result<Vec<Value>>;
}
#[async_trait]
impl DuneSqlRunner for DuneApiClient {
    /// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
    async fn run_sql(&self, sql: &str, performance: DuneQueryPerformance) -> Result<Vec<Value>> {
        let poll_interval = std::env::var("DUNE_SQL_POLL_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3000);
        let max_polls = std::env::var("DUNE_SQL_MAX_POLLS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(40);
        let response = self
            .run_sql(
                sql,
                performance,
                Duration::from_millis(poll_interval),
                max_polls,
            )
            .await?;
        Ok(response.rows)
    }
}
pub struct DuneMarketSyncService {
    /// repo，用于行情、K 线或市场扫描。
    repo: Arc<dyn ExternalMarketSnapshotRepository>,
    /// runner，用于行情、K 线或市场扫描。
    runner: Arc<dyn DuneSqlRunner>,
}
impl DuneMarketSyncService {
    /// 构建 行情与市场数据 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Result<Self> {
        let pool = get_db_pool().clone();
        let repo = Arc::new(ShardedExternalMarketSnapshotRepository::new(pool));
        let runner = Arc::new(DuneApiClient::from_env()?);
        Ok(Self { repo, runner })
    }
    pub fn with_repo_and_runner(
        repo: Arc<dyn ExternalMarketSnapshotRepository>,
        runner: Arc<dyn DuneSqlRunner>,
    ) -> Self {
        Self { repo, runner }
    }
    /// 生成 行情与市场数据 需要的派生数据，供后续执行、展示或审计使用。
    pub fn render_sql_template(template: &str, params: &HashMap<String, String>) -> String {
        params
            .iter()
            .fold(template.to_string(), |sql, (key, value)| {
                sql.replace(&format!("{{{{{}}}}}", key), value)
            })
    }
    /// 同步 行情与市场数据 数据，保证本地状态与外部事实源保持一致。
    pub async fn sync_template_file(
        &self,
        metric_type: String,
        symbol: String,
        template_path: &str,
        params: HashMap<String, String>,
        performance: DuneQueryPerformance,
    ) -> Result<usize> {
        let template = fs::read_to_string(template_path)?;
        self.sync_rendered_sql(metric_type, symbol, template, params, performance)
            .await
    }
    /// 同步 行情与市场数据 数据，保证本地状态与外部事实源保持一致。
    pub async fn sync_rendered_sql(
        &self,
        metric_type: String,
        symbol: String,
        sql_template: String,
        params: HashMap<String, String>,
        performance: DuneQueryPerformance,
    ) -> Result<usize> {
        let sql = Self::render_sql_template(&sql_template, &params);
        let rows = self.runner.run_sql(&sql, performance).await?;
        let snapshots = self.dune_rows_to_snapshots(&metric_type, &symbol, rows)?;
        let count = snapshots.len();
        self.repo.save_batch(snapshots).await?;
        Ok(count)
    }
    /// 提供dunerowstosnapshots的集中实现，避免行情数据调用方重复处理相同细节。
    fn dune_rows_to_snapshots(
        &self,
        metric_type: &str,
        symbol: &str,
        rows: Vec<Value>,
    ) -> Result<Vec<ExternalMarketSnapshot>> {
        rows.into_iter()
            .map(|row| Self::dune_row_to_snapshot(metric_type, symbol, row))
            .collect()
    }
    /// 提供dune数据行to快照的集中实现，避免行情数据调用方重复处理相同细节。
    fn dune_row_to_snapshot(
        metric_type: &str,
        symbol: &str,
        row: Value,
    ) -> Result<ExternalMarketSnapshot> {
        let metric_time = parse_metric_time(&row)?;
        let mut snapshot = ExternalMarketSnapshot::new(
            DUNE_SOURCE.to_string(),
            symbol.to_string(),
            metric_type.to_string(),
            metric_time,
        );
        snapshot.funding_rate = extract_f64(&row, &["funding_rate", "funding"]);
        snapshot.open_interest = extract_f64(&row, &["open_interest", "open_interest_usd"]);
        snapshot.long_short_ratio = extract_f64(&row, &["long_short_ratio"]);
        snapshot.premium = match extract_f64(&row, &["premium"]) {
            Some(value) => Some(value),
            None => extract_f64(&row, &["premium_bps"]).map(|bps| bps / 10000.0),
        };
        snapshot.raw_payload = Some(row);
        Ok(snapshot)
    }
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn parse_metric_time(row: &Value) -> Result<i64> {
    if let Some(ts) = row.get("metric_time").and_then(Value::as_i64) {
        return Ok(ts);
    }
    let raw = row
        .get("hour_bucket")
        .or_else(|| row.get("block_time"))
        .or_else(|| row.get("time"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing metric time field"))?;
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.timestamp_millis());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp_millis());
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.3f UTC") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).timestamp_millis());
    }
    Err(anyhow!("unsupported metric time format: {}", raw))
}
/// 解析输入参数并收敛为 行情与市场数据 可使用的结构化值。
fn extract_f64(row: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        let Some(value) = row.get(*key) else {
            continue;
        };
        match value {
            Value::Number(number) => {
                if let Some(parsed) = number.as_f64() {
                    return Some(parsed);
                }
            }
            Value::String(text) => {
                if let Ok(parsed) = text.parse::<f64>() {
                    return Some(parsed);
                }
            }
            _ => {}
        }
    }
    None
}
