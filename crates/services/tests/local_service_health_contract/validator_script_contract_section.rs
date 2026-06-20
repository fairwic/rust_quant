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
    assert!(script.contains("URL_REFERENCE"));
    assert!(script.contains("LOCAL_PATH_REFERENCE"));
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
