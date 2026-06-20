#[test]
fn full_product_health_admin_ingest_fixture_is_machine_readable_and_redacted() {
    let path = full_product_admin_ingest_fixture_path();
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error));
    let payload: Value = serde_json::from_str(&body)
        .unwrap_or_else(|error| panic!("fixture should be valid json: {error}\n{body}"));

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
            "fixture missing field {field}: {body}"
        );
    }

    let playbook = payload["summary"]["operator_playbook_summary"]
        .as_object()
        .expect("embedded summary should expose operator_playbook_summary");
    assert_eq!(playbook["item_count"], 2);
    assert_eq!(playbook["blocking_item_count"], 0);
    assert_eq!(playbook["manual_review_item_count"], 1);
    assert_eq!(playbook["observe_only_item_count"], 1);
    let items = playbook["items"]
        .as_array()
        .expect("embedded operator_playbook_summary.items should be an array");
    for field in [
        "source",
        "severity",
        "code",
        "section",
        "operator_action",
        "owner",
        "default_next_action",
        "admin_link_target",
    ] {
        assert!(
            items
                .iter()
                .all(|item| item[field].as_str().is_some_and(|value| !value.is_empty())),
            "embedded operator playbook item field {field} should be populated: {body}"
        );
    }
    assert!(
        items
            .iter()
            .any(|item| item["code"] == "NEWS_SOURCE_DEGRADED"
                && item["operator_action"] == "manual_review_before_release"
                && item["owner"] == "news_ops"
                && item["default_next_action"] == "review_news_source_status"
                && item["admin_link_target"] == "admin.full_product_health.news_source_ai_health"),
        "embedded summary should expose registry-backed operator playbook items: {body}"
    );

    let lowered = body.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        "api_key",
        "api_secret",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "file://",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "fixture must not contain sensitive marker {sensitive}: {body}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_smoke_passes_bash_syntax_check() {
    let output = Command::new("bash")
        .arg("-n")
        .arg(full_product_admin_ingest_smoke_path())
        .output()
        .expect("bash -n should be available");

    assert!(
        output.status.success(),
        "bash -n syntax check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn full_product_health_admin_ingest_smoke_stays_no_env_and_localhost_only() {
    let script = read_full_product_admin_ingest_smoke_script();

    for required in [
        "ADMIN_INGEST_URL",
        "ADMIN_INGEST_ALLOW_REMOTE",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "publish_full_product_health_artifact_set.sh",
        "localhost",
        "127.0.0.1",
        "Authorization",
        ".env",
        "postgres://",
        "api_key",
        "api_secret",
        "raw_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "LINKUSDT",
    ] {
        assert!(
            script.contains(required),
            "admin ingest smoke script should document or scan marker {required}"
        );
    }

    for forbidden in [
        "source .env",
        "cat .env",
        "cargo run",
        "podman exec",
        "docker exec",
        "Authorization:",
        "--header Authorization",
    ] {
        assert!(
            !script.contains(forbidden),
            "admin ingest smoke script must avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_contract_wrapper_and_mock_receiver_stay_local_and_safe() {
    let receiver = read_full_product_admin_ingest_mock_receiver_script();
    let contract = read_full_product_admin_ingest_contract_smoke_script();

    for required in [
        "127.0.0.1",
        ".env",
        "Authorization",
        "api_key",
        "api_secret",
        "raw_payload",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
        "LINKUSDT",
    ] {
        assert!(
            receiver.contains(required),
            "mock receiver script should document or scan marker {required}"
        );
    }

    for required in [
        "mock_full_product_health_admin_ingest_receiver.py",
        "smoke_publish_full_product_health_admin_ingest.sh",
        "127.0.0.1",
        "/admin/ingest",
        "mock_contract",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
        ".env",
        "does not scan directories",
        "Authorization",
    ] {
        assert!(
            contract.contains(required),
            "contract smoke script should document or use marker {required}"
        );
    }

    for forbidden in [
        "source .env",
        "cat .env",
        "docker exec",
        "podman exec",
        "curl https://",
        "curl http://",
    ] {
        assert!(
            !contract.contains(forbidden),
            "contract smoke script must avoid {forbidden}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_smoke_prints_parseable_redacted_payload_without_url() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let output = Command::new(full_product_admin_ingest_smoke_path())
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
            "phase-53-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID",
            "phase-53-dry-run",
        )
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
        .expect("admin ingest smoke should run");

    assert!(
        output.status.success(),
        "dry-run smoke should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let payload: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("dry-run stdout should be parseable json: {error}\n{stdout}")
    });

    assert_eq!(
        payload["artifactSetId"],
        "health-2026-05-07T01-00-00Z-38370b9de0be"
    );
    assert_eq!(
        payload["operatorMetadata"]["generatedBy"],
        "phase-53-contract-test"
    );

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "/fapi/v1/order",
        "/api/commerce/internal/execution-tasks/lease",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "dry-run stdout must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_smoke_posts_to_local_mock_without_leaking_payload_or_paths() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener should expose addr");
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("mock server should accept");
        let mut header_bytes = Vec::new();
        let mut single = [0_u8; 1];
        loop {
            stream
                .read_exact(&mut single)
                .expect("mock server should read request headers");
            header_bytes.push(single[0]);
            if header_bytes.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header_text = String::from_utf8(header_bytes).expect("headers should be utf8");
        let content_length = header_text
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("Content-Length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .expect("request should include content-length");
        let mut body_bytes = vec![0_u8; content_length];
        stream
            .read_exact(&mut body_bytes)
            .expect("mock server should read request body");
        let request = format!(
            "{header_text}{}",
            String::from_utf8(body_bytes).expect("body should be utf8")
        );
        tx.send(request).expect("request should be sent to test");
        stream
            .write_all(
                b"HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 41\r\n\r\n{\"status\":\"accepted\",\"requestId\":\"mock-1\"}",
            )
            .expect("mock server should write response");
    });

    let output = Command::new(full_product_admin_ingest_smoke_path())
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
            "phase-53-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID", "phase-53-post")
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
        .env(
            "ADMIN_INGEST_URL",
            format!("http://127.0.0.1:{}/admin/ingest", address.port()),
        )
        .output()
        .expect("admin ingest smoke should run");

    handle.join().expect("mock server thread should finish");

    assert!(
        output.status.success(),
        "localhost POST smoke should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let request = rx.recv().expect("mock server should capture request");
    assert!(request.starts_with("POST /admin/ingest HTTP/1.1\r\n"));
    assert!(request.contains("\r\nHost: 127.0.0.1:"));
    assert!(request.contains("\r\nContent-Type: application/json\r\n"));
    assert!(!request.contains("Authorization:"));
    assert!(!request.to_ascii_lowercase().contains("postgres://"));
    assert!(!request.to_ascii_lowercase().contains("api_key"));
    assert!(!request.contains("/Users/"));

    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .expect("request should contain a body");
    let payload: Value = serde_json::from_str(body)
        .unwrap_or_else(|error| panic!("request body should be parseable json: {error}\n{body}"));
    assert_eq!(payload["operatorMetadata"]["runId"], "phase-53-post");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "\"artifactsetid\"",
    ] {
        assert!(
            !combined.contains(sensitive),
            "POST smoke output must not leak sensitive marker {sensitive}: stdout={stdout}\nstderr={stderr}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_contract_smoke_uses_local_mock_receiver_and_safe_stdout() {
    let examples_dir = full_product_artifact_examples_dir();
    let full_report_path = examples_dir.join("full-product-health.json");
    let summary_path = examples_dir.join("full-product-health-summary.json");
    let markdown_path = examples_dir.join("full-product-health.md");

    let output = Command::new(full_product_admin_ingest_contract_smoke_path())
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
            "phase-54-contract-test",
        )
        .env("FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE", "ci")
        .env(
            "FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID",
            "phase-54-contract",
        )
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
        .expect("admin ingest contract smoke should run");

    assert!(
        output.status.success(),
        "contract smoke should succeed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let payload: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("contract smoke stdout should be parseable json: {error}\n{stdout}")
    });

    assert_eq!(payload["mode"], "mock_contract");
    assert_eq!(payload["request"]["method"], "POST");
    assert_eq!(payload["request"]["path"], "/admin/ingest");
    assert_eq!(payload["request"]["contentType"], "application/json");
    assert_eq!(payload["request"]["hasAuthorization"], false);
    assert_eq!(payload["request"]["body"]["redactionStatus"], "ok");
    assert_eq!(payload["request"]["body"]["sensitiveMarkerCount"], 0);
    assert_eq!(
        payload["request"]["body"]["operatorRunId"],
        "phase-54-contract"
    );
    assert_eq!(payload["delivery"]["http"]["status"], 202);
    assert_eq!(payload["delivery"]["http"]["ok"], true);
    assert_eq!(payload["delivery"]["response"]["status"], "accepted");

    let lowered = stdout.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        ".env",
        "api_key",
        "api_secret",
        "secret@",
        "raw_payload",
        "/users/",
        "docs/dev/full_product_health_examples",
        "\"artifactsetid\"",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "contract smoke stdout must not leak sensitive marker {sensitive}: {stdout}"
        );
    }
}

#[test]
fn full_product_health_admin_ingest_contract_smoke_requires_explicit_artifact_env_before_starting_mock_receiver(
) {
    let output = Command::new(full_product_admin_ingest_contract_smoke_path())
        .output()
        .expect("admin ingest contract smoke should run");

    assert!(
        !output.status.success(),
        "contract smoke should fail safely when explicit artifact env is missing"
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");

    assert!(
        stdout.trim().is_empty(),
        "missing artifact env should fail before emitting stdout summary: {stdout}"
    );

    for required in [
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH",
        "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT",
        "does not read .env",
        "does not scan directories",
    ] {
        assert!(
            stderr.contains(required),
            "missing artifact env failure should mention {required}: {stderr}"
        );
    }

    let lowered = stderr.to_ascii_lowercase();
    for sensitive in [
        "postgres://",
        "mysql://",
        "api_key",
        "api_secret",
        "/fapi/v1/order",
        "linkusdt",
    ] {
        assert!(
            !lowered.contains(sensitive),
            "missing artifact env failure must stay sanitized and avoid {sensitive}: {stderr}"
        );
    }
}
