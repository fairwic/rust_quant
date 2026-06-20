#[test]
fn full_product_health_operator_playbook_summary_drift_contract_spans_examples_and_validator() {
    let schema_path = full_product_artifact_schema_json_path();
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");
    let admin_ingest_path = full_product_admin_ingest_fixture_path();

    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let summary: Value = serde_json::from_str(&summary_body)
        .unwrap_or_else(|error| panic!("invalid summary example: {error}\n{summary_body}"));
    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    let admin_ingest_body = fs::read_to_string(&admin_ingest_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", admin_ingest_path.display(), error)
    });
    let admin_ingest: Value = serde_json::from_str(&admin_ingest_body).unwrap_or_else(|error| {
        panic!("invalid admin ingest example: {error}\n{admin_ingest_body}")
    });

    let playbook_contract = &schema["consumer_contracts"]["operator_playbook_summary"];
    assert_eq!(
        playbook_contract["compatibility_contract_version"], 1,
        "operator_playbook_summary consumer contract should be explicitly versioned"
    );
    let producer_required_paths = playbook_contract["producer_required_paths"]
        .as_array()
        .expect("operator playbook producer_required_paths should be an array");
    for required_path in [
        "summary.operator_playbook_summary",
        "summary.operator_playbook_summary.item_count",
        "summary.operator_playbook_summary.blocking_item_count",
        "summary.operator_playbook_summary.manual_review_item_count",
        "summary.operator_playbook_summary.observe_only_item_count",
        "summary.operator_playbook_summary.items",
        "markdown.## Operator Playbook Summary",
        "admin_ingest.summary.operator_playbook_summary",
    ] {
        assert!(
            producer_required_paths
                .iter()
                .any(|item| item.as_str() == Some(required_path)),
            "operator playbook drift contract should require {required_path}"
        );
    }

    assert_required_nested_fields(
        &schema,
        "summary",
        "operator_playbook_summary",
        &summary["operator_playbook_summary"],
    );
    assert_required_nested_fields(
        &schema,
        "summary",
        "operator_playbook_summary",
        &admin_ingest["summary"]["operator_playbook_summary"],
    );
    assert_eq!(
        summary["operator_playbook_summary"],
        admin_ingest["summary"]["operator_playbook_summary"],
        "Admin ingest fixture should carry the same operator_playbook_summary contract as the summary example"
    );
    assert!(
        markdown_body.contains("## Operator Playbook Summary"),
        "Markdown example should keep the operator playbook section marker"
    );

    let artifact_dir = temp_artifact_dir("full-product-health-validator-playbook-drift");
    let temp_full_report_path = artifact_dir.join("full-product-health.json");
    let temp_summary_path = artifact_dir.join("full-product-health-summary.json");
    let temp_markdown_path = artifact_dir.join("full-product-health.md");
    fs::copy(&full_report_path, &temp_full_report_path).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {}: {}",
            full_report_path.display(),
            temp_full_report_path.display(),
            error
        )
    });
    fs::copy(&markdown_path, &temp_markdown_path).unwrap_or_else(|error| {
        panic!(
            "failed to copy {} to {}: {}",
            markdown_path.display(),
            temp_markdown_path.display(),
            error
        )
    });

    let mut summary_without_items = summary.clone();
    summary_without_items["operator_playbook_summary"]
        .as_object_mut()
        .expect("operator_playbook_summary should be an object")
        .remove("items");
    fs::write(
        &temp_summary_path,
        serde_json::to_string_pretty(&summary_without_items).expect("summary json"),
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", temp_summary_path.display(), error));

    let output = Command::new(full_product_artifact_validator_path())
        .env("FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT", "json")
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH",
            &temp_full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH",
            &temp_summary_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH",
            &temp_markdown_path,
        )
        .env("FULL_PRODUCT_HEALTH_VALIDATION_STRICT", "true")
        .output()
        .expect("full product artifact validator should run");

    assert!(
        !output.status.success(),
        "strict validator should reject summary artifacts missing operator_playbook_summary nested fields"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|finding| {
            finding["code"] == "MISSING_REQUIRED_FIELD"
                && finding["artifact"] == "summary"
                && finding["field"] == "operator_playbook_summary.items"
        }),
        "validator should identify the missing operator_playbook_summary.items field: {stdout}"
    );
}
