use anyhow::{Context, Result};
use chrono::Utc;
use rust_quant_services::market::{
    parse_exchange_symbol_sync_sources, ExchangeSymbolAssetIconCandidate,
    ExchangeSymbolSyncService, MajorExchangeListingSignal,
};
use rust_quant_services::rust_quan_web::{
    ExecutionTaskClient, ExecutionTaskConfig, StrategySignalSubmitRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::collections::BTreeSet;
use tracing::{info, warn};
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeSymbolSyncRequest {
    #[serde(default)]
    /// 列表数据。
    pub sources: Option<Vec<String>>,
    #[serde(default)]
    /// trigger来源；为空时使用默认值或表示不限制。
    pub trigger_source: Option<String>,
    #[serde(default)]
    /// 是否提交信号；为空时使用默认值。
    pub submit_signals: Option<bool>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeSymbolSyncSourceReport {
    /// 数据来源。
    pub source: String,
    /// persisted数据行，用于展示或持久化查询结果。
    pub persisted_rows: usize,
    /// 首次seen数据行，用于展示或持久化查询结果。
    pub first_seen_rows: usize,
    /// majorlistingsignals，用于展示或持久化查询结果。
    pub major_listing_signals: usize,
    /// 资产iconcandidates，用于展示或持久化查询结果。
    pub asset_icon_candidates: usize,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeSymbolSyncResponse {
    /// run ID。
    pub run_id: String,
    /// 当前状态。
    pub status: String,
    /// trigger来源，用于返回接口响应。
    pub trigger_source: String,
    /// 列表数据。
    pub requested_sources: Vec<String>,
    /// persisted数据行，用于返回接口响应。
    pub persisted_rows: usize,
    /// 首次seen数据行，用于返回接口响应。
    pub first_seen_rows: usize,
    /// majorlistingsignals，用于返回接口响应。
    pub major_listing_signals: usize,
    /// 资产iconcandidates，用于返回接口响应。
    pub asset_icon_candidates: usize,
    /// 列表数据。
    pub source_reports: Vec<ExchangeSymbolSyncSourceReport>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetIconRegistrySyncRequest {
    /// 数据来源。
    source: String,
    /// 列表数据。
    assets: Vec<AssetIconRegistrySyncAsset>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetIconRegistrySyncAsset {
    /// 资产类别，用于当前结构体的业务数据。
    asset_class: String,
    /// 基础资产，用于当前结构体的业务数据。
    base_asset: String,
    /// 交易对或资产符号。
    symbol: String,
    /// 交易所名称。
    exchange: String,
    /// 类型标识。
    market_type: String,
    /// 计价资产，用于当前结构体的业务数据。
    quote_asset: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetIconRegistryConfig {
    /// 基础URL，用于配置运行参数。
    base_url: String,
    /// internalSecret，用于配置运行参数。
    internal_secret: String,
}
const ASSET_ICON_REGISTRY_SYNC_PATH: &str = "/api/internal/asset-icons/crypto/sync";
const INTERNAL_SECRET_HEADER: &str = "x-alpha-execution-secret";
/// 封装当前函数，减少量化核心调用方重复实现相同细节。
/// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
pub async fn run_exchange_symbol_sync_from_env(
    request: ExchangeSymbolSyncRequest,
) -> Result<ExchangeSymbolSyncResponse> {
    let trigger_source = request
        .trigger_source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("cli")
        .to_ascii_lowercase();
    let sources = sources_from_request(request.sources)?;
    let submit_signals = request
        .submit_signals
        .unwrap_or_else(|| env_is_true("EXCHANGE_LISTING_SIGNAL_SUBMIT"));
    run_exchange_symbol_sync(&sources, &trigger_source, submit_signals).await
}
/// 封装来源来源请求，减少量化核心调用方重复实现相同细节。
fn sources_from_request(sources: Option<Vec<String>>) -> Result<Vec<String>> {
    if let Some(sources) = sources {
        return parse_exchange_symbol_sync_sources(Some(&sources.join(" ")));
    }
    let env_sources = std::env::var("EXCHANGE_SYMBOL_SOURCES").ok();
    parse_exchange_symbol_sync_sources(env_sources.as_deref())
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn run_exchange_symbol_sync(
    sources: &[String],
    trigger_source: &str,
    submit_signals: bool,
) -> Result<ExchangeSymbolSyncResponse> {
    let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
        .context("exchange symbol sync requires QUANT_CORE_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("connect quant_core Postgres for exchange symbol sync run log")?;
    let run_id = format!(
        "exchange-symbol-sync-{}-{}",
        trigger_source,
        Utc::now().timestamp_millis()
    );
    insert_sync_run(&pool, &run_id, trigger_source, sources).await?;
    let response = match execute_sync_sources(sources, trigger_source, submit_signals).await {
        Ok(mut response) => {
            response.run_id = run_id.clone();
            finish_sync_run_success(&pool, &run_id, &response).await?;
            response
        }
        Err(error) => {
            let error_message = error.to_string();
            finish_sync_run_failed(&pool, &run_id, &error_message).await?;
            return Err(error);
        }
    };
    Ok(response)
}
/// 执行 量化核心 主流程，并把外部依赖调用、状态推进和错误返回串起来。
async fn execute_sync_sources(
    sources: &[String],
    trigger_source: &str,
    submit_signals: bool,
) -> Result<ExchangeSymbolSyncResponse> {
    let service = ExchangeSymbolSyncService::from_env().await?;
    let mut source_reports = Vec::new();
    let mut all_signals = Vec::new();
    let mut asset_icon_candidates = Vec::new();
    let mut persisted_rows = 0usize;
    let mut first_seen_rows = 0usize;
    for source in sources {
        let report = service
            .sync_source_with_report(source)
            .await
            .with_context(|| format!("sync exchange symbols failed: source={source}"))?;
        persisted_rows += report.persisted_count;
        first_seen_rows += report.first_seen_count;
        all_signals.extend(report.major_listing_signals.clone());
        asset_icon_candidates.extend(report.asset_candidates.clone());
        source_reports.push(ExchangeSymbolSyncSourceReport {
            source: source.clone(),
            persisted_rows: report.persisted_count,
            first_seen_rows: report.first_seen_count,
            major_listing_signals: report.major_listing_signals.len(),
            asset_icon_candidates: report.asset_candidates.len(),
        });
    }
    if submit_signals {
        submit_major_listing_signals(&all_signals).await?;
    }
    let asset_icon_candidates = dedupe_asset_icon_candidates(asset_icon_candidates);
    notify_asset_icon_registry_if_configured(&asset_icon_candidates).await;
    Ok(ExchangeSymbolSyncResponse {
        run_id: String::new(),
        status: "success".to_string(),
        trigger_source: trigger_source.to_string(),
        requested_sources: sources.to_vec(),
        persisted_rows,
        first_seen_rows,
        major_listing_signals: all_signals.len(),
        asset_icon_candidates: asset_icon_candidates.len(),
        source_reports,
    })
}
/// 持久化 量化核心 结果，保证写入路径和幂等语义集中处理。
async fn insert_sync_run(
    pool: &PgPool,
    run_id: &str,
    trigger_source: &str,
    sources: &[String],
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO exchange_symbol_sync_runs (
            run_id,
            trigger_source,
            requested_sources,
            run_status,
            started_at
        )
        VALUES ($1, $2, $3, 'running', NOW())
        "#,
    )
    .bind(run_id)
    .bind(trigger_source)
    .bind(sources)
    .execute(pool)
    .await
    .context("insert exchange_symbol_sync_runs running row")?;
    Ok(())
}
/// 执行结束同步运行成功步骤，串起量化核心需要的状态推进和错误处理。
async fn finish_sync_run_success(
    pool: &PgPool,
    run_id: &str,
    response: &ExchangeSymbolSyncResponse,
) -> Result<()> {
    let report_json = serde_json::to_value(response).context("serialize exchange sync report")?;
    sqlx::query(
        r#"
        UPDATE exchange_symbol_sync_runs
        SET
            run_status = 'success',
            finished_at = NOW(),
            duration_ms = ROUND(EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000)::INTEGER,
            persisted_rows = $2,
            first_seen_rows = $3,
            major_listing_signals = $4,
            error_message = '',
            report_json = $5,
            updated_at = NOW()
        WHERE run_id = $1
        "#,
    )
    .bind(run_id)
    .bind(response.persisted_rows as i32)
    .bind(response.first_seen_rows as i32)
    .bind(response.major_listing_signals as i32)
    .bind(report_json)
    .execute(pool)
    .await
    .context("update exchange_symbol_sync_runs success row")?;
    Ok(())
}
/// 执行结束同步运行失败步骤，串起量化核心需要的状态推进和错误处理。
async fn finish_sync_run_failed(pool: &PgPool, run_id: &str, error_message: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE exchange_symbol_sync_runs
        SET
            run_status = 'failed',
            finished_at = NOW(),
            duration_ms = ROUND(EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000)::INTEGER,
            error_message = $2,
            updated_at = NOW()
        WHERE run_id = $1
        "#,
    )
    .bind(run_id)
    .bind(error_message)
    .execute(pool)
    .await
    .context("update exchange_symbol_sync_runs failed row")?;
    Ok(())
}
/// 执行提交主流上市信号步骤，串起量化核心需要的状态推进和错误处理。
async fn submit_major_listing_signals(signals: &[MajorExchangeListingSignal]) -> Result<()> {
    if signals.is_empty() {
        return Ok(());
    }
    let client = ExecutionTaskClient::new(quant_web_execution_task_config_from_env()?)?;
    for signal in signals {
        let request = build_major_listing_strategy_signal_request(signal);
        let response = client
            .submit_strategy_signal(request)
            .await
            .with_context(|| {
                format!(
                    "submit major exchange listing signal failed: {} {}",
                    signal.exchange, signal.normalized_symbol
                )
            })?;
        info!(
            "submitted major exchange listing signal: exchange={}, symbol={}, generated_tasks={}",
            signal.exchange,
            signal.normalized_symbol,
            response.generated_tasks.len()
        );
    }
    Ok(())
}
/// 发送 量化核心 通知或请求，并把投递错误留在当前边界处理。
async fn notify_asset_icon_registry_if_configured(candidates: &[ExchangeSymbolAssetIconCandidate]) {
    if candidates.is_empty() {
        return;
    }
    let Some(config) = quant_web_asset_icon_registry_config_from_env() else {
        return;
    };
    if let Err(error) = sync_asset_icon_registry_candidates(&config, candidates).await {
        warn!(
            "sync exchange symbol asset icon candidates to quant_web failed: {}",
            error
        );
    }
}
/// 同步 量化核心 数据，保证本地状态与外部事实源保持一致。
async fn sync_asset_icon_registry_candidates(
    config: &AssetIconRegistryConfig,
    candidates: &[ExchangeSymbolAssetIconCandidate],
) -> Result<()> {
    let request = build_asset_icon_registry_sync_request(candidates);
    let url = asset_icon_registry_sync_url(&config.base_url)?;
    reqwest::Client::new()
        .post(url)
        .header(INTERNAL_SECRET_HEADER, &config.internal_secret)
        .json(&request)
        .send()
        .await?
        .error_for_status()
        .context("quant_web asset icon registry sync returned non-success status")?;
    info!(
        "synced exchange symbol asset icon candidates to quant_web: count={}",
        request.assets.len()
    );
    Ok(())
}
/// 构建 量化核心 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_asset_icon_registry_sync_request(
    candidates: &[ExchangeSymbolAssetIconCandidate],
) -> AssetIconRegistrySyncRequest {
    AssetIconRegistrySyncRequest {
        source: "exchange_symbol_sync".to_string(),
        assets: candidates
            .iter()
            .map(|candidate| AssetIconRegistrySyncAsset {
                asset_class: "crypto".to_string(),
                base_asset: candidate.base_asset.clone(),
                symbol: candidate.symbol.clone(),
                exchange: candidate.exchange.clone(),
                market_type: candidate.market_type.clone(),
                quote_asset: candidate.quote_asset.clone(),
            })
            .collect(),
    }
}
/// 提供dedupe资产图标candidates的集中实现，避免量化核心调用方重复处理相同细节。
fn dedupe_asset_icon_candidates(
    candidates: Vec<ExchangeSymbolAssetIconCandidate>,
) -> Vec<ExchangeSymbolAssetIconCandidate> {
    candidates
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
/// 提供quantweb资产图标注册表配置from环境变量的集中实现，避免量化核心调用方重复处理相同细节。
fn quant_web_asset_icon_registry_config_from_env() -> Option<AssetIconRegistryConfig> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("ALPHA_EXECUTION_INTERNAL_SECRET"))
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match internal_secret {
        Some(internal_secret) => Some(AssetIconRegistryConfig {
            base_url,
            internal_secret,
        }),
        None => {
            warn!("skip quant_web asset icon registry sync: internal secret is not configured");
            None
        }
    }
}
/// 提供资产图标注册表同步URL的集中实现，避免量化核心调用方重复处理相同细节。
fn asset_icon_registry_sync_url(base_url: &str) -> Result<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        anyhow::bail!("quant_web base_url is empty");
    }
    Ok(format!("{base_url}{ASSET_ICON_REGISTRY_SYNC_PATH}"))
}
/// 构建 量化核心 请求或响应载荷，把字段组装规则集中在同一入口。
fn build_major_listing_strategy_signal_request(
    signal: &MajorExchangeListingSignal,
) -> StrategySignalSubmitRequest {
    let generated_at = Utc::now().to_rfc3339();
    StrategySignalSubmitRequest {
        source: "rust_quant".to_string(),
        external_id: format!(
            "exchange-listing:{}:{}",
            signal.exchange, signal.normalized_symbol
        ),
        strategy_slug: "event_exchange_listing".to_string(),
        strategy_key: "exchange_listing".to_string(),
        symbol: signal.normalized_symbol.clone(),
        signal_type: "buy".to_string(),
        direction: "long".to_string(),
        title: format!(
            "{} listed on {} after prior non-mainstream listings",
            signal.base_asset,
            signal.exchange.to_ascii_uppercase()
        ),
        summary: Some(format!(
            "{} was already listed on {}; {} listing is treated as a major bullish catalyst.",
            signal.base_asset,
            signal.prior_non_mainstream_exchanges.join(", "),
            signal.exchange.to_ascii_uppercase()
        )),
        confidence: Some(0.9),
        payload_json: json!({
            "event_class": "exchange_listing",
            "signal_source": "exchange_symbol_sync",
            "exchange": &signal.exchange,
            "preferred_exchanges": [&signal.exchange],
            "execution_symbol": &signal.normalized_symbol,
            "side": "buy",
            "order_type": "market",
            "auto_execution_allowed": true,
            "listing_catalyst": {
                "classification": "major_exchange_listing_with_prior_non_mainstream_history",
                "source_exchange": &signal.exchange,
                "execution_exchange": &signal.exchange,
                "listed_on_major_execution_exchange": true,
                "auto_execution_allowed": true
            },
            "prior_non_mainstream_exchanges": &signal.prior_non_mainstream_exchanges,
            "base_asset": &signal.base_asset,
            "quote_asset": &signal.quote_asset,
            "market_type": &signal.market_type,
            "exchange_symbol": &signal.exchange_symbol
        })
        .to_string(),
        generated_at: Some(generated_at),
    }
}
/// 提供quantweb执行task配置from环境变量的集中实现，避免量化核心调用方重复处理相同细节。
fn quant_web_execution_task_config_from_env() -> Result<ExecutionTaskConfig> {
    let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
        .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
        .context(
            "EXCHANGE_LISTING_SIGNAL_SUBMIT=1 requires RUST_QUAN_WEB_BASE_URL/QUANT_WEB_BASE_URL",
        )?;
    let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .unwrap_or_else(|_| {
            warn!("RUST_QUAN_WEB_INTERNAL_SECRET/EXECUTION_EVENT_SECRET is not set");
            String::new()
        });
    Ok(ExecutionTaskConfig {
        base_url,
        internal_secret,
    })
}
/// 封装环境变量istrue，减少量化核心调用方重复实现相同细节。
fn env_is_true(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn sources_from_request_normalizes_aliases_and_deduplicates() {
        let sources = sources_from_request(Some(vec![
            "binance_usdm".to_string(),
            "okx,gate".to_string(),
            "binance".to_string(),
        ]))
        .expect("sources");
        assert_eq!(sources, vec!["binance", "okx", "gate"]);
    }
    #[test]
    fn major_listing_strategy_signal_request_targets_event_strategy() {
        let signal = MajorExchangeListingSignal {
            exchange: "binance".to_string(),
            market_type: "perpetual".to_string(),
            exchange_symbol: "TESTUSDT".to_string(),
            normalized_symbol: "TEST-USDT-SWAP".to_string(),
            base_asset: "TEST".to_string(),
            quote_asset: "USDT".to_string(),
            prior_non_mainstream_exchanges: vec!["bitget".to_string()],
        };
        let request = build_major_listing_strategy_signal_request(&signal);
        let payload: Value = serde_json::from_str(&request.payload_json).expect("payload json");
        assert_eq!(request.strategy_slug, "event_exchange_listing");
        assert_eq!(request.strategy_key, "exchange_listing");
        assert_eq!(request.symbol, "TEST-USDT-SWAP");
        assert_eq!(request.signal_type, "buy");
        assert_eq!(request.direction, "long");
        assert_eq!(payload["exchange"], "binance");
        assert_eq!(payload["execution_symbol"], "TEST-USDT-SWAP");
        assert_eq!(payload["prior_non_mainstream_exchanges"], json!(["bitget"]));
        assert_eq!(
            payload["listing_catalyst"]["classification"],
            "major_exchange_listing_with_prior_non_mainstream_history"
        );
    }
    #[test]
    fn asset_icon_registry_sync_request_uses_exchange_symbol_candidates() {
        let request = build_asset_icon_registry_sync_request(&[ExchangeSymbolAssetIconCandidate {
            exchange: "gate".to_string(),
            market_type: "perpetual".to_string(),
            symbol: "LAYER-USDT-SWAP".to_string(),
            base_asset: "LAYER".to_string(),
            quote_asset: "USDT".to_string(),
        }]);
        let payload = serde_json::to_value(request).expect("payload json");
        assert_eq!(payload["source"], "exchange_symbol_sync");
        assert_eq!(payload["assets"][0]["assetClass"], "crypto");
        assert_eq!(payload["assets"][0]["baseAsset"], "LAYER");
        assert_eq!(payload["assets"][0]["symbol"], "LAYER-USDT-SWAP");
        assert_eq!(payload["assets"][0]["exchange"], "gate");
        assert_eq!(payload["assets"][0]["marketType"], "perpetual");
        assert_eq!(payload["assets"][0]["quoteAsset"], "USDT");
    }
    #[test]
    fn asset_icon_registry_sync_url_uses_internal_endpoint_path() {
        let url = asset_icon_registry_sync_url("http://127.0.0.1:8000/")
            .expect("sync url should be valid");
        assert_eq!(
            url,
            "http://127.0.0.1:8000/api/internal/asset-icons/crypto/sync"
        );
    }
    #[test]
    fn asset_icon_candidates_are_deduplicated_before_notify() {
        let candidates = dedupe_asset_icon_candidates(vec![
            ExchangeSymbolAssetIconCandidate {
                exchange: "gate".to_string(),
                market_type: "perpetual".to_string(),
                symbol: "LAYER-USDT-SWAP".to_string(),
                base_asset: "LAYER".to_string(),
                quote_asset: "USDT".to_string(),
            },
            ExchangeSymbolAssetIconCandidate {
                exchange: "gate".to_string(),
                market_type: "perpetual".to_string(),
                symbol: "LAYER-USDT-SWAP".to_string(),
                base_asset: "LAYER".to_string(),
                quote_asset: "USDT".to_string(),
            },
        ]);
        assert_eq!(candidates.len(), 1);
    }
}
