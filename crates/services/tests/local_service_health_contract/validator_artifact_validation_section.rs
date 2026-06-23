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
