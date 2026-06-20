#[test]
fn full_product_health_aggregator_accepts_payment_entitlement_input_and_builds_playbook_item() {
    let payment_input_path = temp_json_file(
        "full-product-health-payment-entitlement",
        r#"{
  "status": "fail",
  "source": "payment_entitlement_readonly_fixture",
  "read_only_input": true,
  "wallet_payment_exception_count": 2,
  "payment_entitlement_blocker_count": 1,
  "summary": {
    "wallet_payment_exception_count": 2,
    "payment_entitlement_blocker_count": 1
  },
  "sample": {
    "payment_exception_id": "pay-ex-1001",
    "entitlement_check_id": "ent-check-2001",
    "payment_state": "exception"
  },
  "alerts": [
    {
      "severity": "P0",
      "code": "PAYMENT_ENTITLEMENT_BLOCKED",
      "section": "payment_entitlement_health",
      "message": "payment entitlement check blocked auto-trade access",
      "metadata": {
        "wallet_payment_exception_count": 2,
        "payment_entitlement_blocker_count": 1,
        "sample_kind": "payment_entitlement"
      }
    }
  ],
  "correlation": {
    "payment_exception_id": "pay-ex-1001",
    "entitlement_check_id": "ent-check-2001"
  }
}"#,
    );

    let output = Command::new(aggregator_runner_path())
        .env("FULL_PRODUCT_HEALTH_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", "false")
        .env("FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH", &payment_input_path)
        .output()
        .expect("full product health aggregator should run");

    assert!(
        output.status.success(),
        "payment entitlement input should produce a full report:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let full_report_body = String::from_utf8(output.stdout).expect("json output should be utf8");
    let full_report: Value = serde_json::from_str(&full_report_body)
        .unwrap_or_else(|error| panic!("invalid full report json: {error}\n{full_report_body}"));
    assert_eq!(full_report["status"], "fail");
    assert_eq!(full_report["summary"]["wallet_payment_exception_count"], 2);
    assert_eq!(
        full_report["summary"]["payment_entitlement_blocker_count"],
        1
    );
    assert_eq!(
        full_report["sections"]["payment_entitlement_health"]["status"],
        "fail"
    );
    assert_eq!(
        full_report["sections"]["payment_entitlement_health"]["source"],
        "json_path"
    );
    assert_eq!(
        full_report["sections"]["payment_entitlement_health"]["read_only_input"],
        true
    );
    assert_eq!(
        full_report["correlation"]["payment_exception_id"],
        "pay-ex-1001"
    );
    assert_eq!(
        full_report["correlation"]["entitlement_check_id"],
        "ent-check-2001"
    );
    assert!(
        alerts(&full_report)
            .iter()
            .any(|alert| alert["severity"] == "P0"
                && alert["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
                && alert["section"] == "payment_entitlement_health"
                && alert["metadata"]["wallet_payment_exception_count"] == 2
                && alert["metadata"]["payment_entitlement_blocker_count"] == 1),
        "payment entitlement alert should preserve safe count metadata: {full_report_body}"
    );
    assert!(
        alert_taxonomy(&full_report)
            .iter()
            .any(|item| item["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
                && item["section"] == "payment_entitlement_health"
                && item["operator_action"] == "block_release_until_resolved"
                && item["correlation_keys"]
                    .as_array()
                    .expect("correlation keys should be an array")
                    .iter()
                    .any(|key| key == "payment_exception_id")),
        "full report taxonomy should expose payment entitlement correlation keys: {full_report_body}"
    );

    let full_report_path = temp_json_file(
        "full-product-health-payment-entitlement-report",
        &full_report_body,
    );
    let summary_output = Command::new(full_product_summary_path())
        .env("FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT", "json")
        .env("FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH", &full_report_path)
        .env("FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT", "10")
        .output()
        .expect("full product health summary should run");

    assert!(
        summary_output.status.success(),
        "payment entitlement full report should summarize:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&summary_output.stdout),
        String::from_utf8_lossy(&summary_output.stderr)
    );

    let summary_body = String::from_utf8(summary_output.stdout).expect("summary should be utf8");
    let summary: Value = serde_json::from_str(&summary_body)
        .unwrap_or_else(|error| panic!("invalid summary json: {error}\n{summary_body}"));
    assert_eq!(summary["status"], "fail");
    assert_eq!(
        summary["section_statuses"]["payment_entitlement_health"],
        "fail"
    );
    assert_eq!(summary["summary"]["wallet_payment_exception_count"], 2);
    assert_eq!(summary["summary"]["payment_entitlement_blocker_count"], 1);

    let playbook_items = summary["operator_playbook_summary"]["items"]
        .as_array()
        .expect("operator playbook items should be an array");
    assert!(
        playbook_items.iter().any(|item| item["source"] == "alert"
            && item["severity"] == "P0"
            && item["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
            && item["section"] == "payment_entitlement_health"
            && item["operator_action"] == "block_release_until_resolved"
            && item["owner"] == "commerce_billing"
            && item["default_next_action"] == "reconcile_payment_entitlement"
            && item["admin_link_target"] == "admin.full_product_health.payment_entitlement_health"
            && item["metadata"]["wallet_payment_exception_count"] == 2
            && item["metadata"]["payment_entitlement_blocker_count"] == 1),
        "operator playbook should expose payment entitlement owner/action/link/metadata: {summary_body}"
    );

    let combined = format!("{full_report_body}\n{summary_body}").to_ascii_lowercase();
    for sensitive in [
        ".env",
        "postgres://",
        "mysql://",
        "database_url",
        "api_key",
        "apikey",
        "api_secret",
        "secret",
        "request_payload",
        "response_payload",
        "raw_payload",
        "https://",
        "http://",
        "file://",
        "/users/",
        "/tmp/",
        "/fapi/v1/order",
        "/fapi/v2/account",
        "/fapi/v1/positionrisk",
        "/fapi/v2/positionrisk",
        "/api/commerce/internal/execution-tasks/lease",
        "linkusdt",
    ] {
        assert!(
            !combined.contains(sensitive),
            "payment entitlement artifacts must not leak sensitive marker {sensitive}: {combined}"
        );
    }
}
fn full_product_health_schema_registers_payment_entitlement_playbook_codes() {
    let schema_path = full_product_artifact_schema_json_path();
    let doc_path = full_product_artifact_schema_doc_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let doc = fs::read_to_string(&doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", doc_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));

    let payment_codes = schema["alert_code_values"]["payment_entitlement_health"]
        .as_array()
        .expect("payment_entitlement_health codes should be registered");
    for expected in [
        "PAYMENT_INPUT_QUERY_FAILED",
        "PAYMENT_INPUT_SKIPPED",
        "WALLET_PAYMENT_EXCEPTION",
        "PAYMENT_ENTITLEMENT_BLOCKED",
    ] {
        assert!(
            payment_codes.iter().any(|code| code == expected),
            "schema should register payment entitlement alert code {expected}"
        );
    }
    for (code, action) in [
        (
            "PAYMENT_INPUT_QUERY_FAILED",
            "inspect_payment_read_only_input",
        ),
        ("PAYMENT_INPUT_SKIPPED", "provide_payment_read_only_input"),
        (
            "WALLET_PAYMENT_EXCEPTION",
            "review_wallet_payment_exceptions",
        ),
        (
            "PAYMENT_ENTITLEMENT_BLOCKED",
            "reconcile_payment_entitlement",
        ),
    ] {
        let metadata = &schema["alert_code_metadata"]["payment_entitlement_health"][code];
        assert_eq!(metadata["owner"], "commerce_billing");
        assert_eq!(metadata["default_next_action"], action);
        assert_eq!(
            metadata["admin_link_target"],
            "admin.full_product_health.payment_entitlement_health"
        );
    }
    for required in [
        "payment_entitlement_health",
        "PAYMENT_INPUT_SKIPPED",
        "PAYMENT_INPUT_QUERY_FAILED",
        "WALLET_PAYMENT_EXCEPTION",
        "PAYMENT_ENTITLEMENT_BLOCKED",
        "wallet_payment_exception_count",
        "payment_entitlement_blocker_count",
    ] {
        assert!(
            doc.contains(required),
            "schema doc should describe payment entitlement contract token {required}"
        );
    }
}
fn full_product_health_schema_documents_payment_entitlement_three_state_contract() {
    let schema_path = full_product_artifact_schema_json_path();
    let schema_doc_path = full_product_artifact_schema_doc_path();
    let handoff_path = full_product_admin_ci_handoff_path();
    let runbook_path = runbook_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let schema_doc = fs::read_to_string(&schema_doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_doc_path.display(), error));
    let handoff = fs::read_to_string(&handoff_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", handoff_path.display(), error));
    let runbook = fs::read_to_string(&runbook_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", runbook_path.display(), error));

    let contract = &schema["consumer_contracts"]["payment_entitlement_health_states"];
    assert_eq!(contract["compatibility_contract_version"], 1);

    for required_path in [
        "sections.payment_entitlement_health.skipped",
        "sections.payment_entitlement_health.query_failed",
        "sections.payment_entitlement_health.wallet_payment_exception_count",
        "sections.payment_entitlement_health.payment_entitlement_blocker_count",
        "summary.wallet_payment_exception_count",
        "summary.payment_entitlement_blocker_count",
    ] {
        assert!(
            contract["producer_required_paths"]
                .as_array()
                .expect("payment state contract should list producer paths")
                .iter()
                .any(|path| path == required_path),
            "payment entitlement state contract should require path {required_path}"
        );
    }

    let states = contract["states"]
        .as_array()
        .expect("payment state contract should list stable states");
    for state_name in ["skipped", "query_failed", "real_count"] {
        let state = states
            .iter()
            .find(|state| state["name"] == state_name)
            .unwrap_or_else(|| panic!("missing payment health state {state_name}: {schema_body}"));
        assert!(state["status"].is_string());
        assert!(state["source"].is_string());
        assert!(
            state["read_only_input"].is_boolean(),
            "payment health state {state_name} should lock read_only_input semantics"
        );
        assert!(
            state["alert_code"].is_string(),
            "payment health state {state_name} should lock alert code semantics"
        );
    }

    for required_doc_token in [
        "payment_entitlement_health_states",
        "skipped",
        "query_failed",
        "real_count",
        "FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL",
        "FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN",
        "FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS",
        "只读 DB opt-in",
        "skipped / query_failed / real_count",
    ] {
        assert!(
            schema_doc.contains(required_doc_token)
                && handoff.contains(required_doc_token)
                && runbook.contains(required_doc_token),
            "schema doc, handoff, and runbook should all document token {required_doc_token}"
        );
    }
}

#[test]
fn full_product_health_schema_documents_admin_latest_readiness_envelope_contract() {
    let schema_path = full_product_artifact_schema_json_path();
    let schema_doc_path = full_product_artifact_schema_doc_path();
    let frontend_contract_path = full_product_admin_frontend_contract_path();
    let handoff_path = full_product_admin_ci_handoff_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let schema_doc = fs::read_to_string(&schema_doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_doc_path.display(), error));
    let frontend_contract = fs::read_to_string(&frontend_contract_path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {}",
            frontend_contract_path.display(),
            error
        )
    });
    let handoff = fs::read_to_string(&handoff_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", handoff_path.display(), error));

    let contract = &schema["consumer_contracts"]["admin_latest_artifact_readiness_envelope"];
    assert_eq!(contract["compatibility_contract_version"], 1);

    for required_path in [
        "latest.ready",
        "latest.stale",
        "latest.staleReason",
        "latest.summary.summary.overall_status",
        "latest.summary.section_statuses",
        "latest.summary.checklist[].ready",
        "latest.summary.checklist[].action_required",
        "latest.summary.checklist[].live_readiness",
        "latest.summary.checklist[].manual_review_required",
        "latest.summary.required_operator_actions",
        "latest.summary.read_only_input_count",
        "latest.validation.status",
        "latest.validation.summary.sensitive_marker_count",
        "latest.redaction.status",
        "latest.paymentPublishIndex.status",
        "latest.paymentPublishIndex.readyToRender",
    ] {
        assert!(
            contract["producer_required_paths"]
                .as_array()
                .expect("Admin readiness contract should list producer paths")
                .iter()
                .any(|path| path == required_path),
            "Admin readiness contract should require path {required_path}"
        );
    }

    for prohibited in [
        "shell_out_from_admin_request",
        "call_signed_exchange_endpoint",
        "lease_execution_task",
        "report_order_result",
        "mutate_execution_task",
        "place_order",
        "trigger_provider_recovery",
        "touch_protected_live_position",
    ] {
        assert!(
            contract["prohibited_automatic_actions"]
                .as_array()
                .expect("Admin readiness contract should list prohibited actions")
                .iter()
                .any(|action| action == prohibited),
            "Admin readiness contract should prohibit {prohibited}"
        );
    }

    assert!(
        schema_doc.contains("admin_latest_artifact_readiness_envelope"),
        "schema doc should name the Admin latest readiness envelope contract"
    );

    for required_doc_token in [
        "paymentPublishIndex",
        "readyToRender",
        "read-only operator surface",
        "artifact drift/unknown",
        "must not automatically trigger recovery",
        "checklist[].live_readiness",
        "validation.summary.sensitive_marker_count",
    ] {
        assert!(
            schema_doc.contains(required_doc_token)
                && frontend_contract.contains(required_doc_token)
                && handoff.contains(required_doc_token),
            "schema doc, frontend contract, and handoff should all document token {required_doc_token}"
        );
    }
}

#[test]
fn full_product_health_schema_documents_admin_wallet_payment_config_env_snapshot_only_contract() {
    let schema_path = full_product_artifact_schema_json_path();
    let schema_doc_path = full_product_artifact_schema_doc_path();
    let frontend_contract_path = full_product_admin_frontend_contract_path();
    let handoff_path = full_product_admin_ci_handoff_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let schema_doc = fs::read_to_string(&schema_doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_doc_path.display(), error));
    let frontend_contract = fs::read_to_string(&frontend_contract_path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {}",
            frontend_contract_path.display(),
            error
        )
    });
    let handoff = fs::read_to_string(&handoff_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", handoff_path.display(), error));

    let contract = &schema["consumer_contracts"]["admin_wallet_payment_config_env_snapshot"];
    assert_eq!(contract["compatibility_contract_version"], 1);

    for required_path in [
        "latest.walletPaymentConfig",
        "latest.walletPaymentConfig.source",
        "latest.walletPaymentConfig.source.kind",
        "latest.walletPaymentConfig.status",
        "latest.walletPaymentConfig.webWalletProviderReadiness",
    ] {
        assert!(
            contract["producer_required_paths"]
                .as_array()
                .expect("walletPaymentConfig contract should list producer paths")
                .iter()
                .any(|path| path == required_path),
            "walletPaymentConfig contract should require path {required_path}"
        );
    }

    for required_rule in [
        "walletPaymentConfig.source.kind must be one of admin_process_env_snapshot or admin_managed_config_draft",
        "walletPaymentConfig is an Admin-only config snapshot or draft",
        "walletPaymentConfig must not represent Web wallet provider readiness",
        "If Web wallet readiness is missing or inconsistent, render degraded/unknown and not ready",
    ] {
        assert!(
            contract["consumer_rules"]
                .as_array()
                .expect("walletPaymentConfig contract should list consumer rules")
                .iter()
                .any(|rule| rule == required_rule),
            "walletPaymentConfig contract should document rule {required_rule}"
        );
    }

    for required_doc_token in [
        "walletPaymentConfig",
        "admin_process_env_snapshot",
        "admin_managed_config_draft",
        "Admin-only config snapshot or draft",
        "must not represent Web wallet provider readiness",
        "degraded/unknown",
        "not ready",
    ] {
        assert!(
            schema_doc.contains(required_doc_token)
                && frontend_contract.contains(required_doc_token)
                && handoff.contains(required_doc_token),
            "schema doc, frontend contract, and handoff should all document token {required_doc_token}"
        );
    }
}

#[test]
fn full_product_health_schema_locks_admin_wallet_payment_config_not_ready_decision_table() {
    let schema_path = full_product_artifact_schema_json_path();
    let schema_doc_path = full_product_artifact_schema_doc_path();
    let frontend_contract_path = full_product_admin_frontend_contract_path();
    let handoff_path = full_product_admin_ci_handoff_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let schema_doc = fs::read_to_string(&schema_doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_doc_path.display(), error));
    let frontend_contract = fs::read_to_string(&frontend_contract_path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {}",
            frontend_contract_path.display(),
            error
        )
    });
    let handoff = fs::read_to_string(&handoff_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", handoff_path.display(), error));

    let contract = &schema["consumer_contracts"]["admin_wallet_payment_config_env_snapshot"];

    for allowed_source in ["admin_process_env_snapshot", "admin_managed_config_draft"] {
        assert!(
            contract["source_kind_allowed_values"]
                .as_array()
                .expect("walletPaymentConfig contract should list allowed source kinds")
                .iter()
                .any(|kind| kind == allowed_source),
            "walletPaymentConfig contract should allow source kind {allowed_source}"
        );
    }

    for not_ready_case in [
        "source_kind_missing_or_not_allowed_admin_config_source",
        "status_configured_without_web_wallet_provider_readiness",
        "status_draft",
        "status_degraded",
        "status_unknown",
        "web_wallet_provider_readiness_missing",
        "web_wallet_provider_readiness_unknown",
        "web_wallet_provider_readiness_incomplete",
        "web_wallet_provider_readiness_inconsistent_with_admin_snapshot",
    ] {
        assert!(
            contract["not_ready_cases"]
                .as_array()
                .expect("walletPaymentConfig contract should list not_ready_cases")
                .iter()
                .any(|case| case == not_ready_case),
            "walletPaymentConfig contract should mark {not_ready_case} as not ready"
        );
    }

    for required_doc_token in [
        "source_kind_missing_or_not_allowed_admin_config_source",
        "status_configured_without_web_wallet_provider_readiness",
        "status_draft",
        "status_degraded",
        "status_unknown",
        "web_wallet_provider_readiness_incomplete",
        "Web wallet readiness is incomplete",
        "cannot be ready",
    ] {
        assert!(
            schema_doc.contains(required_doc_token)
                && frontend_contract.contains(required_doc_token)
                && handoff.contains(required_doc_token),
            "schema doc, frontend contract, and handoff should document token {required_doc_token}"
        );
    }
}

#[test]
fn full_product_health_schema_does_not_expose_wallet_payment_config_operator_next_action() {
    let schema_path = full_product_artifact_schema_json_path();
    let schema_doc_path = full_product_artifact_schema_doc_path();
    let frontend_contract_path = full_product_admin_frontend_contract_path();
    let handoff_path = full_product_admin_ci_handoff_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let schema_doc = fs::read_to_string(&schema_doc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_doc_path.display(), error));
    let frontend_contract = fs::read_to_string(&frontend_contract_path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {}",
            frontend_contract_path.display(),
            error
        )
    });
    let handoff = fs::read_to_string(&handoff_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", handoff_path.display(), error));

    let contract = schema["consumer_contracts"]["admin_wallet_payment_config_env_snapshot"]
        .as_object()
        .expect("walletPaymentConfig consumer contract should be an object");

    assert!(
        !contract.contains_key("operatorNextAction"),
        "walletPaymentConfig should not expose a redundant operatorNextAction"
    );

    for removed_doc_token in [
        "operatorNextAction",
        "verify_web_wallet_provider_readiness",
        "Verify Web wallet provider readiness",
    ] {
        assert!(
            !schema_doc.contains(removed_doc_token)
                && !frontend_contract.contains(removed_doc_token)
                && !handoff.contains(removed_doc_token),
            "schema doc, frontend contract, and handoff should not document removed token {removed_doc_token}"
        );
    }
}

#[test]
fn full_product_health_payment_entitlement_tri_state_examples_are_schema_declared_and_redacted() {
    let schema_path = full_product_artifact_schema_json_path();
    let schema_body = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", schema_path.display(), error));
    let schema: Value = serde_json::from_str(&schema_body)
        .unwrap_or_else(|error| panic!("invalid schema json: {error}\n{schema_body}"));
    let contract = &schema["consumer_contracts"]["payment_entitlement_health_states"];
    let examples = contract["example_fixtures"]
        .as_array()
        .expect("payment entitlement state contract should declare example_fixtures");

    for state_name in ["skipped", "query_failed", "real_count"] {
        let example = examples
            .iter()
            .find(|item| item["state"] == state_name)
            .unwrap_or_else(|| panic!("missing payment entitlement fixture for {state_name}"));
        let relative_path = example["path"]
            .as_str()
            .unwrap_or_else(|| panic!("fixture path for {state_name} should be a string"));
        assert!(
            relative_path
                .starts_with("docs/dev/full_product_health_examples/payment-entitlement-health-"),
            "payment fixture path should stay under explicit examples dir: {relative_path}"
        );
        let fixture_path = repo_root().join(relative_path);
        let body = fs::read_to_string(&fixture_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", fixture_path.display(), error));
        let payload: Value = serde_json::from_str(&body)
            .unwrap_or_else(|error| panic!("invalid fixture json: {error}\n{body}"));

        assert_eq!(payload["contract_state"], state_name);
        assert_eq!(payload["section"], "payment_entitlement_health");
        assert!(payload["status"].as_str().is_some());
        assert!(payload["read_only_input"].is_boolean());
        assert!(payload["wallet_payment_exception_count"].is_u64());
        assert!(payload["payment_entitlement_blocker_count"].is_u64());

        match state_name {
            "skipped" => {
                assert_eq!(payload["status"], "warn");
                assert_eq!(payload["source"], "skipped");
                assert_eq!(payload["read_only_input"], false);
                assert_eq!(payload["skipped"], true);
                assert_ne!(payload["query_failed"], true);
                assert_eq!(payload["wallet_payment_exception_count"], 0);
                assert_eq!(payload["payment_entitlement_blocker_count"], 0);
                assert!(alerts(&payload).iter().any(|alert| {
                    alert["severity"] == "INFO" && alert["code"] == "PAYMENT_INPUT_SKIPPED"
                }));
            }
            "query_failed" => {
                assert_eq!(payload["status"], "warn");
                assert_eq!(payload["source"], "quant_web_payment_readonly_db");
                assert_eq!(payload["read_only_input"], true);
                assert_ne!(payload["skipped"], true);
                assert_eq!(payload["query_failed"], true);
                assert_eq!(payload["wallet_payment_exception_count"], 0);
                assert_eq!(payload["payment_entitlement_blocker_count"], 0);
                assert!(alerts(&payload).iter().any(|alert| {
                    alert["severity"] == "P1" && alert["code"] == "PAYMENT_INPUT_QUERY_FAILED"
                }));
            }
            "real_count" => {
                assert_eq!(payload["source"], "quant_web_payment_readonly_db");
                assert_eq!(payload["read_only_input"], true);
                assert_ne!(payload["skipped"], true);
                assert_ne!(payload["query_failed"], true);
                assert!(payload["wallet_payment_exception_count"].as_u64().unwrap() > 0);
                assert!(
                    payload["payment_entitlement_blocker_count"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
                assert!(alerts(&payload).iter().any(|alert| {
                    alert["severity"] == "P0" && alert["code"] == "PAYMENT_ENTITLEMENT_BLOCKED"
                }));
            }
            _ => unreachable!(),
        }

        let lowered = body.to_ascii_lowercase();
        for sensitive in [
            ".env",
            "postgres://",
            "postgresql://",
            "mysql://",
            "database_url",
            "api_key",
            "api_secret",
            "secret",
            "tx_hash",
            "transaction_hash",
            "payer",
            "payee",
            "request_payload",
            "response_payload",
            "raw_payload",
            "http://",
            "https://",
            "file://",
            "/users/",
            "/tmp/",
            "/fapi/v1/order",
            "/fapi/v2/account",
            "/fapi/v1/positionrisk",
            "/api/commerce/internal/execution-tasks/lease",
            "linkusdt",
            "link-usdt",
        ] {
            assert!(
                !lowered.contains(sensitive),
                "payment entitlement fixture {relative_path} must not leak sensitive marker {sensitive}: {body}"
            );
        }
    }
}

#[test]
fn full_product_health_artifact_validator_rejects_payment_entitlement_state_drift() {
    let artifact_dir = temp_artifact_dir("full-product-health-validator-payment-state-drift");
    let full_report_path = artifact_dir.join("full-product-health.json");
    let summary_path = artifact_dir.join("full-product-health-summary.json");
    let markdown_path = artifact_dir.join("full-product-health.md");

    fs::write(
        &full_report_path,
        r#"{
  "schema_version": 1,
  "status": "warn",
  "generated_at": "2026-05-07T01:00:00Z",
  "summary": {
    "p0_count": 0,
    "p1_count": 0,
    "info_count": 1,
    "read_only_input_count": 3,
    "wallet_payment_exception_count": 2,
    "payment_entitlement_blocker_count": 1
  },
  "sections": {
    "payment_entitlement_health": {
      "status": "warn",
      "source": "skipped",
      "database_engine": "postgresql",
      "read_only_input": false,
      "skipped": true,
      "wallet_payment_exception_count": 2,
      "payment_entitlement_blocker_count": 1,
      "alerts": [
        {
          "severity": "INFO",
          "code": "PAYMENT_INPUT_SKIPPED",
          "section": "payment_entitlement_health",
          "message": "payment input was skipped"
        }
      ]
    }
  },
  "alerts": [
    {
      "severity": "INFO",
      "code": "PAYMENT_INPUT_SKIPPED",
      "section": "payment_entitlement_health",
      "message": "payment input was skipped"
    }
  ],
  "alert_taxonomy": [],
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
  "summary": {
    "overall_status": "warn",
    "p0_count": 0,
    "p1_count": 0,
    "info_count": 1,
    "section_count": 1,
    "blocking_section_count": 0,
    "warning_section_count": 1,
    "top_alert_count": 1,
    "required_operator_action_count": 0,
    "alert_taxonomy_count": 0,
    "operator_playbook_item_count": 1,
    "correlation_id_count": 0,
    "read_only_input_count": 3,
    "wallet_payment_exception_count": 2,
    "payment_entitlement_blocker_count": 1
  },
  "section_statuses": {"payment_entitlement_health": "warn"},
  "checklist": [],
  "top_alerts": [
    {
      "severity": "INFO",
      "code": "PAYMENT_INPUT_SKIPPED",
      "section": "payment_entitlement_health",
      "message": "payment input was skipped"
    }
  ],
  "required_operator_actions": [],
  "alert_taxonomy": [],
  "operator_playbook_summary": {
    "item_count": 1,
    "blocking_item_count": 0,
    "manual_review_item_count": 0,
    "observe_only_item_count": 1,
    "items": [
      {
        "source": "alert",
        "severity": "INFO",
        "code": "PAYMENT_INPUT_SKIPPED",
        "section": "payment_entitlement_health",
        "message": "payment input was skipped",
        "operator_action": "observe_only",
        "owner": "commerce_billing",
        "default_next_action": "provide_payment_read_only_input",
        "admin_link_target": "admin.full_product_health.payment_entitlement_health"
      }
    ]
  },
  "correlation": {},
  "correlation_ids": []
}"#,
    )
    .unwrap_or_else(|error| panic!("failed to write {}: {}", summary_path.display(), error));
    fs::write(
        &markdown_path,
        "# Full Product Health\n\n**Status:** warn\n\n## Counts\n\n## Top Alerts\n\n## Operator Playbook Summary\n\n## Checklist\n\n## Artifact Paths\n\n## Skipped Sections\n",
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
        "strict validator should reject skipped payment artifacts carrying real counts"
    );
    let stdout = String::from_utf8(output.stdout).expect("validation output should be utf8");
    let payload: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("invalid validation json: {error}\n{stdout}"));
    let findings = payload["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|finding| {
            finding["code"] == "PAYMENT_ENTITLEMENT_STATE_DRIFT"
                && finding["artifact"] == "full_report"
                && finding["field"] == "sections.payment_entitlement_health.skipped"
        }),
        "validator should identify skipped/query_failed/real_count drift: {stdout}"
    );
}

include!("payment_artifact_smoke_section.rs");
include!("payment_input_producer_section.rs");
