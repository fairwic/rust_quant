#![allow(dead_code)]

use serde_json::Value;
use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

include!("local_service_health_contract/paths_section.rs");

fn read_script() -> String {
    let path = script_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_aggregator_fixture_script() -> String {
    let path = aggregator_fixture_script_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_aggregator_runner_script() -> String {
    let path = aggregator_runner_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_input_runner_script() -> String {
    let path = full_product_input_runner_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_summary_script() -> String {
    let path = full_product_summary_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_markdown_script() -> String {
    let path = full_product_markdown_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_ci_wrapper_script() -> String {
    let path = full_product_ci_wrapper_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_artifact_validator_script() -> String {
    let path = full_product_artifact_validator_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_artifact_set_publisher_script() -> String {
    let path = full_product_artifact_set_publisher_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_admin_ingest_smoke_script() -> String {
    let path = full_product_admin_ingest_smoke_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_admin_ingest_mock_receiver_script() -> String {
    let path = full_product_admin_ingest_mock_receiver_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_admin_ingest_contract_smoke_script() -> String {
    let path = full_product_admin_ingest_contract_smoke_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_web_input_producer_script() -> String {
    let path = web_input_producer_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_payment_input_producer_script() -> String {
    let path = payment_input_producer_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_full_product_payment_artifact_smoke_script() -> String {
    let path = full_product_payment_artifact_smoke_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_news_input_producer_script() -> String {
    let path = news_input_producer_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn read_admin_input_producer_script() -> String {
    let path = admin_input_producer_path();
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}

fn temp_json_file(prefix: &str, body: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let path = env::temp_dir().join(format!("{prefix}-{}-{unique}.json", std::process::id()));
    fs::write(&path, body)
        .unwrap_or_else(|error| panic!("failed to write {}: {}", path.display(), error));
    path
}

fn temp_artifact_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));
    dir
}

fn write_phase45_validator_schema(
    path: &Path,
    status_values: &[&str],
    severity_values: &[&str],
    full_report_required: &[&str],
    summary_required: &[&str],
    markdown_markers: &[&str],
) {
    let schema = serde_json::json!({
        "schema_version": 1,
        "status_values": status_values,
        "severity_values": severity_values,
        "operator_action_values": [
            "block_release_until_resolved",
            "manual_review_before_release",
            "observe_only"
        ],
        "artifact_schemas": {
            "full_report": {
                "required_top_level": full_report_required,
                "required_summary_fields": ["p0_count", "p1_count", "info_count"],
                "append_only_paths": ["summary.*", "sections.*", "alerts[].*", "correlation.*"]
            },
            "summary": {
                "required_top_level": summary_required,
                "required_summary_fields": ["overall_status", "p0_count", "p1_count", "info_count"],
                "append_only_paths": [
                    "summary.*",
                    "section_statuses.*",
                    "checklist[].*",
                    "top_alerts[].*",
                    "required_operator_actions[].*",
                    "correlation.*",
                    "correlation_ids[].*"
                ]
            },
            "validation": {
                "required_top_level": ["schema_version", "status", "generated_at", "summary", "artifacts", "findings"],
                "required_summary_fields": [
                    "artifact_count",
                    "missing_artifact_count",
                    "json_parse_error_count",
                    "missing_required_field_count",
                    "sensitive_marker_count",
                    "finding_count"
                ],
                "append_only_paths": ["summary.*", "artifacts.*", "findings[].*"]
            }
        },
        "markdown_required_markers": markdown_markers
    });
    fs::write(
        path,
        serde_json::to_string_pretty(&schema).expect("schema json"),
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", path.display(), error));
}

fn write_phase45_valid_artifacts(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let full_report_path = dir.join("full-product-health.json");
    let summary_path = dir.join("full-product-health-summary.json");
    let markdown_path = dir.join("full-product-health.md");

    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "ok",
  "generated_at": "2026-05-07T01:00:00Z",
  "summary": {"p0_count": 0, "p1_count": 0, "info_count": 0, "read_only_input_count": 4},
  "sections": {},
  "alerts": [],
  "alert_taxonomy": [],
  "correlation": {}
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", full_report_path.display(), error));
    fs::write(
        &summary_path,
        r#"{
  "schema_version": 1,
  "source_schema_version": 1,
  "status": "ok",
  "generated_at": "2026-05-07T01:00:01Z",
  "source_generated_at": "2026-05-07T01:00:00Z",
  "summary": {
    "overall_status": "ok",
    "p0_count": 0,
    "p1_count": 0,
    "info_count": 0,
    "section_count": 0,
    "blocking_section_count": 0,
    "warning_section_count": 0,
    "top_alert_count": 0,
    "required_operator_action_count": 0,
    "read_only_input_count": 0
  },
  "section_statuses": {},
  "checklist": [],
  "top_alerts": [],
  "required_operator_actions": [],
  "alert_taxonomy": [],
  "operator_playbook_summary": {
    "item_count": 0,
    "blocking_item_count": 0,
    "manual_review_item_count": 0,
    "observe_only_item_count": 0,
    "items": []
  },
  "correlation": {},
  "correlation_ids": []
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** ok\n\n## Counts\n\n## Top Alerts\n\n## Operator Playbook Summary\n\n## Checklist\n\n## Artifact Paths\n\n## Skipped Sections\n",
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", markdown_path.display(), error));

    (full_report_path, summary_path, markdown_path)
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap_or_else(|error| {
        panic!("failed to write {}: {}", path.display(), error);
    });
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .unwrap_or_else(|error| panic!("failed to stat {}: {}", path.display(), error))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap_or_else(|error| {
            panic!("failed to chmod {}: {}", path.display(), error);
        });
    }
}

fn schema_string_array(schema: &Value, key: &str) -> Vec<String> {
    schema[key]
        .as_array()
        .unwrap_or_else(|| panic!("schema field {key} should be an array"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("schema field {key} should contain only strings"))
                .to_owned()
        })
        .collect()
}

fn artifact_schema_string_array(schema: &Value, artifact: &str, key: &str) -> Vec<String> {
    schema["artifact_schemas"][artifact][key]
        .as_array()
        .unwrap_or_else(|| panic!("schema artifact {artifact}.{key} should be an array"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| {
                    panic!("schema artifact {artifact}.{key} should contain only strings")
                })
                .to_owned()
        })
        .collect()
}

fn assert_required_top_level_fields(schema: &Value, artifact: &str, payload: &Value) {
    for field in artifact_schema_string_array(schema, artifact, "required_top_level") {
        assert!(
            payload.get(&field).is_some(),
            "{artifact} example should contain required top-level field {field}"
        );
    }
}

fn assert_required_nested_fields(
    schema: &Value,
    artifact: &str,
    nested_name: &str,
    payload: &Value,
) {
    let schema_key = format!("required_{nested_name}_fields");
    let object = payload
        .as_object()
        .unwrap_or_else(|| panic!("{artifact}.{nested_name} example should be an object"));
    for field in artifact_schema_string_array(schema, artifact, &schema_key) {
        assert!(
            object.contains_key(&field),
            "{artifact}.{nested_name} example should contain required field {field}"
        );
    }
}

fn assert_enum_value(schema: &Value, enum_key: &str, value: &Value, label: &str) {
    let allowed = schema_string_array(schema, enum_key);
    let actual = value
        .as_str()
        .unwrap_or_else(|| panic!("{label} should be a string enum value"));
    assert!(
        allowed.iter().any(|allowed| allowed == actual),
        "{label} should be one of {allowed:?}, got {actual}"
    );
}

fn assert_json_array_enum(schema: &Value, enum_key: &str, items: &Value, field: &str, label: &str) {
    let allowed = schema_string_array(schema, enum_key);
    let items = items
        .as_array()
        .unwrap_or_else(|| panic!("{label} should be an array"));
    for item in items {
        let actual = item[field]
            .as_str()
            .unwrap_or_else(|| panic!("{label}.{field} should be a string enum value"));
        assert!(
            allowed.iter().any(|allowed| allowed == actual),
            "{label}.{field} should be one of {allowed:?}, got {actual}"
        );
    }
}

fn alert_metadata<'a>(schema: &'a Value, section: &str, code: &str) -> &'a Value {
    schema["alert_code_metadata"]
        .get(section)
        .and_then(|metadata| metadata.get(code))
        .or_else(|| {
            schema["alert_code_metadata"]
                .get("global")
                .and_then(|metadata| metadata.get(code))
        })
        .unwrap_or_else(|| {
            panic!("alert_code_metadata.{section}.{code} or alert_code_metadata.global.{code} should exist")
        })
}

fn assert_alert_code_metadata_alignment(schema: &Value) {
    let code_values = schema["alert_code_values"]
        .as_object()
        .expect("alert_code_values should be an object");
    let metadata = schema["alert_code_metadata"]
        .as_object()
        .expect("alert_code_metadata should be an object");

    for (section, codes) in code_values {
        let codes = codes
            .as_array()
            .unwrap_or_else(|| panic!("alert_code_values.{section} should be an array"));
        let code_strings: Vec<&str> = codes
            .iter()
            .map(|code| {
                code.as_str()
                    .unwrap_or_else(|| panic!("alert_code_values.{section} should contain strings"))
            })
            .collect();
        let section_metadata = metadata
            .get(section)
            .and_then(|value| value.as_object())
            .unwrap_or_else(|| panic!("alert_code_metadata.{section} should be an object"));

        for code in &code_strings {
            let item = section_metadata
                .get(*code)
                .and_then(|value| value.as_object())
                .unwrap_or_else(|| {
                    panic!("alert_code_metadata.{section}.{code} should be an object")
                });
            for field in ["owner", "default_next_action", "admin_link_target"] {
                let value = item
                    .get(field)
                    .and_then(|value| value.as_str())
                    .unwrap_or_else(|| {
                        panic!("alert_code_metadata.{section}.{code}.{field} should be a string")
                    });
                assert!(
                    !value.is_empty() && !value.contains('/') && !value.contains("://"),
                    "alert_code_metadata.{section}.{code}.{field} should be a safe stable key"
                );
            }
        }

        for code in section_metadata.keys() {
            assert!(
                code_strings
                    .iter()
                    .any(|registered| *registered == code.as_str()),
                "metadata code {section}.{code} should also be listed in alert_code_values"
            );
        }
    }
}

fn assert_alert_taxonomy_metadata_matches_registry(schema: &Value, items: &Value, label: &str) {
    let items = items
        .as_array()
        .unwrap_or_else(|| panic!("{label} should be an array"));
    for item in items {
        let section = item["section"]
            .as_str()
            .unwrap_or_else(|| panic!("{label}.section should be a string"));
        let code = item["code"]
            .as_str()
            .unwrap_or_else(|| panic!("{label}.code should be a string"));
        let metadata = alert_metadata(schema, section, code);
        for field in ["owner", "default_next_action", "admin_link_target"] {
            assert_eq!(
                item[field], metadata[field],
                "{label} {section}.{code} should mirror schema {field}"
            );
        }
    }
}

fn fake_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "local-service-health-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));

    write_executable(
        &dir.join("curl"),
        r#"#!/usr/bin/env bash
set -euo pipefail
output_file=""
while (($# > 0)); do
    case "$1" in
        -o)
            output_file="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done
if [[ -n "${output_file}" ]]; then
    : > "${output_file}"
fi
printf '200'
"#,
    );
    write_executable(
        &dir.join("psql"),
        r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" == *"to_regclass('public.exchange_request_audit_logs')"* ]]; then
    printf 'public.exchange_request_audit_logs\n'
elif [[ "$*" == *"FROM exchange_request_audit_logs"* ]]; then
    printf '4|2|250\n'
elif [[ "$*" == *"FROM execution_worker_checkpoints"* && "$*" == *"stale_leased_workers"* ]]; then
    printf '2|1\n'
elif [[ "$*" == *"execution_worker_checkpoints"* ]]; then
    printf 'worker_stale|idle|42|2026-01-01 00:00:00+00|120\n'
    printf 'worker_fresh|idle||2026-01-01 00:01:00+00|5\n'
else
    printf '1\n'
fi
"#,
    );

    dir
}

fn fake_web_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "web-health-input-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));

    write_executable(
        &dir.join("psql"),
        r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
if [[ "${args}" != *"-Atc"* ]]; then
    printf 'expected psql to run in tuples-only mode\n' >&2
    exit 2
fi
for forbidden in \
    "INSERT INTO " \
    "UPDATE " \
    "DELETE FROM " \
    "/api/commerce/internal/execution-tasks/lease" \
    "/api/commerce/internal/execution-results" \
    "/api/commerce/internal/order-results" \
    "/fapi/v1/order" \
    "/fapi/v2/account" \
    "/fapi/v1/positionRisk"
do
    if [[ "${args}" == *"${forbidden}"* ]]; then
        printf 'forbidden marker reached fake psql: %s\n' "${forbidden}" >&2
        exit 3
    fi
done
cat <<'JSON'
{"status":"fail","source":"quant_web_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":3600,"stale_task_secs":900,"missing_result_secs":900,"open_task_count":2,"stale_task_count":1,"missing_order_result_count":1,"failed_task_count":1,"retry_backlog_count":1,"delivery_blocker_count":1,"recent_order_result_count":3,"recent_trade_record_count":2,"sample":{"signal_inbox_id":3801,"execution_task_id":5202,"execution_attempt_id":6101,"order_result_id":null,"trade_record_id":null,"source_signal_type":"news_event","task_status":"completed","age_secs":960},"alerts":[{"severity":"P0","code":"WEB_ORDER_RESULT_MISSING","section":"web_task_order_health","message":"completed execution task missing order result","execution_task_id":5202,"order_result_id":null,"source_signal_type":"news_event"},{"severity":"P1","code":"WEB_RETRY_BACKLOG","section":"web_task_order_health","message":"recent execution task retries need review","execution_task_id":5202,"order_result_id":null,"source_signal_type":"news_event"}],"correlation":{"signal_inbox_id":3801,"execution_task_id":5202,"execution_attempt_id":6101,"order_result_id":null,"trade_record_id":null,"source_signal_type":"news_event"}}
JSON
"#,
    );

    dir
}

fn fake_news_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "news-health-input-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));

    write_executable(
        &dir.join("psql"),
        r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
if [[ "${args}" != *"-Atc"* ]]; then
    printf 'expected psql to run in tuples-only mode\n' >&2
    exit 2
fi
for required in \
    "news_source_states" \
    "news_ai_analysis_results" \
    "news_analysis_jobs" \
    "news_provider_call_logs" \
    "news_items_jinse"
do
    if [[ "${args}" != *"${required}"* ]]; then
        printf 'expected news health query to read %s\n' "${required}" >&2
        exit 4
    fi
done
for forbidden in \
    "INSERT INTO " \
    "UPDATE " \
    "DELETE FROM " \
    "request_json" \
    "response_json" \
    "response_text" \
    "raw_response" \
    "/fapi/v1/order" \
    "/api/commerce/internal/execution-tasks/lease"
do
    if [[ "${args}" == *"${forbidden}"* ]]; then
        printf 'forbidden marker reached fake psql: %s\n' "${forbidden}" >&2
        exit 3
    fi
done
cat <<'JSON'
{"status":"warn","source":"quant_news_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"stale_analysis_secs":1800,"failed_job_secs":7200,"source_failure_threshold":3,"source_count":4,"degraded_source_count":2,"paused_source_count":1,"retryable_source_count":1,"recent_news_count":12,"signal_candidate_count":3,"recent_ai_analysis_count":5,"actionable_analysis_count":2,"failed_analysis_job_count":1,"stuck_analysis_job_count":1,"provider_failure_count":1,"active_prompt_config_count":1,"sample":{"source":"theblockbeats","effective_status":"paused","consecutive_failures":4,"news_id":"jinse-20260507-001","analysis_result_id":9001,"analysis_signal":"buy"},"alerts":[{"severity":"P1","code":"NEWS_SOURCE_DEGRADED","section":"news_source_ai_health","message":"one or more news sources are degraded, paused, or retryable"},{"severity":"P1","code":"NEWS_AI_PROVIDER_UNAVAILABLE","section":"news_source_ai_health","message":"recent AI provider calls failed or active prompt config is missing"},{"severity":"P1","code":"NEWS_ANALYSIS_JOB_FAILED","section":"news_source_ai_health","message":"recent news analysis jobs failed"}],"correlation":{"news_id":"jinse-20260507-001","analysis_result_id":9001,"external_id":null}}
JSON
"#,
    );

    dir
}

fn fake_admin_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "admin-health-input-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));

    write_executable(
        &dir.join("psql"),
        r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
if [[ "${args}" != *"-Atc"* ]]; then
    printf 'expected psql to run in tuples-only mode\n' >&2
    exit 2
fi
for required in \
    "admin_operation_logs" \
    "risk_review_confirm" \
    "api_key_upsert" \
    "onchain_provider_control_upsert" \
    "strategy_config_upsert" \
    "backtest_run" \
    "exchange_symbol_sync" \
    "manual_ai_analysis"
do
    if [[ "${args}" != *"${required}"* ]]; then
        printf 'expected admin health query to read %s\n' "${required}" >&2
        exit 4
    fi
done
for forbidden in \
    "INSERT INTO " \
    "UPDATE " \
    "DELETE FROM " \
    "api_key_cipher" \
    "api_secret_cipher" \
    "passphrase_cipher" \
    "request_payload" \
    "response_payload" \
    "raw_payload" \
    "/fapi/v1/order" \
    "/api/commerce/internal/execution-tasks/lease"
do
    if [[ "${args}" == *"${forbidden}"* ]]; then
        printf 'forbidden marker reached fake psql: %s\n' "${forbidden}" >&2
        exit 3
    fi
done
cat <<'JSON'
{"status":"fail","source":"quant_admin_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"required_action_count":8,"recent_operation_count":11,"high_risk_operation_count":9,"failed_operation_count":2,"missing_required_action_count":1,"readiness_blocker_count":1,"manual_review_count":2,"sample":{"admin_operation_log_id":"admin-op-9002","module":"quant_exchange_symbols","action":"exchange_symbol_sync","outcome":"failed","age_secs":180},"alerts":[{"severity":"P0","code":"ADMIN_LIVE_READINESS_BLOCKED","section":"admin_readiness","message":"admin readiness has blockers or required audit coverage is missing"},{"severity":"P1","code":"ADMIN_HIGH_RISK_OPERATION_FAILED","section":"admin_readiness","message":"recent high-risk admin operation failed"},{"severity":"P1","code":"ADMIN_ACTION_AUDIT_MISSING","section":"admin_readiness","message":"one or more required high-risk admin actions have no recent audit log"},{"severity":"P1","code":"ADMIN_READINESS_REVIEW_REQUIRED","section":"admin_readiness","message":"admin readiness still requires manual review"}],"correlation":{"admin_operation_log_id":"admin-op-9002","admin_module":"quant_exchange_symbols","admin_action":"exchange_symbol_sync"}}
JSON
"#,
    );

    dir
}

fn fake_full_product_input_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "full-product-input-runner-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));

    write_executable(
        &dir.join("psql"),
        r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
if [[ "${args}" != *"-Atc"* ]]; then
    printf 'expected psql to run in tuples-only mode\n' >&2
    exit 2
fi
for forbidden in \
    "INSERT INTO " \
    "UPDATE " \
    "DELETE FROM " \
    "/api/commerce/internal/execution-tasks/lease" \
    "/api/commerce/internal/execution-results" \
    "/api/commerce/internal/order-results" \
    "/fapi/v1/order" \
    "/fapi/v2/account" \
    "/fapi/v1/positionRisk" \
    "request_payload" \
    "response_payload" \
    "raw_payload" \
    "api_key_cipher" \
    "api_secret_cipher" \
    "passphrase_cipher" \
    "request_json" \
    "response_json" \
    "response_text" \
    "raw_response"
do
    if [[ "${args}" == *"${forbidden}"* ]]; then
        printf 'forbidden marker reached fake psql: %s\n' "${forbidden}" >&2
        exit 3
    fi
done
if [[ "${args}" == *"payment_intents"* ]]; then
    cat <<'JSON'
{"status":"fail","source":"quant_web_payment_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":86400,"confirmation_timeout_secs":1800,"wallet_payment_exception_count":2,"payment_entitlement_blocker_count":1,"alerts":[{"severity":"P1","code":"WALLET_PAYMENT_EXCEPTION","section":"payment_entitlement_health","message":"wallet payment exceptions require review","metadata":{"wallet_payment_exception_count":2,"payment_entitlement_blocker_count":1,"sample_kind":"wallet_payment_exception"}},{"severity":"P0","code":"PAYMENT_ENTITLEMENT_BLOCKED","section":"payment_entitlement_health","message":"wallet payment succeeded but entitlement is still blocked","metadata":{"wallet_payment_exception_count":2,"payment_entitlement_blocker_count":1,"sample_kind":"payment_entitlement"}}],"correlation":{"payment_exception_id":2001,"entitlement_check_id":1001,"user_id":null}}
JSON
elif [[ "${args}" == *"admin_operation_logs"* ]]; then
    cat <<'JSON'
{"status":"fail","source":"quant_admin_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"required_action_count":8,"recent_operation_count":11,"high_risk_operation_count":9,"failed_operation_count":2,"missing_required_action_count":1,"readiness_blocker_count":1,"manual_review_count":2,"alerts":[{"severity":"P0","code":"ADMIN_LIVE_READINESS_BLOCKED","section":"admin_readiness","message":"admin readiness has blockers or required audit coverage is missing"}],"correlation":{"admin_operation_log_id":"admin-op-9002","admin_module":"quant_exchange_symbols","admin_action":"exchange_symbol_sync"}}
JSON
elif [[ "${args}" == *"news_ai_analysis_results"* ]]; then
    cat <<'JSON'
{"status":"warn","source":"quant_news_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"stale_analysis_secs":1800,"failed_job_secs":7200,"source_failure_threshold":3,"source_count":4,"degraded_source_count":2,"paused_source_count":1,"retryable_source_count":1,"recent_news_count":12,"signal_candidate_count":3,"recent_ai_analysis_count":5,"actionable_analysis_count":2,"failed_analysis_job_count":1,"stuck_analysis_job_count":1,"provider_failure_count":1,"active_prompt_config_count":1,"alerts":[{"severity":"P1","code":"NEWS_SOURCE_DEGRADED","section":"news_source_ai_health","message":"one or more news sources are degraded, paused, or retryable"}],"correlation":{"news_id":"jinse-20260507-001","analysis_result_id":9001,"external_id":null}}
JSON
else
    cat <<'JSON'
{"status":"fail","source":"quant_web_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":3600,"stale_task_secs":900,"missing_result_secs":900,"open_task_count":2,"stale_task_count":1,"missing_order_result_count":1,"failed_task_count":1,"retry_backlog_count":1,"delivery_blocker_count":1,"recent_order_result_count":3,"recent_trade_record_count":2,"alerts":[{"severity":"P0","code":"WEB_ORDER_RESULT_MISSING","section":"web_task_order_health","message":"completed execution task missing order result","execution_task_id":5202,"order_result_id":null,"source_signal_type":"news_event"}],"correlation":{"signal_inbox_id":3801,"execution_task_id":5202,"execution_attempt_id":6101,"order_result_id":null,"trade_record_id":null,"source_signal_type":"news_event"}}
JSON
fi
"#,
    );

    dir
}

fn fake_payment_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "payment-health-input-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {}", dir.display(), error));

    write_executable(
        &dir.join("psql"),
        r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
if [[ "${args}" != *"-Atc"* ]]; then
    printf 'expected psql to run in tuples-only mode\n' >&2
    exit 2
fi
for required in \
    "payment_intents" \
    "payment_transactions" \
    "membership_orders" \
    "intents.order_type = 'membership'"
do
    if [[ "${args}" != *"${required}"* ]]; then
        printf 'expected payment health query to read %s\n' "${required}" >&2
        exit 4
    fi
done
if [[ "${args}" == *"intents.order_type = 'membership_order'"* ]]; then
    printf 'payment health query must use the real membership payment order_type\n' >&2
    exit 4
fi
for forbidden in \
    "INSERT INTO " \
    "UPDATE " \
    "DELETE FROM " \
    "raw_payload_json" \
    "metadata_json" \
    "external_tx_id" \
    "payer_ref" \
    "payee_ref" \
    "failure_reason" \
    "/api/commerce/internal/execution-tasks/lease" \
    "/api/commerce/internal/execution-results" \
    "/api/commerce/internal/order-results" \
    "/fapi/v1/order" \
    "/fapi/v2/account" \
    "/fapi/v1/positionRisk"
do
    if [[ "${args}" == *"${forbidden}"* ]]; then
        printf 'forbidden marker reached fake psql: %s\n' "${forbidden}" >&2
        exit 3
    fi
done
cat <<'JSON'
{"status":"fail","source":"quant_web_payment_readonly_db","database_engine":"postgresql","contract_state":"real_count","read_only_input":true,"lookback_secs":86400,"confirmation_timeout_secs":1800,"wallet_payment_exception_count":2,"payment_entitlement_blocker_count":1,"sample":{"payment_intent_id":2001,"membership_order_id":1001,"payment_transaction_id":3001,"exception_code":"wallet_entitlement_missing","age_minutes":45},"alerts":[{"severity":"P1","code":"WALLET_PAYMENT_EXCEPTION","section":"payment_entitlement_health","message":"wallet payment exceptions require review","metadata":{"wallet_payment_exception_count":2,"payment_entitlement_blocker_count":1,"sample_kind":"wallet_payment_exception"}},{"severity":"P0","code":"PAYMENT_ENTITLEMENT_BLOCKED","section":"payment_entitlement_health","message":"wallet payment succeeded but entitlement is still blocked","metadata":{"wallet_payment_exception_count":2,"payment_entitlement_blocker_count":1,"sample_kind":"payment_entitlement"}}],"correlation":{"payment_exception_id":2001,"entitlement_check_id":1001,"user_id":null}}
JSON
"#,
    );

    dir
}

fn alerts(payload: &Value) -> &[Value] {
    payload["alerts"]
        .as_array()
        .expect("alerts should be an array")
}

fn alert_taxonomy(payload: &Value) -> &[Value] {
    payload["alert_taxonomy"]
        .as_array()
        .expect("alert_taxonomy should be an array")
}

include!("local_service_health_contract/local_and_docs_section.rs");
include!("local_service_health_contract/aggregator_runner_section.rs");
include!("local_service_health_contract/render_artifacts_section.rs");
include!("local_service_health_contract/validator_script_contract_section.rs");
include!("local_service_health_contract/validator_playbook_contract_section.rs");
include!("local_service_health_contract/validator_schema_rejection_section.rs");
include!("local_service_health_contract/validator_artifact_validation_section.rs");
include!("local_service_health_contract/validator_stable_schema_alignment_section.rs");
include!("local_service_health_contract/ci_input_runner_section.rs");
include!("local_service_health_contract/payment_artifacts_section.rs");
include!("local_service_health_contract/db_input_producers_section.rs");
include!("local_service_health_contract/local_service_health_runtime_section.rs");
include!("local_service_health_contract/publisher_artifact_set_section.rs");
include!("local_service_health_contract/publisher_admin_ingest_smoke_section.rs");
