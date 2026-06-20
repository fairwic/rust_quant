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
        "consumer_contracts",
        "compatibility_contract_version",
        "producer_required_paths",
        "owner",
        "default_next_action",
        "admin_link_target",
        "operator_playbook_summary",
        "operator_playbook_summary.items[]",
        "blocking_item_count",
        "manual_review_item_count",
        "observe_only_item_count",
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
    assert_required_nested_fields(
        &schema,
        "summary",
        "operator_playbook_summary",
        &summary["operator_playbook_summary"],
    );
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
    assert_json_array_enum(
        &schema,
        "severity_values",
        &summary["operator_playbook_summary"]["items"],
        "severity",
        "summary operator_playbook_summary.items",
    );
    assert_json_array_enum(
        &schema,
        "operator_action_values",
        &summary["operator_playbook_summary"]["items"],
        "operator_action",
        "summary operator_playbook_summary.items",
    );
    assert_alert_taxonomy_metadata_matches_registry(
        &schema,
        &summary["operator_playbook_summary"]["items"],
        "summary operator_playbook_summary.items",
    );
    assert_eq!(summary["operator_playbook_summary"]["item_count"], 2);
    assert_eq!(
        summary["operator_playbook_summary"]["blocking_item_count"],
        0
    );
    assert_eq!(
        summary["operator_playbook_summary"]["manual_review_item_count"],
        1
    );
    assert_eq!(
        summary["operator_playbook_summary"]["observe_only_item_count"],
        1
    );
    assert!(
        summary["operator_playbook_summary"]["items"]
            .as_array()
            .expect("operator playbook items should be an array")
            .iter()
            .any(|item| item["code"] == "NEWS_SOURCE_DEGRADED"
                && item["owner"] == "news_ops"
                && item["default_next_action"] == "review_news_source_status"
                && item["admin_link_target"] == "admin.full_product_health.news_source_ai_health"),
        "summary example should expose registry-backed playbook items: {summary_body}"
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
