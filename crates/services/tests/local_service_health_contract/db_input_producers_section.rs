#[test]
fn full_product_health_web_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(web_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn full_product_health_web_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_web_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(script.contains("news_signal_inbox"));
    assert!(script.contains("execution_tasks"));
    assert!(script.contains("execution_task_attempts"));
    assert!(script.contains("exchange_order_results"));
    assert!(script.contains("user_trade_records"));
    assert!(script.contains("combo_signal_delivery_logs"));
    assert!(script.contains("source_signal_type"));
    assert!(script.contains("WEB_ORDER_RESULT_MISSING"));
    assert!(script.contains("WEB_RETRY_BACKLOG"));
    assert!(script.contains("WEB_INPUT_SKIPPED"));
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
        "LINKUSDT",
        "LINK-USDT",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}
fn full_product_health_web_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(web_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("web input producer should run");

    assert!(
        output.status.success(),
        "missing database url should produce degraded json without failing:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["open_task_count"], 0);
    assert_eq!(payload["missing_order_result_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "WEB_INPUT_SKIPPED"
                && alert["section"] == "web_task_order_health"),
        "skipped producer output should explain the degraded input: {stdout}"
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
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped web input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_web_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_web_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(web_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL",
            "postgres://user:secret@db/quant_web",
        )
        .env("FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS", "3600")
        .env("FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS", "900")
        .env("FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS", "900")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("web input producer should run");

    assert!(
        output.status.success(),
        "producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["source"], "quant_web_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 3600);
    assert_eq!(payload["open_task_count"], 2);
    assert_eq!(payload["stale_task_count"], 1);
    assert_eq!(payload["missing_order_result_count"], 1);
    assert_eq!(payload["failed_task_count"], 1);
    assert_eq!(payload["retry_backlog_count"], 1);
    assert_eq!(payload["delivery_blocker_count"], 1);
    assert_eq!(payload["recent_order_result_count"], 3);
    assert_eq!(payload["recent_trade_record_count"], 2);
    assert_eq!(payload["sample"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["signal_inbox_id"], 3801);
    assert_eq!(payload["correlation"]["execution_task_id"], 5202);
    assert_eq!(payload["correlation"]["execution_attempt_id"], 6101);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "WEB_ORDER_RESULT_MISSING"
                && alert["section"] == "web_task_order_health"
                && alert["execution_task_id"] == 5202
                && alert["source_signal_type"] == "news_event"),
        "producer should surface missing Web order result with event-chain handoff context: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "WEB_RETRY_BACKLOG"
                && alert["section"] == "web_task_order_health"),
        "producer should surface retry backlog as P1: {stdout}"
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
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
fn full_product_health_news_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(news_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_news_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_news_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL"));
    assert!(script.contains("news_source_states"));
    assert!(script.contains("news_source_health"));
    assert!(script.contains("news_ai_analysis_results"));
    assert!(script.contains("news_analysis_jobs"));
    assert!(script.contains("news_provider_call_logs"));
    assert!(script.contains("news_items_jinse"));
    assert!(script.contains("NEWS_SOURCE_DEGRADED"));
    assert!(script.contains("NEWS_AI_PROVIDER_UNAVAILABLE"));
    assert!(script.contains("NEWS_INPUT_SKIPPED"));
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
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
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
        "LINKUSDT",
        "LINK-USDT",
        "MINIMAX_API_KEY",
        "OPENAI_API_KEY",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_news_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(news_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("news input producer should run");

    assert!(
        output.status.success(),
        "missing database url should produce degraded json without failing:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["degraded_source_count"], 0);
    assert_eq!(payload["recent_ai_analysis_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "NEWS_INPUT_SKIPPED"
                && alert["section"] == "news_source_ai_health"),
        "skipped producer output should explain the degraded input: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "api_secret",
        "secret",
        "minimax-secret",
        "binance-secret",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped news input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_news_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_news_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(news_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL",
            "postgres://user:secret@db/quant_news",
        )
        .env("FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS", "7200")
        .env("FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS", "1800")
        .env("FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS", "7200")
        .env("FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD", "3")
        .env("MINIMAX_TEST_KEY", "minimax-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("news input producer should run");

    assert!(
        output.status.success(),
        "producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "quant_news_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 7200);
    assert_eq!(payload["stale_analysis_secs"], 1800);
    assert_eq!(payload["failed_job_secs"], 7200);
    assert_eq!(payload["source_count"], 4);
    assert_eq!(payload["degraded_source_count"], 2);
    assert_eq!(payload["paused_source_count"], 1);
    assert_eq!(payload["retryable_source_count"], 1);
    assert_eq!(payload["recent_news_count"], 12);
    assert_eq!(payload["signal_candidate_count"], 3);
    assert_eq!(payload["recent_ai_analysis_count"], 5);
    assert_eq!(payload["actionable_analysis_count"], 2);
    assert_eq!(payload["failed_analysis_job_count"], 1);
    assert_eq!(payload["stuck_analysis_job_count"], 1);
    assert_eq!(payload["provider_failure_count"], 1);
    assert_eq!(payload["active_prompt_config_count"], 1);
    assert_eq!(payload["sample"]["source"], "theblockbeats");
    assert_eq!(payload["sample"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["sample"]["analysis_result_id"], 9001);
    assert_eq!(payload["correlation"]["news_id"], "jinse-20260507-001");
    assert_eq!(payload["correlation"]["analysis_result_id"], 9001);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_SOURCE_DEGRADED"
                && alert["section"] == "news_source_ai_health"),
        "producer should surface degraded news sources as P1: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "NEWS_AI_PROVIDER_UNAVAILABLE"
                && alert["section"] == "news_source_ai_health"),
        "producer should surface AI provider failures as P1: {stdout}"
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
        "minimax-secret",
        "binance-secret",
        "request_json",
        "response_json",
        "response_text",
        "raw_response",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
fn full_product_health_admin_input_producer_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(admin_input_producer_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_admin_input_producer_is_read_only_and_redacts_sensitive_markers() {
    let script = read_admin_input_producer_script();

    assert!(script.contains("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL"));
    assert!(script.contains("admin_operation_logs"));
    assert!(script.contains("risk_review_confirm"));
    assert!(script.contains("api_key_upsert"));
    assert!(script.contains("onchain_provider_control_upsert"));
    assert!(script.contains("strategy_config_upsert"));
    assert!(script.contains("backtest_run"));
    assert!(script.contains("exchange_symbol_sync"));
    assert!(script.contains("manual_ai_analysis"));
    assert!(script.contains("ADMIN_LIVE_READINESS_BLOCKED"));
    assert!(script.contains("ADMIN_HIGH_RISK_OPERATION_FAILED"));
    assert!(script.contains("ADMIN_ACTION_AUDIT_MISSING"));
    assert!(script.contains("ADMIN_INPUT_SKIPPED"));
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
        "apikey",
        "api_secret",
        "secret",
        "api_key_cipher",
        "api_secret_cipher",
        "passphrase_cipher",
        "request_payload",
        "response_payload",
        "raw_payload",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/fapi/v2/positionRisk",
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
        "LINKUSDT",
        "LINK-USDT",
        "BINANCE_API_KEY",
        "BINANCE_API_SECRET",
    ] {
        assert!(
            !script.contains(forbidden),
            "producer must stay read-only and avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_admin_input_producer_outputs_skipped_json_without_database_url() {
    let output = Command::new(admin_input_producer_path())
        .env_remove("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("admin input producer should run");

    assert!(
        output.status.success(),
        "missing database url should produce degraded json without failing:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "warn");
    assert_eq!(payload["source"], "skipped");
    assert_eq!(payload["read_only_input"], false);
    assert_eq!(payload["skipped"], true);
    assert_eq!(payload["high_risk_operation_count"], 0);
    assert_eq!(payload["missing_required_action_count"], 0);
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "ADMIN_INPUT_SKIPPED"
                && alert["section"] == "admin_readiness"),
        "skipped producer output should explain the degraded input: {stdout}"
    );
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "admin-secret",
        "binance-secret",
        "request_payload",
        "response_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !stdout.to_ascii_lowercase().contains(sensitive),
            "skipped admin input output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_input_producer_outputs_mergeable_json_from_read_only_db() {
    let tool_dir = fake_admin_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(admin_input_producer_path())
        .env("PATH", path)
        .env(
            "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL",
            "postgres://user:secret@db/quant_admin",
        )
        .env("FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS", "7200")
        .env("ADMIN_TEST_SECRET", "admin-secret")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("admin input producer should run");

    assert!(
        output.status.success(),
        "producer should emit parseable json for read-only db input:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["status"], "fail");
    assert_eq!(payload["source"], "quant_admin_readonly_db");
    assert_eq!(payload["database_engine"], "postgresql");
    assert_eq!(payload["read_only_input"], true);
    assert_eq!(payload["lookback_secs"], 7200);
    assert_eq!(payload["required_action_count"], 8);
    assert_eq!(payload["recent_operation_count"], 11);
    assert_eq!(payload["high_risk_operation_count"], 9);
    assert_eq!(payload["failed_operation_count"], 2);
    assert_eq!(payload["missing_required_action_count"], 1);
    assert_eq!(payload["readiness_blocker_count"], 1);
    assert_eq!(payload["manual_review_count"], 2);
    assert_eq!(payload["sample"]["admin_operation_log_id"], "admin-op-9002");
    assert_eq!(payload["sample"]["action"], "exchange_symbol_sync");
    assert_eq!(
        payload["correlation"]["admin_operation_log_id"],
        "admin-op-9002"
    );
    assert_eq!(
        payload["correlation"]["admin_action"],
        "exchange_symbol_sync"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "ADMIN_LIVE_READINESS_BLOCKED"
                && alert["section"] == "admin_readiness"),
        "producer should surface admin readiness blockers as P0: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "P1"
                && alert["code"] == "ADMIN_HIGH_RISK_OPERATION_FAILED"
                && alert["section"] == "admin_readiness"),
        "producer should surface failed admin operations as P1: {stdout}"
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
        "admin-secret",
        "binance-secret",
        "api_key_cipher",
        "api_secret_cipher",
        "passphrase_cipher",
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
            "producer output must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}
