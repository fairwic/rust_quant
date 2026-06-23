use super::{
    MARKET_VELOCITY_CREATE_TASK_CONFIRM_TOKEN, MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN,
};
use anyhow::{anyhow, bail, Result};
use rust_quant_services::rust_quan_web::{
    build_market_velocity_scoped_execution_worker_env,
    market_velocity_existing_execution_worker_path, ExecutionWorker,
    MarketVelocityExecutionTaskCreationPreviewRequest,
    MarketVelocityExecutionTaskLiveReadinessResponse, MarketVelocityWorkerHandoffReadiness,
    StrategySignalSubmitRequest,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
/// 构建build市场动量实盘preview请求，集中维护行情数据的载荷和字段组装规则。
pub fn build_market_velocity_live_preview_request(
    signal: &StrategySignalSubmitRequest,
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<MarketVelocityExecutionTaskCreationPreviewRequest> {
    if signal.strategy_slug.trim() != "market_velocity" {
        bail!("Market Velocity live handoff only accepts strategy_slug=market_velocity");
    }
    let payload: Value = serde_json::from_str(&signal.payload_json)
        .map_err(|error| anyhow!("parse market velocity signal payload_json failed: {error}"))?;
    if payload_string(&payload, "source_signal_type").as_deref() != Some("market_velocity") {
        bail!("payload source_signal_type must be market_velocity");
    }
    let exchange = payload_string(&payload, "exchange").unwrap_or_else(|| {
        signal
            .strategy_key
            .split(':')
            .nth(1)
            .unwrap_or("okx")
            .to_string()
    });
    let symbol = payload_string(&payload, "symbol").unwrap_or_else(|| signal.symbol.clone());
    let target_r = nested_payload_f64(&payload, "risk_plan", "target_r").unwrap_or(2.4);
    let horizon_hours =
        nested_payload_i64(&payload, "risk_plan", "max_holding_hours").unwrap_or(48);
    let entry_rule_version = payload_string(&payload, "entry_rule_version");
    let entry_trigger_filter_version = payload
        .get("entry_filter")
        .and_then(|value| payload_string(value, "entry_trigger_filter_version"));
    Ok(MarketVelocityExecutionTaskCreationPreviewRequest {
        rank_event_id: payload_i64(&payload, "rank_event_id"),
        buyer_email: buyer_email
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        combo_id,
        exchange: exchange.trim().to_ascii_lowercase(),
        symbol: symbol.trim().to_ascii_uppercase(),
        target_r,
        horizon_hours: horizon_hours.try_into().unwrap_or(i32::MAX),
        entry_rule_version,
        entry_trigger_filter_version,
        risk_adjusted_win_rate_edge: None,
    })
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_live_worker_manifest(task_id: i64) -> Value {
    json!({
        "execution_path": market_velocity_existing_execution_worker_path(),
        "next_worker_env": build_market_velocity_scoped_execution_worker_env(task_id)
    })
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_live_worker_handoff(
    task_id: i64,
    web_readiness: MarketVelocityExecutionTaskLiveReadinessResponse,
    worker_handoff_readiness: MarketVelocityWorkerHandoffReadiness,
) -> Value {
    json!({
        "task_id": task_id,
        "read_only": true,
        "mutation_allowed": false,
        "status": if web_readiness.status == "ready_for_live_worker"
            && worker_handoff_readiness.status == "ready_for_scoped_live_worker" {
            "ready_for_scoped_live_worker"
        } else {
            "blocked"
        },
        "manifest": build_market_velocity_live_worker_manifest(task_id),
        "web_owner_readiness": web_readiness,
        "worker_handoff_readiness": worker_handoff_readiness,
    })
}
/// 提供市场动量requiredliveownerscope的集中实现，避免行情数据调用方重复处理相同细节。
pub fn market_velocity_required_live_owner_scope(
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<(&str, i64)> {
    let buyer_email = buyer_email
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("MARKET_VELOCITY_LIVE_BUYER_EMAIL is required for live task creation")
        })?;
    let combo_id = combo_id.filter(|value| *value > 0).ok_or_else(|| {
        anyhow!("MARKET_VELOCITY_LIVE_COMBO_ID is required for live task creation")
    })?;
    Ok((buyer_email, combo_id))
}
/// 提供市场动量scope信号toliveowner的集中实现，避免行情数据调用方重复处理相同细节。
pub fn market_velocity_scope_signal_to_live_owner(
    mut signal: StrategySignalSubmitRequest,
    buyer_email: &str,
    combo_id: i64,
) -> Result<StrategySignalSubmitRequest> {
    let mut payload: Value = serde_json::from_str(&signal.payload_json)
        .map_err(|error| anyhow!("parse market velocity signal payload_json failed: {error}"))?;
    let payload_object = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("Market Velocity signal payload_json must be an object"))?;
    payload_object.insert("target_buyer_email".to_string(), json!(buyer_email.trim()));
    payload_object.insert("target_combo_id".to_string(), json!(combo_id));
    payload_object.insert(
        "target_scope_source".to_string(),
        json!("market_velocity_live_handoff"),
    );
    signal.payload_json = serde_json::to_string(&payload)?;
    Ok(signal)
}
pub fn market_velocity_task_creation_apply_authorized(
    apply: bool,
    confirmation: Option<&str>,
) -> bool {
    apply && confirmation.map(str::trim) == Some(MARKET_VELOCITY_CREATE_TASK_CONFIRM_TOKEN)
}
pub fn market_velocity_scoped_worker_apply_authorized(
    apply: bool,
    confirmation: Option<&str>,
) -> bool {
    apply && confirmation.map(str::trim) == Some(MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN)
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_scoped_worker_env_overrides(
    task_id: i64,
    confirmation: &str,
) -> Result<BTreeMap<&'static str, String>> {
    if task_id <= 0 {
        bail!("scoped live worker task_id must be positive");
    }
    if !market_velocity_scoped_worker_apply_authorized(true, Some(confirmation)) {
        bail!(
            "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM={} is required before running scoped live worker",
            MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN
        );
    }
    Ok(BTreeMap::from([
        ("IS_RUN_EXECUTION_WORKER", "true".to_string()),
        ("EXECUTION_WORKER_ONLY", "true".to_string()),
        ("EXECUTION_WORKER_RUN_ONCE", "true".to_string()),
        ("EXECUTION_WORKER_DRY_RUN", "false".to_string()),
        (
            "EXECUTION_WORKER_ID",
            "market_velocity_scoped_live_worker".to_string(),
        ),
        ("EXECUTION_WORKER_LEASE_LIMIT", "1".to_string()),
        ("EXECUTION_WORKER_TARGET_TASK_IDS", task_id.to_string()),
        ("EXECUTION_WORKER_TASK_TYPES", "execute_signal".to_string()),
        (
            "EXECUTION_WORKER_TASK_STATUSES",
            "pending,leased".to_string(),
        ),
        ("EXECUTION_WORKER_CONFIRMATION_MODE", "false".to_string()),
        ("EXECUTION_WORKER_RECONCILIATION_ONLY", "false".to_string()),
        ("EXECUTION_WORKER_REPORT_REPLAY_MODE", "false".to_string()),
        (
            "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
            MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN.to_string(),
        ),
    ]))
}
/// 执行 行情与市场数据 主流程，并把外部依赖调用、状态推进和错误返回串起来。
pub(super) async fn run_market_velocity_scoped_worker_once(
    task_id: i64,
    confirmation: &str,
) -> Result<usize> {
    let overrides = build_market_velocity_scoped_worker_env_overrides(task_id, confirmation)?;
    let _guard = EnvOverrideGuard::apply(&overrides);
    let worker = ExecutionWorker::from_env()?;
    worker.verify_live_audit_ready().await?;
    worker.run_once().await
}
struct EnvOverrideGuard {
    /// 列表数据。
    previous: Vec<(&'static str, Option<String>)>,
}
impl EnvOverrideGuard {
    /// 封装应用，减少行情数据调用方重复实现相同细节。
    fn apply(overrides: &BTreeMap<&'static str, String>) -> Self {
        let previous = overrides
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var(key).ok();
                std::env::set_var(key, value);
                (*key, previous)
            })
            .collect();
        Self { previous }
    }
}
impl Drop for EnvOverrideGuard {
    /// 封装释放，减少行情数据调用方重复实现相同细节。
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
/// 构造载荷字符串，集中维护行情数据的载荷组装规则。
fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(|value| match value {
        Value::String(raw) => Some(raw.trim().to_string()).filter(|value| !value.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}
/// 构造载荷i64，集中维护行情数据的载荷组装规则。
fn payload_i64(payload: &Value, key: &str) -> Option<i64> {
    payload.get(key).and_then(|value| match value {
        Value::Number(number) => number.as_i64(),
        Value::String(raw) => raw.trim().parse::<i64>().ok(),
        _ => None,
    })
}
/// 封装嵌套载荷i64，减少行情数据调用方重复实现相同细节。
fn nested_payload_i64(payload: &Value, parent: &str, key: &str) -> Option<i64> {
    payload
        .get(parent)
        .and_then(|value| payload_i64(value, key))
}
/// 构造载荷f64，集中维护行情数据的载荷组装规则。
fn payload_f64(payload: &Value, key: &str) -> Option<f64> {
    payload.get(key).and_then(|value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    })
}
/// 封装嵌套载荷f64，减少行情数据调用方重复实现相同细节。
fn nested_payload_f64(payload: &Value, parent: &str, key: &str) -> Option<f64> {
    payload
        .get(parent)
        .and_then(|value| payload_f64(value, key))
}
#[cfg(test)]
mod tests {
    use super::*;
    use rust_quant_services::rust_quan_web::{
        build_market_velocity_scoped_worker_handoff_readiness, ExecutionTask,
        MarketVelocityExecutionTaskLiveReadinessResponse, StrategySignalSubmitRequest,
    };
    use serde_json::json;
    /// 构造样例信号请求，集中维护行情数据的载荷组装规则。
    fn sample_signal_request() -> StrategySignalSubmitRequest {
        StrategySignalSubmitRequest {
            source: "rust_quant".to_string(),
            external_id: "rust_quant:market_velocity:2042663".to_string(),
            strategy_slug: "market_velocity".to_string(),
            strategy_key: "market_velocity:okx:ASTER-USDT-SWAP".to_string(),
            symbol: "ASTER-USDT-SWAP".to_string(),
            signal_type: "entry".to_string(),
            direction: "long".to_string(),
            title: "Market Velocity long signal ASTER-USDT-SWAP".to_string(),
            summary: None,
            confidence: Some(0.72),
            payload_json: json!({
                "source_signal_type": "market_velocity",
                "rank_event_id": 2042663,
                "exchange": "okx",
                "symbol": "ASTER-USDT-SWAP",
                "entry_rule_version": "rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1",
                "entry_filter": {
                    "entry_trigger_filter_version": "entry_trigger_allowlist_v1"
                },
                "risk_plan": {
                    "target_r": 2.4,
                    "max_holding_hours": 48
                }
            })
            .to_string(),
            generated_at: Some("2026-06-16T09:30:00Z".to_string()),
        }
    }
    /// 构造样例实盘taskreadiness，集中维护行情数据的载荷组装规则。
    fn sample_live_task_readiness(
        task_id: i64,
    ) -> MarketVelocityExecutionTaskLiveReadinessResponse {
        MarketVelocityExecutionTaskLiveReadinessResponse {
            read_only: true,
            mutation_allowed: false,
            owner_service: "quant_web".to_string(),
            status: "ready_for_live_worker".to_string(),
            task: sample_execution_task(task_id),
            checks: Vec::new(),
            blocker_codes: Vec::new(),
        }
    }
    /// 构造样例executiontask，集中维护行情数据的载荷组装规则。
    fn sample_execution_task(task_id: i64) -> ExecutionTask {
        ExecutionTask {
            id: task_id,
            news_signal_id: None,
            strategy_signal_id: Some(991),
            combo_id: 85,
            buyer_email: "buyer@example.com".to_string(),
            strategy_slug: "market_velocity".to_string(),
            symbol: "ASTER-USDT-SWAP".to_string(),
            task_type: "execute_signal".to_string(),
            task_status: "pending".to_string(),
            priority: 3,
            lease_owner: None,
            lease_until: None,
            scheduled_at: "2026-06-16T09:30:00Z".to_string(),
            request_payload_json: json!({
                "source_signal_type": "market_velocity",
                "strategy_slug": "market_velocity",
                "symbol": "ASTER-USDT-SWAP",
                "exchange": "okx",
                "auto_execution_allowed": true,
                "execution_policy": {
                    "mode": "live_execution_authorized",
                    "live_order_allowed": true,
                    "paper_trade_required": false,
                    "production_stage": "live_execution_allowed"
                },
                "side": "buy",
                "position_side": "long",
                "trade_side": "open",
                "order_type": "market",
                "execution": {
                    "exchange": "okx",
                    "symbol": "ASTER-USDT-SWAP",
                    "side": "buy",
                    "order_type": "market",
                    "size_usdt": 50.0,
                    "position_side": "long",
                    "position_mode": "net"
                },
                "risk_plan": {
                    "entry_price": 100.0,
                    "selected_stop_loss_price": 97.5,
                    "direction": "long",
                    "protective_stop_loss_required": true
                }
            }),
            created_at: "2026-06-16T09:30:00Z".to_string(),
            updated_at: "2026-06-16T09:30:00Z".to_string(),
        }
    }
    #[test]
    fn scoped_worker_auto_run_requires_exact_live_order_confirmation() {
        assert!(!market_velocity_scoped_worker_apply_authorized(false, None));
        assert!(!market_velocity_scoped_worker_apply_authorized(true, None));
        assert!(!market_velocity_scoped_worker_apply_authorized(
            true,
            Some("true")
        ));
        assert!(market_velocity_scoped_worker_apply_authorized(
            true,
            Some(MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN)
        ));
    }
    #[test]
    fn scoped_worker_env_overrides_force_one_reviewed_live_task() {
        let overrides = build_market_velocity_scoped_worker_env_overrides(
            2042663,
            MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM_TOKEN,
        )
        .expect("overrides");
        assert_eq!(overrides["IS_RUN_EXECUTION_WORKER"], "true");
        assert_eq!(overrides["EXECUTION_WORKER_ONLY"], "true");
        assert_eq!(overrides["EXECUTION_WORKER_RUN_ONCE"], "true");
        assert_eq!(overrides["EXECUTION_WORKER_DRY_RUN"], "false");
        assert_eq!(overrides["EXECUTION_WORKER_TARGET_TASK_IDS"], "2042663");
        assert_eq!(overrides["EXECUTION_WORKER_LEASE_LIMIT"], "1");
        assert_eq!(overrides["EXECUTION_WORKER_TASK_TYPES"], "execute_signal");
        assert_eq!(
            overrides["EXECUTION_WORKER_TASK_STATUSES"],
            "pending,leased"
        );
        assert_eq!(
            overrides["EXECUTION_WORKER_LIVE_ORDER_CONFIRM"],
            "I_UNDERSTAND_LIVE_ORDERS"
        );
    }
    #[test]
    fn scoped_worker_verifies_live_audit_before_run_once() {
        let source = include_str!("handoff.rs");
        let worker_start = source
            .find("async fn run_market_velocity_scoped_worker_once")
            .expect("scoped worker entrypoint should exist");
        let guard_start = source
            .find("struct EnvOverrideGuard")
            .expect("env override guard should follow scoped worker entrypoint");
        let worker_entrypoint = &source[worker_start..guard_start];
        assert!(
            worker_entrypoint
                .find("worker.verify_live_audit_ready().await?")
                .expect("scoped live worker should verify live audit readiness")
                < worker_entrypoint
                    .find("worker.run_once().await")
                    .expect("scoped live worker should run after readiness")
        );
    }
    #[test]
    fn preview_request_is_derived_from_rust_market_velocity_signal_payload() {
        let preview = build_market_velocity_live_preview_request(
            &sample_signal_request(),
            Some("buyer@example.com"),
            Some(85),
        )
        .expect("preview request");
        assert_eq!(preview.rank_event_id, Some(2042663));
        assert_eq!(preview.buyer_email.as_deref(), Some("buyer@example.com"));
        assert_eq!(preview.combo_id, Some(85));
        assert_eq!(preview.exchange, "okx");
        assert_eq!(preview.symbol, "ASTER-USDT-SWAP");
        assert_eq!(preview.target_r, 2.4);
        assert_eq!(preview.horizon_hours, 48);
        assert_eq!(
            preview.entry_rule_version.as_deref(),
            Some("rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1")
        );
        assert_eq!(
            preview.entry_trigger_filter_version.as_deref(),
            Some("entry_trigger_allowlist_v1")
        );
    }
    #[test]
    fn live_worker_manifest_reuses_existing_execution_worker_without_scripts() {
        let manifest = build_market_velocity_live_worker_manifest(228);
        let encoded = serde_json::to_string(&manifest).expect("manifest json");
        assert_eq!(
            manifest["execution_path"]["kind"],
            "existing_execution_worker"
        );
        assert_eq!(
            manifest["execution_path"]["reuse"],
            "vegas_style_execution_task_worker"
        );
        assert_eq!(
            manifest["execution_path"]["creates_new_order_system"],
            false
        );
        assert_eq!(
            manifest["next_worker_env"]["EXECUTION_WORKER_TARGET_TASK_IDS"],
            "228"
        );
        assert_eq!(
            manifest["next_worker_env"]["EXECUTION_WORKER_DRY_RUN"],
            "false"
        );
        assert!(
            !encoded.contains(".sh") && !encoded.contains("scripts/dev"),
            "production handoff manifest must not point to shell scripts: {encoded}"
        );
    }
    #[test]
    fn live_worker_handoff_includes_web_and_worker_readiness() {
        let web_readiness = sample_live_task_readiness(228);
        let worker_readiness =
            build_market_velocity_scoped_worker_handoff_readiness(&web_readiness);
        let handoff =
            build_market_velocity_live_worker_handoff(228, web_readiness, worker_readiness);
        assert_eq!(handoff["status"], "ready_for_scoped_live_worker");
        assert_eq!(handoff["read_only"], true);
        assert_eq!(handoff["mutation_allowed"], false);
        assert_eq!(
            handoff["manifest"]["execution_path"]["reuse"],
            "vegas_style_execution_task_worker"
        );
        assert_eq!(
            handoff["manifest"]["next_worker_env"]["EXECUTION_WORKER_TARGET_TASK_IDS"],
            "228"
        );
        assert_eq!(
            handoff["web_owner_readiness"]["status"],
            "ready_for_live_worker"
        );
        assert_eq!(
            handoff["worker_handoff_readiness"]["status"],
            "ready_for_scoped_live_worker"
        );
    }
    #[test]
    fn live_task_creation_requires_owner_scope() {
        assert!(market_velocity_required_live_owner_scope(None, Some(85)).is_err());
        assert!(
            market_velocity_required_live_owner_scope(Some("buyer@example.com"), None).is_err()
        );
        assert!(
            market_velocity_required_live_owner_scope(Some("buyer@example.com"), Some(85)).is_ok()
        );
    }
    #[test]
    fn live_task_signal_payload_is_scoped_to_owner_combo_before_submit() {
        let signal = market_velocity_scope_signal_to_live_owner(
            sample_signal_request(),
            "buyer@example.com",
            85,
        )
        .expect("scoped signal");
        let payload: Value = serde_json::from_str(&signal.payload_json).expect("payload json");
        assert_eq!(payload["target_buyer_email"], "buyer@example.com");
        assert_eq!(payload["target_combo_id"], 85);
        assert_eq!(
            payload["target_scope_source"],
            "market_velocity_live_handoff"
        );
    }
    #[test]
    fn task_creation_apply_requires_explicit_confirmation() {
        assert!(!market_velocity_task_creation_apply_authorized(
            true,
            Some("wrong")
        ));
        assert!(market_velocity_task_creation_apply_authorized(
            true,
            Some("I_UNDERSTAND_THIS_CREATES_WEB_EXECUTION_TASK")
        ));
        assert!(!market_velocity_task_creation_apply_authorized(false, None));
    }
}
