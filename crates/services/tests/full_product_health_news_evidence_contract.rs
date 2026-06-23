use serde_json::Value;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}
fn news_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_news_input.sh")
}
fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
    }
}
fn fake_news_evidence_tool_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!(
        "news-evidence-health-contract-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create {}: {error}", dir.display()));
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
    "/fapi/v1/order" \
    "/fapi/v2/account" \
    "/fapi/v1/positionRisk" \
    "/api/commerce/internal/execution-tasks/lease" \
    "/api/commerce/internal/execution-results" \
    "/api/commerce/internal/order-results"
do
    if [[ "${args}" == *"${forbidden}"* ]]; then
        printf 'forbidden marker reached fake psql: %s\n' "${forbidden}" >&2
        exit 3
    fi
done
if [[ "${args}" == *"news_signal_inbox"* ]]; then
    for required in \
        "combo_signal_delivery_logs" \
        "execution_tasks" \
        "request_payload_json" \
        "selected_stop_loss_price" \
        "risk_plan_missing"
    do
        if [[ "${args}" != *"${required}"* ]]; then
            printf 'expected web evidence query to read %s\n' "${required}" >&2
            exit 5
        fi
    done
    cat <<'JSON'
{"status":"ok","source":"quant_web_readonly_db","read_only_input":true,"risk_plan_evidence_status":"ready","risk_plan_selected_stop_loss":3136.0,"risk_plan_evidence_source":"quant_web.execution_tasks.risk_plan.selected_stop_loss_price","web_signal_inbox_id":801,"web_execution_task_id":901,"web_delivery_blocker_count":1,"web_delivery_blocker_codes":["risk_plan_missing"],"web_delivery_blocker_source":"quant_web.combo_signal_delivery_logs","sample":{"web_signal_inbox_id":801,"web_execution_task_id":901,"web_delivery_blocker_code":"risk_plan_missing"}}
JSON
    exit 0
fi
for required in \
    "news_ai_analysis_results" \
    "to_jsonb(nar)" \
    "'raw' || '_response'" \
    "ticker_source" \
    "ticker_at"
do
    if [[ "${args}" != *"${required}"* ]]; then
        printf 'expected news evidence query to read %s\n' "${required}" >&2
        exit 4
    fi
done
cat <<'JSON'
{"status":"ok","source":"quant_news_readonly_db","database_engine":"postgresql","read_only_input":true,"lookback_secs":7200,"stale_analysis_secs":1800,"failed_job_secs":7200,"source_failure_threshold":3,"source_count":1,"degraded_source_count":0,"paused_source_count":0,"retryable_source_count":0,"recent_news_count":1,"signal_candidate_count":1,"recent_ai_analysis_count":1,"actionable_analysis_count":1,"failed_analysis_job_count":0,"stuck_analysis_job_count":0,"provider_failure_count":0,"active_prompt_config_count":1,"ticker_source":"binance_futures_mark_price","ticker_at":"2026-06-02T03:00:00Z","entry_reference_price":"3200.00","risk_plan_evidence_status":"not_collected","risk_plan_selected_stop_loss":null,"risk_plan_evidence_source":"not_collected","sample":{"source":"jinse","news_id":"jinse-20260602-001","analysis_result_id":9101,"analysis_signal":"buy","ticker_source":"binance_futures_mark_price","ticker_at":"2026-06-02T03:00:00Z","entry_reference_price":"3200.00"},"alerts":[],"correlation":{"news_id":"jinse-20260602-001","analysis_result_id":9101,"external_id":null}}
JSON
"#,
    );
    dir
}
#[test]
fn news_input_merges_producer_price_and_web_risk_plan_evidence() {
    let tool_dir = fake_news_evidence_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(news_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://readonly:redacted@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://readonly:redacted@db/quant_web",
        )
        .env("FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS", "7200")
        .env("MINIMAX_API_KEY", "must-not-leak")
        .env("BINANCE_API_SECRET", "must-not-leak")
        .output()
        .expect("news input producer should run");
    assert!(
        output.status.success(),
        "producer should emit degraded json instead of failing the process:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));
    assert_eq!(payload["source"], "quant_news_readonly_db");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["ticker_source"], "binance_futures_mark_price");
    assert_eq!(payload["ticker_at"], "2026-06-02T03:00:00Z");
    assert_eq!(
        payload["sample"]["ticker_source"],
        "binance_futures_mark_price"
    );
    assert_eq!(payload["sample"]["ticker_at"], "2026-06-02T03:00:00Z");
    assert_eq!(payload["risk_plan_evidence_status"], "ready");
    assert_eq!(payload["risk_plan_selected_stop_loss"], 3136.0);
    assert_eq!(
        payload["risk_plan_evidence_source"],
        "quant_web.execution_tasks.risk_plan.selected_stop_loss_price"
    );
    assert_eq!(payload["web_signal_inbox_id"], 801);
    assert_eq!(payload["web_execution_task_id"], 901);
    assert_eq!(payload["web_delivery_blocker_count"], 1);
    assert_eq!(
        payload["web_delivery_blocker_codes"][0],
        "risk_plan_missing"
    );
    assert_eq!(
        payload["web_delivery_blocker_source"],
        "quant_web.combo_signal_delivery_logs"
    );
    assert_eq!(
        payload["sample"]["web_delivery_blocker_code"],
        "risk_plan_missing"
    );
    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "database_url",
        "api_key",
        "api_secret",
        "must-not-leak",
        "raw_response",
        "request_json",
        "response_json",
        "response_text",
        "request_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
