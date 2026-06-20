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
fn full_product_health_artifact_validator_rejects_unsafe_playbook_metadata() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-playbook-metadata");
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
  "alerts": [
    {
      "severity": "P1",
      "code": "WEB_RETRY_BACKLOG",
      "section": "web_task_order_health",
      "message": "retry backlog needs review"
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
    "section_count": 1,
    "blocking_section_count": 0,
    "warning_section_count": 1,
    "top_alert_count": 1,
    "required_operator_action_count": 1,
    "alert_taxonomy_count": 1,
    "correlation_id_count": 1,
    "read_only_input_count": 4
  },
  "section_statuses": {"web_task_order_health": "warn"},
  "checklist": [],
  "top_alerts": [
    {
      "severity": "P1",
      "code": "WEB_RETRY_BACKLOG",
      "section": "web_task_order_health",
      "message": "retry backlog needs review",
      "owner": "web_execution",
      "default_next_action": "review_retry_backlog",
      "admin_link_target": "https://runbooks.invalid/retry-backlog"
    }
  ],
  "required_operator_actions": [
    {
      "severity": "P1",
      "code": "WEB_RETRY_BACKLOG",
      "section": "web_task_order_health",
      "message": "retry backlog needs review",
      "action": "manual_review_before_release",
      "owner": "web_execution",
      "default_next_action": "review_retry_backlog",
      "admin_link_target": "/Users/mac2/runbooks/retry-backlog"
    }
  ],
  "alert_taxonomy": [
    {
      "severity": "P1",
      "code": "WEB_RETRY_BACKLOG",
      "section": "web_task_order_health",
      "operator_action": "manual_review_before_release",
      "owner": "web_execution",
      "default_next_action": "review_retry_backlog",
      "admin_link_target": "file:///tmp/retry-backlog",
      "correlation_keys": ["execution_task_id"]
    }
  ],
  "operator_playbook_summary": {
    "item_count": 1,
    "blocking_item_count": 0,
    "manual_review_item_count": 1,
    "items": [
      {
        "source": "alert",
        "severity": "P1",
        "code": "WEB_RETRY_BACKLOG",
        "section": "web_task_order_health",
        "operator_action": "manual_review_before_release",
        "owner": "web_execution",
        "default_next_action": "review_retry_backlog",
        "admin_link_target": "https://runbooks.invalid/retry-backlog"
      }
    ]
  },
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
        "strict validator should reject URL and local-path playbook metadata"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    for expected_marker in ["URL_REFERENCE", "LOCAL_PATH_REFERENCE"] {
        assert!(
            findings
                .iter()
                .any(|finding| finding["marker_code"] == expected_marker),
            "validator should report unsafe playbook metadata marker {expected_marker}: {stdout}"
        );
    }

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in ["https://", "file://", "/users/", "/tmp/"] {
        assert!(
            !lowered.contains(sensitive),
            "validation output must not echo unsafe playbook marker {sensitive}: {stdout}"
        );
    }
}
