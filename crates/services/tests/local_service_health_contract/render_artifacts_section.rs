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
fn full_product_health_summary_script_is_read_only_and_redacts_sensitive_markers() {
    let script = read_full_product_summary_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT"));
    assert!(script.contains("checklist"));
    assert!(script.contains("top_alerts"));
    assert!(script.contains("required_operator_actions"));
    assert!(script.contains("alert_taxonomy"));
    assert!(script.contains("operator_playbook_summary"));
    assert!(script.contains("alert_code_metadata"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_PATH"));
    assert!(script.contains("URL_REFERENCE"));
    assert!(script.contains("LOCAL_PATH_REFERENCE"));
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
        "https://",
        "file://",
        "/Users/",
        "/tmp/",
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
      "message": "completed execution task missing order result",
      "execution_task_id": 5202,
      "order_result_id": 244,
      "source_signal_type": "news_event",
      "protection_status": "failed",
      "blocker_code": "protective_order_failed"
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
    assert_eq!(top_alerts[0]["owner"], "web_execution");
    assert_eq!(
        top_alerts[0]["default_next_action"],
        "reconcile_missing_order_result"
    );
    assert_eq!(
        top_alerts[0]["admin_link_target"],
        "admin.full_product_health.web_task_order_health"
    );
    assert_eq!(top_alerts[0]["execution_task_id"], 5202);
    assert_eq!(top_alerts[0]["order_result_id"], 244);
    assert_eq!(top_alerts[0]["source_signal_type"], "news_event");
    assert_eq!(top_alerts[0]["protection_status"], "failed");
    assert_eq!(top_alerts[0]["blocker_code"], "protective_order_failed");
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
                && item["action"] == "block_release_until_resolved"
                && item["owner"] == "web_execution"
                && item["default_next_action"] == "reconcile_missing_order_result"
                && item["admin_link_target"] == "admin.full_product_health.web_task_order_health"),
        "P0 alert should produce a blocking operator action: {stdout}"
    );
    assert!(
        required_actions
            .iter()
            .any(|item| item["code"] == "NEWS_SOURCE_DEGRADED"
                && item["action"] == "manual_review_before_release"
                && item["owner"] == "news_ops"
                && item["default_next_action"] == "review_news_source_status"
                && item["admin_link_target"] == "admin.full_product_health.news_source_ai_health"),
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
                && item["owner"] == "web_execution"
                && item["default_next_action"] == "reconcile_missing_order_result"
                && item["admin_link_target"] == "admin.full_product_health.web_task_order_health"
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
    let playbook = payload["operator_playbook_summary"]
        .as_object()
        .expect("operator_playbook_summary should be an object");
    assert_eq!(playbook["item_count"], 4);
    assert_eq!(playbook["blocking_item_count"], 1);
    assert_eq!(playbook["manual_review_item_count"], 2);
    assert!(
        playbook["items"]
            .as_array()
            .expect("operator playbook items should be an array")
            .iter()
            .any(|item| item["code"] == "WEB_ORDER_RESULT_MISSING"
                && item["source"] == "alert"
                && item["execution_task_id"] == 5202
                && item["order_result_id"] == 244
                && item["source_signal_type"] == "news_event"
                && item["protection_status"] == "failed"
                && item["blocker_code"] == "protective_order_failed"
                && item["operator_action"] == "block_release_until_resolved"
                && item["owner"] == "web_execution"
                && item["default_next_action"] == "reconcile_missing_order_result"
                && item["admin_link_target"] == "admin.full_product_health.web_task_order_health"),
        "operator playbook should map P0 alert to metadata registry: {stdout}"
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
        "https://",
        "file://",
        "/users/",
        "/tmp/",
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
    "alert_taxonomy_count": 3,
    "correlation_id_count": 0,
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
  ],
  "operator_playbook_summary": {
    "item_count": 3,
    "blocking_item_count": 1,
    "manual_review_item_count": 1,
    "observe_only_item_count": 1,
    "items": [
      {
        "source": "alert",
        "severity": "P0",
        "code": "WEB_ORDER_RESULT_MISSING",
        "section": "web_task_order_health",
        "operator_action": "block_release_until_resolved",
        "owner": "web_execution",
        "default_next_action": "reconcile_missing_order_result",
        "admin_link_target": "admin.full_product_health.web_task_order_health"
      },
      {
        "source": "alert",
        "severity": "P1",
        "code": "ADMIN_READINESS_REVIEW_REQUIRED",
        "section": "admin_readiness",
        "operator_action": "manual_review_before_release",
        "owner": "admin_ops",
        "default_next_action": "complete_admin_manual_review",
        "admin_link_target": "admin.full_product_health.admin_readiness"
      },
      {
        "source": "alert",
        "severity": "INFO",
        "code": "NEWS_INPUT_SKIPPED",
        "section": "news_source_ai_health",
        "operator_action": "observe_only",
        "owner": "news_ops",
        "default_next_action": "provide_news_read_only_input",
        "admin_link_target": "admin.full_product_health.news_source_ai_health"
      }
    ]
  }
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
        "## Operator Playbook Summary",
        "blocking_item_count",
        "manual_review_item_count",
        "observe_only_item_count",
        "Owner",
        "Default Next Action",
        "Admin Link Target",
        "web_execution",
        "reconcile_missing_order_result",
        "admin.full_product_health.web_task_order_health",
        "## Checklist",
        "web_task_order_health",
        "news_source_ai_health",
        "## Artifact Paths",
        "full-product-health.json",
        "full-product-health-summary.json",
        "full-product-health.md",
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
