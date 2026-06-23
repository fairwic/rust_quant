use serde_json::Value;
use std::fs;
use std::path::PathBuf;
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}
fn load_schema() -> Value {
    let path = repo_root().join("docs/dev/full_product_health_artifact_schema.json");
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read schema {}: {err}", path.display()));
    serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("parse schema {}: {err}", path.display()))
}
#[test]
fn full_product_health_schema_registers_known_producer_alert_codes() {
    let schema = load_schema();
    let expected: &[(&str, &[&str])] = &[
        (
            "web_task_order_health",
            &[
                "WEB_EXECUTION_TASK_STALE",
                "WEB_ORDER_RESULT_MISSING",
                "WEB_RETRY_BACKLOG",
                "WEB_DELIVERY_BLOCKER",
                "WEB_INPUT_SKIPPED",
                "WEB_INPUT_QUERY_FAILED",
                "WEB_INPUT_QUERY_EMPTY",
                "WEB_INPUT_JSON_INVALID",
                "WEB_INPUT_OUTPUT_REJECTED",
            ],
        ),
        (
            "news_source_ai_health",
            &[
                "NEWS_SOURCE_DEGRADED",
                "NEWS_AI_PROVIDER_UNAVAILABLE",
                "NEWS_ANALYSIS_JOB_FAILED",
                "NEWS_ANALYSIS_JOB_STUCK",
                "NO_RECENT_AI_ANALYSIS",
                "NEWS_INPUT_SKIPPED",
                "NEWS_INPUT_QUERY_FAILED",
                "NEWS_INPUT_QUERY_EMPTY",
                "NEWS_INPUT_JSON_INVALID",
                "NEWS_INPUT_OUTPUT_REJECTED",
            ],
        ),
        (
            "quant_worker_checkpoint_audit",
            &[
                "QUANT_EXPECTED_WORKER_STALE",
                "QUANT_EXCHANGE_AUDIT_FAILURES",
                "QUANT_WORKER_LEASE_STALE",
                "QUANT_LOCAL_HEALTH_FAIL",
                "QUANT_LOCAL_HEALTH_WARN",
                "IGNORED_HISTORICAL_WORKER",
                "EXECUTION_AUDIT_TABLE_MISSING",
            ],
        ),
        (
            "admin_readiness",
            &[
                "ADMIN_LIVE_READINESS_BLOCKED",
                "ADMIN_HIGH_RISK_OPERATION_FAILED",
                "ADMIN_ACTION_AUDIT_MISSING",
                "ADMIN_READINESS_REVIEW_REQUIRED",
                "ADMIN_INPUT_SKIPPED",
                "ADMIN_INPUT_QUERY_FAILED",
                "ADMIN_INPUT_QUERY_EMPTY",
                "ADMIN_INPUT_JSON_INVALID",
                "ADMIN_INPUT_OUTPUT_REJECTED",
                "FULL_PRODUCT_HEALTH_SUMMARY_FAILED",
                "FULL_PRODUCT_INPUT_RUNNER_FAILED",
                "COLLECTOR_OUTPUT_REJECTED",
            ],
        ),
        (
            "payment_entitlement_health",
            &[
                "WALLET_PAYMENT_EXCEPTION",
                "PAYMENT_ENTITLEMENT_BLOCKED",
                "PAYMENT_INPUT_SKIPPED",
                "PAYMENT_INPUT_QUERY_FAILED",
            ],
        ),
    ];
    for (section, codes) in expected {
        let values = schema["alert_code_values"][section]
            .as_array()
            .unwrap_or_else(|| panic!("alert_code_values.{section} should be an array"));
        let metadata = schema["alert_code_metadata"][section]
            .as_object()
            .unwrap_or_else(|| panic!("alert_code_metadata.{section} should be an object"));
        for code in *codes {
            assert!(
                values.iter().any(|value| value == code),
                "schema should register producer alert code {section}.{code}"
            );
            let entry = metadata
                .get(*code)
                .unwrap_or_else(|| panic!("schema should register metadata for {section}.{code}"));
            for field in ["owner", "default_next_action", "admin_link_target"] {
                assert!(
                    entry[field].as_str().is_some_and(|value| !value.is_empty()),
                    "metadata {section}.{code}.{field} should be populated"
                );
            }
        }
    }
}
