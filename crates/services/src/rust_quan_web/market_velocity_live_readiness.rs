use anyhow::{anyhow, Result};
use crypto_exc_all::ExchangeId;
use serde::Serialize;
use serde_json::{json, Value};

use super::execution_payload::{
    order_payload, order_side_lower, payload_bool, payload_string,
    validate_execute_signal_risk_contract,
};
use super::execution_worker::{is_protected_link_symbol, ExecutionOrderTask};
use super::{
    ExecutionTask, ExecutionTaskClient, ExecutionTaskConfig, ExecutionWorkerConfig,
    MarketVelocityExecutionTaskLiveReadinessCheck,
    MarketVelocityExecutionTaskLiveReadinessResponse,
};

#[derive(Debug, Clone)]
pub struct MarketVelocityLiveReadinessConfig {
    pub base_url: String,
    pub internal_secret: String,
    pub target_task_id: i64,
    pub worker_config: ExecutionWorkerConfig,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MarketVelocityWorkerHandoffReadiness {
    pub read_only: bool,
    pub mutation_allowed: bool,
    pub status: String,
    pub task_id: i64,
    pub web_owner_status: String,
    pub checks: Vec<MarketVelocityExecutionTaskLiveReadinessCheck>,
    pub blocker_codes: Vec<String>,
}

impl MarketVelocityLiveReadinessConfig {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("RUST_QUAN_WEB_BASE_URL")
            .or_else(|_| std::env::var("QUANT_WEB_BASE_URL"))
            .map_err(|_| anyhow!("RUST_QUAN_WEB_BASE_URL is required"))?;
        let internal_secret = std::env::var("EXECUTION_EVENT_SECRET")
            .or_else(|_| std::env::var("RUST_QUAN_WEB_INTERNAL_SECRET"))
            .unwrap_or_default();
        let worker_config = ExecutionWorkerConfig::from_env();
        let target_task_id = single_market_velocity_target_task_id(&worker_config)?;

        Ok(Self {
            base_url,
            internal_secret,
            target_task_id,
            worker_config,
        })
    }
}

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
        && handoff_readiness.status == "ready_for_scoped_live_worker"
    {
        "ready_for_scoped_live_worker"
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

pub fn build_market_velocity_scoped_execution_worker_env(task_id: i64) -> Value {
    json!({
        "IS_RUN_EXECUTION_WORKER": "true",
        "EXECUTION_WORKER_ONLY": "true",
        "EXECUTION_WORKER_RUN_ONCE": "true",
        "EXECUTION_WORKER_DRY_RUN": "false",
        "EXECUTION_WORKER_TARGET_TASK_IDS": task_id.to_string(),
        "EXECUTION_WORKER_TASK_TYPES": "execute_signal",
        "EXECUTION_WORKER_TASK_STATUSES": "pending,leased",
        "EXECUTION_WORKER_LIVE_ORDER_CONFIRM": "I_UNDERSTAND_LIVE_ORDERS"
    })
}

pub fn build_market_velocity_scoped_worker_handoff_readiness(
    web_readiness: &MarketVelocityExecutionTaskLiveReadinessResponse,
) -> MarketVelocityWorkerHandoffReadiness {
    let worker_config = build_market_velocity_scoped_execution_worker_config(&web_readiness.task);
    build_market_velocity_worker_handoff_readiness(web_readiness, &worker_config)
}

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
        target_task_ids: vec![task.id],
        confirmation_mode: false,
        report_replay_mode: false,
        report_replay_max_per_run: 1,
        report_replay_failure_backoff_seconds: 300,
        report_replay_throttle_ms: 0,
    }
}

pub(crate) fn build_market_velocity_worker_handoff_readiness(
    web_readiness: &MarketVelocityExecutionTaskLiveReadinessResponse,
    worker_config: &ExecutionWorkerConfig,
) -> MarketVelocityWorkerHandoffReadiness {
    let task = &web_readiness.task;
    let mut checks = vec![
        web_owner_readiness_check(web_readiness),
        target_task_scope_check(task.id, worker_config),
        worker_live_mode_check(worker_config),
        market_velocity_payload_check(task),
        protected_symbol_check(task),
    ];
    checks.push(worker_order_contract_check(
        task,
        worker_config.default_exchange,
    ));

    let blocker_codes = checks
        .iter()
        .filter_map(|check| check.blocker_code.clone())
        .collect::<Vec<_>>();
    let status = if blocker_codes.is_empty() {
        "ready_for_scoped_live_worker"
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

fn single_market_velocity_target_task_id(config: &ExecutionWorkerConfig) -> Result<i64> {
    match config.target_task_ids.as_slice() {
        [task_id] if *task_id > 0 => Ok(*task_id),
        [] => Err(anyhow!(
            "EXECUTION_WORKER_TARGET_TASK_IDS must contain exactly one reviewed Market Velocity task id"
        )),
        _ => Err(anyhow!(
            "Market Velocity live readiness requires exactly one positive target task id"
        )),
    }
}

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

fn target_task_scope_check(
    task_id: i64,
    config: &ExecutionWorkerConfig,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    if config.target_task_ids == vec![task_id] {
        passed_check(
            "target_task_scope",
            "Target task scope",
            "Worker is scoped to the same reviewed execution task id.",
        )
    } else {
        blocked_check(
            "target_task_scope",
            "Target task scope",
            "execution_worker_target_scope_mismatch",
            "EXECUTION_WORKER_TARGET_TASK_IDS must contain only the reviewed readiness task id.",
        )
    }
}

fn worker_live_mode_check(
    config: &ExecutionWorkerConfig,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    if config.dry_run {
        return blocked_check(
            "worker_live_mode",
            "Worker live mode",
            "execution_worker_dry_run_still_enabled",
            "Production live handoff requires EXECUTION_WORKER_DRY_RUN=false.",
        );
    }
    match config.validate_live_worker_scope() {
        Ok(()) => passed_check(
            "worker_live_mode",
            "Worker live mode",
            "Worker is in live mode and still scoped to explicit target task ids.",
        ),
        Err(error) => blocked_check(
            "worker_live_mode",
            "Worker live mode",
            "execution_worker_live_scope_invalid",
            &error.to_string(),
        ),
    }
}

fn market_velocity_payload_check(
    task: &ExecutionTask,
) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    let payload = order_payload(&task.request_payload_json);
    let source_signal_type = payload_string(&payload, "source_signal_type");
    let auto_execution_allowed = payload_bool(&payload, "auto_execution_allowed").unwrap_or(false);
    let live_order_allowed = payload
        .get("execution_policy")
        .and_then(|value| payload_bool(value, "live_order_allowed"))
        .unwrap_or(false);
    let paper_trade_required = payload
        .get("execution_policy")
        .and_then(|value| payload_bool(value, "paper_trade_required"))
        .unwrap_or(true);
    let policy_mode = payload
        .get("execution_policy")
        .and_then(|value| payload_string(value, "mode"));
    let production_stage = payload
        .get("execution_policy")
        .and_then(|value| payload_string(value, "production_stage"));
    let dry_run_policy = policy_mode
        .as_deref()
        .map(|value| value.to_ascii_lowercase())
        .is_some_and(|value| value.contains("dry_run") || value.contains("dry-run"))
        || production_stage.as_deref() == Some("execution_task_dry_run");

    if source_signal_type.as_deref() == Some("market_velocity")
        && auto_execution_allowed
        && live_order_allowed
        && !paper_trade_required
        && !dry_run_policy
        && production_stage.as_deref() == Some("live_execution_allowed")
    {
        passed_check(
            "market_velocity_payload",
            "Market Velocity payload",
            "Payload is a live-authorized Market Velocity signal.",
        )
    } else {
        blocked_check(
            "market_velocity_payload",
            "Market Velocity payload",
            "market_velocity_payload_not_live_authorized",
            "Payload must be source_signal_type=market_velocity, live-authorized, not paper-only, and production_stage=live_execution_allowed.",
        )
    }
}

fn protected_symbol_check(task: &ExecutionTask) -> MarketVelocityExecutionTaskLiveReadinessCheck {
    let payload = order_payload(&task.request_payload_json);
    let symbol = payload_string(&payload, "symbol").unwrap_or_else(|| task.symbol.clone());
    if is_protected_link_symbol(&symbol) || is_protected_link_symbol(&task.symbol) {
        blocked_check(
            "protected_symbol_policy",
            "Protected symbol policy",
            "link_symbol_requires_separate_authorization",
            "LINKUSDT remains blocked unless separately authorized for live mutation.",
        )
    } else {
        passed_check(
            "protected_symbol_policy",
            "Protected symbol policy",
            "Task is not scoped to the protected LINKUSDT live position.",
        )
    }
}

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
    if let Err(error) = validate_execute_signal_risk_contract(task, &order_task) {
        return blocked_check(
            "worker_order_contract",
            "Worker order contract",
            "execution_worker_risk_contract_rejected",
            &error.message,
        );
    }
    if is_protected_link_symbol(&order_task.symbol) {
        return blocked_check(
            "worker_order_contract",
            "Worker order contract",
            "link_symbol_requires_separate_authorization",
            "Parsed worker order target is LINKUSDT and requires separate authorization.",
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
        let worker_config = worker_config(false, vec![228]);
        let web_readiness = web_readiness(task(live_payload("ETHUSDT")), "ready_for_live_worker");

        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);

        assert_eq!(readiness.status, "ready_for_scoped_live_worker");
        assert!(readiness.blocker_codes.is_empty());
        assert_check(&readiness, "target_task_scope", "passed", None);
        assert_check(&readiness, "worker_order_contract", "passed", None);
    }

    #[test]
    fn scoped_worker_readiness_can_be_derived_from_created_task() {
        let web_readiness = web_readiness(task(live_payload("ETHUSDT")), "ready_for_live_worker");

        let readiness = build_market_velocity_scoped_worker_handoff_readiness(&web_readiness);

        assert_eq!(readiness.status, "ready_for_scoped_live_worker");
        assert_eq!(readiness.task_id, 228);
        assert!(readiness.blocker_codes.is_empty());
        assert_check(&readiness, "target_task_scope", "passed", None);
        assert_check(&readiness, "worker_live_mode", "passed", None);
        assert_check(&readiness, "worker_order_contract", "passed", None);
    }

    #[test]
    fn worker_handoff_readiness_blocks_dry_run_live_node() {
        let worker_config = worker_config(true, vec![228]);
        let web_readiness = web_readiness(task(live_payload("ETHUSDT")), "ready_for_live_worker");

        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);

        assert_eq!(readiness.status, "blocked");
        assert!(readiness
            .blocker_codes
            .contains(&"execution_worker_dry_run_still_enabled".to_string()));
    }

    #[test]
    fn worker_handoff_readiness_blocks_target_scope_mismatch() {
        let worker_config = worker_config(false, vec![229]);
        let web_readiness = web_readiness(task(live_payload("ETHUSDT")), "ready_for_live_worker");

        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);

        assert_eq!(readiness.status, "blocked");
        assert!(readiness
            .blocker_codes
            .contains(&"execution_worker_target_scope_mismatch".to_string()));
    }

    #[test]
    fn worker_handoff_readiness_blocks_dry_run_policy_stage() {
        let worker_config = worker_config(false, vec![228]);
        let mut payload = live_payload("ETHUSDT");
        payload["execution_policy"]["mode"] = json!("execution_task_dry_run");
        payload["execution_policy"]["production_stage"] = json!("execution_task_dry_run");
        let web_readiness = web_readiness(task(payload), "ready_for_live_worker");

        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);

        assert_eq!(readiness.status, "blocked");
        assert!(readiness
            .blocker_codes
            .contains(&"market_velocity_payload_not_live_authorized".to_string()));
    }

    #[test]
    fn worker_handoff_readiness_blocks_link_symbol() {
        let worker_config = worker_config(false, vec![228]);
        let web_readiness = web_readiness(task(live_payload("LINKUSDT")), "ready_for_live_worker");

        let readiness =
            build_market_velocity_worker_handoff_readiness(&web_readiness, &worker_config);

        assert_eq!(readiness.status, "blocked");
        assert!(readiness
            .blocker_codes
            .contains(&"link_symbol_requires_separate_authorization".to_string()));
    }

    #[test]
    fn scoped_worker_env_reuses_existing_worker_without_shell_entrypoint() {
        let env = build_market_velocity_scoped_execution_worker_env(228);
        let encoded = serde_json::to_string(&env).expect("worker env json");

        assert_eq!(env["IS_RUN_EXECUTION_WORKER"], "true");
        assert_eq!(env["EXECUTION_WORKER_ONLY"], "true");
        assert_eq!(env["EXECUTION_WORKER_RUN_ONCE"], "true");
        assert_eq!(env["EXECUTION_WORKER_DRY_RUN"], "false");
        assert_eq!(env["EXECUTION_WORKER_TARGET_TASK_IDS"], "228");
        assert_eq!(env["EXECUTION_WORKER_TASK_TYPES"], "execute_signal");
        assert_eq!(
            env["EXECUTION_WORKER_LIVE_ORDER_CONFIRM"],
            "I_UNDERSTAND_LIVE_ORDERS"
        );
        assert!(
            !encoded.contains(".sh") && !encoded.contains("scripts/dev"),
            "Market Velocity live handoff must not point to shell scripts: {encoded}"
        );
    }

    fn worker_config(dry_run: bool, target_task_ids: Vec<i64>) -> ExecutionWorkerConfig {
        ExecutionWorkerConfig {
            worker_id: "readiness-test".to_string(),
            lease_limit: 1,
            dry_run,
            default_exchange: ExchangeId::Binance,
            task_types: vec!["execute_signal".to_string()],
            task_statuses: vec!["pending".to_string(), "leased".to_string()],
            target_task_ids,
            confirmation_mode: false,
            report_replay_mode: false,
            report_replay_max_per_run: 1,
            report_replay_failure_backoff_seconds: 300,
            report_replay_throttle_ms: 0,
        }
    }

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
