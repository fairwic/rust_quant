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
