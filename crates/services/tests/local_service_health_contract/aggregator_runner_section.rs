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
