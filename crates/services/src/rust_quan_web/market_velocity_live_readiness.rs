use super::execution_payload::{order_side_lower, validate_execute_signal_risk_contract};
use super::execution_worker::ExecutionOrderTask;
use super::{
    ExecutionTask, ExecutionTaskClient, ExecutionTaskConfig, ExecutionWorkerConfig,
    MarketVelocityExecutionTaskLiveReadinessCheck,
    MarketVelocityExecutionTaskLiveReadinessResponse,
};
use anyhow::{anyhow, Result};
use crypto_exc_all::ExchangeId;
use serde::Serialize;
use serde_json::{json, Value};
#[derive(Debug, Clone)]
pub struct MarketVelocityLiveReadinessConfig {
    /// 基础URL，用于配置运行参数。
    pub base_url: String,
    /// internalSecret，用于配置运行参数。
    pub internal_secret: String,
    /// targettask ID。
    pub target_task_id: i64,
    /// 配置项。
    pub worker_config: ExecutionWorkerConfig,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MarketVelocityWorkerHandoffReadiness {
    /// readonly，用于行情、K 线或市场扫描。
    pub read_only: bool,
    /// 是否允许该操作。
    pub mutation_allowed: bool,
    /// 当前状态。
    pub status: String,
    /// 任务 ID。
    pub task_id: i64,
    /// 状态值。
    pub web_owner_status: String,
    /// 列表数据。
    pub checks: Vec<MarketVelocityExecutionTaskLiveReadinessCheck>,
    /// 是否阻塞当前流程。
    pub blocker_codes: Vec<String>,
}
impl MarketVelocityLiveReadinessConfig {
    /// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
            .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
            .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
        let internal_secret = required_internal_secret_from_env()?;
        let target_task_id = market_velocity_live_readiness_task_id_from_env()?;
        let worker_config = ExecutionWorkerConfig::from_env();
        Ok(Self {
            base_url,
            internal_secret,
            target_task_id,
            worker_config,
        })
    }
}
/// 封装当前函数，减少Web 商业链路调用方重复实现相同细节。
/// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
fn required_internal_secret_from_env() -> Result<String> {
    std::env::var("EXECUTION_EVENT_SECRET")
        .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required")
        })
}
/// 解析只读 readiness 要检查的执行任务；它是诊断参数，不参与常驻 worker 租约过滤。
fn market_velocity_live_readiness_task_id_from_env() -> Result<i64> {
    let raw = std::env::var("MARKET_VELOCITY_LIVE_READINESS_TASK_ID")
        .map_err(|_| anyhow!("MARKET_VELOCITY_LIVE_READINESS_TASK_ID is required"))?;
    let task_id = raw.trim().parse::<i64>().map_err(|_| {
        anyhow!("MARKET_VELOCITY_LIVE_READINESS_TASK_ID must be a positive task id")
    })?;
    if task_id > 0 {
        Ok(task_id)
    } else {
        Err(anyhow!(
            "MARKET_VELOCITY_LIVE_READINESS_TASK_ID must be a positive task id"
        ))
    }
}
/// 执行 Web 商业、会员和执行准备度 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub async fn run_market_velocity_live_readiness_from_env() -> Result<Value> {
    let config = MarketVelocityLiveReadinessConfig::from_env()?;
    let client = ExecutionTaskClient::new(ExecutionTaskConfig {
        base_url: config.base_url.clone(),
        internal_secret: config.internal_secret.clone(),
    })?;
    let web_readiness = client
        .market_velocity_live_task_readiness(config.target_task_id)
        .await?;
    let handoff_readiness =
        build_market_velocity_worker_handoff_readiness(&web_readiness, &config.worker_config);
    let status = if web_readiness.status == "ready_for_live_worker"
        && handoff_readiness.status == "ready_for_live_worker"
    {
        "ready_for_live_worker"
    } else {
        "blocked"
    };
    Ok(json!({
        "read_only": true,
        "mutation_allowed": false,
        "status": status,
        "target_task_id": config.target_task_id,
        "web_owner_readiness": web_readiness,
        "worker_handoff_readiness": handoff_readiness,
        "execution_path": market_velocity_existing_execution_worker_path(),
        "next_worker_env": build_market_velocity_scoped_execution_worker_env(config.target_task_id)
    }))
}
/// 提供市场动量existing执行worker路径的集中实现，避免Web 商业链路调用方重复处理相同细节。
pub fn market_velocity_existing_execution_worker_path() -> Value {
    json!({
        "kind": "existing_execution_worker",
        "reuse": "vegas_style_execution_task_worker",
        "worker_entrypoint": "rust_quant::app::bootstrap::run_execution_worker_from_env",
        "worker_mode_env": "IS_RUN_EXECUTION_WORKER",
        "creates_new_order_system": false,
        "legacy_shell_entrypoint": false
    })
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_scoped_execution_worker_env(task_id: i64) -> Value {
    json!({
        "reference_task_id": task_id,
        "IS_RUN_EXECUTION_WORKER": "true",
        "EXECUTION_WORKER_ONLY": "true",
        "EXECUTION_WORKER_RUN_ONCE": "false",
        "EXECUTION_WORKER_TASK_TYPES": "execute_signal,close_position",
        "EXECUTION_WORKER_TASK_STATUSES": "pending,pending_close"
    })
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_scoped_worker_handoff_readiness(
    web_readiness: &MarketVelocityExecutionTaskLiveReadinessResponse,
) -> MarketVelocityWorkerHandoffReadiness {
    let worker_config = build_market_velocity_scoped_execution_worker_config(&web_readiness.task);
    build_market_velocity_worker_handoff_readiness(web_readiness, &worker_config)
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_scoped_execution_worker_config(
    task: &ExecutionTask,
) -> ExecutionWorkerConfig {
    let default_exchange = ExecutionOrderTask::from_task_with_default(task, ExchangeId::Okx)
        .map(|order_task| order_task.exchange)
        .unwrap_or(ExchangeId::Okx);
    ExecutionWorkerConfig {
        worker_id: "market_velocity_scoped_live_worker".to_string(),
        lease_limit: 1,
        dry_run: false,
        default_exchange,
        task_types: vec!["execute_signal".to_string()],
        task_statuses: vec!["pending".to_string(), "leased".to_string()],
        target_task_ids: Vec::new(),
        confirmation_mode: false,
        report_replay_mode: false,
        report_replay_max_per_run: 1,
        report_replay_failure_backoff_seconds: 300,
        report_replay_throttle_ms: 0,
    }
}
/// 构建 Web 商业、会员和执行准备度 请求或响应载荷，把字段组装规则集中在同一入口。
pub(crate) fn build_market_velocity_worker_handoff_readiness(
    web_readiness: &MarketVelocityExecutionTaskLiveReadinessResponse,
    worker_config: &ExecutionWorkerConfig,
) -> MarketVelocityWorkerHandoffReadiness {
    let task = &web_readiness.task;
    let mut checks = vec![web_owner_readiness_check(web_readiness)];
    checks.push(worker_order_contract_check(
        task,
        worker_config.default_exchange,
    ));
    let blocker_codes = checks
        .iter()
        .filter_map(|check| check.blocker_code.clone())
        .collect::<Vec<_>>();
    let status = if blocker_codes.is_empty() {
        "ready_for_live_worker"
    } else {
        "blocked"
    }
    .to_string();
    MarketVelocityWorkerHandoffReadiness {
        read_only: true,
        mutation_allowed: false,
        status,
        task_id: task.id,
        web_owner_status: web_readiness.status.clone(),
        checks,
        blocker_codes,
    }
}
/// 提供webownerreadiness检查的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn web_owner_readiness_check(
    web_readiness: &MarketVelocityExecutionTaskLiveReadinessResponse,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    if web_readiness.status == "ready_for_live_worker" && web_readiness.blocker_codes.is_empty() {
        passed_check(
            "web_owner_readiness",
            "Web owner readiness",
            "Web owner confirmed entitlement, risk settings, API key, signed preflight, and strategy task contract.",
        )
    } else {
        blocked_check(
            "web_owner_readiness",
            "Web owner readiness",
            "web_owner_readiness_blocked",
            "Web owner live readiness is blocked; do not start live worker.",
        )
    }
}
/// 提供worker订单contract检查的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn worker_order_contract_check(
    task: &ExecutionTask,
    default_exchange: ExchangeId,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    let order_task = match ExecutionOrderTask::from_task_with_default(task, default_exchange) {
        Ok(order_task) => order_task,
        Err(error) => {
            return blocked_check(
                "worker_order_contract",
                "Worker order contract",
                "execution_worker_order_parse_failed",
                &error.to_string(),
            );
        }
    };
    if let Err(error) = validate_execute_signal_risk_contract(task, &order_task, true) {
        return blocked_check(
            "worker_order_contract",
            "Worker order contract",
            "execution_worker_risk_contract_rejected",
            &error.message,
        );
    }
    passed_check(
        "worker_order_contract",
        "Worker order contract",
        &format!(
            "Existing worker can parse a protected {} {} order for {}.",
            order_task.exchange.as_str(),
            order_side_lower(order_task.side),
            order_task.symbol
        ),
    )
}
/// 提供passed检查的集中实现，避免Web 商业链路调用方重复处理相同细节。
fn passed_check(
    code: &str,
    label: &str,
    detail: &str,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    MarketVelocityExecutionTaskLiveReadinessCheck {
        code: code.to_string(),
        label: label.to_string(),
        status: "passed".to_string(),
        blocker_code: None,
        detail: detail.to_string(),
    }
}
/// 封装阻塞check，减少Web 商业链路调用方重复实现相同细节。
fn blocked_check(
    code: &str,
    label: &str,
    blocker_code: &str,
    detail: &str,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    MarketVelocityExecutionTaskLiveReadinessCheck {
        code: code.to_string(),
        label: label.to_string(),
        status: "blocked".to_string(),
        blocker_code: Some(blocker_code.to_string()),
        detail: detail.to_string(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn existing_execution_path_declares_rust_native_vegas_style_worker() {
        let execution_path = market_velocity_existing_execution_worker_path();
        let encoded = serde_json::to_string(&execution_path).expect("execution path json");
        assert_eq!(execution_path["kind"], "existing_execution_worker");
        assert_eq!(execution_path["reuse"], "vegas_style_execution_task_worker");
        assert_eq!(
            execution_path["worker_entrypoint"],
            "rust_quant::app::bootstrap::run_execution_worker_from_env"
        );
        assert_eq!(execution_path["worker_mode_env"], "IS_RUN_EXECUTION_WORKER");
        assert_eq!(execution_path["creates_new_order_system"], false);
        assert_eq!(execution_path["legacy_shell_entrypoint"], false);
        assert!(
            !encoded.contains(".sh") && !encoded.contains("scripts/dev"),
            "Market Velocity live path must not declare a shell entrypoint: {encoded}"
        );
    }
    #[test]
    fn worker_handoff_readiness_passes_existing_worker_contract() {
        let worker_config = worker_config(false);
        let web_readiness = web_readiness(task(live_payload("ETHUSDT")), "ready_for_live_worker");
        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);
        assert_eq!(readiness.status, "ready_for_live_worker");
        assert!(readiness.blocker_codes.is_empty());
        assert_check(&readiness, "worker_order_contract", "passed", None);
    }
    #[test]
    fn scoped_worker_readiness_can_be_derived_from_created_task() {
        let web_readiness = web_readiness(task(live_payload("ETHUSDT")), "ready_for_live_worker");
        let readiness = build_market_velocity_scoped_worker_handoff_readiness(&web_readiness);
        assert_eq!(readiness.status, "ready_for_live_worker");
        assert_eq!(readiness.task_id, 228);
        assert!(readiness.blocker_codes.is_empty());
        assert_check(&readiness, "worker_order_contract", "passed", None);
    }
    #[test]
    fn worker_handoff_readiness_ignores_payload_dry_run_policy_switches() {
        let worker_config = worker_config(false);
        let mut payload = live_payload("ETHUSDT");
        payload["execution_policy"]["mode"] = json!("execution_task_dry_run");
        payload["execution_policy"]["production_stage"] = json!("execution_task_dry_run");
        let web_readiness = web_readiness(task(payload), "ready_for_live_worker");
        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);
        assert_eq!(readiness.status, "ready_for_live_worker");
        assert!(readiness.blocker_codes.is_empty());
    }
    #[test]
    fn worker_handoff_readiness_allows_any_supported_symbol_with_stop_loss() {
        let worker_config = worker_config(false);
        let web_readiness = web_readiness(task(live_payload("LINKUSDT")), "ready_for_live_worker");
        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);
        assert_eq!(readiness.status, "ready_for_live_worker");
        assert!(readiness.blocker_codes.is_empty());
    }
    #[test]
    fn scoped_worker_env_reuses_existing_worker_without_shell_entrypoint() {
        let env = build_market_velocity_scoped_execution_worker_env(228);
        let encoded = serde_json::to_string(&env).expect("worker env json");
        assert_eq!(env["reference_task_id"], 228);
        assert_eq!(env["IS_RUN_EXECUTION_WORKER"], "true");
        assert_eq!(env["EXECUTION_WORKER_ONLY"], "true");
        assert_eq!(env["EXECUTION_WORKER_RUN_ONCE"], "false");
        assert_eq!(
            env["EXECUTION_WORKER_TASK_TYPES"],
            "execute_signal,close_position"
        );
        assert_eq!(
            env["EXECUTION_WORKER_TASK_STATUSES"],
            "pending,pending_close"
        );
        assert!(env.get("EXECUTION_WORKER_DRY_RUN").is_none());
        assert!(env.get("EXECUTION_WORKER_TARGET_TASK_IDS").is_none());
        assert!(env.get("EXECUTION_WORKER_LIVE_ORDER_CONFIRM").is_none());
        assert!(
            !encoded.contains(".sh") && !encoded.contains("scripts/dev"),
            "Market Velocity live handoff must not point to shell scripts: {encoded}"
        );
    }
    /// 提供worker配置的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn worker_config(dry_run: bool) -> ExecutionWorkerConfig {
        ExecutionWorkerConfig {
            worker_id: "readiness-test".to_string(),
            lease_limit: 1,
            dry_run,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string(), "leased".to_string()],
            target_task_ids: Vec::new(),
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        }
    }
    /// 提供webreadiness的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn web_readiness(
        task: ExecutionTask,
        status: &str,
    ) -> MarketVelocityExecutionTaskLiveReadinessResponse {
        MarketVelocityExecutionTaskLiveReadinessResponse {
            read_only: true,
            mutation_allowed: false,
            owner_service: "quant_web".to_string(),
            status: status.to_string(),
            task,
            checks: Vec::new(),
            blocker_codes: Vec::new(),
        }
    }
    /// 提供task的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn task(payload: Value) -> ExecutionTask {
        ExecutionTask {
            id: 228,
            news_signal_id: None,
            strategy_signal_id: Some(991),
            combo_id: 9001,
            buyer_email: "buyer@example.com".to_string(),
            strategy_slug: "market_velocity".to_string(),
            symbol: payload["symbol"].as_str().unwrap_or("ETHUSDT").to_string(),
            task_type: "execute_signal".to_string(),
            task_status: "pending".to_string(),
            priority: 3,
            lease_owner: None,
            lease_until: None,
            scheduled_at: "2026-06-15T06:30:00".to_string(),
            request_payload_json: payload,
            created_at: "2026-06-15T06:00:00".to_string(),
            updated_at: "2026-06-15T06:30:00".to_string(),
        }
    }
    /// 封装实盘载荷，减少Web 商业链路调用方重复实现相同细节。
    fn live_payload(symbol: &str) -> Value {
        json!({
            "source_signal_type": "market_velocity",
            "strategy_slug": "market_velocity",
            "symbol": symbol,
            "exchange": "binance",
            "auto_execution_allowed": true,
            "execution_policy": {
                "live_order_allowed": true,
                "paper_trade_required": false,
                "production_stage": "live_execution_allowed"
            },
            "side": "buy",
            "position_side": "long",
            "trade_side": "open",
            "order_type": "market",
            "execution": {
                "exchange": "binance",
                "symbol": symbol,
                "side": "buy",
                "order_type": "market",
                "size_usdt": 50.0,
                "position_side": "long",
                "position_mode": "hedge"
            },
            "risk_plan": {
                "entry_price": 100.0,
                "selected_stop_loss_price": 97.5,
                "direction": "long",
                "protective_stop_loss_required": true
            }
        })
    }
    /// 提供assert检查的集中实现，避免Web 商业链路调用方重复处理相同细节。
    fn assert_check(
        readiness: &MarketVelocityWorkerHandoffReadiness,
        code: &str,
        status: &str,
        blocker_code: Option<&str>,
    ) {
        let check = readiness
            .checks
            .iter()
            .find(|check| check.code == code)
            .unwrap_or_else(|| panic!("missing check {code}"));
        assert_eq!(check.status, status);
        assert_eq!(check.blocker_code.as_deref(), blocker_code);
    }
}
