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

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}

fn script_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("check_local_service_health.sh")
}

fn runbook_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("local_service_health_runbook.md")
}

fn aggregator_fixture_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_aggregator.fixture.json")
}

fn full_product_artifact_schema_json_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_artifact_schema.json")
}

fn full_product_artifact_schema_doc_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_artifact_schema.md")
}

fn full_product_admin_ci_handoff_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_admin_ci_handoff.md")
}

fn full_product_admin_frontend_contract_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_admin_frontend_contract.md")
}

fn admin_recovery_action_guardrails_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("admin_recovery_action_guardrails.md")
}

fn full_product_artifact_examples_dir() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_examples")
}

fn full_product_admin_ingest_fixture_path() -> PathBuf {
    full_product_artifact_examples_dir().join("admin-ingest-handoff.json")
}

fn aggregator_fixture_script_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("check_full_product_health_aggregator_fixture.sh")
}

fn aggregator_runner_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("check_full_product_health.sh")
}

fn full_product_input_runner_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_inputs.sh")
}

fn full_product_summary_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("summarize_full_product_health.sh")
}

fn full_product_markdown_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("render_full_product_health_markdown.sh")
}

fn full_product_ci_wrapper_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("run_full_product_health_ci.sh")
}

fn full_product_artifact_validator_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("validate_full_product_health_artifacts.sh")
}

fn full_product_artifact_set_publisher_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("publish_full_product_health_artifact_set.sh")
}

fn full_product_admin_ingest_smoke_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("smoke_publish_full_product_health_admin_ingest.sh")
}

fn full_product_admin_ingest_mock_receiver_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("mock_full_product_health_admin_ingest_receiver.py")
}

fn full_product_admin_ingest_contract_smoke_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("smoke_publish_full_product_health_admin_ingest_contract.sh")
}

fn web_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_web_input.sh")
}

fn news_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_news_input.sh")
}

fn admin_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_admin_input.sh")
}

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
  "correlation": {},
  "correlation_ids": []
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** ok\n\n## Counts\n\n## Top Alerts\n\n## Checklist\n\n## Artifact Paths\n\n## Skipped Sections\n",
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
{"status":"fail","source":"quant_web_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":3600,"stale_task_secs":900,"missing_result_secs":900,"open_task_count":2,"stale_task_count":1,"missing_order_result_count":1,"failed_task_count":1,"retry_backlog_count":1,"delivery_blocker_count":1,"recent_order_result_count":3,"recent_trade_record_count":2,"sample":{"signal_inbox_id":3801,"execution_task_id":5202,"execution_attempt_id":6101,"order_result_id":null,"trade_record_id":null,"task_status":"completed","age_secs":960},"alerts":[{"severity":"P0","code":"WEB_ORDER_RESULT_MISSING","section":"web_task_order_health","message":"completed execution task missing order result"},{"severity":"P1","code":"WEB_RETRY_BACKLOG","section":"web_task_order_health","message":"recent execution task retries need review"}],"correlation":{"signal_inbox_id":3801,"execution_task_id":5202,"execution_attempt_id":6101,"order_result_id":null,"trade_record_id":null}}
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
if [[ "${args}" == *"admin_operation_logs"* ]]; then
    cat <<'JSON'
{"status":"fail","source":"quant_admin_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"required_action_count":8,"recent_operation_count":11,"high_risk_operation_count":9,"failed_operation_count":2,"missing_required_action_count":1,"readiness_blocker_count":1,"manual_review_count":2,"alerts":[{"severity":"P0","code":"ADMIN_LIVE_READINESS_BLOCKED","section":"admin_readiness","message":"admin readiness has blockers or required audit coverage is missing"}],"correlation":{"admin_operation_log_id":"admin-op-9002","admin_module":"quant_exchange_symbols","admin_action":"exchange_symbol_sync"}}
JSON
elif [[ "${args}" == *"news_ai_analysis_results"* ]]; then
    cat <<'JSON'
{"status":"warn","source":"quant_news_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"stale_analysis_secs":1800,"failed_job_secs":7200,"source_failure_threshold":3,"source_count":4,"degraded_source_count":2,"paused_source_count":1,"retryable_source_count":1,"recent_news_count":12,"signal_candidate_count":3,"recent_ai_analysis_count":5,"actionable_analysis_count":2,"failed_analysis_job_count":1,"stuck_analysis_job_count":1,"provider_failure_count":1,"active_prompt_config_count":1,"alerts":[{"severity":"P1","code":"NEWS_SOURCE_DEGRADED","section":"news_source_ai_health","message":"one or more news sources are degraded, paused, or retryable"}],"correlation":{"news_id":"jinse-20260507-001","analysis_result_id":9001,"external_id":null}}
JSON
else
    cat <<'JSON'
{"status":"fail","source":"quant_web_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":3600,"stale_task_secs":900,"missing_result_secs":900,"open_task_count":2,"stale_task_count":1,"missing_order_result_count":1,"failed_task_count":1,"retry_backlog_count":1,"delivery_blocker_count":1,"recent_order_result_count":3,"recent_trade_record_count":2,"alerts":[{"severity":"P0","code":"WEB_ORDER_RESULT_MISSING","section":"web_task_order_health","message":"completed execution task missing order result"}],"correlation":{"signal_inbox_id":3801,"execution_task_id":5202,"execution_attempt_id":6101,"order_result_id":null,"trade_record_id":null}}
JSON
fi
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

#[test]
fn local_service_health_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(script_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn local_service_health_script_is_read_only_and_avoids_live_exchange_state() {
    let script = read_script();

    assert!(
        script.contains("HEALTH_CHECK_BINANCE:=false"),
        "Binance checks must be opt-in and disabled by default"
    );
    assert!(
        script.contains("HEALTH_CHECK_OUTPUT:=human"),
        "human output must remain the default"
    );
    assert!(
        script.contains("HEALTH_CHECK_WORKER_STALE_SECS"),
        "worker checkpoint staleness must be configurable"
    );
    assert!(
        script.contains("HEALTH_CHECK_WORKER_MODE:=all"),
        "all-worker mode should remain the conservative default"
    );
    assert!(
        script.contains("HEALTH_CHECK_EXPECTED_WORKERS"),
        "expected online workers must be configurable"
    );
    assert!(
        !script.contains("LINKUSDT") && !script.contains("LINK-USDT"),
        "local health checks must not reference the real LINK position"
    );
    assert!(
        !script.contains("/fapi/v1/order")
            && !script.contains("/fapi/v1/positionSide/dual")
            && !script.contains("/fapi/v2/account")
            && !script.contains("/fapi/v2/positionRisk")
            && !script.contains("/fapi/v1/positionRisk")
            && !script.contains("/fapi/v1/leverage")
            && !script.contains("/fapi/v1/marginType"),
        "local health checks must not call signed/account/order/position Binance endpoints"
    );
    assert!(
        !script.contains("/api/commerce/internal/execution-tasks/lease")
            && !script.contains("/api/commerce/internal/execution-results")
            && !script.contains("/api/commerce/internal/order-results")
            && !script.contains("/risk-review"),
        "local health checks must not mutate Web execution task state"
    );
}

#[test]
fn local_service_health_json_output_is_machine_readable_and_redacted() {
    let tool_dir = fake_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(script_path())
        .env("PATH", path)
        .env("HEALTH_CHECK_OUTPUT", "json")
        .env("HEALTH_CHECK_BINANCE", "false")
        .env("HEALTH_CHECK_DATABASES", "true")
        .env("HEALTH_CHECK_EXECUTION_AUDIT", "false")
        .env("HEALTH_CHECK_WORKER_STALE_SECS", "60")
        .env(
            "QUANT_CORE_DATABASE_URL",
            "postgres://user:secret@db/quant_core",
        )
        .env("WEB_DATABASE_URL", "postgres://user:secret@db/quant_web")
        .env("NEWS_DATABASE_URL", "postgres://user:secret@db/quant_news")
        .env("EXECUTION_EVENT_SECRET", "execution-secret")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("health script should run");

    assert!(
        output.status.success(),
        "json health check should exit successfully without strict mode:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["output"], "json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["database_checks"], "true");
    assert_eq!(payload["binance_public_check"], "false");
    assert_eq!(payload["execution_audit_check"], "false");
    assert_eq!(payload["worker_stale_secs"], "60");
    assert_eq!(payload["worker_mode"], "all");
    assert_eq!(payload["expected_workers"], "");
    assert_eq!(payload["summary"]["expected_worker_failures"], 0);
    assert_eq!(payload["summary"]["expected_worker_warnings"], 0);
    assert_eq!(payload["summary"]["ignored_worker_count"], 0);
    assert_eq!(payload["summary"]["ignored_stale_worker_count"], 1);
    assert!(
        payload["warnings"].as_u64().unwrap_or_default() == 0,
        "default all mode should not warn on historical workers: {stdout}"
    );
    assert!(
        payload["checks"]
            .as_array()
            .expect("checks should be an array")
            .iter()
            .any(|check| check["level"] == "INFO"
                && check["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("ignored_stale_worker_id=worker_stale")),
        "historical stale worker should be visible but ignored in json checks: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "IGNORED_STALE_WORKER"
                && alert["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("ignored_stale_worker_id=worker_stale")),
        "default all mode should surface historical stale worker as INFO alert: {stdout}"
    );
    for secret in [
        "postgres://user:secret@db",
        "execution-secret",
        "binance-key",
        "binance-secret",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !stdout.contains(secret),
            "json output must not leak sensitive value {secret}: {stdout}"
        );
    }
}

#[test]
fn local_service_health_runbook_documents_read_only_and_opt_in_checks() {
    let path = runbook_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    assert!(docs.contains("check_local_service_health.sh"));
    assert!(docs.contains("HEALTH_CHECK_OUTPUT=json"));
    assert!(docs.contains("HEALTH_CHECK_WORKER_STALE_SECS"));
    assert!(docs.contains("HEALTH_CHECK_WORKER_MODE=expected"));
    assert!(docs.contains("HEALTH_CHECK_EXPECTED_WORKERS"));
    assert!(docs.contains("CI / Preflight"));
    assert!(docs.contains("exit code"));
    assert!(docs.contains("summary.expected_worker_failures"));
    assert!(docs.contains("summary.ignored_worker_count"));
    assert!(docs.contains("HEALTH_CHECK_EXECUTION_AUDIT=true"));
    assert!(docs.contains("exchange_request_audit_logs"));
    assert!(docs.contains("stale_leased_workers"));
    assert!(docs.contains("alerts"));
    assert!(docs.contains("severity"));
    assert!(docs.contains("code"));
    assert!(docs.contains("HEALTH_CHECK_STRICT=true"));
    assert!(docs.contains("JSON Stability Contract"));
    assert!(docs.contains("P0"));
    assert!(docs.contains("P1"));
    assert!(docs.contains("阻止实盘"));
    assert!(docs.contains("阻止发布"));
    assert!(docs.contains("历史 smoke 噪声"));
    assert!(docs.contains("只读"));
    assert!(docs.contains("HEALTH_CHECK_BINANCE=false"));
    assert!(docs.contains("显式 opt-in"));
    assert!(docs.contains("不调用 Binance signed/account/order/position endpoint"));
    assert!(docs.contains("不触碰 LINKUSDT"));
}

#[test]
fn local_service_health_runbook_documents_cross_service_aggregator_contract() {
    let path = runbook_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    assert!(docs.contains("Cross-Service Read-Only Aggregator Contract"));
    assert!(docs.contains("check_full_product_health.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_SCHEMA_VERSION"));
    assert!(docs.contains("build_full_product_health_inputs.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_KEEP_INPUTS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH=false"));
    assert!(docs.contains("未提供的 section"));
    assert!(docs.contains("summarize_full_product_health.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT"));
    assert!(docs.contains("run_full_product_health_ci.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never"));
    assert!(docs.contains("validate_full_product_health_artifacts.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH"));
    assert!(docs.contains("checklist"));
    assert!(docs.contains("top_alerts"));
    assert!(docs.contains("required_operator_actions"));
    assert!(docs.contains("correlation_ids"));
    assert!(docs.contains("build_full_product_health_web_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS"));
    assert!(docs.contains("WEB_INPUT_SKIPPED"));
    assert!(docs.contains("WEB_INPUT_QUERY_FAILED"));
    assert!(docs.contains("build_full_product_health_news_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS"));
    assert!(docs.contains("NEWS_INPUT_SKIPPED"));
    assert!(docs.contains("NEWS_INPUT_QUERY_FAILED"));
    assert!(docs.contains("build_full_product_health_admin_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS"));
    assert!(docs.contains("ADMIN_INPUT_SKIPPED"));
    assert!(docs.contains("ADMIN_INPUT_QUERY_FAILED"));
    assert!(docs.contains("web_task_order_health"));
    assert!(docs.contains("news_source_ai_health"));
    assert!(docs.contains("quant_worker_checkpoint_audit"));
    assert!(docs.contains("admin_readiness"));
    assert!(docs.contains("WEB_EXECUTION_TASK_STALE"));
    assert!(docs.contains("WEB_ORDER_RESULT_MISSING"));
    assert!(docs.contains("NEWS_SOURCE_DEGRADED"));
    assert!(docs.contains("NEWS_AI_PROVIDER_UNAVAILABLE"));
    assert!(docs.contains("QUANT_EXPECTED_WORKER_STALE"));
    assert!(docs.contains("ADMIN_LIVE_READINESS_BLOCKED"));
    assert!(docs.contains("ADMIN_HIGH_RISK_OPERATION_FAILED"));
    assert!(docs.contains("ADMIN_ACTION_AUDIT_MISSING"));
    assert!(docs.contains("不写库"));
    assert!(docs.contains("不 lease task"));
    assert!(docs.contains("不 report result"));
    assert!(docs.contains("不读取或打印 `.env`"));
    assert!(docs.contains("不调用 Binance signed/account/order/position endpoint"));
    assert!(docs.contains("news_id"));
    assert!(docs.contains("analysis_result_id"));
    assert!(docs.contains("signal_inbox_id"));
    assert!(docs.contains("execution_task_id"));
    assert!(docs.contains("order_result_id"));
    assert!(docs.contains("trade_record_id"));
    assert!(docs.contains("request_id"));
}

#[test]
fn full_product_health_admin_ci_handoff_documents_command_matrix_and_boundaries() {
    let path = full_product_admin_ci_handoff_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Default CI Safe",
        "Read-Only DB Opt-In",
        "Never Run In Default CI",
        "Command Matrix",
    ] {
        assert!(
            docs.contains(heading),
            "handoff guide should contain heading {heading}"
        );
    }

    for command in [
        "FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false",
        "FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md",
        "FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true",
        "FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH=/tmp/full-product-health-ci/full-product-health-validation.json",
        "FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH=/tmp/full_product_health_artifact_schema.candidate.json",
        "FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true",
        "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=mysql://readonly@host/quant_web",
        "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=postgres://readonly@host/quant_news",
        "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=postgres://readonly@host/quant_admin",
        "./scripts/dev/run_full_product_health_ci.sh",
        "./scripts/dev/validate_full_product_health_artifacts.sh",
        "./scripts/dev/render_full_product_health_markdown.sh",
        "./scripts/dev/smoke_publish_full_product_health_admin_ingest_contract.sh",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json",
        "./scripts/dev/run_binance_live_eth_micro_order_smoke.sh",
    ] {
        assert!(
            docs.contains(command),
            "handoff guide should document command or env {command}"
        );
    }

    for boundary in [
        "no-env",
        "no-service",
        "no-exchange",
        "不读取 `.env`",
        "不访问本地服务",
        "不外呼交易所",
        "不下单",
        "不 lease task",
        "不 report result",
        "不 mutate task",
        "不触碰 `LINKUSDT`",
        "只读 DB URL",
        "默认 CI",
        "显式 opt-in",
        "绝不能在默认 CI 调用",
        "does not read `.env`",
        "does not scan directories",
    ] {
        assert!(
            docs.contains(boundary),
            "handoff guide should document boundary {boundary}"
        );
    }
}

#[test]
fn full_product_health_admin_frontend_consumption_contract_documents_ui_ready_mapping_and_safety() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Admin / Frontend Consumption Contract",
        "Primary Artifacts",
        "Stable Fields",
        "Status Mapping",
        "Do Not Interpret As Ready",
        "Redaction Requirements",
        "Refresh And CI Artifact Usage",
        "Frontend Display Rules",
    ] {
        assert!(
            docs.contains(heading),
            "Admin/frontend contract should contain heading {heading}"
        );
    }

    for field in [
        "summary.overall_status",
        "section_statuses",
        "checklist[].ready",
        "checklist[].action_required",
        "top_alerts[].severity",
        "top_alerts[].code",
        "required_operator_actions[].action",
        "alert_taxonomy[].operator_action",
        "alert_taxonomy[].correlation_keys[]",
        "correlation_ids[]",
        "validation.summary.sensitive_marker_count",
        "validation.findings[]",
    ] {
        assert!(
            docs.contains(field),
            "Admin/frontend contract should document stable field {field}"
        );
    }

    for mapping in [
        "`ok` -> green/pass",
        "`warn` -> amber/review",
        "`fail` -> red/blocking",
        "`P0` -> blocking",
        "`P1` -> manual review",
        "`INFO` -> context only",
        "`block_release_until_resolved`",
        "`manual_review_before_release`",
        "`observe_only`",
    ] {
        assert!(
            docs.contains(mapping),
            "Admin/frontend contract should document mapping {mapping}"
        );
    }

    for not_ready in [
        "`summary.overall_status != \"ok\"`",
        "`section_statuses.* == \"warn\"`",
        "`section_statuses.* == \"fail\"`",
        "`checklist[].ready == false`",
        "`checklist[].action_required == true`",
        "`top_alerts[].severity == \"P0\"`",
        "`required_operator_actions` is not empty",
        "`validation.status != \"ok\"`",
        "`validation.summary.sensitive_marker_count > 0`",
        "`*_INPUT_SKIPPED`",
        "`read_only_input_count == 0`",
        "`admin_readiness.live_readiness` is `blocked` or `review`",
        "`manual_review_required == true`",
    ] {
        assert!(
            docs.contains(not_ready),
            "Admin/frontend contract should list non-ready condition {not_ready}"
        );
    }

    for safety in [
        "must not read `.env`",
        "must not call local services",
        "must not call signed/account/order/position endpoints",
        "must not lease task",
        "must not report result",
        "must not mutate task",
        "must not place orders",
        "must not touch `LINKUSDT`",
        "must render `[redacted]`",
        "must not show raw database URLs",
        "must not show API keys",
        "must not show request or response payloads",
    ] {
        assert!(
            docs.contains(safety),
            "Admin/frontend contract should document safety rule {safety}"
        );
    }

    for artifact in [
        "full-product-health-summary.json",
        "full-product-health.json",
        "full-product-health-validation.json",
        "full-product-health.md",
        "run_full_product_health_ci.sh",
        "validate_full_product_health_artifacts.sh",
        "FAIL_ON_STATUS=never",
    ] {
        assert!(
            docs.contains(artifact),
            "Admin/frontend contract should document artifact or command {artifact}"
        );
    }
}

#[test]
fn full_product_health_latest_stored_artifact_api_contract_documents_safe_response_and_readiness() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for field in [
        "GET `/admin/quant/full-product-health/latest`",
        "`artifactSetId`",
        "`storedAt`",
        "`summary`",
        "`validation`",
        "`markdownUrl`",
        "`fullArtifactUrl`",
        "`ready`",
        "`stale`",
        "`redaction`",
    ] {
        assert!(
            docs.contains(field),
            "stored artifact API contract should document response field {field}"
        );
    }

    for safety in [
        "handler must not shell out",
        "handler must not read `.env`",
        "handler must not run live probes",
        "handler must not call signed/account/order/position endpoints",
        "handler must not call lease/report/mutate task endpoints",
        "handler must not compute readiness from command exit code",
        "read from stored artifact storage only",
    ] {
        assert!(
            docs.contains(safety),
            "stored artifact API contract should document handler safety rule {safety}"
        );
    }
}

#[test]
fn full_product_health_stored_artifact_contract_documents_index_hashes_sla_and_retention() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Stored Artifact Storage Model",
        "Storage Index",
        "Freshness SLA",
        "Retention",
        "Operator Metadata",
    ] {
        assert!(
            docs.contains(heading),
            "stored artifact contract should document heading {heading}"
        );
    }

    for field in [
        "`artifactSetId`",
        "`storedAt`",
        "`sourceGeneratedAt`",
        "`schemaVersion`",
        "`summaryHash`",
        "`validationHash`",
        "`fullArtifactHash`",
        "`markdownHash`",
        "`storageStatus`",
        "`retentionClass`",
        "`artifactSlaSeconds`",
        "`staleReason`",
    ] {
        assert!(
            docs.contains(field),
            "stored artifact index should document field {field}"
        );
    }

    for operator_field in [
        "`operatorMetadata.generatedBy`",
        "`operatorMetadata.triggerType`",
        "`operatorMetadata.runId`",
        "`operatorMetadata.commitSha`",
        "`operatorMetadata.sourceRepo`",
    ] {
        assert!(
            docs.contains(operator_field),
            "stored artifact index should document operator metadata {operator_field}"
        );
    }

    for rule in [
        "latest valid artifact set",
        "at least 30 days",
        "rejected artifact sets",
        "hash mismatch marks the set rejected",
        "stale cannot be rendered as ready",
    ] {
        assert!(
            docs.contains(rule),
            "stored artifact storage model should document rule {rule}"
        );
    }
}

#[test]
fn full_product_health_stored_artifact_contract_documents_url_auth_and_redaction() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "URL Authorization",
        "Validation Finding Redaction",
        "Handler Acceptance Tests",
    ] {
        assert!(
            docs.contains(heading),
            "stored artifact contract should document heading {heading}"
        );
    }

    for url_rule in [
        "`markdownUrl` and `fullArtifactUrl` are authorized download URLs",
        "short-lived",
        "`artifact:health:read`",
        "`artifact:health:download`",
        "must not expose local filesystem paths",
        "must not proxy arbitrary URLs",
    ] {
        assert!(
            docs.contains(url_rule),
            "stored artifact URL authorization should document rule {url_rule}"
        );
    }

    for finding_rule in [
        "validation findings only return `code`, `artifact`, `field`, and `marker`",
        "must not return source text",
        "must not return raw payload",
        "must not return database URL",
        "must not return API key",
        "must not return secret",
        "must not return cipher",
        "must not return signed endpoint",
    ] {
        assert!(
            docs.contains(finding_rule),
            "stored artifact redaction should document finding rule {finding_rule}"
        );
    }

    for handler_rule in [
        "handler must not accept direct file paths from request parameters",
        "handler must not shell out",
        "handler must not read `.env`",
        "handler must not run live probes",
        "handler must not call signed/account/order/position endpoints",
        "handler must not call lease/report/mutate task endpoints",
        "handler must not compute readiness from command exit code",
        "handler must not mutate task state",
    ] {
        assert!(
            docs.contains(handler_rule),
            "stored artifact handler acceptance should document rule {handler_rule}"
        );
    }

    for forbidden in [
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            !docs.contains(forbidden),
            "stored artifact contract should avoid raw dangerous endpoint {forbidden}"
        );
    }
}

#[test]
fn admin_recovery_action_guardrails_document_initial_action_and_redaction_contract() {
    let path = admin_recovery_action_guardrails_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Admin Recovery Action Guardrails",
        "Non-Goals And Hard Boundaries",
        "Action Classes",
        "Global Action Requirements",
        "Redaction Contract",
        "Live Order Boundary",
        "Initial Recovery Actions",
        "RBAC Matrix",
        "Audit Event Contract",
        "Admin Workbench Starting Rules",
    ] {
        assert!(
            docs.contains(heading),
            "recovery guardrail doc should contain heading {heading}"
        );
    }

    for action_class in [
        "read_only",
        "guarded_recovery",
        "manual_approval",
        "disabled_until_live_order_closed",
    ] {
        assert!(
            docs.contains(action_class),
            "recovery guardrail doc should define action class {action_class}"
        );
    }

    for requirement in [
        "reason",
        "impact_objects",
        "audit_log",
        "idempotency_key",
        "rate_limit",
        "dry_run_preview",
        "rbac_role",
        "operator_confirmed_at",
    ] {
        assert!(
            docs.contains(requirement),
            "recovery guardrail doc should require {requirement}"
        );
    }

    for initial_action in [
        "notification_retry",
        "task_retry",
        "task_release",
        "pause_user",
        "pause_strategy",
        "pause_symbol",
        "manual_ai_reanalysis",
        "symbol_sync",
    ] {
        assert!(
            docs.contains(initial_action),
            "recovery guardrail doc should define initial action {initial_action}"
        );
    }

    for redacted in [
        ".env",
        "database_url",
        "api_key",
        "api_secret",
        "passphrase",
        "cipher",
        "request_payload",
        "response_payload",
        "raw_payload",
        "signed_endpoint",
        "account_endpoint",
        "order_endpoint",
        "position_endpoint",
        "LINKUSDT",
    ] {
        assert!(
            docs.contains(redacted),
            "recovery guardrail doc should list redaction or blocked marker {redacted}"
        );
    }

    for live_boundary in [
        "OPEN_LIVE_ORDER_PRESENT",
        "live_order_not_closed",
        "disable by default",
        "manual approval",
        "no signed/order/position endpoint",
        "no lease/report/mutate task endpoint",
        "no real order",
    ] {
        assert!(
            docs.contains(live_boundary),
            "recovery guardrail doc should define live boundary {live_boundary}"
        );
    }

    for forbidden in [
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            !docs.contains(forbidden),
            "recovery guardrail doc should avoid raw dangerous endpoint {forbidden}"
        );
    }
}

#[test]
fn admin_recovery_workbench_disabled_preview_contract_documents_server_acceptance_conditions() {
    let path = admin_recovery_action_guardrails_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Disabled And Read-Only Preview Server Contract",
        "Server Acceptance Conditions",
    ] {
        assert!(
            docs.contains(heading),
            "recovery workbench contract should contain heading {heading}"
        );
    }

    for requirement in [
        "GET preview endpoints are read-only",
        "`enabled: false`",
        "`disabled_reason_code`",
        "`preview_token`",
        "`preview_expires_at`",
        "`preview_hash`",
        "`redacted_preview`",
        "`idempotency_key`",
        "`audit_log`",
        "server-side RBAC",
        "no mutation before confirmation",
        "same `idempotency_key` must not execute twice",
        "dry-run preview must not contain raw payload",
    ] {
        assert!(
            docs.contains(requirement),
            "recovery workbench contract should document server requirement {requirement}"
        );
    }
}

#[test]
fn full_product_health_aggregator_fixture_is_machine_readable_and_redacted() {
    let path = aggregator_fixture_path();
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));
    let payload: Value =
        serde_json::from_str(&body).unwrap_or_else(|error| panic!("invalid json: {error}\n{body}"));

    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["status"], "fail");
    assert!(payload["generated_at"].as_str().is_some());
    assert!(payload["summary"].as_object().is_some());
    assert!(payload["sections"].as_object().is_some());
    assert!(payload["alerts"].as_array().is_some());
    assert!(payload["correlation"].as_object().is_some());

    assert_eq!(payload["summary"]["p0_count"], 1);
    assert_eq!(payload["summary"]["p1_count"], 1);
    assert_eq!(payload["summary"]["info_count"], 1);
    assert!(payload["sections"]["web_task_order_health"]
        .as_object()
        .is_some());
    assert!(payload["sections"]["news_source_ai_health"]
        .as_object()
        .is_some());
    assert!(payload["sections"]["quant_worker_checkpoint_audit"]
        .as_object()
        .is_some());
    assert!(payload["sections"]["admin_readiness"].as_object().is_some());

    let alerts = alerts(&payload);
    assert!(
        alerts
            .iter()
            .any(|alert| alert["severity"] == "P0" && alert["code"] == "WEB_ORDER_RESULT_MISSING"),
        "fixture must include a P0 web/order alert: {body}"
    );
    assert!(
        alerts
            .iter()
            .any(|alert| alert["severity"] == "P1" && alert["code"] == "NEWS_SOURCE_DEGRADED"),
        "fixture must include a P1 news alert: {body}"
    );
    assert!(
        alerts.iter().any(
            |alert| alert["severity"] == "INFO" && alert["code"] == "MOCK_DEV_BOUNDARY_ACTIVE"
        ),
        "fixture must include an INFO mock/dev boundary alert: {body}"
    );
    for alert in alerts {
        for key in ["severity", "code", "section", "message"] {
            assert!(
                alert[key].as_str().is_some(),
                "alert field {key} must remain a stable string: {body}"
            );
        }
    }

    let correlation = &payload["correlation"];
    for key in [
        "news_id",
        "analysis_result_id",
        "signal_inbox_id",
        "external_id",
        "execution_task_id",
        "execution_attempt_id",
        "request_id",
        "order_result_id",
        "trade_record_id",
        "worker_id",
    ] {
        assert!(
            correlation.get(key).is_some(),
            "fixture correlation missing {key}: {body}"
        );
    }

    let lowered = body.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api key",
        "api_secret",
        "apisecret",
        "api secret",
        "secret",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/fapi/v1/positionside/dual",
        "request_payload",
        "response_payload",
        "request payload",
        "response payload",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "aggregator fixture must not contain sensitive marker {sensitive}: {body}"
        );
    }
}

#[test]
fn full_product_health_aggregator_fixture_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(aggregator_fixture_script_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_aggregator_fixture_script_is_read_only_and_scans_sensitive_markers() {
    let script = read_aggregator_fixture_script();

    assert!(script.contains("full_product_health_aggregator.fixture.json"));
    assert!(script.contains("check_local_service_health.sh"));
    assert!(script.contains("schema_version"));
    assert!(script.contains("sections"));
    assert!(script.contains("alerts"));
    assert!(script.contains("correlation"));
    assert!(
        script.contains("python3") || script.contains("python "),
        "runner should use a structured json validator instead of shell string parsing"
    );
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "runner must scan fixture inputs for sensitive marker {required}"
        );
    }
    for forbidden in [
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "source .env",
        "cat .env",
    ] {
        assert!(
            !script.contains(forbidden),
            "runner must stay local/read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_aggregator_fixture_script_outputs_machine_readable_json() {
    let output = Command::new(aggregator_fixture_script_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .output()
        .expect("fixture runner should run");

    assert!(
        output.status.success(),
        "fixture runner should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["schema_version"], 1);
    assert!(matches!(
        payload["status"].as_str(),
        Some("ok" | "warn" | "fail")
    ));
    assert!(payload["summary"].as_object().is_some());
    assert!(payload["sections"].as_object().is_some());
    assert!(payload["alerts"].as_array().is_some());
    assert!(payload["correlation"].as_object().is_some());

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "fixture runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_runner_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(aggregator_runner_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_runner_is_read_only_and_disables_unsafe_default_probes() {
    let script = read_aggregator_runner_script();

    assert!(script.contains("check_local_service_health.sh"));
    assert!(script.contains("full_product_health_aggregator.fixture.json"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH"));
    assert!(script.contains("\"HEALTH_CHECK_OUTPUT\": \"json\""));
    assert!(script.contains("\"HEALTH_CHECK_DATABASES\": \"false\""));
    assert!(script.contains("\"HEALTH_CHECK_BINANCE\": \"false\""));
    assert!(script.contains("\"HEALTH_CHECK_EXECUTION_AUDIT\": \"false\""));
    assert!(script.contains("schema_version"));
    assert!(script.contains("sections"));
    assert!(script.contains("alerts"));
    assert!(script.contains("correlation"));
    assert!(
        script.contains("subprocess.run"),
        "runner should use an explicit subprocess boundary for local health collection"
    );
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "runner must scan inputs and output for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
    ] {
        assert!(
            !script.contains(forbidden),
            "runner must stay local/read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_runner_outputs_machine_readable_json_from_read_only_inputs() {
    let local_json = temp_json_file(
        "full-product-local-health",
        r#"{
  "output": "json",
  "status": "warn",
  "summary": {
    "expected_worker_failures": 0,
    "expected_worker_warnings": 1,
    "ignored_worker_count": 1,
    "ignored_stale_worker_count": 0,
    "execution_audit_recent_failures": 0,
    "execution_audit_stale_leased_workers": 0
  },
  "alerts": [
    {
      "severity": "P1",
      "code": "EXPECTED_WORKER_STALE",
      "section": "Databases",
      "message": "expected worker heartbeat stale"
    }
  ]
}"#,
    );
    let web_json = temp_json_file(
        "full-product-web-health",
        r#"{
  "status": "fail",
  "open_task_count": 2,
  "missing_order_result_count": 1,
  "alerts": [
    {
      "severity": "P0",
      "code": "WEB_ORDER_RESULT_MISSING",
      "section": "web_task_order_health",
      "message": "completed execution task missing order result"
    }
  ],
  "correlation": {
    "signal_inbox_id": 3801,
    "execution_task_id": 5202,
    "order_result_id": null
  }
}"#,
    );
    let news_json = temp_json_file(
        "full-product-news-health",
        r#"{
  "status": "warn",
  "degraded_source_count": 2,
  "recent_ai_analysis_count": 5,
  "failed_analysis_job_count": 1,
  "alerts": [
    {
      "severity": "P1",
      "code": "NEWS_SOURCE_DEGRADED",
      "section": "news_source_ai_health",
      "message": "one or more news sources are degraded, paused, or retryable"
    }
  ],
  "correlation": {
    "news_id": "jinse-20260507-001",
    "analysis_result_id": 9001
  }
}"#,
    );
    let output = Command::new(aggregator_runner_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH", local_json)
        .env("FULL_PRODUCT_HEALTH_WEB_JSON_PATH", web_json)
        .env("FULL_PRODUCT_HEALTH_NEWS_JSON_PATH", news_json)
        .output()
        .expect("full product health runner should run");

    assert!(
        output.status.success(),
        "runner should emit parseable json even when aggregated status is fail:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["status"], "fail");
    assert!(payload["generated_at"].as_str().is_some());
    assert_eq!(payload["summary"]["p0_count"], 1);
    assert_eq!(payload["summary"]["p1_count"], 2);
    assert_eq!(payload["summary"]["web_open_task_count"], 2);
    assert_eq!(payload["summary"]["news_degraded_source_count"], 2);
    assert_eq!(payload["summary"]["quant_expected_worker_failures"], 0);
    assert_eq!(payload["summary"]["quant_expected_worker_warnings"], 1);
    assert_eq!(
        payload["sections"]["quant_worker_checkpoint_audit"]["status"],
        "warn"
    );
    assert_eq!(
        payload["sections"]["web_task_order_health"]["missing_order_result_count"],
        1
    );
    assert_eq!(payload["correlation"]["signal_inbox_id"], 3801);
    assert_eq!(payload["correlation"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["correlation"]["analysis_result_id"], 9001);
    assert_eq!(
        payload["sections"]["news_source_ai_health"]["degraded_source_count"],
        2
    );
    assert_eq!(
        payload["sections"]["news_source_ai_health"]["recent_ai_analysis_count"],
        5
    );
    assert!(payload["sections"]["admin_readiness"].as_object().is_some());
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "WEB_ORDER_RESULT_MISSING"
                && alert["section"] == "web_task_order_health"),
        "web input should contribute a P0 alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "QUANT_EXPECTED_WORKER_STALE"
                && alert["section"] == "quant_worker_checkpoint_audit"),
        "local health input should contribute a mapped quant P1 alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_SOURCE_DEGRADED"
                && alert["section"] == "news_source_ai_health"),
        "news input should contribute a P1 alert: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_runner_outputs_alert_taxonomy_for_correlation_drilldown() {
    let web_json = temp_json_file(
        "full-product-taxonomy-web-health",
        r#"{
  "status": "fail",
  "open_task_count": 1,
  "missing_order_result_count": 1,
  "alerts": [
    {
      "severity": "P0",
      "code": "WEB_ORDER_RESULT_MISSING",
      "section": "web_task_order_health",
      "message": "completed execution task missing order result"
    }
  ],
  "correlation": {
    "signal_inbox_id": 3801,
    "execution_task_id": 5202,
    "execution_attempt_id": 6101,
    "order_result_id": null,
    "trade_record_id": null
  }
}"#,
    );
    let admin_json = temp_json_file(
        "full-product-taxonomy-admin-health",
        r#"{
  "status": "fail",
  "alerts": [
    {
      "severity": "P1",
      "code": "ADMIN_ACTION_AUDIT_MISSING",
      "section": "admin_readiness",
      "message": "required admin audit event is missing"
    }
  ],
  "correlation": {
    "admin_operation_log_id": "admin-op-9002",
    "admin_module": "quant_exchange_symbols",
    "admin_action": "exchange_symbol_sync"
  }
}"#,
    );
    let output = Command::new(aggregator_runner_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env("FULL_PRODUCT_HEALTH_WEB_JSON_PATH", web_json)
        .env("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH", admin_json)
        .output()
        .expect("full product health runner should run");

    assert!(
        output.status.success(),
        "runner should emit parseable json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));
    let taxonomy = alert_taxonomy(&payload);

    assert!(
        taxonomy.iter().any(|item| item["severity"] == "P0"
            && item["code"] == "WEB_ORDER_RESULT_MISSING"
            && item["section"] == "web_task_order_health"
            && item["operator_action"] == "block_release_until_resolved"
            && item["correlation_keys"]
                .as_array()
                .expect("web taxonomy correlation_keys should be an array")
                .iter()
                .any(|key| key == "execution_task_id")),
        "web/order alert should expose stable action and correlation keys: {stdout}"
    );
    assert!(
        taxonomy.iter().any(|item| item["severity"] == "P1"
            && item["code"] == "ADMIN_ACTION_AUDIT_MISSING"
            && item["section"] == "admin_readiness"
            && item["operator_action"] == "manual_review_before_release"
            && item["correlation_keys"]
                .as_array()
                .expect("admin taxonomy correlation_keys should be an array")
                .iter()
                .any(|key| key == "admin_operation_log_id")),
        "admin audit alert should expose stable action and correlation keys: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "alert taxonomy output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_runner_merges_admin_readiness_input_and_correlation() {
    let admin_json = temp_json_file(
        "full-product-admin-health",
        r#"{
  "status": "fail",
  "source": "quant_admin_readonly_db",
  "read_only_input": true,
  "lookback_secs": 7200,
  "required_action_count": 8,
  "recent_operation_count": 11,
  "high_risk_operation_count": 9,
  "failed_operation_count": 2,
  "missing_required_action_count": 1,
  "readiness_blocker_count": 1,
  "manual_review_count": 2,
  "alerts": [
    {
      "severity": "P0",
      "code": "ADMIN_LIVE_READINESS_BLOCKED",
      "section": "admin_readiness",
      "message": "admin readiness has blockers or required audit coverage is missing"
    },
    {
      "severity": "P1",
      "code": "ADMIN_HIGH_RISK_OPERATION_FAILED",
      "section": "admin_readiness",
      "message": "recent high-risk admin operation failed"
    }
  ],
  "correlation": {
    "admin_operation_log_id": "admin-op-9002",
    "admin_module": "quant_exchange_symbols",
    "admin_action": "exchange_symbol_sync"
  }
}"#,
    );
    let output = Command::new(aggregator_runner_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH", admin_json)
        .output()
        .expect("full product health runner should run");

    assert!(
        output.status.success(),
        "runner should emit parseable json with admin input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(
        payload["sections"]["admin_readiness"]["failed_operation_count"],
        2
    );
    assert_eq!(
        payload["sections"]["admin_readiness"]["missing_required_action_count"],
        1
    );
    assert_eq!(
        payload["sections"]["admin_readiness"]["readiness_blocker_count"],
        1
    );
    assert_eq!(
        payload["correlation"]["admin_operation_log_id"],
        "admin-op-9002"
    );
    assert_eq!(
        payload["correlation"]["admin_action"],
        "exchange_symbol_sync"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "ADMIN_LIVE_READINESS_BLOCKED"
                && alert["section"] == "admin_readiness"),
        "admin readiness input should contribute a P0 alert: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_summary_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_summary_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_summary_script_is_read_only_and_redacts_sensitive_markers() {
    let script = read_full_product_summary_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT"));
    assert!(script.contains("checklist"));
    assert!(script.contains("top_alerts"));
    assert!(script.contains("required_operator_actions"));
    assert!(script.contains("alert_taxonomy"));
    assert!(script.contains("correlation_ids"));
    assert!(script.contains("sanitize_json"));
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
        "LINKUSDT",
    ] {
        assert!(
            script.contains(required),
            "summary script must scan output for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
    ] {
        assert!(
            !script.contains(forbidden),
            "summary script must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_summary_outputs_ci_checklist_artifact_from_full_product_json() {
    let health_json = temp_json_file(
        "full-product-health-summary-input",
        r#"{
  "schema_version": 1,
  "status": "fail",
  "generated_at": "2026-05-07T00:00:00Z",
  "summary": {
    "p0_count": 1,
    "p1_count": 2,
    "info_count": 1,
    "web_open_task_count": 2,
    "news_degraded_source_count": 2,
    "quant_expected_worker_failures": 0,
    "quant_expected_worker_warnings": 1,
    "read_only_input_count": 4
  },
  "sections": {
    "web_task_order_health": {
      "status": "fail",
      "open_task_count": 2,
      "missing_order_result_count": 1
    },
    "news_source_ai_health": {
      "status": "warn",
      "degraded_source_count": 2
    },
    "quant_worker_checkpoint_audit": {
      "status": "warn",
      "expected_worker_warnings": 1
    },
    "admin_readiness": {
      "status": "fail",
      "live_readiness": "blocked",
      "manual_review_required": true
    }
  },
  "alerts": [
    {
      "severity": "P0",
      "code": "WEB_ORDER_RESULT_MISSING",
      "section": "web_task_order_health",
      "message": "completed execution task missing order result"
    },
    {
      "severity": "P1",
      "code": "NEWS_SOURCE_DEGRADED",
      "section": "news_source_ai_health",
      "message": "one or more news sources are degraded, paused, or retryable"
    },
    {
      "severity": "P1",
      "code": "QUANT_EXPECTED_WORKER_STALE",
      "section": "quant_worker_checkpoint_audit",
      "message": "expected worker heartbeat stale"
    },
    {
      "severity": "INFO",
      "code": "READ_ONLY_COLLECTOR_ACTIVE",
      "section": "admin_readiness",
      "message": "read-only collector used local JSON inputs only"
    }
  ],
  "correlation": {
    "news_id": "jinse-20260507-001",
    "analysis_result_id": 9001,
    "signal_inbox_id": 3801,
    "execution_task_id": 5202,
    "order_result_id": null,
    "admin_operation_log_id": "admin-op-9002"
  }
}"#,
    );

    let output = Command::new(full_product_summary_path())
        .env("FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH", health_json)
        .env("FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT", "2")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("full product health summary should run");

    assert!(
        output.status.success(),
        "summary should emit parseable json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["source_schema_version"], 1);
    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["summary"]["overall_status"], "fail");
    assert_eq!(payload["summary"]["p0_count"], 1);
    assert_eq!(payload["summary"]["p1_count"], 2);
    assert_eq!(payload["summary"]["info_count"], 1);
    assert_eq!(payload["summary"]["top_alert_count"], 2);
    assert_eq!(payload["summary"]["required_operator_action_count"], 3);
    assert_eq!(payload["section_statuses"]["web_task_order_health"], "fail");
    assert_eq!(payload["section_statuses"]["news_source_ai_health"], "warn");
    assert_eq!(
        payload["section_statuses"]["quant_worker_checkpoint_audit"],
        "warn"
    );
    assert_eq!(payload["section_statuses"]["admin_readiness"], "fail");

    let checklist = payload["checklist"]
        .as_array()
        .expect("summary checklist should be an array");
    assert!(
        checklist
            .iter()
            .any(|item| item["section"] == "web_task_order_health"
                && item["status"] == "fail"
                && item["ready"] == false
                && item["action_required"] == true
                && item["p0_count"] == 1),
        "web section should be represented as a blocking checklist item: {stdout}"
    );
    assert!(
        checklist
            .iter()
            .any(|item| item["section"] == "admin_readiness"
                && item["status"] == "fail"
                && item["action_required"] == true),
        "admin readiness should remain visible in the checklist: {stdout}"
    );

    let top_alerts = payload["top_alerts"]
        .as_array()
        .expect("top_alerts should be an array");
    assert_eq!(top_alerts.len(), 2);
    assert_eq!(top_alerts[0]["code"], "WEB_ORDER_RESULT_MISSING");
    assert_eq!(top_alerts[0]["severity"], "P0");
    assert_eq!(top_alerts[1]["code"], "NEWS_SOURCE_DEGRADED");
    assert_eq!(top_alerts[1]["severity"], "P1");

    let required_actions = payload["required_operator_actions"]
        .as_array()
        .expect("required_operator_actions should be an array");
    assert_eq!(required_actions.len(), 3);
    assert!(
        required_actions
            .iter()
            .any(|item| item["code"] == "WEB_ORDER_RESULT_MISSING"
                && item["action"] == "block_release_until_resolved"),
        "P0 alert should produce a blocking operator action: {stdout}"
    );
    assert!(
        required_actions
            .iter()
            .any(|item| item["code"] == "NEWS_SOURCE_DEGRADED"
                && item["action"] == "manual_review_before_release"),
        "P1 alert should produce a manual-review operator action: {stdout}"
    );
    assert_eq!(payload["correlation"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["news_id"], "jinse-20260507-001");
    assert!(
        payload["correlation_ids"]
            .as_array()
            .expect("correlation_ids should be an array")
            .iter()
            .any(|item| item["key"] == "signal_inbox_id" && item["value"] == 3801),
        "non-null correlation ids should be flattened for CI display: {stdout}"
    );
    assert_eq!(payload["summary"]["alert_taxonomy_count"], 4);
    assert!(
        alert_taxonomy(&payload)
            .iter()
            .any(|item| item["code"] == "WEB_ORDER_RESULT_MISSING"
                && item["section"] == "web_task_order_health"
                && item["operator_action"] == "block_release_until_resolved"
                && item["correlation_keys"]
                    .as_array()
                    .expect("taxonomy correlation_keys should be an array")
                    .iter()
                    .any(|key| key == "execution_task_id")),
        "summary should preserve taxonomy for Web task/order drill-down: {stdout}"
    );
    assert!(
        alert_taxonomy(&payload)
            .iter()
            .any(|item| item["code"] == "READ_ONLY_COLLECTOR_ACTIVE"
                && item["severity"] == "INFO"
                && item["operator_action"] == "observe_only"),
        "summary should classify INFO alerts as observe-only taxonomy items: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "summary output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_markdown_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_markdown_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_markdown_script_is_read_only_and_redacts_sensitive_markers() {
    let script = read_full_product_markdown_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_PATH"));
    assert!(script.contains("sanitize_json"));
    assert!(script.contains("render_markdown"));
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
        "LINKUSDT",
    ] {
        assert!(
            script.contains(required),
            "markdown script must scan output for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
    ] {
        assert!(
            !script.contains(forbidden),
            "markdown script must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_markdown_renders_operator_readable_artifact_from_summary_json() {
    let summary_json = temp_json_file(
        "full-product-health-markdown-input",
        r#"{
  "schema_version": 1,
  "source_schema_version": 1,
  "status": "fail",
  "generated_at": "2026-05-07T01:00:00Z",
  "source_generated_at": "2026-05-07T00:59:00Z",
  "summary": {
    "overall_status": "fail",
    "p0_count": 1,
    "p1_count": 1,
    "info_count": 1,
    "section_count": 4,
    "blocking_section_count": 1,
    "warning_section_count": 2,
    "top_alert_count": 3,
    "required_operator_action_count": 2,
    "read_only_input_count": 3
  },
  "section_statuses": {
    "web_task_order_health": "fail",
    "news_source_ai_health": "warn",
    "quant_worker_checkpoint_audit": "ok",
    "admin_readiness": "warn"
  },
  "checklist": [
    {
      "section": "web_task_order_health",
      "status": "fail",
      "ready": false,
      "action_required": true,
      "p0_count": 1,
      "p1_count": 0,
      "info_count": 0
    },
    {
      "section": "news_source_ai_health",
      "status": "warn",
      "ready": true,
      "action_required": false,
      "skipped": true,
      "reason_code": "NEWS_INPUT_SKIPPED",
      "p0_count": 0,
      "p1_count": 0,
      "info_count": 1
    },
    {
      "section": "quant_worker_checkpoint_audit",
      "status": "ok",
      "ready": true,
      "action_required": false,
      "p0_count": 0,
      "p1_count": 0,
      "info_count": 0
    }
  ],
  "top_alerts": [
    {
      "severity": "P0",
      "code": "WEB_ORDER_RESULT_MISSING",
      "section": "web_task_order_health",
      "message": "completed execution task missing order result"
    },
    {
      "severity": "P1",
      "code": "ADMIN_READINESS_REVIEW_REQUIRED",
      "section": "admin_readiness",
      "message": "admin readiness still requires manual review"
    },
    {
      "severity": "INFO",
      "code": "NEWS_INPUT_SKIPPED",
      "section": "news_source_ai_health",
      "message": "news input producer skipped because no read-only URL was provided"
    }
  ],
  "required_operator_actions": [
    {
      "severity": "P0",
      "code": "WEB_ORDER_RESULT_MISSING",
      "section": "web_task_order_health",
      "message": "completed execution task missing order result",
      "action": "block_release_until_resolved"
    },
    {
      "severity": "P1",
      "code": "ADMIN_READINESS_REVIEW_REQUIRED",
      "section": "admin_readiness",
      "message": "admin readiness still requires manual review",
      "action": "manual_review_before_release"
    }
  ]
}"#,
    );

    let output = Command::new(full_product_markdown_path())
        .env("FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT", "markdown")
        .env(
            "FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH",
            summary_json,
        )
        .env(
            "FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH",
            "/tmp/full-product-health.json",
        )
        .env(
            "FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH",
            "/tmp/full-product-health-summary.json",
        )
        .env(
            "FULL_PRODUCT_HEALTH_MARKDOWN_PATH",
            "/tmp/full-product-health.md",
        )
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("full product health markdown renderer should run");

    assert!(
        output.status.success(),
        "markdown renderer should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("markdown output should be utf8");
    for expected in [
        "# Full Product Health",
        "**Status:** fail",
        "## Counts",
        "p0_count",
        "1",
        "## Top Alerts",
        "WEB_ORDER_RESULT_MISSING",
        "ADMIN_READINESS_REVIEW_REQUIRED",
        "## Checklist",
        "web_task_order_health",
        "news_source_ai_health",
        "## Artifact Paths",
        "/tmp/full-product-health.json",
        "/tmp/full-product-health-summary.json",
        "/tmp/full-product-health.md",
        "## Skipped Sections",
        "NEWS_INPUT_SKIPPED",
    ] {
        assert!(
            stdout.contains(expected),
            "markdown output should contain {expected}: {stdout}"
        );
    }

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "markdown output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_artifact_validator_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_artifact_validator_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_artifact_validator_is_read_only_and_scans_sensitive_markers() {
    let script = read_full_product_artifact_validator_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_VALIDATION_STRICT"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH"));
    assert!(script.contains("full_product_health_artifact_schema.json"));
    assert!(script.contains("required_top_level"));
    assert!(script.contains("status_values"));
    assert!(script.contains("severity_values"));
    assert!(script.contains("markdown_required_markers"));
    assert!(script.contains("DB_CONNECTION_STRING"));
    assert!(script.contains("CREDENTIAL_TOKEN"));
    assert!(script.contains("CIPHER_OR_PASSPHRASE"));
    assert!(script.contains("RAW_CONTENT"));
    assert!(script.contains("SIGNED_EXCHANGE_ENDPOINT"));
    assert!(script.contains("WEB_MUTATION_ENDPOINT"));
    assert!(script.contains("LINK_POSITION_SYMBOL"));
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "passphrase",
        "cipher",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
        "LINKUSDT",
    ] {
        assert!(
            script.contains(required),
            "artifact validator must scan for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "psql ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
    ] {
        assert!(
            !script.contains(forbidden),
            "artifact validator must stay file-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_artifact_validator_uses_schema_path_for_required_fields() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-schema-required");
    let schema_path = artifact_dir.join("custom-schema.json");
    let (full_report_path, summary_path, markdown_path) =
        write_phase45_valid_artifacts(&artifact_dir);

    write_phase45_validator_schema(
        &schema_path,
        &["ok", "warn", "fail"],
        &["P0", "P1", "INFO"],
        &[
            "schema_version",
            "status",
            "generated_at",
            "summary",
            "sections",
            "alerts",
            "correlation",
            "phase45_schema_driven_required",
        ],
        &[
            "schema_version",
            "source_schema_version",
            "status",
            "generated_at",
            "source_generated_at",
            "summary",
            "section_statuses",
            "checklist",
            "top_alerts",
            "required_operator_actions",
            "correlation",
            "correlation_ids",
        ],
        &[
            "# Full Product Health",
            "**Status:**",
            "## Counts",
            "## Top Alerts",
            "## Checklist",
            "## Artifact Paths",
        ],
    );

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH", &schema_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should fail when a schema-required field is absent"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|finding| {
            finding["code"] == "MISSING_REQUIRED_FIELD"
                && finding["artifact"] == "full_report"
                && finding["field"] == "phase45_schema_driven_required"
        }),
        "validator should enforce required_top_level from schema: {stdout}"
    );
}

#[test]
fn full_product_health_artifact_validator_strict_fails_when_schema_is_missing() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-schema-missing");
    let missing_schema_path = artifact_dir.join("missing-schema.json");
    let (full_report_path, summary_path, markdown_path) =
        write_phase45_valid_artifacts(&artifact_dir);

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH",
            &missing_schema_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should fail when schema is missing"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    assert_eq!(payload["status"], "fail");
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .any(|finding| finding["code"] == "SCHEMA_MISSING" && finding["artifact"] == "schema"),
        "validator should emit an explicit schema-missing finding: {stdout}"
    );
}

#[test]
fn full_product_health_artifact_validator_enforces_schema_status_and_severity_enums() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-schema-enums");
    let schema_path = artifact_dir.join("custom-schema.json");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    write_phase45_validator_schema(
        &schema_path,
        &["ok"],
        &["P0"],
        &[
            "schema_version",
            "status",
            "generated_at",
            "summary",
            "sections",
            "alerts",
            "correlation",
        ],
        &[
            "schema_version",
            "source_schema_version",
            "status",
            "generated_at",
            "source_generated_at",
            "summary",
            "section_statuses",
            "checklist",
            "top_alerts",
            "required_operator_actions",
            "correlation",
            "correlation_ids",
        ],
        &[
            "# Full Product Health",
            "**Status:**",
            "## Counts",
            "## Top Alerts",
            "## Checklist",
            "## Artifact Paths",
        ],
    );
    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:00Z",
  "summary": {"p0_count": 0, "p1_count": 0, "info_count": 0},
  "sections": {},
  "alerts": [{"severity": "INFO", "code": "EXAMPLE", "message": "redacted"}],
  "correlation": {}
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", full_report_path.display(), error));
    fs::write(
        &summary_path,
        r#"{
  "schema_version": 1,
  "source_schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:01Z",
  "source_generated_at": "2026-05-07T01:00:00Z",
  "summary": {"overall_status": "warn", "p0_count": 0, "p1_count": 0, "info_count": 0},
  "section_statuses": {},
  "checklist": [],
  "top_alerts": [{"severity": "INFO", "code": "EXAMPLE", "message": "redacted"}],
  "required_operator_actions": [],
  "correlation": {},
  "correlation_ids": []
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** warn\n\n## Counts\n\n## Top Alerts\n\n## Checklist\n\n## Artifact Paths\n",
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", markdown_path.display(), error));

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH", &schema_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should reject enum values not allowed by schema"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .filter(|finding| finding["code"] == "INVALID_ENUM_VALUE")
            .count()
            >= 3,
        "validator should reject schema-disallowed status and severity values: {stdout}"
    );
}

#[test]
fn full_product_health_artifact_validator_rejects_unregistered_alert_taxonomy_code() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-alert-code-registry");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:00Z",
  "summary": {"p0_count": 0, "p1_count": 1, "info_count": 0, "read_only_input_count": 4},
  "sections": {
    "web_task_order_health": {"status": "warn"}
  },
  "alerts": [],
  "alert_taxonomy": [
    {
      "severity": "P1",
      "code": "WEB_UNREGISTERED_ALERT_CODE",
      "section": "web_task_order_health",
      "operator_action": "manual_review_before_release",
      "correlation_keys": ["execution_task_id"]
    }
  ],
  "correlation": {"execution_task_id": 5202}
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", full_report_path.display(), error));
    fs::write(
        &summary_path,
        r#"{
  "schema_version": 1,
  "source_schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:01Z",
  "source_generated_at": "2026-05-07T01:00:00Z",
  "summary": {
    "overall_status": "warn",
    "p0_count": 0,
    "p1_count": 1,
    "info_count": 0,
    "section_count": 1,
    "blocking_section_count": 0,
    "warning_section_count": 1,
    "top_alert_count": 0,
    "required_operator_action_count": 0,
    "alert_taxonomy_count": 1,
    "correlation_id_count": 1,
    "read_only_input_count": 4
  },
  "section_statuses": {"web_task_order_health": "warn"},
  "checklist": [],
  "top_alerts": [],
  "required_operator_actions": [],
  "alert_taxonomy": [
    {
      "severity": "P1",
      "code": "WEB_UNREGISTERED_ALERT_CODE",
      "section": "web_task_order_health",
      "operator_action": "manual_review_before_release",
      "correlation_keys": ["execution_task_id"]
    }
  ],
  "correlation": {"execution_task_id": 5202},
  "correlation_ids": [{"key": "execution_task_id", "value": 5202}]
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** warn\n\n## Counts\n\n## Top Alerts\n\n## Checklist\n\n## Artifact Paths\n\n## Skipped Sections\n",
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", markdown_path.display(), error));

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should reject taxonomy codes outside alert_code_values"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|finding| {
            finding["code"] == "INVALID_ALERT_CODE" && finding["field"] == "alert_taxonomy[0].code"
        }),
        "validator should identify the unregistered alert taxonomy code: {stdout}"
    );
}

#[test]
fn full_product_health_artifact_validator_rejects_unregistered_emitted_alert_codes() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-emitted-alert-codes");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:00Z",
  "summary": {"p0_count": 0, "p1_count": 1, "info_count": 0, "read_only_input_count": 4},
  "sections": {
    "web_task_order_health": {"status": "warn"},
    "news_source_ai_health": {"status": "warn"}
  },
  "alerts": [
    {
      "severity": "P1",
      "code": "WEB_UNREGISTERED_EMITTED_ALERT",
      "section": "web_task_order_health",
      "message": "synthetic emitted alert code is not registered"
    }
  ],
  "alert_taxonomy": [],
  "correlation": {"execution_task_id": 5202}
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", full_report_path.display(), error));
    fs::write(
        &summary_path,
        r#"{
  "schema_version": 1,
  "source_schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:01Z",
  "source_generated_at": "2026-05-07T01:00:00Z",
  "summary": {
    "overall_status": "warn",
    "p0_count": 0,
    "p1_count": 1,
    "info_count": 0,
    "section_count": 2,
    "blocking_section_count": 0,
    "warning_section_count": 2,
    "top_alert_count": 1,
    "required_operator_action_count": 0,
    "alert_taxonomy_count": 0,
    "correlation_id_count": 1,
    "read_only_input_count": 4
  },
  "section_statuses": {
    "web_task_order_health": "warn",
    "news_source_ai_health": "warn"
  },
  "checklist": [],
  "top_alerts": [
    {
      "severity": "P1",
      "code": "NEWS_UNREGISTERED_TOP_ALERT",
      "section": "news_source_ai_health",
      "message": "synthetic top alert code is not registered"
    }
  ],
  "required_operator_actions": [],
  "alert_taxonomy": [],
  "correlation": {"execution_task_id": 5202},
  "correlation_ids": [{"key": "execution_task_id", "value": 5202}]
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** warn\n\n## Counts\n\n## Top Alerts\n\n## Checklist\n\n## Artifact Paths\n\n## Skipped Sections\n",
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", markdown_path.display(), error));

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should reject emitted alert codes outside alert_code_values"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    for expected_field in ["alerts[0].code", "top_alerts[0].code"] {
        assert!(
            findings.iter().any(|finding| {
                finding["code"] == "INVALID_ALERT_CODE" && finding["field"] == expected_field
            }),
            "validator should identify unregistered emitted alert code at {expected_field}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_artifact_validator_accepts_complete_redacted_artifacts() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-ok");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "ok",
  "generated_at": "2026-05-07T01:00:00Z",
  "summary": {"p0_count": 0, "p1_count": 0, "info_count": 0, "read_only_input_count": 4},
  "sections": {
    "web_task_order_health": {"status": "ok"},
    "news_source_ai_health": {"status": "ok"},
    "quant_worker_checkpoint_audit": {"status": "ok"},
    "admin_readiness": {"status": "ok"}
  },
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
    "section_count": 4,
    "blocking_section_count": 0,
    "warning_section_count": 0,
    "top_alert_count": 0,
    "required_operator_action_count": 0,
    "alert_taxonomy_count": 0,
    "correlation_id_count": 0,
    "read_only_input_count": 4
  },
  "section_statuses": {
    "web_task_order_health": "ok",
    "news_source_ai_health": "ok",
    "quant_worker_checkpoint_audit": "ok",
    "admin_readiness": "ok"
  },
  "checklist": [],
  "top_alerts": [],
  "required_operator_actions": [],
  "alert_taxonomy": [],
  "correlation": {},
  "correlation_ids": []
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** ok\n\n## Counts\n\n## Top Alerts\n\n## Checklist\n\n## Artifact Paths\n\n## Skipped Sections\n",
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", markdown_path.display(), error));

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        output.status.success(),
        "validator should accept complete redacted artifacts:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["summary"]["artifact_count"], 3);
    assert_eq!(payload["summary"]["sensitive_marker_count"], 0);
    assert_eq!(payload["artifacts"]["full_report"]["json_valid"], true);
    assert_eq!(payload["artifacts"]["summary"]["json_valid"], true);
    assert_eq!(payload["artifacts"]["markdown"]["exists"], true);
    assert!(payload["findings"].as_array().unwrap().is_empty());
}

#[test]
fn full_product_health_artifact_validator_strict_fails_missing_fields_and_sensitive_markers() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-fail");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "blocked",
  "summary": {"p0_count": 0},
  "alerts": [
    {
      "severity": "CRITICAL",
      "message": "api_key=binance-key postgres://user:secret@db/quant /fapi/v1/order raw_payload LINKUSDT"
    }
  ],
  "correlation": {}
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", full_report_path.display(), error));
    fs::write(
        &summary_path,
        r#"{
  "schema_version": 1,
  "status": "warn",
  "summary": {"overall_status": "warn"},
  "section_statuses": {},
  "checklist": [],
  "top_alerts": [],
  "required_operator_actions": [],
  "correlation": {},
  "correlation_ids": []
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** warn\n\n## Counts\n\napi secret appeared here\n",
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", markdown_path.display(), error));

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should exit non-zero for invalid artifacts"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert!(
        payload["summary"]["missing_required_field_count"]
            .as_u64()
            .expect("missing field count should be numeric")
            > 0
    );
    assert!(
        payload["summary"]["sensitive_marker_count"]
            .as_u64()
            .expect("sensitive marker count should be numeric")
            > 0
    );
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings
            .iter()
            .any(|finding| finding["code"] == "MISSING_REQUIRED_FIELD"),
        "validator should report missing required fields: {stdout}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding["code"] == "INVALID_ENUM_VALUE"),
        "validator should report invalid status/severity enum values: {stdout}"
    );
    for expected_marker in [
        "CREDENTIAL_TOKEN",
        "DB_CONNECTION_STRING",
        "SIGNED_EXCHANGE_ENDPOINT",
        "RAW_CONTENT",
        "LINK_POSITION_SYMBOL",
    ] {
        assert!(
            findings
                .iter()
                .any(|finding| finding["marker_code"] == expected_marker),
            "validator should report marker code {expected_marker}: {stdout}"
        );
    }

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "api_key",
        "api secret",
        "binance-key",
        "postgres://",
        "secret@",
        "/fapi/v1/order",
        "raw_payload",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "validation output must not echo sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_stable_artifact_schema_examples_and_validator_are_aligned() {
    let schema_path = full_product_artifact_schema_json_path();
    let doc_path = full_product_artifact_schema_doc_path();
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");
    let validation_path = examples_dir.join("full-product-health-validation.json");

    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let doc = fs::read_to_string(&doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", doc_path.display(), error));
    let full_report_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    let validation_body = fs::read_to_string(&validation_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", validation_path.display(), error));

    let full_report: Value = serde_json::from_str(&full_report_body)
        .unwrap_or_else(|error| panic!("invalid full report example: {error}\n{full_report_body}"));
    let summary: Value = serde_json::from_str(&summary_body)
        .unwrap_or_else(|error| panic!("invalid summary example: {error}\n{summary_body}"));
    let validation: Value = serde_json::from_str(&validation_body)
        .unwrap_or_else(|error| panic!("invalid validation example: {error}\n{validation_body}"));

    assert_eq!(schema["schema_version"], 1);
    for required_doc_token in [
        "full_product_health_artifact_schema.json",
        "full_product_health_examples/full-product-health.json",
        "full_product_health_examples/full-product-health-summary.json",
        "full_product_health_examples/full-product-health.md",
        "full_product_health_examples/full-product-health-validation.json",
        "Required Fields",
        "Append-Only Boundary",
        "Status Values",
        "Severity Values",
        "Operator Action Values",
        "Alert Code Values",
        "alert_code_metadata",
        "owner",
        "default_next_action",
        "admin_link_target",
        "alerts[].code",
        "top_alerts[].code",
        "alert_taxonomy",
    ] {
        assert!(
            doc.contains(required_doc_token),
            "schema doc should mention {required_doc_token}"
        );
    }

    let status_values = schema["status_values"]
        .as_array()
        .expect("status_values should be an array");
    for expected in ["ok", "warn", "fail"] {
        assert!(
            status_values.iter().any(|item| item == expected),
            "schema should allow status {expected}"
        );
    }
    let severity_values = schema["severity_values"]
        .as_array()
        .expect("severity_values should be an array");
    for expected in ["P0", "P1", "INFO"] {
        assert!(
            severity_values.iter().any(|item| item == expected),
            "schema should allow severity {expected}"
        );
    }
    let operator_action_values = schema["operator_action_values"]
        .as_array()
        .expect("operator_action_values should be an array");
    for expected in [
        "block_release_until_resolved",
        "manual_review_before_release",
        "observe_only",
    ] {
        assert!(
            operator_action_values.iter().any(|item| item == expected),
            "schema should allow operator action {expected}"
        );
    }
    let alert_code_values = schema["alert_code_values"]
        .as_object()
        .expect("alert_code_values should be an object keyed by section");
    for (section, expected) in [
        ("web_task_order_health", "WEB_ORDER_RESULT_MISSING"),
        ("news_source_ai_health", "NEWS_SOURCE_DEGRADED"),
        ("quant_worker_checkpoint_audit", "QUANT_WORKER_LEASE_STALE"),
        ("admin_readiness", "ADMIN_ACTION_AUDIT_MISSING"),
        ("global", "MOCK_DEV_BOUNDARY_ACTIVE"),
    ] {
        let values = alert_code_values
            .get(section)
            .and_then(|item| item.as_array())
            .unwrap_or_else(|| panic!("alert_code_values.{section} should be an array"));
        assert!(
            values.iter().any(|item| item == expected),
            "schema should register alert code {expected} for {section}"
        );
    }
    assert_alert_code_metadata_alignment(&schema);

    assert_required_top_level_fields(&schema, "full_report", &full_report);
    assert_required_top_level_fields(&schema, "summary", &summary);
    assert_required_top_level_fields(&schema, "validation", &validation);
    assert_required_nested_fields(&schema, "summary", "summary", &summary["summary"]);
    assert_required_nested_fields(&schema, "validation", "summary", &validation["summary"]);

    assert_enum_value(
        &schema,
        "status_values",
        &full_report["status"],
        "full report status",
    );
    assert_enum_value(
        &schema,
        "status_values",
        &summary["status"],
        "summary status",
    );
    assert_enum_value(
        &schema,
        "status_values",
        &summary["summary"]["overall_status"],
        "summary overall_status",
    );
    assert_enum_value(
        &schema,
        "status_values",
        &validation["status"],
        "validation status",
    );
    assert_json_array_enum(
        &schema,
        "severity_values",
        &full_report["alerts"],
        "severity",
        "full report alerts",
    );
    assert_json_array_enum(
        &schema,
        "severity_values",
        &summary["top_alerts"],
        "severity",
        "summary top_alerts",
    );
    assert_json_array_enum(
        &schema,
        "severity_values",
        &summary["required_operator_actions"],
        "severity",
        "summary required_operator_actions",
    );
    assert_json_array_enum(
        &schema,
        "severity_values",
        &full_report["alert_taxonomy"],
        "severity",
        "full report alert_taxonomy",
    );
    assert_json_array_enum(
        &schema,
        "operator_action_values",
        &full_report["alert_taxonomy"],
        "operator_action",
        "full report alert_taxonomy",
    );
    assert_json_array_enum(
        &schema,
        "severity_values",
        &summary["alert_taxonomy"],
        "severity",
        "summary alert_taxonomy",
    );
    assert_json_array_enum(
        &schema,
        "operator_action_values",
        &summary["alert_taxonomy"],
        "operator_action",
        "summary alert_taxonomy",
    );
    assert_alert_taxonomy_metadata_matches_registry(
        &schema,
        &full_report["alert_taxonomy"],
        "full report alert_taxonomy",
    );
    assert_alert_taxonomy_metadata_matches_registry(
        &schema,
        &summary["alert_taxonomy"],
        "summary alert_taxonomy",
    );

    for marker in schema["markdown_required_markers"]
        .as_array()
        .expect("markdown_required_markers should be an array")
    {
        let marker = marker.as_str().expect("markdown marker should be a string");
        assert!(
            markdown_body.contains(marker),
            "markdown example should contain marker {marker}: {markdown_body}"
        );
    }

    let examples_combined =
        format!("{full_report_body}\n{summary_body}\n{markdown_body}\n{validation_body}")
            .to_ascii_lowercase();
    for forbidden in [
        ".env",
        "postgres://",
        "postgresql://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api secret",
        "api_secret",
        "secret",
        "passphrase",
        "cipher",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/fapi/v1/positionside/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
        "linkusdt",
        "link-usdt",
    ] {
        assert!(
            !examples_combined.contains(forbidden),
            "example artifacts must not contain forbidden marker {forbidden}: {examples_combined}"
        );
    }

    let validator_script = read_full_product_artifact_validator_script();
    for required in [
        "allowed_status_values",
        "allowed_severity_values",
        "allowed_operator_action_values",
        "allowed_alert_code_values",
        "validate_alert_taxonomy_values",
        "validate_alert_code_values",
        "validate_codes=True",
        "INVALID_ENUM_VALUE",
        "INVALID_ALERT_CODE",
    ] {
        assert!(
            validator_script.contains(required),
            "validator should enforce schema enum {required}"
        );
    }

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH", &summary_path)
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run against examples");

    assert!(
        output.status.success(),
        "validator should accept schema examples:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_ci_wrapper_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_ci_wrapper_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_ci_wrapper_is_safe_and_uses_explicit_artifacts() {
    let script = read_full_product_ci_wrapper_script();

    assert!(script.contains("build_full_product_health_inputs.sh"));
    assert!(script.contains("summarize_full_product_health.sh"));
    assert!(script.contains("render_full_product_health_markdown.sh"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH"));
    assert!(script.contains("env -i"));
    assert!(script.contains("linkusdt"));
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "CI wrapper must scan artifacts for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
        "LINKUSDT",
        "LINK-USDT",
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
        "MINIMAX_API_KEY",
        "OPENAI_API_KEY",
    ] {
        assert!(
            !script.contains(forbidden),
            "CI wrapper must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_ci_wrapper_writes_skipped_report_and_summary_without_urls() {
    let artifact_dir = temp_artifact_dir("full-product-health-ci-skipped");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");

    let output = Command::new(full_product_ci_wrapper_path())
        .env("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR", &artifact_dir)
        .env("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH", &summary_path)
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product CI wrapper should run");

    assert!(
        output.status.success(),
        "missing urls should still produce skipped artifacts:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        full_report_path.is_file(),
        "CI wrapper should write full report artifact"
    );
    assert!(
        summary_path.is_file(),
        "CI wrapper should write summary artifact"
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let full_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let stdout_summary: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid stdout json: {error}\n{stdout}"));
    let full_payload: Value = serde_json::from_str(&full_body)
        .unwrap_or_else(|error| panic!("invalid full report json: {error}\n{full_body}"));
    let summary_payload: Value = serde_json::from_str(&summary_body)
        .unwrap_or_else(|error| panic!("invalid summary json: {error}\n{summary_body}"));

    assert_eq!(stdout_summary, summary_payload);
    assert_eq!(full_payload["status"], "ok");
    assert_eq!(full_payload["summary"]["p0_count"], 0);
    assert_eq!(full_payload["summary"]["p1_count"], 0);
    assert_eq!(full_payload["summary"]["read_only_input_count"], 3);
    assert_eq!(
        full_payload["sections"]["web_task_order_health"]["skipped"],
        true
    );
    assert_eq!(
        full_payload["sections"]["news_source_ai_health"]["skipped"],
        true
    );
    assert_eq!(full_payload["sections"]["admin_readiness"]["skipped"], true);
    assert_eq!(summary_payload["status"], "ok");
    assert_eq!(summary_payload["summary"]["overall_status"], "ok");
    assert_eq!(
        summary_payload["section_statuses"]["web_task_order_health"],
        "warn"
    );
    assert!(
        alerts(&full_payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "WEB_INPUT_SKIPPED"
                && alert["section"] == "web_task_order_health"),
        "web skipped section should remain visible in the full report: {full_body}"
    );
    assert!(
        summary_payload["top_alerts"]
            .as_array()
            .expect("top_alerts should be an array")
            .iter()
            .any(|alert| alert["code"] == "WEB_INPUT_SKIPPED"),
        "summary should include skipped input context: {summary_body}"
    );

    let combined = format!("{stdout}\n{full_body}\n{summary_body}").to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "minimax-secret",
        "admin-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !combined.contains(sensitive),
            "CI wrapper artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}

#[test]
fn full_product_health_ci_wrapper_writes_optional_markdown_artifact_without_urls() {
    let artifact_dir = temp_artifact_dir("full-product-health-ci-markdown-skipped");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    let output = Command::new(full_product_ci_wrapper_path())
        .env("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR", &artifact_dir)
        .env("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH", &summary_path)
        .env("FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH", &markdown_path)
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product CI wrapper should run");

    assert!(
        output.status.success(),
        "missing urls should still produce optional markdown artifact:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        full_report_path.is_file(),
        "CI wrapper should write full report artifact"
    );
    assert!(
        summary_path.is_file(),
        "CI wrapper should write summary artifact"
    );
    assert!(
        markdown_path.is_file(),
        "CI wrapper should write optional markdown artifact"
    );

    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    for expected in [
        "# Full Product Health",
        "**Status:** ok",
        "## Artifact Paths",
        "full-product-health.json",
        "full-product-health-summary.json",
        "full-product-health.md",
        "## Skipped Sections",
        "web_task_order_health",
        "WEB_INPUT_SKIPPED",
        "news_source_ai_health",
        "NEWS_INPUT_SKIPPED",
        "admin_readiness",
        "ADMIN_INPUT_SKIPPED",
    ] {
        assert!(
            markdown_body.contains(expected),
            "markdown artifact should contain {expected}: {markdown_body}"
        );
    }

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let full_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let combined =
        format!("{stdout}\n{full_body}\n{summary_body}\n{markdown_body}").to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "minimax-secret",
        "admin-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !combined.contains(sensitive),
            "CI wrapper artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}

#[test]
fn full_product_health_ci_wrapper_writes_optional_validation_artifact_without_urls() {
    let artifact_dir = temp_artifact_dir("full-product-health-ci-validation-skipped");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");
    let validation_path = artifact_dir.join("full-product-health-validation.json");

    let output = Command::new(full_product_ci_wrapper_path())
        .env("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR", &artifact_dir)
        .env("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH", &summary_path)
        .env("FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH", &markdown_path)
        .env("FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS", "true")
        .env("FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH", &validation_path)
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product CI wrapper should run");

    assert!(
        output.status.success(),
        "validation-enabled wrapper should produce safe artifacts:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        full_report_path.is_file(),
        "CI wrapper should write full report artifact"
    );
    assert!(
        summary_path.is_file(),
        "CI wrapper should write summary artifact"
    );
    assert!(
        markdown_path.is_file(),
        "CI wrapper should write optional markdown artifact"
    );
    assert!(
        validation_path.is_file(),
        "CI wrapper should write optional validation artifact"
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let validation_body = fs::read_to_string(&validation_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", validation_path.display(), error));
    let validation_payload: Value = serde_json::from_str(&validation_body)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{validation_body}"));
    assert_eq!(validation_payload["status"], "ok");
    assert_eq!(validation_payload["summary"]["artifact_count"], 3);
    assert_eq!(validation_payload["summary"]["sensitive_marker_count"], 0);
    assert_eq!(validation_payload["artifacts"]["markdown"]["exists"], true);

    let full_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    let combined =
        format!("{stdout}\n{full_body}\n{summary_body}\n{markdown_body}\n{validation_body}")
            .to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "minimax-secret",
        "admin-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !combined.contains(sensitive),
            "validation-enabled CI artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}

#[test]
fn full_product_health_ci_wrapper_exits_from_overall_status_unless_disabled() {
    let tool_dir = fake_full_product_input_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let blocking_artifact_dir = temp_artifact_dir("full-product-health-ci-blocking");
    let blocking_full_report_path = blocking_artifact_dir.join("full-product-health.json");
    let blocking_summary_path = blocking_artifact_dir.join("full-product-health-summary.json");

    let blocking_output = Command::new(full_product_ci_wrapper_path())
        .env("PATH", &path)
        .env(
            "FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR",
            &blocking_artifact_dir,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH",
            &blocking_full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH",
            &blocking_summary_path,
        )
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .output()
        .expect("full product CI wrapper should run");

    assert!(
        !blocking_output.status.success(),
        "fail overall status should make the CI wrapper exit non-zero by default"
    );
    assert!(
        blocking_full_report_path.is_file(),
        "blocking run should still write full report artifact"
    );
    assert!(
        blocking_summary_path.is_file(),
        "blocking run should still write summary artifact"
    );

    let blocking_summary_body =
        fs::read_to_string(&blocking_summary_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {}",
                blocking_summary_path.display(),
                error
            )
        });
    let blocking_summary: Value = serde_json::from_str(&blocking_summary_body)
        .unwrap_or_else(|error| panic!("invalid summary json: {error}\n{blocking_summary_body}"));
    assert_eq!(blocking_summary["status"], "fail");
    assert_eq!(blocking_summary["summary"]["overall_status"], "fail");

    let report_only_artifact_dir = temp_artifact_dir("full-product-health-ci-report-only");
    let report_only_full_report_path = report_only_artifact_dir.join("full-product-health.json");
    let report_only_summary_path =
        report_only_artifact_dir.join("full-product-health-summary.json");
    let report_only_output = Command::new(full_product_ci_wrapper_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR",
            &report_only_artifact_dir,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH",
            &report_only_full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH",
            &report_only_summary_path,
        )
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS", "never")
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .output()
        .expect("full product CI wrapper should run");

    assert!(
        report_only_output.status.success(),
        "FAIL_ON_STATUS=never should keep the CI wrapper report-only:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&report_only_output.stdout),
        String::from_utf8_lossy(&report_only_output.stderr)
    );
    let report_only_summary_body =
        fs::read_to_string(&report_only_summary_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {}",
                report_only_summary_path.display(),
                error
            )
        });
    let report_only_summary: Value = serde_json::from_str(&report_only_summary_body)
        .unwrap_or_else(|error| {
            panic!("invalid summary json: {error}\n{report_only_summary_body}")
        });
    assert_eq!(report_only_summary["status"], "fail");
    assert_eq!(report_only_summary["summary"]["overall_status"], "fail");
}

#[test]
fn full_product_health_input_runner_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_input_runner_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_input_runner_is_safe_and_uses_only_read_only_producers() {
    let script = read_full_product_input_runner_script();

    assert!(script.contains("build_full_product_health_web_input.sh"));
    assert!(script.contains("build_full_product_health_news_input.sh"));
    assert!(script.contains("build_full_product_health_admin_input.sh"));
    assert!(script.contains("check_full_product_health.sh"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH"));
    assert!(script.contains("WEB_INPUT_SKIPPED"));
    assert!(script.contains("NEWS_INPUT_SKIPPED"));
    assert!(script.contains("ADMIN_INPUT_SKIPPED"));
    assert!(script.contains("mktemp -d"));
    assert!(script.contains("trap "));
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "input runner must scan generated inputs for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
        "LINKUSDT",
        "LINK-USDT",
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
    ] {
        assert!(
            !script.contains(forbidden),
            "input runner must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_input_runner_outputs_skipped_sections_without_urls() {
    let output = Command::new(full_product_input_runner_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product input runner should run");

    assert!(
        output.status.success(),
        "missing urls should still produce merged json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["summary"]["p0_count"], 0);
    assert_eq!(payload["summary"]["p1_count"], 0);
    assert_eq!(payload["summary"]["read_only_input_count"], 3);
    assert_eq!(
        payload["sections"]["web_task_order_health"]["skipped"],
        true
    );
    assert_eq!(
        payload["sections"]["news_source_ai_health"]["skipped"],
        true
    );
    assert_eq!(payload["sections"]["admin_readiness"]["skipped"], true);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "WEB_INPUT_SKIPPED"
                && alert["section"] == "web_task_order_health"),
        "web skipped section should be represented as an INFO alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "NEWS_INPUT_SKIPPED"
                && alert["section"] == "news_source_ai_health"),
        "news skipped section should be represented as an INFO alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "ADMIN_INPUT_SKIPPED"
                && alert["section"] == "admin_readiness"),
        "admin skipped section should be represented as an INFO alert: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "minimax-secret",
        "admin-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "input runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_input_runner_calls_producers_for_explicit_read_only_urls() {
    let tool_dir = fake_full_product_input_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(full_product_input_runner_path())
        .env("PATH", path)
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product input runner should run");

    assert!(
        output.status.success(),
        "explicit urls should produce merged json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["summary"]["p0_count"], 2);
    assert_eq!(payload["summary"]["p1_count"], 1);
    assert_eq!(payload["summary"]["web_open_task_count"], 2);
    assert_eq!(payload["summary"]["news_degraded_source_count"], 2);
    assert_eq!(
        payload["sections"]["web_task_order_health"]["source"],
        "json_path"
    );
    assert_eq!(
        payload["sections"]["web_task_order_health"]["missing_order_result_count"],
        1
    );
    assert_eq!(
        payload["sections"]["news_source_ai_health"]["recent_ai_analysis_count"],
        5
    );
    assert_eq!(
        payload["sections"]["admin_readiness"]["missing_required_action_count"],
        1
    );
    assert_eq!(payload["correlation"]["signal_inbox_id"], 3801);
    assert_eq!(payload["correlation"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["correlation"]["analysis_result_id"], 9001);
    assert_eq!(
        payload["correlation"]["admin_operation_log_id"],
        "admin-op-9002"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "WEB_ORDER_RESULT_MISSING"
                && alert["section"] == "web_task_order_health"),
        "web producer alert should be merged: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_SOURCE_DEGRADED"
                && alert["section"] == "news_source_ai_health"),
        "news producer alert should be merged: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "ADMIN_LIVE_READINESS_BLOCKED"
                && alert["section"] == "admin_readiness"),
        "admin producer alert should be merged: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "minimax-secret",
        "admin-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "api_key_cipher",
        "api_secret_cipher",
        "passphrase_cipher",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "input runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_web_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(web_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_web_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_web_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(script.contains("news_signal_inbox"));
    assert!(script.contains("execution_tasks"));
    assert!(script.contains("execution_task_attempts"));
    assert!(script.contains("exchange_order_results"));
    assert!(script.contains("user_trade_records"));
    assert!(script.contains("combo_signal_delivery_logs"));
    assert!(script.contains("WEB_ORDER_RESULT_MISSING"));
    assert!(script.contains("WEB_RETRY_BACKLOG"));
    assert!(script.contains("WEB_INPUT_SKIPPED"));
    assert!(
        script.contains("python3") || script.contains("python "),
        "producer should use structured json generation instead of shell string parsing"
    );
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/fapi/v1/positionSide/dual",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "producer must scan output for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
        "LINKUSDT",
        "LINK-USDT",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_web_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(web_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("web input producer should run");

    assert!(
        output.status.success(),
        "missing database url should produce degraded json without failing:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["open_task_count"], 0);
    assert_eq!(payload["missing_order_result_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "WEB_INPUT_SKIPPED"
                && alert["section"] == "web_task_order_health"),
        "skipped producer output should explain the degraded input: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped web input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_web_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_web_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(web_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env("FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS", "3600")
        .env("FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS", "900")
        .env("FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS", "900")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("web input producer should run");

    assert!(
        output.status.success(),
        "producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["source"], "quant_web_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 3600);
    assert_eq!(payload["open_task_count"], 2);
    assert_eq!(payload["stale_task_count"], 1);
    assert_eq!(payload["missing_order_result_count"], 1);
    assert_eq!(payload["failed_task_count"], 1);
    assert_eq!(payload["retry_backlog_count"], 1);
    assert_eq!(payload["delivery_blocker_count"], 1);
    assert_eq!(payload["recent_order_result_count"], 3);
    assert_eq!(payload["recent_trade_record_count"], 2);
    assert_eq!(payload["sample"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["signal_inbox_id"], 3801);
    assert_eq!(payload["correlation"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["execution_attempt_id"], 6101);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "WEB_ORDER_RESULT_MISSING"
                && alert["section"] == "web_task_order_health"),
        "producer should surface missing Web order result as P0: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "WEB_RETRY_BACKLOG"
                && alert["section"] == "web_task_order_health"),
        "producer should surface retry backlog as P1: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_news_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(news_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_news_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_news_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL"));
    assert!(script.contains("news_source_states"));
    assert!(script.contains("news_source_health"));
    assert!(script.contains("news_ai_analysis_results"));
    assert!(script.contains("news_analysis_jobs"));
    assert!(script.contains("news_provider_call_logs"));
    assert!(script.contains("news_items_jinse"));
    assert!(script.contains("NEWS_SOURCE_DEGRADED"));
    assert!(script.contains("NEWS_AI_PROVIDER_UNAVAILABLE"));
    assert!(script.contains("NEWS_INPUT_SKIPPED"));
    assert!(
        script.contains("python3") || script.contains("python "),
        "producer should use structured json generation instead of shell string parsing"
    );
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "producer must scan output for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
        "LINKUSDT",
        "LINK-USDT",
        "MINIMAX_API_KEY",
        "OPENAI_API_KEY",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_news_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(news_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("news input producer should run");

    assert!(
        output.status.success(),
        "missing database url should produce degraded json without failing:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["degraded_source_count"], 0);
    assert_eq!(payload["recent_ai_analysis_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "NEWS_INPUT_SKIPPED"
                && alert["section"] == "news_source_ai_health"),
        "skipped producer output should explain the degraded input: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "minimax-secret",
        "binance-secret",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped news input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_news_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_news_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(news_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env("FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS", "7200")
        .env("FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS", "1800")
        .env("FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS", "7200")
        .env("FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD", "3")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("news input producer should run");

    assert!(
        output.status.success(),
        "producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "quant_news_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 7200);
    assert_eq!(payload["stale_analysis_secs"], 1800);
    assert_eq!(payload["failed_job_secs"], 7200);
    assert_eq!(payload["source_count"], 4);
    assert_eq!(payload["degraded_source_count"], 2);
    assert_eq!(payload["paused_source_count"], 1);
    assert_eq!(payload["retryable_source_count"], 1);
    assert_eq!(payload["recent_news_count"], 12);
    assert_eq!(payload["signal_candidate_count"], 3);
    assert_eq!(payload["recent_ai_analysis_count"], 5);
    assert_eq!(payload["actionable_analysis_count"], 2);
    assert_eq!(payload["failed_analysis_job_count"], 1);
    assert_eq!(payload["stuck_analysis_job_count"], 1);
    assert_eq!(payload["provider_failure_count"], 1);
    assert_eq!(payload["active_prompt_config_count"], 1);
    assert_eq!(payload["sample"]["source"], "theblockbeats");
    assert_eq!(payload["sample"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["sample"]["analysis_result_id"], 9001);
    assert_eq!(payload["correlation"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["correlation"]["analysis_result_id"], 9001);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_SOURCE_DEGRADED"
                && alert["section"] == "news_source_ai_health"),
        "producer should surface degraded news sources as P1: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_AI_PROVIDER_UNAVAILABLE"
                && alert["section"] == "news_source_ai_health"),
        "producer should surface AI provider failures as P1: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "minimax-secret",
        "binance-secret",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(admin_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_admin_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_admin_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL"));
    assert!(script.contains("admin_operation_logs"));
    assert!(script.contains("risk_review_confirm"));
    assert!(script.contains("api_key_upsert"));
    assert!(script.contains("onchain_provider_control_upsert"));
    assert!(script.contains("strategy_config_upsert"));
    assert!(script.contains("backtest_run"));
    assert!(script.contains("exchange_symbol_sync"));
    assert!(script.contains("manual_ai_analysis"));
    assert!(script.contains("ADMIN_LIVE_READINESS_BLOCKED"));
    assert!(script.contains("ADMIN_HIGH_RISK_OPERATION_FAILED"));
    assert!(script.contains("ADMIN_ACTION_AUDIT_MISSING"));
    assert!(script.contains("ADMIN_INPUT_SKIPPED"));
    assert!(
        script.contains("python3") || script.contains("python "),
        "producer should use structured json generation instead of shell string parsing"
    );
    for required in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "api_key_cipher",
        "api_secret_cipher",
        "passphrase_cipher",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            script.contains(required),
            "producer must scan output for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "curl ",
        "wget ",
        "podman exec",
        "docker exec",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
        "LINKUSDT",
        "LINK-USDT",
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_admin_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(admin_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("admin input producer should run");

    assert!(
        output.status.success(),
        "missing database url should produce degraded json without failing:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["high_risk_operation_count"], 0);
    assert_eq!(payload["missing_required_action_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "ADMIN_INPUT_SKIPPED"
                && alert["section"] == "admin_readiness"),
        "skipped producer output should explain the degraded input: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "admin-secret",
        "binance-secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped admin input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_admin_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(admin_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .env("FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS", "7200")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("admin input producer should run");

    assert!(
        output.status.success(),
        "producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["source"], "quant_admin_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 7200);
    assert_eq!(payload["required_action_count"], 8);
    assert_eq!(payload["recent_operation_count"], 11);
    assert_eq!(payload["high_risk_operation_count"], 9);
    assert_eq!(payload["failed_operation_count"], 2);
    assert_eq!(payload["missing_required_action_count"], 1);
    assert_eq!(payload["readiness_blocker_count"], 1);
    assert_eq!(payload["manual_review_count"], 2);
    assert_eq!(payload["sample"]["admin_operation_log_id"], "admin-op-9002");
    assert_eq!(payload["sample"]["action"], "exchange_symbol_sync");
    assert_eq!(
        payload["correlation"]["admin_operation_log_id"],
        "admin-op-9002"
    );
    assert_eq!(
        payload["correlation"]["admin_action"],
        "exchange_symbol_sync"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "ADMIN_LIVE_READINESS_BLOCKED"
                && alert["section"] == "admin_readiness"),
        "producer should surface admin readiness blockers as P0: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "ADMIN_HIGH_RISK_OPERATION_FAILED"
                && alert["section"] == "admin_readiness"),
        "producer should surface failed admin operations as P1: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "admin-secret",
        "binance-secret",
        "api_key_cipher",
        "api_secret_cipher",
        "passphrase_cipher",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn local_service_health_json_contract_keeps_backward_compatible_admin_ci_fields() {
    let tool_dir = fake_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(script_path())
        .env("PATH", path)
        .env("HEALTH_CHECK_OUTPUT", "json")
        .env("HEALTH_CHECK_BINANCE", "false")
        .env("HEALTH_CHECK_DATABASES", "true")
        .env("HEALTH_CHECK_WORKER_STALE_SECS", "60")
        .env("HEALTH_CHECK_EXPECTED_WORKERS", "worker_stale")
        .env("HEALTH_CHECK_WORKER_STALE_LEVEL", "fail")
        .output()
        .expect("health script should run");

    assert!(
        !output.status.success(),
        "expected worker failure should produce non-zero exit status"
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    for key in [
        "output",
        "status",
        "warnings",
        "failures",
        "repo",
        "umbrella",
        "quant_core_database_url",
        "web_database_url",
        "news_database_url",
        "execution_event_secret",
        "database_checks",
        "binance_public_check",
        "execution_audit_check",
        "worker_stale_secs",
        "worker_stale_level",
        "worker_mode",
        "expected_workers",
        "summary",
        "checks",
        "alerts",
    ] {
        assert!(
            payload.get(key).is_some(),
            "json contract missing top-level field {key}: {stdout}"
        );
    }

    for key in [
        "expected_worker_failures",
        "expected_worker_warnings",
        "ignored_worker_count",
        "ignored_stale_worker_count",
    ] {
        assert!(
            payload["summary"][key].as_u64().is_some(),
            "json summary field {key} must be numeric: {stdout}"
        );
    }
    assert_eq!(payload["summary"]["expected_worker_failures"], 1);
    assert_eq!(payload["summary"]["ignored_worker_count"], 1);
    assert!(
        payload["summary"]["execution_audit_recent_failures"]
            .as_u64()
            .is_some(),
        "execution audit summary must be numeric when present: {stdout}"
    );
    assert!(
        payload["checks"].as_array().is_some(),
        "checks must remain an array for existing consumers: {stdout}"
    );
    let alert_list = alerts(&payload);
    assert!(
        alert_list.iter().any(|alert| alert["severity"] == "P0"
            && alert["code"] == "EXPECTED_WORKER_STALE"
            && alert["section"] == "Databases"
            && alert["message"]
                .as_str()
                .unwrap_or_default()
                .contains("expected_stale_worker_id=worker_stale")),
        "expected worker failure should produce a P0 alert: {stdout}"
    );
    assert!(
        alert_list.iter().any(|alert| alert["severity"] == "P1"
            && alert["code"] == "UNEXPECTED_WORKER"
            && alert["message"]
                .as_str()
                .unwrap_or_default()
                .contains("ignored_worker_id=worker_fresh")),
        "ignored unlisted worker should produce a P1 alert: {stdout}"
    );
    for alert in alert_list {
        for key in ["severity", "code", "section", "message"] {
            assert!(
                alert[key].as_str().is_some(),
                "alert field {key} must be a stable string: {stdout}"
            );
        }
    }
}

#[test]
fn local_service_health_execution_audit_opt_in_is_read_only_and_alerts_on_audit_failures() {
    let tool_dir = fake_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(script_path())
        .env("PATH", path)
        .env("HEALTH_CHECK_OUTPUT", "json")
        .env("HEALTH_CHECK_BINANCE", "false")
        .env("HEALTH_CHECK_DATABASES", "true")
        .env("HEALTH_CHECK_EXECUTION_AUDIT", "true")
        .env("HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS", "24")
        .env("HEALTH_CHECK_WORKER_STALE_SECS", "60")
        .output()
        .expect("health script should run");

    assert!(
        output.status.success(),
        "execution audit warnings should not fail without strict mode:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["execution_audit_check"], "true");
    assert_eq!(payload["summary"]["execution_audit_recent_failures"], 2);
    assert_eq!(
        payload["summary"]["execution_audit_stale_leased_workers"],
        1
    );
    assert!(
        payload["checks"]
            .as_array()
            .expect("checks should be an array")
            .iter()
            .any(|check| check["message"]
                .as_str()
                .unwrap_or_default()
                .contains("exchange_request_audit_logs: recent_total=4 recent_failures=2")),
        "execution audit should report recent failed request count without endpoint or payload: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "EXCHANGE_REQUEST_AUDIT_FAILURES"
                && alert["section"] == "Execution Audit"),
        "recent exchange audit failures should produce P1 alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "WORKER_LEASE_STALE"
                && alert["section"] == "Execution Audit"),
        "stale leased worker checkpoint should produce P1 alert: {stdout}"
    );
    for sensitive in [
        "postgres://user:secret@db",
        "request_payload",
        "response_payload",
        "api_key",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !stdout.contains(sensitive),
            "execution audit output must not leak sensitive value {sensitive}: {stdout}"
        );
    }
}

#[test]
fn local_service_health_expected_worker_stale_can_warn_or_fail_without_old_workers() {
    let tool_dir = fake_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );

    let warn_output = Command::new(script_path())
        .env("PATH", &path)
        .env("HEALTH_CHECK_OUTPUT", "json")
        .env("HEALTH_CHECK_BINANCE", "false")
        .env("HEALTH_CHECK_DATABASES", "true")
        .env("HEALTH_CHECK_WORKER_STALE_SECS", "60")
        .env("HEALTH_CHECK_EXPECTED_WORKERS", "worker_stale")
        .output()
        .expect("health script should run");

    assert!(
        warn_output.status.success(),
        "expected stale worker should warn but not fail by default:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&warn_output.stdout),
        String::from_utf8_lossy(&warn_output.stderr)
    );
    let warn_stdout = String::from_utf8(warn_output.stdout).expect("json output should be utf8");
    let warn_payload: Value = serde_json::from_str(&warn_stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{warn_stdout}"));

    assert_eq!(warn_payload["status"], "warn");
    assert_eq!(warn_payload["worker_mode"], "expected");
    assert_eq!(warn_payload["expected_workers"], "worker_stale");
    assert_eq!(warn_payload["warnings"], 1);
    assert_eq!(warn_payload["failures"], 0);
    assert_eq!(warn_payload["summary"]["expected_worker_failures"], 0);
    assert_eq!(warn_payload["summary"]["expected_worker_warnings"], 1);
    assert_eq!(warn_payload["summary"]["ignored_worker_count"], 1);
    assert_eq!(warn_payload["summary"]["ignored_stale_worker_count"], 0);
    let warn_checks = warn_payload["checks"]
        .as_array()
        .expect("checks should be an array");
    assert!(
        warn_checks.iter().any(|check| check["level"] == "WARN"
            && check["message"]
                .as_str()
                .unwrap_or_default()
                .contains("expected_stale_worker_id=worker_stale")),
        "expected stale worker should warn: {warn_stdout}"
    );
    assert!(
        warn_checks.iter().any(|check| check["level"] == "INFO"
            && check["message"]
                .as_str()
                .unwrap_or_default()
                .contains("ignored_worker_id=worker_fresh")),
        "unlisted worker should be visible as ignored: {warn_stdout}"
    );
    assert!(
        alerts(&warn_payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "EXPECTED_WORKER_STALE"
                && alert["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("expected_stale_worker_id=worker_stale")),
        "expected stale warning should produce a P1 alert: {warn_stdout}"
    );

    let fail_output = Command::new(script_path())
        .env("PATH", path)
        .env("HEALTH_CHECK_OUTPUT", "json")
        .env("HEALTH_CHECK_BINANCE", "false")
        .env("HEALTH_CHECK_DATABASES", "true")
        .env("HEALTH_CHECK_WORKER_STALE_SECS", "60")
        .env("HEALTH_CHECK_EXPECTED_WORKERS", "worker_stale")
        .env("HEALTH_CHECK_WORKER_STALE_LEVEL", "fail")
        .output()
        .expect("health script should run");

    assert!(
        !fail_output.status.success(),
        "expected stale worker should fail when stale level is fail"
    );
    let fail_stdout = String::from_utf8(fail_output.stdout).expect("json output should be utf8");
    let fail_payload: Value = serde_json::from_str(&fail_stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{fail_stdout}"));
    assert_eq!(fail_payload["status"], "fail");
    assert_eq!(fail_payload["warnings"], 0);
    assert_eq!(fail_payload["failures"], 1);
    assert_eq!(fail_payload["summary"]["expected_worker_failures"], 1);
    assert_eq!(fail_payload["summary"]["expected_worker_warnings"], 0);
    assert_eq!(fail_payload["summary"]["ignored_worker_count"], 1);
    assert_eq!(fail_payload["summary"]["ignored_stale_worker_count"], 0);
    assert!(
        fail_payload["checks"]
            .as_array()
            .expect("checks should be an array")
            .iter()
            .any(|check| check["level"] == "FAIL"
                && check["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("expected_stale_worker_id=worker_stale")),
        "expected stale worker should fail: {fail_stdout}"
    );
    assert!(
        alerts(&fail_payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "EXPECTED_WORKER_STALE"
                && alert["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("expected_stale_worker_id=worker_stale")),
        "expected stale failure should produce a P0 alert: {fail_stdout}"
    );
}

#[test]
fn local_service_health_script_redacts_sensitive_runtime_values() {
    let script = read_script();
    let printed_lines = script
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("echo ") || trimmed.starts_with("printf ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(script.contains("redact_value()"));
    assert!(
        !printed_lines.contains("${QUANT_CORE_DATABASE_URL}")
            && !printed_lines.contains("${WEB_DATABASE_URL}")
            && !printed_lines.contains("${NEWS_DATABASE_URL}")
            && !printed_lines.contains("${DATABASE_URL}")
            && !printed_lines.contains("${EXECUTION_EVENT_SECRET}")
            && !printed_lines.contains("${BINANCE_API_KEY}")
            && !printed_lines.contains("${BINANCE_API_SECRET}"),
        "script must not print raw database URLs or secrets"
    );
}

#[test]
fn local_service_health_script_surfaces_worker_checkpoint_observability() {
    let script = read_script();

    assert!(script.contains("execution_worker_checkpoints"));
    assert!(script.contains("exchange_request_audit_logs"));
    assert!(script.contains("worker_id"));
    assert!(script.contains("worker_status"));
    assert!(script.contains("last_task_id"));
    assert!(script.contains("last_heartbeat_at"));
    assert!(
        script.contains("SELECT") && !script.contains("UPDATE execution_worker_checkpoints"),
        "checkpoint diagnostics should be read-only"
    );
    assert!(
        !script.contains("INSERT INTO execution_worker_checkpoints")
            && !script.contains("DELETE FROM execution_worker_checkpoints")
            && !script.contains("INSERT INTO exchange_request_audit_logs")
            && !script.contains("UPDATE exchange_request_audit_logs")
            && !script.contains("DELETE FROM exchange_request_audit_logs"),
        "execution health diagnostics must not write audit tables"
    );
}

#[test]
fn local_service_health_script_can_use_local_postgres_container_without_raw_urls() {
    let script = read_script();

    assert!(script.contains("POSTGRES_CONTAINER"));
    assert!(script.contains("podman exec"));
    assert!(script.contains("QUANT_CORE_POSTGRES_DB"));
    assert!(script.contains("WEB_POSTGRES_DB"));
    assert!(script.contains("NEWS_POSTGRES_DB"));
    assert!(
        !script.contains("podman exec ${POSTGRES_CONTAINER}"),
        "podman command should keep shell quoting explicit"
    );
}

#[test]
fn full_product_health_artifact_set_publisher_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_artifact_set_publisher_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_artifact_set_publisher_is_explicit_path_only_and_read_only() {
    let script = read_full_product_artifact_set_publisher_script();

    for required in [
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SLA_SECONDS",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL",
        "python3 -",
        ".env",
        "postgres://",
        "api_key",
        "api_secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "LINKUSDT",
        "LOCAL_PATH_URL_BLOCKED",
    ] {
        assert!(
            script.contains(required),
            "publisher script should document or scan marker {required}"
        );
    }

    for forbidden in [
        "source .env",
        "cat .env",
        "find ",
        "rg ",
        "curl ",
        "wget ",
        "cargo run",
        "podman exec",
        "docker exec",
    ] {
        assert!(
            !script.contains(forbidden),
            "publisher must stay explicit-path-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_artifact_set_publisher_requires_explicit_existing_paths() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let markdown_path = examples_dir.join("full-product-health.md");
    let missing_summary_path = examples_dir.join("missing-summary.json");

    let output = Command::new(full_product_artifact_set_publisher_path())
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
            &missing_summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
            &markdown_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
            "2026-05-07T01:03:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW",
            "2026-05-07T01:05:00Z",
        )
        .output()
        .expect("artifact publisher should run");

    assert!(
        !output.status.success(),
        "publisher should fail when an explicit artifact path is missing"
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["storageStatus"], "rejected");
    assert_eq!(payload["retentionClass"], "rejected");
    assert_eq!(payload["validation"]["status"], "fail");
    assert_eq!(payload["validation"]["summary"]["missingArtifactCount"], 1);
    assert!(
        payload["validation"]["findings"]
            .as_array()
            .expect("findings should be an array")
            .iter()
            .any(|item| item["code"] == "ARTIFACT_MISSING" && item["artifact"] == "summary"),
        "missing summary path should produce an ARTIFACT_MISSING finding: {stdout}"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "secret@",
        "/fapi/v1/order",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "publisher output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_artifact_set_publisher_emits_storage_ready_metadata_and_redaction_summary() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");
    let summary_fixture: Value = serde_json::from_str(
        &fs::read_to_string(&summary_path).expect("summary example should be readable"),
    )
    .expect("summary example should be valid json");

    let output = Command::new(full_product_artifact_set_publisher_path())
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
            &summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
            &markdown_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
            "2026-05-07T01:03:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW",
            "2026-05-07T01:05:00Z",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_SLA_SECONDS", "900")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY",
            "phase-51-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID", "phase-51-run")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA", "abcdef1")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO", "rust_quant")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json",
        )
        .output()
        .expect("artifact publisher should run");

    assert!(
        output.status.success(),
        "publisher should emit parseable json for complete fixtures:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["schemaVersion"], 1);
    assert_eq!(payload["storedAt"], "2026-05-07T01:03:00Z");
    assert_eq!(payload["sourceGeneratedAt"], "2026-05-07T01:00:00Z");
    assert_eq!(payload["storageStatus"], "current");
    assert_eq!(payload["retentionClass"], "current");
    assert_eq!(payload["artifactSlaSeconds"], 900);
    assert_eq!(payload["stale"], false);
    assert_eq!(payload["staleReason"], Value::Null);
    assert_eq!(payload["summary"], summary_fixture);
    assert_eq!(payload["validation"]["status"], "ok");
    assert_eq!(payload["validation"]["summary"]["sensitiveMarkerCount"], 0);
    assert_eq!(payload["validation"]["summary"]["missingArtifactCount"], 0);
    assert_eq!(payload["redaction"]["status"], "ok");
    assert_eq!(payload["redaction"]["sensitiveMarkerCount"], 0);
    assert_eq!(
        payload["markdownUrl"],
        "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md"
    );
    assert_eq!(
        payload["fullArtifactUrl"],
        "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json"
    );
    assert_eq!(
        payload["operatorMetadata"]["generatedBy"],
        "phase-51-contract-test"
    );
    assert_eq!(payload["operatorMetadata"]["triggerType"], "ci");
    assert_eq!(payload["operatorMetadata"]["runId"], "phase-51-run");
    assert_eq!(payload["operatorMetadata"]["commitSha"], "abcdef1");
    assert_eq!(payload["operatorMetadata"]["sourceRepo"], "rust_quant");
    assert!(
        payload["artifactSetId"]
            .as_str()
            .expect("artifactSetId should be a string")
            .starts_with("health-2026-05-07T01-00-00Z-"),
        "artifactSetId should derive from sourceGeneratedAt and content hash: {stdout}"
    );

    for field in [
        "artifactSetId",
        "schemaVersion",
        "storedAt",
        "sourceGeneratedAt",
        "summaryHash",
        "validationHash",
        "fullArtifactHash",
        "markdownHash",
        "storageStatus",
        "retentionClass",
        "artifactSlaSeconds",
        "stale",
        "staleReason",
        "summary",
        "validation",
        "redaction",
        "markdownUrl",
        "fullArtifactUrl",
        "operatorMetadata",
    ] {
        assert!(
            payload.get(field).is_some(),
            "missing top-level field {field}: {stdout}"
        );
    }

    for field in [
        "summaryHash",
        "validationHash",
        "fullArtifactHash",
        "markdownHash",
    ] {
        let value = payload[field]
            .as_str()
            .unwrap_or_else(|| panic!("{field} should be a string: {stdout}"));
        assert_eq!(value.len(), 64, "{field} should be a SHA-256 hex digest");
        assert!(
            value.chars().all(|ch| ch.is_ascii_hexdigit()),
            "{field} should contain only hex digits: {value}"
        );
    }

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "publisher output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_artifact_set_publisher_redacts_operator_metadata_and_urls_must_not_be_local_paths(
) {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let output = Command::new(full_product_artifact_set_publisher_path())
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
            &summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
            &markdown_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
            "2026-05-07T01:03:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW",
            "2026-05-07T01:05:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY",
            "/Users/mac2/phase-52-worker",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL",
            "/Users/mac2/tmp/full-product-health.md",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL",
            "file:///Users/mac2/tmp/full-product-health.json",
        )
        .output()
        .expect("artifact publisher should run");

    assert!(
        !output.status.success(),
        "publisher should reject local filesystem paths in handoff urls"
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["storageStatus"], "rejected");
    assert_eq!(payload["retentionClass"], "rejected");
    assert_eq!(payload["operatorMetadata"]["generatedBy"], "[redacted]");
    assert_eq!(payload["markdownUrl"], Value::Null);
    assert_eq!(payload["fullArtifactUrl"], Value::Null);
    assert!(
        payload["validation"]["findings"]
            .as_array()
            .expect("findings should be an array")
            .iter()
            .any(|item| item["code"] == "OPERATOR_METADATA_REDACTED"
                && item["field"] == "generatedBy"),
        "operator metadata path should be redacted: {stdout}"
    );
    assert!(
        payload["validation"]["findings"]
            .as_array()
            .expect("findings should be an array")
            .iter()
            .filter(|item| item["code"] == "LOCAL_PATH_URL_BLOCKED")
            .count()
            == 2,
        "both local-path urls should be blocked: {stdout}"
    );
    assert!(
        !stdout.contains("/Users/mac2/tmp"),
        "publisher output must not leak local filesystem paths: {stdout}"
    );
}

#[test]
fn full_product_health_admin_ingest_fixture_is_machine_readable_and_redacted() {
    let path = full_product_admin_ingest_fixture_path();
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));
    let payload: Value = serde_json::from_str(&body)
        .unwrap_or_else(|error| panic!("fixture should be valid json: {error}\n{body}"));

    for field in [
        "artifactSetId",
        "schemaVersion",
        "storedAt",
        "sourceGeneratedAt",
        "summaryHash",
        "validationHash",
        "fullArtifactHash",
        "markdownHash",
        "storageStatus",
        "retentionClass",
        "artifactSlaSeconds",
        "stale",
        "staleReason",
        "summary",
        "validation",
        "redaction",
        "markdownUrl",
        "fullArtifactUrl",
        "operatorMetadata",
    ] {
        assert!(
            payload.get(field).is_some(),
            "fixture missing field {field}: {body}"
        );
    }

    let lowered = body.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        "api_key",
        "api_secret",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "file://",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "fixture must not contain sensitive marker {sensitive}: {body}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_smoke_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_admin_ingest_smoke_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_admin_ingest_smoke_stays_no_env_and_localhost_only() {
    let script = read_full_product_admin_ingest_smoke_script();

    for required in [
        "ADMIN_INGEST_URL",
        "ADMIN_INGEST_ALLOW_REMOTE",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "publish_full_product_health_artifact_set.sh",
        "localhost",
        "127.0.0.1",
        "Authorization",
        ".env",
        "postgres://",
        "api_key",
        "api_secret",
        "raw_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "LINKUSDT",
    ] {
        assert!(
            script.contains(required),
            "admin ingest smoke script should document or scan marker {required}"
        );
    }

    for forbidden in [
        "source .env",
        "cat .env",
        "cargo run",
        "podman exec",
        "docker exec",
        "Authorization:",
        "--header Authorization",
    ] {
        assert!(
            !script.contains(forbidden),
            "admin ingest smoke script must avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_contract_wrapper_and_mock_receiver_stay_local_and_safe() {
    let receiver = read_full_product_admin_ingest_mock_receiver_script();
    let contract = read_full_product_admin_ingest_contract_smoke_script();

    for required in [
        "127.0.0.1",
        ".env",
        "Authorization",
        "api_key",
        "api_secret",
        "raw_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "LINKUSDT",
    ] {
        assert!(
            receiver.contains(required),
            "mock receiver script should document or scan marker {required}"
        );
    }

    for required in [
        "mock_full_product_health_admin_ingest_receiver.py",
        "smoke_publish_full_product_health_admin_ingest.sh",
        "127.0.0.1",
        "/admin/ingest",
        "mock_contract",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
        ".env",
        "does not scan directories",
        "Authorization",
    ] {
        assert!(
            contract.contains(required),
            "contract smoke script should document or use marker {required}"
        );
    }

    for forbidden in [
        "source .env",
        "cat .env",
        "docker exec",
        "podman exec",
        "curl https://",
        "curl http://",
    ] {
        assert!(
            !contract.contains(forbidden),
            "contract smoke script must avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_smoke_prints_parseable_redacted_payload_without_url() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let output = Command::new(full_product_admin_ingest_smoke_path())
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
            &summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
            &markdown_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
            "2026-05-07T01:03:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW",
            "2026-05-07T01:05:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY",
            "phase-53-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID",
            "phase-53-dry-run",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA", "abcdef1")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO", "rust_quant")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json",
        )
        .output()
        .expect("admin ingest smoke should run");

    assert!(
        output.status.success(),
        "dry-run smoke should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let payload: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("dry-run stdout should be parseable json: {error}\n{stdout}")
    });

    assert_eq!(
        payload["artifactSetId"],
        "health-2026-05-07T01-00-00Z-c77aa5e0ee88"
    );
    assert_eq!(
        payload["operatorMetadata"]["generatedBy"],
        "phase-53-contract-test"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "dry-run stdout must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_smoke_posts_to_local_mock_without_leaking_payload_or_paths() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener should expose addr");
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("mock server should accept");
        let mut header_bytes = Vec::new();
        let mut single = [0_u8; 1];
        loop {
            stream
                .read_exact(&mut single)
                .expect("mock server should read request headers");
            header_bytes.push(single[0]);
            if header_bytes.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header_text = String::from_utf8(header_bytes).expect("headers should be utf8");
        let content_length = header_text
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("Content-Length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .expect("request should include content-length");
        let mut body_bytes = vec![0_u8; content_length];
        stream
            .read_exact(&mut body_bytes)
            .expect("mock server should read request body");
        let request = format!(
            "{header_text}{}",
            String::from_utf8(body_bytes).expect("body should be utf8")
        );
        tx.send(request).expect("request should be sent to test");
        stream
            .write_all(
                b"HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 41\r\n\r\n{\"status\":\"accepted\",\"requestId\":\"mock-1\"}",
            )
            .expect("mock server should write response");
    });

    let output = Command::new(full_product_admin_ingest_smoke_path())
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
            &summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
            &markdown_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
            "2026-05-07T01:03:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW",
            "2026-05-07T01:05:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY",
            "phase-53-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID", "phase-53-post")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA", "abcdef1")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO", "rust_quant")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json",
        )
        .env(
            "ADMIN_INGEST_URL",
            format!("http://127.0.0.1:{}/admin/ingest", address.port()),
        )
        .output()
        .expect("admin ingest smoke should run");

    handle.join().expect("mock server thread should finish");

    assert!(
        output.status.success(),
        "localhost POST smoke should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let request = rx.recv().expect("mock server should capture request");
    assert!(request.starts_with("POST /admin/ingest HTTP/1.1\r\n"));
    assert!(request.contains("\r\nHost: 127.0.0.1:"));
    assert!(request.contains("\r\nContent-Type: application/json\r\n"));
    assert!(!request.contains("Authorization:"));
    assert!(!request.to_ascii_lowercase().contains("postgres://"));
    assert!(!request.to_ascii_lowercase().contains("api_key"));
    assert!(!request.contains("/Users/"));

    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .expect("request should contain a body");
    let payload: Value = serde_json::from_str(body)
        .unwrap_or_else(|error| panic!("request body should be parseable json: {error}\n{body}"));
    assert_eq!(payload["operatorMetadata"]["runId"], "phase-53-post");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "\"artifactsetid\"",
    ] {
        assert!(
            !combined.contains(sensitive),
            "POST smoke output must not leak sensitive marker {sensitive}: stdout={stdout}\nstderr={stderr}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_contract_smoke_uses_local_mock_receiver_and_safe_stdout() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let output = Command::new(full_product_admin_ingest_contract_smoke_path())
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
            &full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
            &summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
            &markdown_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
            "2026-05-07T01:03:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW",
            "2026-05-07T01:05:00Z",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY",
            "phase-54-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID",
            "phase-54-contract",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA", "abcdef1")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO", "rust_quant")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL",
            "/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json",
        )
        .output()
        .expect("admin ingest contract smoke should run");

    assert!(
        output.status.success(),
        "contract smoke should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let payload: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("contract smoke stdout should be parseable json: {error}\n{stdout}")
    });

    assert_eq!(payload["mode"], "mock_contract");
    assert_eq!(payload["request"]["method"], "POST");
    assert_eq!(payload["request"]["path"], "/admin/ingest");
    assert_eq!(payload["request"]["contentType"], "application/json");
    assert_eq!(payload["request"]["hasAuthorization"], false);
    assert_eq!(payload["request"]["body"]["redactionStatus"], "ok");
    assert_eq!(payload["request"]["body"]["sensitiveMarkerCount"], 0);
    assert_eq!(
        payload["request"]["body"]["operatorRunId"],
        "phase-54-contract"
    );
    assert_eq!(payload["delivery"]["http"]["status"], 202);
    assert_eq!(payload["delivery"]["http"]["ok"], true);
    assert_eq!(payload["delivery"]["response"]["status"], "accepted");

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "\"artifactsetid\"",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "contract smoke stdout must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_contract_smoke_requires_explicit_artifact_env_before_starting_mock_receiver(
) {
    let output = Command::new(full_product_admin_ingest_contract_smoke_path())
        .output()
        .expect("admin ingest contract smoke should run");

    assert!(
        !output.status.success(),
        "contract smoke should fail safely when explicit artifact env is missing"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");

    assert!(
        stdout.trim().is_empty(),
        "missing artifact env should fail before emitting stdout summary: {stdout}"
    );

    for required in [
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
        "does not read .env",
        "does not scan directories",
    ] {
        assert!(
            stderr.contains(required),
            "missing artifact env failure should mention {required}: {stderr}"
        );
    }

    let lowered = stderr.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        "api_key",
        "api_secret",
        "/fapi/v1/order",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "missing artifact env failure must stay sanitized and avoid {sensitive}: {stderr}"
        );
    }
}
