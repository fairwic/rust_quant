#[test]
fn local_service_health_script_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(script_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn local_service_health_script_is_read_only_and_avoids_live_exchange_state() {
    let script = read_script();

    assert!(
        script.contains("HEALTH_CHECK_BINANCE:=false"),
        "Binance checks must be opt-in and disabled by default"
    );
    assert!(
        script.contains("HEALTH_CHECK_OUTPUT:=human"),
        "human output must remain the default"
    );
    assert!(
        script.contains("HEALTH_CHECK_WORKER_STALE_SECS"),
        "worker checkpoint staleness must be configurable"
    );
    assert!(
        script.contains("HEALTH_CHECK_WORKER_MODE:=all"),
        "all-worker mode should remain the conservative default"
    );
    assert!(
        script.contains("HEALTH_CHECK_EXPECTED_WORKERS"),
        "expected online workers must be configurable"
    );
    assert!(
        !script.contains("LINKUSDT") && !script.contains("LINK-USDT"),
        "local health checks must not reference the real LINK position"
    );
    assert!(
        !script.contains("/fapi/v1/order")
            && !script.contains("/fapi/v1/positionSide/dual")
            && !script.contains("/fapi/v2/account")
            && !script.contains("/fapi/v2/positionRisk")
            && !script.contains("/fapi/v1/positionRisk")
            && !script.contains("/fapi/v1/leverage")
            && !script.contains("/fapi/v1/marginType"),
        "local health checks must not call signed/account/order/position Binance endpoints"
    );
    assert!(
        !script.contains("/api/commerce/internal/execution-tasks/lease")
            && !script.contains("/api/commerce/internal/execution-results")
            && !script.contains("/api/commerce/internal/order-results")
            && !script.contains("/risk-review"),
        "local health checks must not mutate Web execution task state"
    );
}
fn local_service_health_json_output_is_machine_readable_and_redacted() {
    let tool_dir = fake_tool_dir();
    let path = format!(
        "{}:{}",
        tool_dir.display(),
        env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(script_path())
        .env("PATH", path)
        .env("HEALTH_CHECK_OUTPUT", "json")
        .env("HEALTH_CHECK_BINANCE", "false")
        .env("HEALTH_CHECK_DATABASES", "true")
        .env("HEALTH_CHECK_EXECUTION_AUDIT", "false")
        .env("HEALTH_CHECK_WORKER_STALE_SECS", "60")
        .env(
            "QUANT_CORE_DATABASE_URL",
            "postgres://user:secret@db/quant_core",
        )
        .env("WEB_DATABASE_URL", "postgres://user:secret@db/quant_web")
        .env("NEWS_DATABASE_URL", "postgres://user:secret@db/quant_news")
        .env("EXECUTION_EVENT_SECRET", "execution-secret")
        .env("BINANCE_API_KEY", "binance-key")
        .env("BINANCE_API_SECRET", "binance-secret")
        .output()
        .expect("health script should run");

    assert!(
        output.status.success(),
        "json health check should exit successfully without strict mode:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("json output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid json: {error}\n{stdout}"));

    assert_eq!(payload["output"], "json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["database_checks"], "true");
    assert_eq!(payload["binance_public_check"], "false");
    assert_eq!(payload["execution_audit_check"], "false");
    assert_eq!(payload["worker_stale_secs"], "60");
    assert_eq!(payload["worker_mode"], "all");
    assert_eq!(payload["expected_workers"], "");
    assert_eq!(payload["summary"]["expected_worker_failures"], 0);
    assert_eq!(payload["summary"]["expected_worker_warnings"], 0);
    assert_eq!(payload["summary"]["ignored_worker_count"], 0);
    assert_eq!(payload["summary"]["ignored_stale_worker_count"], 1);
    assert!(
        payload["warnings"].as_u64().unwrap_or_default() == 0,
        "default all mode should not warn on historical workers: {stdout}"
    );
    assert!(
        payload["checks"]
            .as_array()
            .expect("checks should be an array")
            .iter()
            .any(|check| check["level"] == "INFO"
                && check["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("ignored_stale_worker_id=worker_stale")),
        "historical stale worker should be visible but ignored in json checks: {stdout}"
    );
    assert!(
        alerts(&payload)
            .iter()
            .any(|alert| alert["severity"] == "INFO"
                && alert["code"] == "IGNORED_STALE_WORKER"
                && alert["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("ignored_stale_worker_id=worker_stale")),
        "default all mode should surface historical stale worker as INFO alert: {stdout}"
    );
    for secret in [
        "postgres://user:secret@db",
        "execution-secret",
        "binance-key",
        "binance-secret",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !stdout.contains(secret),
            "json output must not leak sensitive value {secret}: {stdout}"
        );
    }
}

#[test]
fn local_service_health_runbook_documents_read_only_and_opt_in_checks() {
    let path = runbook_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    assert!(docs.contains("check_local_service_health.sh"));
    assert!(docs.contains("HEALTH_CHECK_OUTPUT=json"));
    assert!(docs.contains("HEALTH_CHECK_WORKER_STALE_SECS"));
    assert!(docs.contains("HEALTH_CHECK_WORKER_MODE=expected"));
    assert!(docs.contains("HEALTH_CHECK_EXPECTED_WORKERS"));
    assert!(docs.contains("CI / Preflight"));
    assert!(docs.contains("exit code"));
    assert!(docs.contains("summary.expected_worker_failures"));
    assert!(docs.contains("summary.ignored_worker_count"));
    assert!(docs.contains("HEALTH_CHECK_EXECUTION_AUDIT=true"));
    assert!(docs.contains("exchange_request_audit_logs"));
    assert!(docs.contains("stale_leased_workers"));
    assert!(docs.contains("alerts"));
    assert!(docs.contains("severity"));
    assert!(docs.contains("code"));
    assert!(docs.contains("HEALTH_CHECK_STRICT=true"));
    assert!(docs.contains("JSON Stability Contract"));
    assert!(docs.contains("P0"));
    assert!(docs.contains("P1"));
    assert!(docs.contains("阻止实盘"));
    assert!(docs.contains("阻止发布"));
    assert!(docs.contains("历史 smoke 噪声"));
    assert!(docs.contains("只读"));
    assert!(docs.contains("HEALTH_CHECK_BINANCE=false"));
    assert!(docs.contains("显式 opt-in"));
    assert!(docs.contains("不调用 Binance signed/account/order/position endpoint"));
    assert!(docs.contains("不触碰 LINKUSDT"));
}

#[test]
fn local_service_health_runbook_documents_cross_service_aggregator_contract() {
    let path = runbook_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    assert!(docs.contains("Cross-Service Read-Only Aggregator Contract"));
    assert!(docs.contains("check_full_product_health.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_SCHEMA_VERSION"));
    assert!(docs.contains("build_full_product_health_inputs.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_KEEP_INPUTS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH=false"));
    assert!(docs.contains("未提供的 section"));
    assert!(docs.contains("summarize_full_product_health.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT"));
    assert!(docs.contains("run_full_product_health_ci.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never"));
    assert!(docs.contains("validate_full_product_health_artifacts.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH"));
    assert!(docs.contains("checklist"));
    assert!(docs.contains("top_alerts"));
    assert!(docs.contains("required_operator_actions"));
    assert!(docs.contains("correlation_ids"));
    assert!(docs.contains("build_full_product_health_web_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS"));
    assert!(docs.contains("WEB_INPUT_SKIPPED"));
    assert!(docs.contains("WEB_INPUT_QUERY_FAILED"));
    assert!(docs.contains("build_full_product_health_news_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS"));
    assert!(docs.contains("build_full_product_health_payment_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS"));
    assert!(docs.contains("PAYMENT_INPUT_SKIPPED"));
    assert!(docs.contains("PAYMENT_INPUT_QUERY_FAILED"));
    assert!(docs.contains("NEWS_INPUT_SKIPPED"));
    assert!(docs.contains("NEWS_INPUT_QUERY_FAILED"));
    assert!(docs.contains("build_full_product_health_admin_input.sh"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL"));
    assert!(docs.contains("FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS"));
    assert!(docs.contains("ADMIN_INPUT_SKIPPED"));
    assert!(docs.contains("ADMIN_INPUT_QUERY_FAILED"));
    assert!(docs.contains("web_task_order_health"));
    assert!(docs.contains("news_source_ai_health"));
    assert!(docs.contains("quant_worker_checkpoint_audit"));
    assert!(docs.contains("admin_readiness"));
    assert!(docs.contains("WEB_EXECUTION_TASK_STALE"));
    assert!(docs.contains("WEB_ORDER_RESULT_MISSING"));
    assert!(docs.contains("NEWS_SOURCE_DEGRADED"));
    assert!(docs.contains("NEWS_AI_PROVIDER_UNAVAILABLE"));
    assert!(docs.contains("QUANT_EXPECTED_WORKER_STALE"));
    assert!(docs.contains("ADMIN_LIVE_READINESS_BLOCKED"));
    assert!(docs.contains("ADMIN_HIGH_RISK_OPERATION_FAILED"));
    assert!(docs.contains("ADMIN_ACTION_AUDIT_MISSING"));
    assert!(docs.contains("不写库"));
    assert!(docs.contains("不 lease task"));
    assert!(docs.contains("不 report result"));
    assert!(docs.contains("不读取或打印 `.env`"));
    assert!(docs.contains("不调用 Binance signed/account/order/position endpoint"));
    assert!(docs.contains("news_id"));
    assert!(docs.contains("analysis_result_id"));
    assert!(docs.contains("signal_inbox_id"));
    assert!(docs.contains("execution_task_id"));
    assert!(docs.contains("order_result_id"));
    assert!(docs.contains("trade_record_id"));
    assert!(docs.contains("request_id"));
}

#[test]
fn full_product_health_admin_ci_handoff_documents_command_matrix_and_boundaries() {
    let path = full_product_admin_ci_handoff_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Default CI Safe",
        "Read-Only DB Opt-In",
        "Never Run In Default CI",
        "Read-Only Operator Surfaces",
        "Command Matrix",
    ] {
        assert!(
            docs.contains(heading),
            "handoff guide should contain heading {heading}"
        );
    }

    for command in [
        "FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false",
        "FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md",
        "FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true",
        "FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH=/tmp/full-product-health-ci/full-product-health-validation.json",
        "FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH=/tmp/full_product_health_artifact_schema.candidate.json",
        "FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true",
        "FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=mysql://readonly@host/quant_web",
        "FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL=postgres://readonly@host/quant_web",
        "FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=postgres://readonly@host/quant_news",
        "FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=postgres://readonly@host/quant_admin",
        "./scripts/dev/build_full_product_health_payment_input.sh",
        "FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json",
        "paymentPublishIndex.readyToRender",
        "summary.checklist[].live_readiness",
        "./scripts/dev/smoke_full_product_health_payment_artifact_handoff.sh",
        "./scripts/dev/run_full_product_health_ci.sh",
        "./scripts/dev/validate_full_product_health_artifacts.sh",
        "./scripts/dev/render_full_product_health_markdown.sh",
        "./scripts/dev/smoke_publish_full_product_health_admin_ingest_contract.sh",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json",
        "./scripts/dev/run_binance_live_eth_micro_order_smoke.sh",
    ] {
        assert!(
            docs.contains(command),
            "handoff guide should document command or env {command}"
        );
    }

    for boundary in [
        "no-env",
        "no-service",
        "no-exchange",
        "不读取 `.env`",
        "不访问本地服务",
        "不外呼交易所",
        "不下单",
        "不 lease task",
        "不 report result",
        "不 mutate task",
        "不触碰 `LINKUSDT`",
        "read-only operator surface",
        "artifact drift/unknown",
        "must not automatically",
        "只读 DB URL",
        "默认 CI",
        "显式 opt-in",
        "绝不能在默认 CI 调用",
        "does not read `.env`",
        "does not scan directories",
    ] {
        assert!(
            docs.contains(boundary),
            "handoff guide should document boundary {boundary}"
        );
    }
}

#[test]
fn full_product_health_admin_frontend_consumption_contract_documents_ui_ready_mapping_and_safety() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Admin / Frontend Consumption Contract",
        "Primary Artifacts",
        "Stable Fields",
        "Status Mapping",
        "Do Not Interpret As Ready",
        "Redaction Requirements",
        "Refresh And CI Artifact Usage",
        "Frontend Display Rules",
    ] {
        assert!(
            docs.contains(heading),
            "Admin/frontend contract should contain heading {heading}"
        );
    }

    for field in [
        "summary.overall_status",
        "section_statuses",
        "checklist[].ready",
        "checklist[].action_required",
        "top_alerts[].severity",
        "top_alerts[].code",
        "required_operator_actions[].action",
        "alert_taxonomy[].operator_action",
        "alert_taxonomy[].correlation_keys[]",
        "checklist[].live_readiness",
        "checklist[].manual_review_required",
        "paymentPublishIndex.status",
        "paymentPublishIndex.readyToRender",
        "paymentPublishIndex.walletPaymentExceptionCount",
        "paymentPublishIndex.paymentEntitlementBlockerCount",
        "paymentPublishIndex.playbookItems[]",
        "correlation_ids[]",
        "validation.summary.sensitive_marker_count",
        "validation.findings[]",
    ] {
        assert!(
            docs.contains(field),
            "Admin/frontend contract should document stable field {field}"
        );
    }

    for mapping in [
        "`ok` -> green/pass",
        "`warn` -> amber/review",
        "`fail` -> red/blocking",
        "`P0` -> blocking",
        "`P1` -> manual review",
        "`INFO` -> context only",
        "`block_release_until_resolved`",
        "`manual_review_before_release`",
        "`observe_only`",
    ] {
        assert!(
            docs.contains(mapping),
            "Admin/frontend contract should document mapping {mapping}"
        );
    }

    for not_ready in [
        "`summary.overall_status != \"ok\"`",
        "`section_statuses.* == \"warn\"`",
        "`section_statuses.* == \"fail\"`",
        "`checklist[].ready == false`",
        "`checklist[].action_required == true`",
        "`top_alerts[].severity == \"P0\"`",
        "`required_operator_actions` is not empty",
        "`validation.status != \"ok\"`",
        "`validation.summary.sensitive_marker_count > 0`",
        "`*_INPUT_SKIPPED`",
        "`read_only_input_count == 0`",
        "`admin_readiness.live_readiness` is `blocked` or `review`",
        "`manual_review_required == true`",
    ] {
        assert!(
            docs.contains(not_ready),
            "Admin/frontend contract should list non-ready condition {not_ready}"
        );
    }

    for safety in [
        "must not read `.env`",
        "must not call local services",
        "must not call signed/account/order/position endpoints",
        "must not lease task",
        "must not report result",
        "must not mutate task",
        "must not place orders",
        "must not touch `LINKUSDT`",
        "must not trigger automatic recovery",
        "must render `[redacted]`",
        "must not show raw database URLs",
        "must not show API keys",
        "must not show request or response payloads",
    ] {
        assert!(
            docs.contains(safety),
            "Admin/frontend contract should document safety rule {safety}"
        );
    }

    for artifact in [
        "full-product-health-summary.json",
        "full-product-health.json",
        "full-product-health-validation.json",
        "full-product-health.md",
        "run_full_product_health_ci.sh",
        "validate_full_product_health_artifacts.sh",
        "FAIL_ON_STATUS=never",
    ] {
        assert!(
            docs.contains(artifact),
            "Admin/frontend contract should document artifact or command {artifact}"
        );
    }
}

#[test]
fn full_product_health_latest_stored_artifact_api_contract_documents_safe_response_and_readiness() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for field in [
        "GET `/admin/quant/full-product-health/latest`",
        "`artifactSetId`",
        "`storedAt`",
        "`summary`",
        "`validation`",
        "`markdownUrl`",
        "`fullArtifactUrl`",
        "`ready`",
        "`stale`",
        "`paymentPublishIndex`",
        "`redaction`",
    ] {
        assert!(
            docs.contains(field),
            "stored artifact API contract should document response field {field}"
        );
    }

    for safety in [
        "handler must not shell out",
        "handler must not read `.env`",
        "handler must not run live probes",
        "handler must not call signed/account/order/position endpoints",
        "handler must not call lease/report/mutate task endpoints",
        "handler must not compute readiness from command exit code",
        "read from stored artifact storage only",
    ] {
        assert!(
            docs.contains(safety),
            "stored artifact API contract should document handler safety rule {safety}"
        );
    }
}

#[test]
fn full_product_health_stored_artifact_contract_documents_index_hashes_sla_and_retention() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Stored Artifact Storage Model",
        "Storage Index",
        "Freshness SLA",
        "Retention",
        "Operator Metadata",
    ] {
        assert!(
            docs.contains(heading),
            "stored artifact contract should document heading {heading}"
        );
    }

    for field in [
        "`artifactSetId`",
        "`storedAt`",
        "`sourceGeneratedAt`",
        "`schemaVersion`",
        "`summaryHash`",
        "`validationHash`",
        "`fullArtifactHash`",
        "`markdownHash`",
        "`storageStatus`",
        "`retentionClass`",
        "`artifactSlaSeconds`",
        "`staleReason`",
    ] {
        assert!(
            docs.contains(field),
            "stored artifact index should document field {field}"
        );
    }

    for operator_field in [
        "`operatorMetadata.generatedBy`",
        "`operatorMetadata.triggerType`",
        "`operatorMetadata.runId`",
        "`operatorMetadata.commitSha`",
        "`operatorMetadata.sourceRepo`",
    ] {
        assert!(
            docs.contains(operator_field),
            "stored artifact index should document operator metadata {operator_field}"
        );
    }

    for rule in [
        "latest valid artifact set",
        "at least 30 days",
        "rejected artifact sets",
        "hash mismatch marks the set rejected",
        "stale cannot be rendered as ready",
    ] {
        assert!(
            docs.contains(rule),
            "stored artifact storage model should document rule {rule}"
        );
    }
}

#[test]
fn full_product_health_stored_artifact_contract_documents_url_auth_and_redaction() {
    let path = full_product_admin_frontend_contract_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "URL Authorization",
        "Validation Finding Redaction",
        "Handler Acceptance Tests",
    ] {
        assert!(
            docs.contains(heading),
            "stored artifact contract should document heading {heading}"
        );
    }

    for url_rule in [
        "`markdownUrl` and `fullArtifactUrl` are authorized download URLs",
        "short-lived",
        "`artifact:health:read`",
        "`artifact:health:download`",
        "must not expose local filesystem paths",
        "must not proxy arbitrary URLs",
    ] {
        assert!(
            docs.contains(url_rule),
            "stored artifact URL authorization should document rule {url_rule}"
        );
    }

    for finding_rule in [
        "validation findings only return `code`, `artifact`, `field`, and `marker`",
        "must not return source text",
        "must not return raw payload",
        "must not return database URL",
        "must not return API key",
        "must not return secret",
        "must not return cipher",
        "must not return signed endpoint",
    ] {
        assert!(
            docs.contains(finding_rule),
            "stored artifact redaction should document finding rule {finding_rule}"
        );
    }

    for handler_rule in [
        "handler must not accept direct file paths from request parameters",
        "handler must not shell out",
        "handler must not read `.env`",
        "handler must not run live probes",
        "handler must not call signed/account/order/position endpoints",
        "handler must not call lease/report/mutate task endpoints",
        "handler must not compute readiness from command exit code",
        "handler must not mutate task state",
    ] {
        assert!(
            docs.contains(handler_rule),
            "stored artifact handler acceptance should document rule {handler_rule}"
        );
    }

    for forbidden in [
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            !docs.contains(forbidden),
            "stored artifact contract should avoid raw dangerous endpoint {forbidden}"
        );
    }
}

#[test]
fn admin_recovery_action_guardrails_document_initial_action_and_redaction_contract() {
    let path = admin_recovery_action_guardrails_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Admin Recovery Action Guardrails",
        "Non-Goals And Hard Boundaries",
        "Action Classes",
        "Global Action Requirements",
        "Redaction Contract",
        "Live Order Boundary",
        "Initial Recovery Actions",
        "RBAC Matrix",
        "Audit Event Contract",
        "Admin Workbench Starting Rules",
    ] {
        assert!(
            docs.contains(heading),
            "recovery guardrail doc should contain heading {heading}"
        );
    }

    for action_class in [
        "read_only",
        "guarded_recovery",
        "manual_approval",
        "disabled_until_live_order_closed",
    ] {
        assert!(
            docs.contains(action_class),
            "recovery guardrail doc should define action class {action_class}"
        );
    }

    for requirement in [
        "reason",
        "impact_objects",
        "audit_log",
        "idempotency_key",
        "rate_limit",
        "dry_run_preview",
        "rbac_role",
        "operator_confirmed_at",
    ] {
        assert!(
            docs.contains(requirement),
            "recovery guardrail doc should require {requirement}"
        );
    }

    for initial_action in [
        "notification_retry",
        "task_retry",
        "task_release",
        "pause_user",
        "pause_strategy",
        "pause_symbol",
        "manual_ai_reanalysis",
        "symbol_sync",
    ] {
        assert!(
            docs.contains(initial_action),
            "recovery guardrail doc should define initial action {initial_action}"
        );
    }

    for redacted in [
        ".env",
        "database_url",
        "api_key",
        "api_secret",
        "passphrase",
        "cipher",
        "request_payload",
        "response_payload",
        "raw_payload",
        "signed_endpoint",
        "account_endpoint",
        "order_endpoint",
        "position_endpoint",
        "LINKUSDT",
    ] {
        assert!(
            docs.contains(redacted),
            "recovery guardrail doc should list redaction or blocked marker {redacted}"
        );
    }

    for live_boundary in [
        "OPEN_LIVE_ORDER_PRESENT",
        "live_order_not_closed",
        "disable by default",
        "manual approval",
        "no signed/order/position endpoint",
        "no lease/report/mutate task endpoint",
        "no real order",
    ] {
        assert!(
            docs.contains(live_boundary),
            "recovery guardrail doc should define live boundary {live_boundary}"
        );
    }

    for forbidden in [
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionRisk",
        "/api/commerce/internal/execution-tasks/lease",
        "/api/commerce/internal/execution-results",
        "/api/commerce/internal/order-results",
    ] {
        assert!(
            !docs.contains(forbidden),
            "recovery guardrail doc should avoid raw dangerous endpoint {forbidden}"
        );
    }
}

#[test]
fn admin_recovery_workbench_disabled_preview_contract_documents_server_acceptance_conditions() {
    let path = admin_recovery_action_guardrails_path();
    let docs = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));

    for heading in [
        "Disabled And Read-Only Preview Server Contract",
        "Server Acceptance Conditions",
    ] {
        assert!(
            docs.contains(heading),
            "recovery workbench contract should contain heading {heading}"
        );
    }

    for requirement in [
        "GET preview endpoints are read-only",
        "`enabled: false`",
        "`disabled_reason_code`",
        "`preview_token`",
        "`preview_expires_at`",
        "`preview_hash`",
        "`redacted_preview`",
        "`idempotency_key`",
        "`audit_log`",
        "server-side RBAC",
        "no mutation before confirmation",
        "same `idempotency_key` must not execute twice",
        "dry-run preview must not contain raw payload",
    ] {
        assert!(
            docs.contains(requirement),
            "recovery workbench contract should document server requirement {requirement}"
        );
    }
}
