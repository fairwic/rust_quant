#[test]
fn full_product_health_docs_expose_operator_safe_payment_fixture_commands() {
    let handoff_path = full_product_admin_ci_handoff_path();
    let runbook_path = runbook_path();
    let schema_doc_path = full_product_artifact_schema_doc_path();
    let handoff = fs::read_to_string(&handoff_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", handoff_path.display(), error));
    let runbook = fs::read_to_string(&runbook_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", runbook_path.display(), error));
    let schema_doc = fs::read_to_string(&schema_doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_doc_path.display(), error));
    let combined = format!("{handoff}\n{runbook}\n{schema_doc}");
    for required in [
        "payment-entitlement-health-skipped.json",
        "payment-entitlement-health-query-failed.json",
        "payment-entitlement-health-real-count.json",
        "FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json",
        "FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json",
        "FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR=/tmp/full-product-health-payment-smoke",
        "FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH=/tmp/full-product-health-payment-smoke/full-product-health-publish-index.json",
        "./scripts/dev/smoke_full_product_health_payment_artifact_handoff.sh",
        "full-product-health.md",
        "full-product-health-validation.json",
        "full-product-health-publish-index.json",
        "publish_full_product_health_artifact_set.sh",
        "storageStatus",
        "operator_playbook_summary.items[]",
        "wallet_payment_exception_count",
        "FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true",
        "./scripts/dev/validate_full_product_health_artifacts.sh",
        "operator-safe",
        "不连接真实 DB",
        "不读取 `.env`",
        "不外呼交易所",
        "不 lease/report/mutate task",
    ] {
        assert!(
            combined.contains(required),
            "payment entitlement docs should expose operator-safe fixture command token {required}"
        );
    }
}
#[test]
fn full_product_health_payment_artifact_smoke_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_payment_artifact_smoke_path())
        .output()
        .expect("bash -n should be available");
    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn full_product_health_payment_artifact_smoke_is_explicit_file_only_and_no_env() {
    let script = read_full_product_payment_artifact_smoke_script();
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH=false"));
    assert!(script.contains("summarize_full_product_health.sh"));
    assert!(script.contains("render_full_product_health_markdown.sh"));
    assert!(script.contains("validate_full_product_health_artifacts.sh"));
    assert!(script.contains("publish_full_product_health_artifact_set.sh"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH"));
    assert!(
        script.contains("env -i"),
        "payment artifact smoke should call child scripts through an allowlisted environment"
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
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            script.contains(required),
            "payment artifact smoke must scan supplied JSON for sensitive marker {required}"
        );
    }
    for forbidden in [
        "source .env",
        "cat .env",
        "FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL",
        "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
        "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
        "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
        "psql ",
        "curl ",
        "wget ",
        "INSERT INTO ",
        "UPDATE ",
        "DELETE FROM ",
    ] {
        assert!(
            !script.contains(forbidden),
            "payment artifact smoke must stay file-only and avoid {forbidden}"
        );
    }
}
#[test]
fn full_product_health_payment_artifact_smoke_generates_report_summary_and_validation() {
    let input_path =
        full_product_artifact_examples_dir().join("payment-entitlement-health-real-count.json");
    let output_dir = temp_artifact_dir("full-product-health-payment-artifact-smoke");
    let full_report_path = output_dir.join("full-product-health.json");
    let summary_path = output_dir.join("full-product-health-summary.json");
    let markdown_path = output_dir.join("full-product-health.md");
    let validation_path = output_dir.join("full-product-health-validation.json");
    let publish_index_path = output_dir.join("full-product-health-publish-index.json");
    let output = Command::new(full_product_payment_artifact_smoke_path())
        .env("FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH", &input_path)
        .env("FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR", &output_dir)
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("DATABASE_URL", "postgres://user:secret@db/quant_core")
        .output()
        .expect("payment artifact smoke should run");
    assert!(
        output.status.success(),
        "payment artifact smoke should generate all artifacts:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("smoke manifest should be utf8");
    let manifest: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid smoke manifest json: {error}\n{stdout}"));
    assert_eq!(manifest["status"], "ok");
    assert_eq!(manifest["payment"]["wallet_payment_exception_count"], 2);
    assert_eq!(manifest["payment"]["payment_entitlement_blocker_count"], 1);
    assert_eq!(
        manifest["artifacts"]["full_report"],
        full_report_path.display().to_string()
    );
    assert_eq!(
        manifest["artifacts"]["summary"],
        summary_path.display().to_string()
    );
    assert_eq!(
        manifest["artifacts"]["validation"],
        validation_path.display().to_string()
    );
    assert_eq!(
        manifest["artifacts"]["markdown"],
        markdown_path.display().to_string()
    );
    assert_eq!(
        manifest["artifacts"]["publish_index"],
        publish_index_path.display().to_string()
    );
    assert_eq!(manifest["publish_index_status"], "current");
    let full_report_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    let validation_body = fs::read_to_string(&validation_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", validation_path.display(), error));
    let publish_index_body = fs::read_to_string(&publish_index_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", publish_index_path.display(), error)
    });
    let full_report: Value = serde_json::from_str(&full_report_body)
        .unwrap_or_else(|error| panic!("invalid full report json: {error}\n{full_report_body}"));
    let summary: Value = serde_json::from_str(&summary_body)
        .unwrap_or_else(|error| panic!("invalid summary json: {error}\n{summary_body}"));
    let validation: Value = serde_json::from_str(&validation_body)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{validation_body}"));
    let publish_index: Value = serde_json::from_str(&publish_index_body).unwrap_or_else(|error| {
        panic!("invalid publish index json: {error}\n{publish_index_body}")
    });
    assert_eq!(full_report["status"], "fail");
    assert_eq!(full_report["summary"]["wallet_payment_exception_count"], 2);
    assert_eq!(
        full_report["summary"]["payment_entitlement_blocker_count"],
        1
    );
    assert_eq!(
        full_report["sections"]["payment_entitlement_health"]["source"],
        "json_path"
    );
    assert_eq!(summary["status"], "fail");
    assert_eq!(summary["summary"]["wallet_payment_exception_count"], 2);
    assert_eq!(summary["summary"]["payment_entitlement_blocker_count"], 1);
    assert_eq!(validation["status"], "ok");
    assert_eq!(validation["summary"]["finding_count"], 0);
    assert_eq!(publish_index["storageStatus"], "current");
    assert_eq!(publish_index["validation"]["status"], "ok");
    assert_eq!(
        publish_index["summary"]["summary"]["wallet_payment_exception_count"],
        2
    );
    assert_eq!(
        publish_index["summary"]["summary"]["payment_entitlement_blocker_count"],
        1
    );
    assert!(
        summary["operator_playbook_summary"]["items"]
            .as_array()
            .expect("operator playbook items should be an array")
            .iter()
            .any(|item| item["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
                && item["operator_action"] == "block_release_until_resolved"
                && item["owner"] == "commerce_billing"),
        "summary should expose Admin handoff playbook item: {summary_body}"
    );
    assert!(
        publish_index["summary"]["operator_playbook_summary"]["items"]
            .as_array()
            .expect("publish index playbook items should be an array")
            .iter()
            .any(|item| item["code"] == "WALLET_PAYMENT_EXCEPTION"
                && item["metadata"]["wallet_payment_exception_count"] == 2
                && item["default_next_action"] == "review_wallet_payment_exceptions"),
        "publish index should preserve wallet exception playbook metadata: {publish_index_body}"
    );
    assert!(
        markdown_body.contains("## Operator Playbook Summary")
            && markdown_body.contains("WALLET_PAYMENT_EXCEPTION")
            && markdown_body.contains("PAYMENT_ENTITLEMENT_BLOCKED"),
        "Markdown should expose operator playbook consumption path: {markdown_body}"
    );
    let combined_artifacts =
        format!("{full_report_body}\n{summary_body}\n{markdown_body}\n{validation_body}\n{publish_index_body}")
            .to_ascii_lowercase();
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
        "minimax-secret",
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
            !combined_artifacts.contains(sensitive),
            "payment artifact smoke output must not leak sensitive marker {sensitive}: {combined_artifacts}"
        );
    }
}
#[test]
fn full_product_health_payment_artifact_smoke_rejects_missing_invalid_and_sensitive_json() {
    let missing_path =
        temp_artifact_dir("full-product-health-payment-smoke-missing").join("missing-payment.json");
    let invalid_path = temp_json_file("full-product-health-payment-smoke-invalid", "{");
    let sensitive_path = temp_json_file(
        "full-product-health-payment-smoke-sensitive",
        r#"{"status":"ok","source":"operator","api_key":"must-not-pass"}"#,
    );
    for (path, expected_stderr) in [
        (&missing_path, "input file is missing"),
        (&invalid_path, "input JSON is invalid"),
        (&sensitive_path, "input JSON contains a blocked marker"),
    ] {
        let output = Command::new(full_product_payment_artifact_smoke_path())
            .env("FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH", path)
            .output()
            .expect("payment artifact smoke should run");
        assert!(
            !output.status.success(),
            "payment artifact smoke should reject {}",
            path.display()
        );
        let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
        assert!(
            stderr.contains(expected_stderr),
            "stderr should contain {expected_stderr}, got {stderr}"
        );
        let lowered = stderr.to_ascii_lowercase();
        assert!(
            !lowered.contains("must-not-pass"),
            "stderr should not leak rejected JSON content: {stderr}"
        );
    }
}
