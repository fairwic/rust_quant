use anyhow::{anyhow, bail, Result};
use rust_quant_services::market::MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_SLUG;
use rust_quant_services::rust_quan_web::{
    build_market_velocity_scoped_execution_worker_env,
    market_velocity_existing_execution_worker_path, ExecutionTask,
    MarketVelocityExecutionTaskCreationPreviewRequest,
    MarketVelocityExecutionTaskLiveReadinessResponse, MarketVelocityWorkerHandoffReadiness,
    StrategySignalSubmitRequest,
};
use serde_json::{json, Value};
/// 构建build市场动量实盘preview请求，集中维护行情数据的载荷和字段组装规则。
pub fn build_market_velocity_live_preview_request(
    signal: &StrategySignalSubmitRequest,
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<MarketVelocityExecutionTaskCreationPreviewRequest> {
    let payload: Value = serde_json::from_str(&signal.payload_json)
        .map_err(|error| anyhow!("parse market velocity signal payload_json failed: {error}"))?;
    let strategy_slug = signal.strategy_slug.trim();
    let source_signal_type = payload_string(&payload, "source_signal_type");
    match strategy_slug {
        "market_velocity" => {
            if source_signal_type.as_deref() != Some("market_velocity") {
                bail!("payload source_signal_type must be market_velocity");
            }
        }
        MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_SLUG => {
            if source_signal_type.as_deref() != Some(MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_SLUG)
            {
                bail!("payload source_signal_type must be market_velocity_breakdown_short");
            }
            if !breakdown_short_live_preview_payload_allowed(&payload) {
                bail!("breakdown short preview requires live-authorized execution_policy");
            }
        }
        _ => bail!("Market Velocity live handoff only accepts market_velocity strategies"),
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
/// 校验破位做空 preview payload 已显式进入 live 执行合同。
fn breakdown_short_live_preview_payload_allowed(payload: &Value) -> bool {
    let execution_policy = payload.get("execution_policy").unwrap_or(&Value::Null);
    payload
        .get("auto_execution_allowed")
        .and_then(Value::as_bool)
        == Some(true)
        && payload_string(execution_policy, "mode").as_deref() == Some("live_execution_authorized")
        && execution_policy
            .get("live_order_allowed")
            .and_then(Value::as_bool)
            == Some(true)
        && execution_policy
            .get("paper_trade_required")
            .and_then(Value::as_bool)
            == Some(false)
}
/// 构建 行情与市场数据 请求或响应载荷，把字段组装规则集中在同一入口。
pub fn build_market_velocity_live_worker_manifest(task_id: i64) -> Value {
    json!({
        "execution_path": market_velocity_existing_execution_worker_path(),
        "next_worker_env": build_market_velocity_scoped_execution_worker_env(task_id)
    })
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MarketVelocityHandoffLogContext {
    pub(super) external_id: String,
    pub(super) source_signal_type: String,
    pub(super) rank_event_id: Option<i64>,
    pub(super) strategy_signal_id: Option<i64>,
    pub(super) execution_task_id: Option<i64>,
    pub(super) combo_id: Option<i64>,
    pub(super) buyer_email: Option<String>,
    pub(super) exchange: String,
    pub(super) symbol: String,
}
pub(super) fn market_velocity_handoff_log_context(
    signal: &StrategySignalSubmitRequest,
    task: Option<&ExecutionTask>,
) -> MarketVelocityHandoffLogContext {
    let payload = serde_json::from_str::<Value>(&signal.payload_json).unwrap_or(Value::Null);
    let task_payload = task.map(|task| &task.request_payload_json);
    let source_signal_type = payload_string(&payload, "source_signal_type")
        .or_else(|| task_payload.and_then(|payload| payload_string(payload, "source_signal_type")))
        .unwrap_or_else(|| "unknown".to_string());
    let exchange = payload_string(&payload, "exchange")
        .or_else(|| task_payload.and_then(|payload| payload_string(payload, "exchange")))
        .unwrap_or_else(|| {
            signal
                .strategy_key
                .split(':')
                .nth(1)
                .unwrap_or("")
                .to_string()
        })
        .trim()
        .to_ascii_lowercase();
    let symbol = payload_string(&payload, "symbol")
        .or_else(|| task_payload.and_then(|payload| payload_string(payload, "symbol")))
        .unwrap_or_else(|| signal.symbol.clone())
        .trim()
        .to_ascii_uppercase();
    MarketVelocityHandoffLogContext {
        external_id: signal.external_id.clone(),
        source_signal_type,
        rank_event_id: payload_i64(&payload, "rank_event_id")
            .or_else(|| task_payload.and_then(|payload| payload_i64(payload, "rank_event_id"))),
        strategy_signal_id: task.and_then(|task| task.strategy_signal_id),
        execution_task_id: task.map(|task| task.id),
        combo_id: task.map(|task| task.combo_id),
        buyer_email: task.map(|task| task.buyer_email.clone()),
        exchange,
        symbol,
    }
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
            && worker_handoff_readiness.status == "ready_for_live_worker" {
            "ready_for_live_worker"
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
/// 判断 live handoff 是否配置了完整 owner scope；未配置时保持广播信号，由 Web 按订阅 fan-out。
pub fn market_velocity_live_owner_scope(
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<Option<(&str, i64)>> {
    match (
        buyer_email.map(str::trim).filter(|value| !value.is_empty()),
        combo_id.filter(|value| *value > 0),
    ) {
        (None, None) => Ok(None),
        (Some(buyer_email), Some(combo_id)) => Ok(Some((buyer_email, combo_id))),
        (Some(_), None) => {
            bail!("MARKET_VELOCITY_LIVE_COMBO_ID is required when buyer scope is configured")
        }
        (None, Some(_)) => {
            bail!("MARKET_VELOCITY_LIVE_BUYER_EMAIL is required when combo scope is configured")
        }
    }
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
/// 仅在配置了完整 owner scope 时给信号绑定单一用户；否则保留未绑定信号，让 Web owner service fan-out。
pub fn market_velocity_scope_signal_to_live_owner_if_configured(
    signal: StrategySignalSubmitRequest,
    buyer_email: Option<&str>,
    combo_id: Option<i64>,
) -> Result<StrategySignalSubmitRequest> {
    match market_velocity_live_owner_scope(buyer_email, combo_id)? {
        Some((buyer_email, combo_id)) => {
            market_velocity_scope_signal_to_live_owner(signal, buyer_email, combo_id)
        }
        None => Ok(signal),
    }
}
/// 未绑定用户的广播信号只忽略“缺少 user context”类 preview blocker；产品、符号和交易所能力仍阻断。
pub fn market_velocity_handoff_hard_preview_blockers(
    blocker_codes: &[String],
    owner_scope_configured: bool,
) -> Vec<String> {
    blocker_codes
        .iter()
        .filter(|code| {
            owner_scope_configured
                || !matches!(
                    code.as_str(),
                    "user_context_missing_for_risk_filters"
                        | "user_context_missing_for_entitlement"
                        | "user_context_missing_for_api_key_readiness"
                        | "user_context_missing_for_signed_preflight"
                )
        })
        .cloned()
        .collect()
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
    fn preview_request_rejects_breakdown_short_strategy_before_live_handoff_cutover() {
        let mut signal = sample_signal_request();
        signal.strategy_slug = "market_velocity_breakdown_short".to_string();
        signal.direction = "short".to_string();
        signal.payload_json = json!({
            "source_signal_type": "market_velocity_breakdown_short",
            "rank_event_id": 2042663,
            "exchange": "okx",
            "symbol": "ASTER-USDT-SWAP",
            "entry_rule_version": "rank_radar_15m_short_r0375_10r_15msup_brkdn_d5_72_p1p5_12_v1",
            "risk_plan": {
                "direction": "short",
                "target_r": 1.0,
                "max_holding_hours": 48
            }
        })
        .to_string();

        let error = build_market_velocity_live_preview_request(
            &signal,
            Some("buyer@example.com"),
            Some(85),
        )
        .expect_err("breakdown short must not be accepted by live handoff yet");
        assert!(error
            .to_string()
            .contains("requires live-authorized execution_policy"));
    }
    #[test]
    fn preview_request_accepts_live_authorized_breakdown_short_signal() {
        let mut signal = sample_signal_request();
        signal.external_id = "rust_quant:market_velocity_breakdown_short:2042663".to_string();
        signal.strategy_slug = "market_velocity_breakdown_short".to_string();
        signal.strategy_key = "market_velocity_breakdown_short:okx:ASTER-USDT-SWAP".to_string();
        signal.direction = "short".to_string();
        signal.payload_json = json!({
            "source_signal_type": "market_velocity_breakdown_short",
            "rank_event_id": 2042663,
            "exchange": "okx",
            "symbol": "ASTER-USDT-SWAP",
            "entry_rule_version": "rank_radar_15m_short_r04_10r_15msup_brkdn_d5_100_p2_12_vol10_d14_v6",
            "auto_execution_allowed": true,
            "execution_policy": {
                "mode": "live_execution_authorized",
                "live_order_allowed": true,
                "paper_trade_required": false,
                "production_stage": "live_execution_allowed"
            },
            "risk_plan": {
                "direction": "short",
                "target_r": 1.0,
                "max_holding_hours": 24,
                "selected_stop_loss_price": 3536.0,
                "protective_stop_loss_required": true
            }
        })
        .to_string();

        let request = build_market_velocity_live_preview_request(
            &signal,
            Some("buyer@example.com"),
            Some(85),
        )
        .expect("live-authorized breakdown short should be accepted by preview");

        assert_eq!(request.rank_event_id, Some(2042663));
        assert_eq!(request.exchange, "okx");
        assert_eq!(request.symbol, "ASTER-USDT-SWAP");
        assert_eq!(request.target_r, 1.0);
        assert_eq!(request.horizon_hours, 24);
        assert_eq!(
            request.entry_rule_version.as_deref(),
            Some("rank_radar_15m_short_r04_10r_15msup_brkdn_d5_100_p2_12_vol10_d14_v6")
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
        assert_eq!(manifest["next_worker_env"]["reference_task_id"], 228);
        assert!(manifest["next_worker_env"]
            .get("EXECUTION_WORKER_TARGET_TASK_IDS")
            .is_none());
        assert!(manifest["next_worker_env"]
            .get("EXECUTION_WORKER_DRY_RUN")
            .is_none());
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
        assert_eq!(handoff["status"], "ready_for_live_worker");
        assert_eq!(handoff["read_only"], true);
        assert_eq!(handoff["mutation_allowed"], false);
        assert_eq!(
            handoff["manifest"]["execution_path"]["reuse"],
            "vegas_style_execution_task_worker"
        );
        assert_eq!(
            handoff["manifest"]["next_worker_env"]["reference_task_id"],
            228
        );
        assert_eq!(
            handoff["web_owner_readiness"]["status"],
            "ready_for_live_worker"
        );
        assert_eq!(
            handoff["worker_handoff_readiness"]["status"],
            "ready_for_live_worker"
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
    fn live_task_signal_payload_stays_unscoped_without_owner_combo() {
        let signal = market_velocity_scope_signal_to_live_owner_if_configured(
            sample_signal_request(),
            None,
            None,
        )
        .expect("unscoped signal");
        let payload: Value = serde_json::from_str(&signal.payload_json).expect("payload json");
        assert!(payload.get("target_buyer_email").is_none());
        assert!(payload.get("target_combo_id").is_none());
        assert!(payload.get("target_scope_source").is_none());
    }
    #[test]
    fn live_task_signal_payload_is_scoped_to_owner_combo_before_submit() {
        let signal = market_velocity_scope_signal_to_live_owner_if_configured(
            sample_signal_request(),
            Some("buyer@example.com"),
            Some(85),
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
    fn live_task_signal_payload_rejects_partial_owner_scope() {
        assert!(market_velocity_scope_signal_to_live_owner_if_configured(
            sample_signal_request(),
            Some("buyer@example.com"),
            None,
        )
        .is_err());
        assert!(market_velocity_scope_signal_to_live_owner_if_configured(
            sample_signal_request(),
            None,
            Some(85),
        )
        .is_err());
    }
    #[test]
    fn unscoped_preview_ignores_user_context_missing_blockers_only() {
        let blockers = vec![
            "user_context_missing_for_risk_filters".to_string(),
            "user_context_missing_for_entitlement".to_string(),
            "user_context_missing_for_api_key_readiness".to_string(),
            "user_context_missing_for_signed_preflight".to_string(),
            "strategy_product_not_published".to_string(),
        ];
        assert_eq!(
            market_velocity_handoff_hard_preview_blockers(&blockers, false),
            vec!["strategy_product_not_published".to_string()]
        );
        assert_eq!(
            market_velocity_handoff_hard_preview_blockers(&blockers, true),
            blockers
        );
    }
    #[test]
    fn market_velocity_handoff_log_context_carries_task_chain_identifiers() {
        let signal = sample_signal_request();
        let task = sample_execution_task(228);
        let context = market_velocity_handoff_log_context(&signal, Some(&task));

        assert_eq!(context.external_id, "rust_quant:market_velocity:2042663");
        assert_eq!(context.rank_event_id, Some(2042663));
        assert_eq!(context.strategy_signal_id, Some(991));
        assert_eq!(context.execution_task_id, Some(228));
        assert_eq!(context.combo_id, Some(85));
        assert_eq!(context.buyer_email.as_deref(), Some("buyer@example.com"));
        assert_eq!(context.exchange, "okx");
        assert_eq!(context.symbol, "ASTER-USDT-SWAP");
        assert_eq!(context.source_signal_type, "market_velocity");
    }
}
