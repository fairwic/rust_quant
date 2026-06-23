#[test]
fn full_product_health_ci_wrapper_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_ci_wrapper_path())
        .output()
        .expect("bash -n should be available");
    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn full_product_health_ci_wrapper_is_safe_and_uses_explicit_artifacts() {
    let script = read_full_product_ci_wrapper_script();
    assert!(script.contains("build_full_product_health_inputs.sh"));
    assert!(script.contains("summarize_full_product_health.sh"));
    assert!(script.contains("render_full_product_health_markdown.sh"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_INPUT_PRODUCER_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH"));
    assert!(script.contains("env -i"));
    assert!(script.contains("linkusdt"));
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
    ] {
        assert!(
            script.contains(required),
            "CI wrapper must scan artifacts for sensitive marker {required}"
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
        "LINKUSDT",
        "LINK-USDT",
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
        "MINIMAX_API_KEY",
        "OPENAI_API_KEY",
    ] {
        assert!(
            !script.contains(forbidden),
            "CI wrapper must stay read-only and avoid {forbidden}"
        );
    }
}
fn full_product_health_ci_wrapper_writes_skipped_report_and_summary_without_urls() {
    let artifact_dir = temp_artifact_dir("full-product-health-ci-skipped");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let output = Command::new(full_product_ci_wrapper_path())
        .env("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR", &artifact_dir)
        .env("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH", &summary_path)
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product CI wrapper should run");
    assert!(
        output.status.success(),
        "missing urls should still produce skipped artifacts:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        full_report_path.is_file(),
        "CI wrapper should write full report artifact"
    );
    assert!(
        summary_path.is_file(),
        "CI wrapper should write summary artifact"
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let full_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let stdout_summary: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid stdout json: {error}\n{stdout}"));
    let full_payload: Value = serde_json::from_str(&full_body)
        .unwrap_or_else(|error| panic!("invalid full report json: {error}\n{full_body}"));
    let summary_payload: Value = serde_json::from_str(&summary_body)
        .unwrap_or_else(|error| panic!("invalid summary json: {error}\n{summary_body}"));
    assert_eq!(stdout_summary, summary_payload);
    assert_eq!(full_payload["status"], "ok");
    assert_eq!(full_payload["summary"]["p0_count"], 0);
    assert_eq!(full_payload["summary"]["p1_count"], 0);
    assert_eq!(full_payload["summary"]["read_only_input_count"], 4);
    assert_eq!(
        full_payload["sections"]["web_task_order_health"]["skipped"],
        true
    );
    assert_eq!(
        full_payload["sections"]["payment_entitlement_health"]["skipped"],
        true
    );
    assert_eq!(
        full_payload["sections"]["news_source_ai_health"]["skipped"],
        true
    );
    assert_eq!(full_payload["sections"]["admin_readiness"]["skipped"], true);
    assert_eq!(summary_payload["status"], "ok");
    assert_eq!(summary_payload["summary"]["overall_status"], "ok");
    assert_eq!(
        summary_payload["section_statuses"]["web_task_order_health"],
        "warn"
    );
    assert!(
        alerts(&full_payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "WEB_INPUT_SKIPPED"
                && alert["section"] == "web_task_order_health"),
        "web skipped section should remain visible in the full report: {full_body}"
    );
    assert!(
        alerts(&full_payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "PAYMENT_INPUT_SKIPPED"
                && alert["section"] == "payment_entitlement_health"),
        "payment skipped section should remain visible in the full report: {full_body}"
    );
    assert!(
        summary_payload["top_alerts"]
            .as_array()
            .expect("top_alerts should be an array")
            .iter()
            .any(|alert| alert["code"] == "WEB_INPUT_SKIPPED"
                || alert["code"] == "PAYMENT_INPUT_SKIPPED"),
        "summary should include skipped input context: {summary_body}"
    );
    let combined = format!("{stdout}\n{full_body}\n{summary_body}").to_ascii_lowercase();
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
        "admin-secret",
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
            !combined.contains(sensitive),
            "CI wrapper artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}
#[test]
fn full_product_health_ci_wrapper_writes_optional_markdown_artifact_without_urls() {
    let artifact_dir = temp_artifact_dir("full-product-health-ci-markdown-skipped");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");
    let output = Command::new(full_product_ci_wrapper_path())
        .env("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR", &artifact_dir)
        .env("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH", &summary_path)
        .env("FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH", &markdown_path)
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product CI wrapper should run");
    assert!(
        output.status.success(),
        "missing urls should still produce optional markdown artifact:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        full_report_path.is_file(),
        "CI wrapper should write full report artifact"
    );
    assert!(
        summary_path.is_file(),
        "CI wrapper should write summary artifact"
    );
    assert!(
        markdown_path.is_file(),
        "CI wrapper should write optional markdown artifact"
    );
    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    for expected in [
        "# Full Product Health",
        "**Status:** ok",
        "## Artifact Paths",
        "full-product-health.json",
        "full-product-health-summary.json",
        "full-product-health.md",
        "## Skipped Sections",
        "web_task_order_health",
        "WEB_INPUT_SKIPPED",
        "payment_entitlement_health",
        "PAYMENT_INPUT_SKIPPED",
        "news_source_ai_health",
        "NEWS_INPUT_SKIPPED",
        "admin_readiness",
        "ADMIN_INPUT_SKIPPED",
    ] {
        assert!(
            markdown_body.contains(expected),
            "markdown artifact should contain {expected}: {markdown_body}"
        );
    }
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let full_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let combined =
        format!("{stdout}\n{full_body}\n{summary_body}\n{markdown_body}").to_ascii_lowercase();
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
        "admin-secret",
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
            !combined.contains(sensitive),
            "CI wrapper artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}
#[test]
fn full_product_health_ci_wrapper_writes_optional_validation_artifact_without_urls() {
    let artifact_dir = temp_artifact_dir("full-product-health-ci-validation-skipped");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");
    let validation_path = artifact_dir.join("full-product-health-validation.json");
    let output = Command::new(full_product_ci_wrapper_path())
        .env("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR", &artifact_dir)
        .env("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH", &summary_path)
        .env("FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH", &markdown_path)
        .env("FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS", "true")
        .env("FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH", &validation_path)
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product CI wrapper should run");
    assert!(
        output.status.success(),
        "validation-enabled wrapper should produce safe artifacts:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        full_report_path.is_file(),
        "CI wrapper should write full report artifact"
    );
    assert!(
        summary_path.is_file(),
        "CI wrapper should write summary artifact"
    );
    assert!(
        markdown_path.is_file(),
        "CI wrapper should write optional markdown artifact"
    );
    assert!(
        validation_path.is_file(),
        "CI wrapper should write optional validation artifact"
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let validation_body = fs::read_to_string(&validation_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", validation_path.display(), error));
    let validation_payload: Value = serde_json::from_str(&validation_body)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{validation_body}"));
    assert_eq!(validation_payload["status"], "ok");
    assert_eq!(validation_payload["summary"]["artifact_count"], 3);
    assert_eq!(validation_payload["summary"]["sensitive_marker_count"], 0);
    assert_eq!(validation_payload["artifacts"]["markdown"]["exists"], true);
    let full_body = fs::read_to_string(&full_report_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", full_report_path.display(), error));
    let summary_body = fs::read_to_string(&summary_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", summary_path.display(), error));
    let markdown_body = fs::read_to_string(&markdown_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", markdown_path.display(), error));
    let combined =
        format!("{stdout}\n{full_body}\n{summary_body}\n{markdown_body}\n{validation_body}")
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
        "admin-secret",
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
            !combined.contains(sensitive),
            "validation-enabled CI artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}
#[test]
fn full_product_health_ci_wrapper_exits_from_overall_status_unless_disabled() {
    let tool_dir = fake_full_product_input_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let blocking_artifact_dir = temp_artifact_dir("full-product-health-ci-blocking");
    let blocking_full_report_path = blocking_artifact_dir.join("full-product-health.json");
    let blocking_summary_path = blocking_artifact_dir.join("full-product-health-summary.json");
    let blocking_output = Command::new(full_product_ci_wrapper_path())
        .env("PATH", &path)
        .env(
            "FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR",
            &blocking_artifact_dir,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH",
            &blocking_full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH",
            &blocking_summary_path,
        )
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .output()
        .expect("full product CI wrapper should run");
    assert!(
        !blocking_output.status.success(),
        "fail overall status should make the CI wrapper exit non-zero by default"
    );
    assert!(
        blocking_full_report_path.is_file(),
        "blocking run should still write full report artifact"
    );
    assert!(
        blocking_summary_path.is_file(),
        "blocking run should still write summary artifact"
    );
    let blocking_summary_body =
        fs::read_to_string(&blocking_summary_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {}",
                blocking_summary_path.display(),
                error
            )
        });
    let blocking_summary: Value = serde_json::from_str(&blocking_summary_body)
        .unwrap_or_else(|error| panic!("invalid summary json: {error}\n{blocking_summary_body}"));
    assert_eq!(blocking_summary["status"], "fail");
    assert_eq!(blocking_summary["summary"]["overall_status"], "fail");
    let report_only_artifact_dir = temp_artifact_dir("full-product-health-ci-report-only");
    let report_only_full_report_path = report_only_artifact_dir.join("full-product-health.json");
    let report_only_summary_path =
        report_only_artifact_dir.join("full-product-health-summary.json");
    let report_only_output = Command::new(full_product_ci_wrapper_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR",
            &report_only_artifact_dir,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH",
            &report_only_full_report_path,
        )
        .env(
            "FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH",
            &report_only_summary_path,
        )
        .env("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH", "false")
        .env("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS", "never")
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .output()
        .expect("full product CI wrapper should run");
    assert!(
        report_only_output.status.success(),
        "FAIL_ON_STATUS=never should keep the CI wrapper report-only:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&report_only_output.stdout),
        String::from_utf8_lossy(&report_only_output.stderr)
    );
    let report_only_summary_body =
        fs::read_to_string(&report_only_summary_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {}",
                report_only_summary_path.display(),
                error
            )
        });
    let report_only_summary: Value = serde_json::from_str(&report_only_summary_body)
        .unwrap_or_else(|error| {
            panic!("invalid summary json: {error}\n{report_only_summary_body}")
        });
    assert_eq!(report_only_summary["status"], "fail");
    assert_eq!(report_only_summary["summary"]["overall_status"], "fail");
}
#[test]
fn full_product_health_input_runner_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_input_runner_path())
        .output()
        .expect("bash -n should be available");
    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn full_product_health_input_runner_is_safe_and_uses_only_read_only_producers() {
    let script = read_full_product_input_runner_script();
    assert!(script.contains("build_full_product_health_web_input.sh"));
    assert!(script.contains("build_full_product_health_news_input.sh"));
    assert!(script.contains("build_full_product_health_admin_input.sh"));
    assert!(script.contains("build_full_product_health_payment_input.sh"));
    assert!(script.contains("check_full_product_health.sh"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_JSON_PATH"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH"));
    assert!(script.contains("WEB_INPUT_SKIPPED"));
    assert!(script.contains("PAYMENT_INPUT_SKIPPED"));
    assert!(script.contains("NEWS_INPUT_SKIPPED"));
    assert!(script.contains("ADMIN_INPUT_SKIPPED"));
    assert!(script.contains("mktemp -d"));
    assert!(script.contains("trap "));
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
    ] {
        assert!(
            script.contains(required),
            "input runner must scan generated inputs for sensitive marker {required}"
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
        "LINKUSDT",
        "LINK-USDT",
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
    ] {
        assert!(
            !script.contains(forbidden),
            "input runner must stay read-only and avoid {forbidden}"
        );
    }
}
#[test]
fn full_product_health_input_runner_outputs_skipped_sections_without_urls() {
    let output = Command::new(full_product_input_runner_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product input runner should run");
    assert!(
        output.status.success(),
        "missing urls should still produce merged json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["summary"]["p0_count"], 0);
    assert_eq!(payload["summary"]["p1_count"], 0);
    assert_eq!(payload["summary"]["read_only_input_count"], 4);
    assert_eq!(
        payload["sections"]["web_task_order_health"]["skipped"],
        true
    );
    assert_eq!(
        payload["sections"]["payment_entitlement_health"]["skipped"],
        true
    );
    assert_eq!(
        payload["sections"]["news_source_ai_health"]["skipped"],
        true
    );
    assert_eq!(payload["sections"]["admin_readiness"]["skipped"], true);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "WEB_INPUT_SKIPPED"
                && alert["section"] == "web_task_order_health"),
        "web skipped section should be represented as an INFO alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "PAYMENT_INPUT_SKIPPED"
                && alert["section"] == "payment_entitlement_health"),
        "payment skipped section should be represented as an INFO alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "NEWS_INPUT_SKIPPED"
                && alert["section"] == "news_source_ai_health"),
        "news skipped section should be represented as an INFO alert: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "ADMIN_INPUT_SKIPPED"
                && alert["section"] == "admin_readiness"),
        "admin skipped section should be represented as an INFO alert: {stdout}"
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
        "minimax-secret",
        "admin-secret",
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
            "input runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
fn full_product_health_input_runner_calls_producers_for_explicit_read_only_urls() {
    let tool_dir = fake_full_product_input_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(full_product_input_runner_path())
        .env("PATH", path)
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .output()
        .expect("full product input runner should run");
    assert!(
        output.status.success(),
        "explicit urls should produce merged json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));
    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["summary"]["p0_count"], 3);
    assert_eq!(payload["summary"]["p1_count"], 2);
    assert_eq!(payload["summary"]["web_open_task_count"], 2);
    assert_eq!(payload["summary"]["news_degraded_source_count"], 2);
    assert_eq!(payload["summary"]["wallet_payment_exception_count"], 2);
    assert_eq!(payload["summary"]["payment_entitlement_blocker_count"], 1);
    assert_eq!(
        payload["sections"]["web_task_order_health"]["source"],
        "json_path"
    );
    assert_eq!(
        payload["sections"]["payment_entitlement_health"]["source"],
        "json_path"
    );
    assert_eq!(
        payload["sections"]["payment_entitlement_health"]["wallet_payment_exception_count"],
        2
    );
    assert_eq!(
        payload["sections"]["web_task_order_health"]["missing_order_result_count"],
        1
    );
    assert_eq!(
        payload["sections"]["news_source_ai_health"]["recent_ai_analysis_count"],
        5
    );
    assert_eq!(
        payload["sections"]["admin_readiness"]["missing_required_action_count"],
        1
    );
    assert_eq!(payload["correlation"]["signal_inbox_id"], 3801);
    assert_eq!(payload["correlation"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["correlation"]["analysis_result_id"], 9001);
    assert_eq!(
        payload["correlation"]["admin_operation_log_id"],
        "admin-op-9002"
    );
    assert_eq!(payload["correlation"]["payment_exception_id"], 2001);
    assert_eq!(payload["correlation"]["entitlement_check_id"], 1001);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "WEB_ORDER_RESULT_MISSING"
                && alert["section"] == "web_task_order_health"),
        "web producer alert should be merged: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_SOURCE_DEGRADED"
                && alert["section"] == "news_source_ai_health"),
        "news producer alert should be merged: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
                && alert["section"] == "payment_entitlement_health"),
        "payment producer alert should be merged: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "ADMIN_LIVE_READINESS_BLOCKED"
                && alert["section"] == "admin_readiness"),
        "admin producer alert should be merged: {stdout}"
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
        "minimax-secret",
        "admin-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "api_key_cipher",
        "api_secret_cipher",
        "passphrase_cipher",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "input runner output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
