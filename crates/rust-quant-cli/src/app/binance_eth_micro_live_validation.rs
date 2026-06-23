use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use crypto_exc_all::ExchangeId;
use rust_decimal::Decimal;
use rust_quant_services::exchange::CryptoExcAllGateway;
use rust_quant_services::rust_quan_web::{
    ApiCredentialCheckSummary, ExecutionTaskClient, ExecutionTaskConfig, ExecutionWorker,
    ExecutionWorkerConfig, PostgresExecutionAuditRepository,
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgPool};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

mod binance_futures_http;
mod env_override_guard;
#[cfg(test)]
mod tests;

use binance_futures_http::{
    available_usdt_balance, ensure_eth_position_flat, ensure_no_open_orders,
    symbol_config_leverage, BinanceFuturesHttp,
};
use env_override_guard::EnvOverrideGuard;

pub const BINANCE_ETH_MICRO_CONFIRM_TOKEN: &str = "I_UNDERSTAND_TINY_ETH_LIVE_ORDER";
pub const EXECUTION_WORKER_CONFIRM_TOKEN: &str = "I_UNDERSTAND_LIVE_ORDERS";
const DEFAULT_WEB_SYMBOL: &str = "ETH-USDT-SWAP";
const DEFAULT_EXCHANGE_SYMBOL: &str = "ETHUSDT";
const DEFAULT_QTY: &str = "0.010";
const DEFAULT_STOP_LOSS_BPS: i64 = 200;
const DEFAULT_MARGIN_BUFFER_BPS: i64 = 12_000;
const DEFAULT_BINANCE_FAPI_BASE_URL: &str = "https://fapi.binance.com";

#[derive(Debug, Clone, PartialEq)]
pub struct BinanceEthMicroLiveValidationConfig {
    pub web_database_url: String,
    pub quant_core_database_url: String,
    pub web_base_url: String,
    pub internal_secret: String,
    pub buyer_email: String,
    pub strategy_slug: String,
    pub strategy_key: String,
    pub web_symbol: String,
    pub qty: Decimal,
    pub credential_id: Option<i64>,
    pub combo_id: Option<i64>,
    pub stop_loss_price: Option<Decimal>,
    pub stop_loss_bps: i64,
    pub margin_buffer_bps: i64,
    pub binance_fapi_base_url: String,
    pub proxy_url: Option<String>,
    pub apply: bool,
    pub confirm: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinancePositionMode {
    OneWay,
    Hedge,
}

impl BinancePositionMode {
    /// 返回 Binance 持仓模式在本地结果中的稳定字符串。
    fn as_str(self) -> &'static str {
        match self {
            Self::OneWay => "one_way",
            Self::Hedge => "hedge",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinanceSymbolFilters {
    /// 下单数量步长。
    pub quantity_step: Decimal,
    /// 最小下单数量。
    pub min_quantity: Decimal,
    /// 最小名义金额。
    pub min_notional: Decimal,
    /// 价格最小变动单位。
    pub price_tick: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinanceEthMicroPreparedPlan {
    /// Binance 交易所实际使用的合约交易对。
    pub exchange_symbol: String,
    /// Web 侧订阅和展示使用的交易对。
    pub web_symbol: String,
    /// 经过交易所数量精度处理后的下单数量。
    pub qty: Decimal,
    /// Binance 当前标记价。
    pub mark_price: Decimal,
    /// 本次计划订单的名义金额。
    pub notional: Decimal,
    /// 已通过价格精度处理的止损价格。
    pub stop_loss_price: Decimal,
    /// Binance 账户当前持仓模式。
    pub position_mode: BinancePositionMode,
    /// 合约账户可用 USDT 余额。
    pub available_usdt: Decimal,
    /// 交易所 symbol filters，用于数量、价格和名义金额校验。
    pub filters: BinanceSymbolFilters,
}

#[derive(Debug, Clone, FromRow)]
struct OrderResultRow {
    id: i64,
    external_order_id: String,
    order_side: String,
    order_status: String,
    filled_qty: Option<Decimal>,
    filled_quote: Option<Decimal>,
    raw_payload_json: String,
}
/// 从环境变量读取配置并启动 Binance ETH micro live validation。
pub async fn run_binance_eth_micro_live_validation_from_env() -> Result<Value> {
    run_binance_eth_micro_live_validation(binance_eth_micro_live_validation_config_from_env()?)
        .await
}
/// 汇总环境变量中的 Web、Core、Binance 和执行确认配置。
pub fn binance_eth_micro_live_validation_config_from_env(
) -> Result<BinanceEthMicroLiveValidationConfig> {
    let web_database_url = first_non_empty_env(&["WEB_DATABASE_URL", "QUANT_WEB_DATABASE_URL"])
        .context("WEB_DATABASE_URL or QUANT_WEB_DATABASE_URL is required")?;
    let quant_core_database_url = first_non_empty_env(&[
        "QUANT_CORE_DATABASE_URL",
        "POSTGRES_QUANT_CORE_DATABASE_URL",
    ])
    .context("QUANT_CORE_DATABASE_URL is required for live execution audit")?;
    let web_base_url = first_non_empty_env(&["RUST_QUAN_WEB_BASE_URL", "QUANT_WEB_BASE_URL"])
        .context("RUST_QUAN_WEB_BASE_URL is required")?;
    let internal_secret =
        first_non_empty_env(&["EXECUTION_EVENT_SECRET", "RUST_QUAN_WEB_INTERNAL_SECRET"])
            .context("EXECUTION_EVENT_SECRET is required")?;
    let buyer_email = first_non_empty_env(&["BINANCE_ETH_MICRO_BUYER_EMAIL"])
        .unwrap_or_else(|| "demo-exec-worker@example.com".to_string());
    let strategy_slug =
        first_non_empty_env(&["BINANCE_ETH_MICRO_STRATEGY_SLUG"]).unwrap_or_else(|| "vegas".into());
    let strategy_key = first_non_empty_env(&["BINANCE_ETH_MICRO_STRATEGY_KEY"])
        .unwrap_or_else(|| "vegas_eth_micro_live_validation".into());
    let web_symbol = normalize_eth_symbol(
        &first_non_empty_env(&["BINANCE_ETH_MICRO_SYMBOL"])
            .unwrap_or_else(|| DEFAULT_WEB_SYMBOL.to_string()),
    )?;
    Ok(BinanceEthMicroLiveValidationConfig {
        web_database_url,
        quant_core_database_url,
        web_base_url,
        internal_secret,
        buyer_email,
        strategy_slug,
        strategy_key,
        web_symbol,
        qty: parse_decimal_env("BINANCE_ETH_MICRO_QTY", DEFAULT_QTY)?,
        credential_id: parse_optional_i64_env("BINANCE_ETH_MICRO_CREDENTIAL_ID")?,
        combo_id: parse_optional_i64_env("BINANCE_ETH_MICRO_COMBO_ID")?,
        stop_loss_price: parse_optional_decimal_env("BINANCE_ETH_MICRO_STOP_LOSS_PRICE")?,
        stop_loss_bps: parse_i64_env("BINANCE_ETH_MICRO_STOP_LOSS_BPS", DEFAULT_STOP_LOSS_BPS)?,
        margin_buffer_bps: parse_i64_env(
            "BINANCE_ETH_MICRO_MARGIN_BUFFER_BPS",
            DEFAULT_MARGIN_BUFFER_BPS,
        )?,
        binance_fapi_base_url: first_non_empty_env(&["BINANCE_FAPI_BASE_URL"])
            .unwrap_or_else(|| DEFAULT_BINANCE_FAPI_BASE_URL.to_string()),
        proxy_url: parse_optional_proxy_env("BINANCE_PROXY_URL")?,
        apply: parse_bool_env("BINANCE_ETH_MICRO_LIVE_APPLY", false)?,
        confirm: first_non_empty_env(&["BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM"]),
    })
}
/// 执行 Binance ETH micro live validation 的准备、任务创建、worker 触发和结果校验。
pub async fn run_binance_eth_micro_live_validation(
    config: BinanceEthMicroLiveValidationConfig,
) -> Result<Value> {
    validate_config(&config)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.web_database_url)
        .await
        .context("connect quant_web database for Binance ETH micro live validation")?;
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url: config.web_base_url.clone(),
        internal_secret: config.internal_secret.clone(),
    })?;
    let credential_id = match config.credential_id {
        Some(id) => id,
        None => resolve_single_binance_credential_id(&pool, &config.buyer_email).await?,
    };
    let combo_id = match config.combo_id {
        Some(id) => id,
        None => resolve_single_eth_combo_id(&pool, &config).await?,
    };
    let credential_check = client.check_internal_api_credential(credential_id).await?;
    validate_credential_check(&credential_check)?;
    let user_config = client
        .resolve_user_exchange_config_for_credential(
            &config.buyer_email,
            ExchangeId::Binance.as_str(),
            credential_id,
        )
        .await?;
    let binance = BinanceFuturesHttp::new(&config, &user_config)?;
    let filters = binance
        .exchange_info_filters(DEFAULT_EXCHANGE_SYMBOL)
        .await?;
    let mark_price = binance.mark_price(DEFAULT_EXCHANGE_SYMBOL).await?;
    let account = binance
        .signed_get_json("/fapi/v2/account", Vec::new())
        .await?;
    let open_orders = binance
        .signed_get_json(
            "/fapi/v1/openOrders",
            vec![("symbol".to_string(), DEFAULT_EXCHANGE_SYMBOL.to_string())],
        )
        .await?;
    let symbol_config = binance
        .signed_get_json(
            "/fapi/v1/symbolConfig",
            vec![("symbol".to_string(), DEFAULT_EXCHANGE_SYMBOL.to_string())],
        )
        .await?;
    let position_mode = binance.position_mode().await?;
    let prepared = build_prepared_plan(
        &config,
        filters,
        mark_price,
        &account,
        &open_orders,
        &symbol_config,
        position_mode,
    )?;
    let base_report = json!({
        "status": if config.apply { "ready_to_apply" } else { "preflight_only" },
        "exchange": "binance",
        "symbol": config.web_symbol,
        "exchange_symbol": DEFAULT_EXCHANGE_SYMBOL,
        "buyer_email": config.buyer_email,
        "combo_id": combo_id,
        "api_credential_id": credential_id,
        "credential_check": {
            "status": credential_check.status,
            "last_check_code": credential_check.last_check_code,
            "execution_can_execute": credential_check.execution_readiness.can_execute,
        },
        "preflight": prepared_plan_json(&prepared),
        "mutation_allowed": config.apply,
        "rust_entrypoint": "cargo run -q -p rust-quant-cli --bin binance_eth_micro_live_validation",
        "shell_live_validation_deprecated": true,
    });
    if !config.apply {
        return Ok(base_report);
    }
    ensure_live_apply_confirmation(&config)?;
    let open_task_id = insert_open_task(&pool, &config, combo_id, credential_id, &prepared).await?;
    let open_worker_count =
        run_scoped_worker_once(&config, open_task_id, "execute_signal", "pending").await?;
    let open_result = verify_order_result(&pool, open_task_id, "buy", "open").await?;
    let protection_sync = verify_open_protection_sync(&open_result)?;
    let close_task_id = insert_close_task(
        &pool,
        &config,
        combo_id,
        credential_id,
        open_task_id,
        &prepared,
    )
    .await?;
    let close_worker_count = run_scoped_worker_once(
        &config,
        close_task_id,
        "risk_control_close_candidate",
        "pending_close",
    )
    .await?;
    let close_result = verify_order_result(&pool, close_task_id, "sell", "close").await?;
    let final_account = binance
        .signed_get_json("/fapi/v2/account", Vec::new())
        .await?;
    ensure_eth_position_flat(&final_account, DEFAULT_EXCHANGE_SYMBOL)
        .context("final ETHUSDT position flat check failed")?;
    let final_open_orders = binance
        .signed_get_json(
            "/fapi/v1/openOrders",
            vec![("symbol".to_string(), DEFAULT_EXCHANGE_SYMBOL.to_string())],
        )
        .await?;
    ensure_no_open_orders(&final_open_orders, DEFAULT_EXCHANGE_SYMBOL)
        .context("final ETHUSDT open-orders check failed")?;
    Ok(json!({
        "status": "completed",
        "exchange": "binance",
        "symbol": config.web_symbol,
        "exchange_symbol": DEFAULT_EXCHANGE_SYMBOL,
        "buyer_email": config.buyer_email,
        "combo_id": combo_id,
        "api_credential_id": credential_id,
        "preflight": prepared_plan_json(&prepared),
        "open": order_result_json(open_task_id, open_worker_count, &open_result),
        "open_protection_sync": protection_sync,
        "close": order_result_json(close_task_id, close_worker_count, &close_result),
        "final_eth_position": "flat",
        "final_eth_open_orders": "clear",
        "mutation_scope": "minimal_binance_eth_micro_open_then_reduce_only_close",
    }))
}
/// 构建开仓执行任务载荷，保留止损和交易所上下文。
pub fn build_open_task_payload(
    credential_id: i64,
    prepared: &BinanceEthMicroPreparedPlan,
) -> Value {
    let mut execution = json!({
        "exchange": "binance",
        "symbol": prepared.web_symbol,
        "side": "buy",
        "signal_type": "buy",
        "direction": "long",
        "order_type": "market",
        "size": decimal_string(prepared.qty),
        "qty": decimal_string(prepared.qty),
        "margin_coin": "USDT",
        "trade_side": "open",
        "protective_stop_loss_required": true,
    });
    if prepared.position_mode == BinancePositionMode::Hedge {
        execution["position_side"] = json!("LONG");
        execution["position_mode"] = json!("hedge");
    } else {
        execution["position_mode"] = json!("one_way");
    }
    json!({
        "source": "rust_quant_cli_binance_eth_micro_live_validation",
        "exchange": "binance",
        "symbol": prepared.web_symbol,
        "side": "buy",
        "signal_type": "buy",
        "direction": "long",
        "order_type": "market",
        "size": decimal_string(prepared.qty),
        "qty": decimal_string(prepared.qty),
        "margin_coin": "USDT",
        "trade_side": "open",
        "api_credential_id": credential_id,
        "protective_stop_loss_required": true,
        "execution": execution,
        "risk_plan": {
            "protective_stop_loss_required": true,
            "entry_price": decimal_string(prepared.mark_price),
            "selected_stop_loss_price": decimal_string(prepared.stop_loss_price),
            "direction": "long"
        },
        "validation": {
            "entrypoint": "binance_eth_micro_live_validation",
            "shell_live_validation_deprecated": true,
            "notional": decimal_string(prepared.notional),
            "position_mode": prepared.position_mode.as_str()
        }
    })
}
/// 构建平仓执行任务载荷，用于验证开仓后的关闭路径。
pub fn build_close_task_payload(
    credential_id: i64,
    source_open_task_id: i64,
    prepared: &BinanceEthMicroPreparedPlan,
) -> Value {
    let mut close_order = json!({
        "exchange": "binance",
        "symbol": prepared.web_symbol,
        "side": "sell",
        "order_type": "market",
        "size": decimal_string(prepared.qty),
        "qty": decimal_string(prepared.qty),
        "margin_coin": "USDT",
        "trade_side": "close"
    });
    if prepared.position_mode == BinancePositionMode::Hedge {
        close_order["position_side"] = json!("LONG");
    } else {
        close_order["reduce_only"] = json!(true);
    }
    json!({
        "source": "rust_quant_cli_binance_eth_micro_live_validation",
        "exchange": "binance",
        "symbol": prepared.web_symbol,
        "api_credential_id": credential_id,
        "source_open_task_id": source_open_task_id,
        "risk_control": {
            "action": "close_candidate",
            "reason": "minimal_binance_eth_micro_live_validation_reduce_only_close"
        },
        "manual_review": {
            "action": "close_candidate",
            "reviewed_by": "rust_quant_cli",
            "review_note": "authorized minimal ETH micro validation close"
        },
        "close_order": close_order,
        "validation": {
            "entrypoint": "binance_eth_micro_live_validation",
            "position_mode": prepared.position_mode.as_str()
        }
    })
}
/// 构建 scoped worker 配置，限制本次验证只处理指定任务。
pub fn build_worker_config(
    task_id: i64,
    task_type: &str,
    task_status: &str,
) -> Result<ExecutionWorkerConfig> {
    if task_id <= 0 {
        bail!("scoped live worker task_id must be positive");
    }
    Ok(ExecutionWorkerConfig {
        worker_id: "binance_eth_micro_live_validation".to_string(),
        lease_limit: 1,
        dry_run: false,
        default_exchange: ExchangeId::Binance,
        task_types: vec![task_type.to_string()],
        task_statuses: vec![task_status.to_string()],
        target_task_ids: vec![task_id],
        confirmation_mode: false,
        report_replay_mode: false,
        report_replay_max_per_run: 1,
        report_replay_failure_backoff_seconds: 300,
        report_replay_throttle_ms: 0,
    })
}
/// 生成 worker 运行所需环境变量清单。
pub fn build_worker_env_manifest(
    task_id: i64,
    task_type: &str,
    task_status: &str,
) -> Result<BTreeMap<&'static str, String>> {
    if task_id <= 0 {
        bail!("scoped live worker task_id must be positive");
    }
    Ok(BTreeMap::from([
        ("EXECUTION_WORKER_DRY_RUN", "false".to_string()),
        ("EXECUTION_WORKER_DEFAULT_EXCHANGE", "binance".to_string()),
        ("EXECUTION_WORKER_LEASE_LIMIT", "1".to_string()),
        ("EXECUTION_WORKER_TARGET_TASK_IDS", task_id.to_string()),
        ("EXECUTION_WORKER_TASK_TYPES", task_type.to_string()),
        ("EXECUTION_WORKER_TASK_STATUSES", task_status.to_string()),
        (
            "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
            EXECUTION_WORKER_CONFIRM_TOKEN.to_string(),
        ),
    ]))
}
/// 校验 live validation 配置，避免缺少库连接、密钥或确认参数。
fn validate_config(config: &BinanceEthMicroLiveValidationConfig) -> Result<()> {
    if config.buyer_email.trim().is_empty() {
        bail!("BINANCE_ETH_MICRO_BUYER_EMAIL must not be empty");
    }
    normalize_eth_symbol(&config.web_symbol)?;
    if config.qty <= Decimal::ZERO {
        bail!("BINANCE_ETH_MICRO_QTY must be positive");
    }
    if !(1..10_000).contains(&config.stop_loss_bps) {
        bail!("BINANCE_ETH_MICRO_STOP_LOSS_BPS must be in 1..10000");
    }
    if config.margin_buffer_bps < 10_000 {
        bail!("BINANCE_ETH_MICRO_MARGIN_BUFFER_BPS must be >= 10000");
    }
    Ok(())
}
/// 校验 Web 凭证检查结果，确保 Binance 凭证已通过执行前置检查。
fn validate_credential_check(check: &ApiCredentialCheckSummary) -> Result<()> {
    if !check.execution_readiness.can_execute {
        bail!(
            "credential {} is not execution-ready: {:?}",
            check.id,
            check.execution_readiness.blocker_code
        );
    }
    if !check.status.eq_ignore_ascii_case("active") {
        bail!(
            "credential {} status is not active: {}",
            check.id,
            check.status
        );
    }
    let code = check.last_check_code.as_deref().unwrap_or_default();
    if !matches!(
        code,
        "signed_exchange_preflight_passed" | "signed_exchange_check_passed"
    ) {
        bail!(
            "credential {} last_check_code is not signed-ready: {}",
            check.id,
            code
        );
    }
    Ok(())
}
/// 读取交易所账户和 symbol filters，生成可提交前的订单计划。
fn build_prepared_plan(
    config: &BinanceEthMicroLiveValidationConfig,
    filters: BinanceSymbolFilters,
    mark_price: Decimal,
    account: &Value,
    open_orders: &Value,
    symbol_config: &Value,
    position_mode: BinancePositionMode,
) -> Result<BinanceEthMicroPreparedPlan> {
    if mark_price <= Decimal::ZERO {
        bail!("Binance ETH mark price must be positive");
    }
    let qty = quantize_down(config.qty, filters.quantity_step);
    if qty <= Decimal::ZERO || qty < filters.min_quantity {
        bail!(
            "ETH micro quantity {} is below Binance min quantity {} after rounding",
            decimal_string(qty),
            decimal_string(filters.min_quantity)
        );
    }
    let notional = qty * mark_price;
    if notional < filters.min_notional {
        bail!(
            "ETH micro notional {} is below Binance minNotional {}",
            decimal_string(notional),
            decimal_string(filters.min_notional)
        );
    }
    ensure_eth_position_flat(account, DEFAULT_EXCHANGE_SYMBOL)?;
    ensure_no_open_orders(open_orders, DEFAULT_EXCHANGE_SYMBOL)?;
    let available_usdt = available_usdt_balance(account)?;
    let leverage = symbol_config_leverage(symbol_config, DEFAULT_EXCHANGE_SYMBOL)?;
    let required_margin =
        notional / leverage * Decimal::from(config.margin_buffer_bps) / Decimal::from(10_000);
    if available_usdt < required_margin {
        bail!(
            "available USDT {} is below required buffered initial margin {}",
            decimal_string(available_usdt),
            decimal_string(required_margin)
        );
    }
    let stop_loss_price = match config.stop_loss_price {
        Some(price) => price,
        None => mark_price * Decimal::from(10_000 - config.stop_loss_bps) / Decimal::from(10_000),
    };
    let stop_loss_price = quantize_down(stop_loss_price, filters.price_tick);
    if stop_loss_price <= Decimal::ZERO || stop_loss_price >= mark_price {
        bail!(
            "long stop loss {} must be positive and below entry {}",
            decimal_string(stop_loss_price),
            decimal_string(mark_price)
        );
    }
    Ok(BinanceEthMicroPreparedPlan {
        exchange_symbol: DEFAULT_EXCHANGE_SYMBOL.to_string(),
        web_symbol: config.web_symbol.clone(),
        qty,
        mark_price,
        notional,
        stop_loss_price,
        position_mode,
        available_usdt,
        filters,
    })
}
/// 写入开仓任务，并返回后续 worker 处理所需的任务标识。
async fn insert_open_task(
    pool: &PgPool,
    config: &BinanceEthMicroLiveValidationConfig,
    combo_id: i64,
    credential_id: i64,
    prepared: &BinanceEthMicroPreparedPlan,
) -> Result<i64> {
    let now = Utc::now();
    let external_id = format!(
        "binance-eth-micro-open-{}-{}",
        now.timestamp_millis(),
        std::process::id()
    );
    let payload = build_open_task_payload(credential_id, prepared);
    insert_strategy_task(
        pool,
        &external_id,
        config,
        combo_id,
        "entry",
        "long",
        "Binance ETH micro live open validation",
        "Rust-native Binance ETH micro live validation open task.",
        "execute_signal",
        "pending",
        1000,
        payload,
        now,
    )
    .await
}
/// 写入平仓任务，并复用开仓结果作为关闭验证输入。
async fn insert_close_task(
    pool: &PgPool,
    config: &BinanceEthMicroLiveValidationConfig,
    combo_id: i64,
    credential_id: i64,
    source_open_task_id: i64,
    prepared: &BinanceEthMicroPreparedPlan,
) -> Result<i64> {
    let now = Utc::now();
    let external_id = format!(
        "binance-eth-micro-close-{}-{}",
        now.timestamp_millis(),
        std::process::id()
    );
    let payload = build_close_task_payload(credential_id, source_open_task_id, prepared);
    insert_strategy_task(
        pool,
        &external_id,
        config,
        combo_id,
        "exit",
        "close_long",
        "Binance ETH micro live close validation",
        "Rust-native Binance ETH micro live validation reduce-only close task.",
        "risk_control_close_candidate",
        "pending_close",
        1100,
        payload,
        now,
    )
    .await
}
/// 向 Web 执行任务表写入策略任务记录。
async fn insert_strategy_task(
    pool: &PgPool,
    external_id: &str,
    config: &BinanceEthMicroLiveValidationConfig,
    combo_id: i64,
    signal_type: &str,
    direction: &str,
    title: &str,
    summary: &str,
    task_type: &str,
    task_status: &str,
    priority: i32,
    payload: Value,
    now: DateTime<Utc>,
) -> Result<i64> {
    let now_naive = now.naive_utc();
    let signal_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO strategy_signal_inbox (
            source, external_id, strategy_slug, strategy_key, symbol,
            signal_type, direction, title, summary, confidence,
            payload_json, generated_at, created_at, updated_at
        ) VALUES (
            'rust_quant_cli', $1, $2, $3, $4,
            $5, $6, $7, $8, NULL,
            $9, $10, $10, $10
        )
        RETURNING id
        "#,
    )
    .bind(external_id)
    .bind(&config.strategy_slug)
    .bind(&config.strategy_key)
    .bind(&config.web_symbol)
    .bind(signal_type)
    .bind(direction)
    .bind(title)
    .bind(summary)
    .bind(payload.to_string())
    .bind(now_naive)
    .fetch_one(pool)
    .await?;
    let task_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO execution_tasks (
            strategy_signal_id, combo_id, buyer_email, strategy_slug, symbol,
            task_type, task_status, priority, lease_owner, lease_until,
            scheduled_at, request_payload_json, created_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, NULL, NULL,
            $9, $10, $9, $9
        )
        RETURNING id
        "#,
    )
    .bind(signal_id)
    .bind(combo_id)
    .bind(&config.buyer_email)
    .bind(&config.strategy_slug)
    .bind(&config.web_symbol)
    .bind(task_type)
    .bind(task_status)
    .bind(priority)
    .bind(now_naive)
    .bind(payload.to_string())
    .fetch_one(pool)
    .await?;
    Ok(task_id)
}
/// 按任务 ID 临时覆盖 worker 环境并运行一次处理循环。
async fn run_scoped_worker_once(
    config: &BinanceEthMicroLiveValidationConfig,
    task_id: i64,
    task_type: &str,
    task_status: &str,
) -> Result<usize> {
    let _env_guard = EnvOverrideGuard::apply(&BTreeMap::from([
        (
            "QUANT_CORE_DATABASE_URL",
            config.quant_core_database_url.clone(),
        ),
        (
            "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
            EXECUTION_WORKER_CONFIRM_TOKEN.to_string(),
        ),
    ]));
    let audit_repository = PostgresExecutionAuditRepository::from_env()?
        .ok_or_else(|| anyhow!("QUANT_CORE_DATABASE_URL is required for live execution audit"))?;
    let worker = ExecutionWorker::new(
        ExecutionTaskClient::new(ExecutionTaskConfig {
            base_url: config.web_base_url.clone(),
            internal_secret: config.internal_secret.clone(),
        })?,
        CryptoExcAllGateway::dry_run(),
        build_worker_config(task_id, task_type, task_status)?,
    )
    .with_audit_repository(std::sync::Arc::new(audit_repository));
    worker.verify_live_audit_ready().await?;
    worker.run_once().await
}
/// 读取订单结果并校验方向、状态和成交数量。
async fn verify_order_result(
    pool: &PgPool,
    task_id: i64,
    expected_side: &str,
    stage: &str,
) -> Result<OrderResultRow> {
    let row = sqlx::query_as::<_, OrderResultRow>(
        r#"
        SELECT
            id, external_order_id, order_side, order_status,
            filled_qty, filled_quote, raw_payload_json
        FROM exchange_order_results
        WHERE execution_task_id = $1
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow!("{stage} task {task_id} did not report exchange_order_results"))?;
    if !row.order_side.eq_ignore_ascii_case(expected_side) {
        bail!(
            "{stage} task {task_id} reported side {}, expected {}",
            row.order_side,
            expected_side
        );
    }
    if !row.order_status.eq_ignore_ascii_case("FILLED") {
        bail!(
            "{stage} task {task_id} order status is {}, expected FILLED",
            row.order_status
        );
    }
    Ok(row)
}
/// 校验开仓后的保护单同步结果。
fn verify_open_protection_sync(row: &OrderResultRow) -> Result<Value> {
    let payload = serde_json::from_str::<Value>(&row.raw_payload_json)
        .context("parse open order raw_payload_json")?;
    let sync = payload
        .get("protection_sync")
        .ok_or_else(|| anyhow!("open order result missing protection_sync"))?;
    if sync
        .get("protective_order_confirmed")
        .and_then(Value::as_bool)
        != Some(true)
    {
        bail!("open protection_sync did not confirm protective order");
    }
    if sync
        .get("protective_order_external_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        bail!("open protection_sync missing protective_order_external_id");
    }
    Ok(sync.clone())
}
/// 按买家邮箱解析唯一 Binance API 凭证 ID。
async fn resolve_single_binance_credential_id(pool: &PgPool, buyer_email: &str) -> Result<i64> {
    let ids = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT id
        FROM user_api_credentials
        WHERE buyer_email = $1
          AND status = 'active'
          AND last_check_code = ANY($2)
          AND api_key_cipher LIKE 'v4:local_aes256gcm:%'
          AND api_secret_cipher LIKE 'v4:local_aes256gcm:%'
          AND (
              passphrase_cipher IS NULL
              OR BTRIM(passphrase_cipher) = ''
              OR passphrase_cipher LIKE 'v4:local_aes256gcm:%'
          )
          AND (LOWER(BTRIM(exchange)) = 'binance' OR BTRIM(exchange) = '币安')
        ORDER BY id ASC
        "#,
    )
    .bind(buyer_email)
    .bind(vec![
        "signed_exchange_preflight_passed".to_string(),
        "signed_exchange_check_passed".to_string(),
    ])
    .fetch_all(pool)
    .await?;
    require_single_id(ids, "ready Binance API credential")
}
/// 按买家、策略和交易对解析唯一 ETH combo ID。
async fn resolve_single_eth_combo_id(
    pool: &PgPool,
    config: &BinanceEthMicroLiveValidationConfig,
) -> Result<i64> {
    let ids = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT id
        FROM strategy_combo_subscriptions
        WHERE buyer_email = $1
          AND strategy_slug = $2
          AND LOWER(symbol) = LOWER($3)
          AND status = 'active'
          AND service_mode = 'api_trade_enabled'
          AND expired_at >= NOW()
          AND (
              execution_exchange IS NULL
              OR BTRIM(execution_exchange) = ''
              OR LOWER(BTRIM(execution_exchange)) = 'binance'
              OR BTRIM(execution_exchange) = '币安'
          )
        ORDER BY id ASC
        "#,
    )
    .bind(&config.buyer_email)
    .bind(&config.strategy_slug)
    .bind(&config.web_symbol)
    .fetch_all(pool)
    .await?;
    require_single_id(ids, "active Binance ETH api-trade combo")
}
/// 要求查询结果只返回一个 ID，避免 live validation 误选目标。
fn require_single_id(ids: Vec<i64>, label: &str) -> Result<i64> {
    match ids.as_slice() {
        [id] => Ok(*id),
        [] => bail!("no {label} found; set BINANCE_ETH_MICRO_CREDENTIAL_ID/BINANCE_ETH_MICRO_COMBO_ID explicitly"),
        _ => bail!("multiple {label} rows found; set an explicit id before live validation"),
    }
}
/// 校验 live mutation 确认口令。
fn ensure_live_apply_confirmation(config: &BinanceEthMicroLiveValidationConfig) -> Result<()> {
    if config.confirm.as_deref().map(str::trim) != Some(BINANCE_ETH_MICRO_CONFIRM_TOKEN) {
        bail!(
            "BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM={} is required before live mutation",
            BINANCE_ETH_MICRO_CONFIRM_TOKEN
        );
    }
    Ok(())
}
/// 把准备好的下单计划转换成审计 JSON。
fn prepared_plan_json(prepared: &BinanceEthMicroPreparedPlan) -> Value {
    json!({
        "position_mode": prepared.position_mode.as_str(),
        "qty": decimal_string(prepared.qty),
        "mark_price": decimal_string(prepared.mark_price),
        "notional": decimal_string(prepared.notional),
        "stop_loss_price": decimal_string(prepared.stop_loss_price),
        "available_usdt": decimal_string(prepared.available_usdt),
        "filters": {
            "quantity_step": decimal_string(prepared.filters.quantity_step),
            "min_quantity": decimal_string(prepared.filters.min_quantity),
            "min_notional": decimal_string(prepared.filters.min_notional),
            "price_tick": decimal_string(prepared.filters.price_tick),
        }
    })
}
/// 把订单结果行转换成审计 JSON。
fn order_result_json(task_id: i64, worker_count: usize, row: &OrderResultRow) -> Value {
    json!({
        "task_id": task_id,
        "worker_handled_count": worker_count,
        "order_result_id": row.id,
        "external_order_id": row.external_order_id,
        "order_side": row.order_side,
        "order_status": row.order_status,
        "filled_qty": row.filled_qty.map(decimal_string),
        "filled_quote": row.filled_quote.map(decimal_string),
    })
}
/// 归一化 ETH 交易对，兼容 Web 和 Binance 的符号写法。
fn normalize_eth_symbol(symbol: &str) -> Result<String> {
    match symbol.trim().to_ascii_uppercase().as_str() {
        "ETHUSDT" | "ETH-USDT-SWAP" => Ok(DEFAULT_WEB_SYMBOL.to_string()),
        other => bail!("Refusing non-ETH symbol {other}; only ETHUSDT/ETH-USDT-SWAP is allowed"),
    }
}
/// 按交易所步长向下量化数量或价格。
fn quantize_down(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    (value / step).floor() * step
}

fn decimal_string(value: Decimal) -> String {
    value.normalize().to_string()
}
/// 从环境变量解析 Decimal 值。
fn parse_decimal_env(key: &str, default: &str) -> Result<Decimal> {
    let raw = std::env::var(key).unwrap_or_else(|_| default.to_string());
    Decimal::from_str(raw.trim()).with_context(|| format!("{key} must be decimal"))
}
/// 从环境变量解析可选 Decimal 值。
fn parse_optional_decimal_env(key: &str) -> Result<Option<Decimal>> {
    match std::env::var(key) {
        Ok(raw) if !raw.trim().is_empty() => Decimal::from_str(raw.trim())
            .map(Some)
            .with_context(|| format!("{key} must be decimal")),
        _ => Ok(None),
    }
}
/// 从环境变量解析可选 i64 值。
fn parse_optional_i64_env(key: &str) -> Result<Option<i64>> {
    match std::env::var(key) {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<i64>()
            .map(Some)
            .with_context(|| format!("{key} must be i64")),
        _ => Ok(None),
    }
}
/// 从环境变量解析 i64 值。
fn parse_i64_env(key: &str, default: i64) -> Result<i64> {
    match std::env::var(key) {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<i64>()
            .with_context(|| format!("{key} must be i64")),
        _ => Ok(default),
    }
}
/// 从环境变量解析布尔值。
fn parse_bool_env(key: &str, default: bool) -> Result<bool> {
    match std::env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => bail!("{key} must be boolean"),
        },
        Err(_) => Ok(default),
    }
}
/// 从环境变量读取 HTTP/SOCKS 代理；设置不支持的 scheme 时 fail closed。
fn parse_optional_proxy_env(key: &str) -> Result<Option<String>> {
    let Some(value) = first_non_empty_env(&[key]) else {
        return Ok(None);
    };
    normalize_proxy_url(key, value).map(Some)
}
fn normalize_proxy_url(key: &str, value: String) -> Result<String> {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("socks5://")
        || lower.starts_with("socks5h://")
    {
        return Ok(value);
    }
    bail!("{key} must start with http://, https://, socks5://, or socks5h://");
}
/// 按优先级读取第一个非空环境变量。
fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}
