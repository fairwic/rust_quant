fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}
fn script_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("check_local_service_health.sh")
}
fn runbook_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("local_service_health_runbook.md")
}
fn aggregator_fixture_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_aggregator.fixture.json")
}
fn full_product_artifact_schema_json_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_artifact_schema.json")
}
fn full_product_artifact_schema_doc_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_artifact_schema.md")
}
fn full_product_admin_ci_handoff_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_admin_ci_handoff.md")
}
fn full_product_admin_frontend_contract_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_admin_frontend_contract.md")
}
fn admin_recovery_action_guardrails_path() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("admin_recovery_action_guardrails.md")
}
fn full_product_artifact_examples_dir() -> PathBuf {
    repo_root()
        .join("docs")
        .join("dev")
        .join("full_product_health_examples")
}
fn full_product_admin_ingest_fixture_path() -> PathBuf {
    full_product_artifact_examples_dir().join("admin-ingest-handoff.json")
}
fn aggregator_fixture_script_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("check_full_product_health_aggregator_fixture.sh")
}
fn aggregator_runner_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("check_full_product_health.sh")
}
fn full_product_input_runner_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_inputs.sh")
}
fn full_product_summary_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("summarize_full_product_health.sh")
}
fn full_product_markdown_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("render_full_product_health_markdown.sh")
}
fn full_product_ci_wrapper_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("run_full_product_health_ci.sh")
}
fn full_product_artifact_validator_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("validate_full_product_health_artifacts.sh")
}
fn full_product_artifact_set_publisher_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("publish_full_product_health_artifact_set.sh")
}
fn full_product_admin_ingest_smoke_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("smoke_publish_full_product_health_admin_ingest.sh")
}
fn full_product_admin_ingest_mock_receiver_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("mock_full_product_health_admin_ingest_receiver.py")
}
fn full_product_admin_ingest_contract_smoke_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("smoke_publish_full_product_health_admin_ingest_contract.sh")
}
fn web_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_web_input.sh")
}
fn news_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_news_input.sh")
}
fn admin_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_admin_input.sh")
}
fn payment_input_producer_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("build_full_product_health_payment_input.sh")
}
fn full_product_payment_artifact_smoke_path() -> PathBuf {
    repo_root()
        .join("scripts")
        .join("dev")
        .join("smoke_full_product_health_payment_artifact_handoff.sh")
}
