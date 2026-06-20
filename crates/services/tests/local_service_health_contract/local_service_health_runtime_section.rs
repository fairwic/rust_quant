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
