#[test]
fn full_product_health_payment_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(payment_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_payment_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(payment_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL")
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("payment input producer should run");

    assert!(
        output.status.success(),
        "missing database url should still produce skipped json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["contract_state"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["wallet_payment_exception_count"], 0);
    assert_eq!(payload["payment_entitlement_blocker_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "PAYMENT_INPUT_SKIPPED"
                && alert["section"] == "payment_entitlement_health"),
        "skipped payment producer output should explain the missing explicit db input: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "http://",
        "https://",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped payment input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_payment_input_producer_prefers_dedicated_payment_database_url() {
    let output = Command::new(payment_input_producer_path())
        .env(
            "FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL",
            "postgres://payment-user:secret@db/quant_web",
        )
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://web-user:secret@db/quant_web",
        )
        .output()
        .expect("payment input producer should run");

    assert!(
        output.status.success(),
        "unsupported explicit database url should degrade to mergeable json:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "quant_web_payment_readonly_db");
    assert_eq!(payload["contract_state"], "query_failed");
    assert_eq!(payload["query_failed"], true);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "PAYMENT_INPUT_QUERY_FAILED"
                && alert["section"] == "payment_entitlement_health"),
        "dedicated payment database url should take precedence when query execution degrades: {stdout}"
    );
}

#[test]
fn full_product_health_payment_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_payment_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL"));
    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(script.contains("payment_intents"));
    assert!(script.contains("payment_transactions"));
    assert!(script.contains("membership_orders"));
    assert!(script.contains("WALLET_PAYMENT_EXCEPTION"));
    assert!(script.contains("PAYMENT_ENTITLEMENT_BLOCKED"));
    assert!(script.contains("PAYMENT_INPUT_SKIPPED"));
    assert!(
        script.contains("SELECT"),
        "payment producer should rely on SELECT-only read models"
    );
    assert!(
        script.contains("python3") || script.contains("python "),
        "producer should use structured json generation instead of shell string parsing"
    );
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
        "http://",
        "https://",
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
            "producer must scan output for sensitive marker {required}"
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
        "raw_payload_json",
        "metadata_json",
        "external_tx_id",
        "payer_ref",
        "payee_ref",
        "failure_reason",
        "LINKUSDT",
        "LINK-USDT",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_payment_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_payment_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(payment_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL",
            "postgres://payment-user:secret@db/quant_web",
        )
        .env("FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS", "86400")
        .env(
            "FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS",
            "1800",
        )
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("payment input producer should run");

    assert!(
        output.status.success(),
        "payment producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["source"], "quant_web_payment_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["contract_state"], "real_count");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 86400);
    assert_eq!(payload["confirmation_timeout_secs"], 1800);
    assert_eq!(payload["wallet_payment_exception_count"], 2);
    assert_eq!(payload["payment_entitlement_blocker_count"], 1);
    assert_eq!(payload["sample"]["payment_intent_id"], 2001);
    assert_eq!(payload["correlation"]["payment_exception_id"], 2001);
    assert_eq!(payload["correlation"]["entitlement_check_id"], 1001);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "WALLET_PAYMENT_EXCEPTION"
                && alert["section"] == "payment_entitlement_health"
                && alert["metadata"]["wallet_payment_exception_count"] == 2),
        "producer should surface wallet payment exceptions with safe count metadata: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
                && alert["section"] == "payment_entitlement_health"
                && alert["metadata"]["payment_entitlement_blocker_count"] == 1),
        "producer should surface entitlement blockers as P0: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "binance-key",
        "binance-secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "raw_payload_json",
        "metadata_json",
        "external_tx_id",
        "payer_ref",
        "payee_ref",
        "failure_reason",
        "http://",
        "https://",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "payment input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
