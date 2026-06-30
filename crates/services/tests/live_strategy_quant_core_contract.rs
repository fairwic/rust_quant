use std::{fs, path::PathBuf};
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("services crate should live under crates/services")
        .to_path_buf()
}
fn read_repo_file(parts: &[&str]) -> String {
    let path = parts.iter().fold(repo_root(), |path, part| path.join(part));
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", path.display(), error))
}
fn assert_legacy_gate_before(
    source_name: &str,
    source: &str,
    function_marker: &str,
    mutation_marker: &str,
) {
    let function_offset = source
        .find(function_marker)
        .unwrap_or_else(|| panic!("{source_name} missing function marker {function_marker}"));
    let function_body = &source[function_offset..];
    let gate_offset = function_body
        .find("ensure_legacy_direct_live_exchange_order_allowed")
        .unwrap_or_else(|| {
            panic!("{source_name}:{function_marker} missing legacy direct live gate")
        });
    let mutation_offset = function_body.find(mutation_marker).unwrap_or_else(|| {
        panic!("{source_name}:{function_marker} missing mutation marker {mutation_marker}")
    });
    assert!(
        gate_offset < mutation_offset,
        "{source_name}:{function_marker} must check legacy direct live gate before {mutation_marker}"
    );
}
fn assert_signed_read_gate_before(
    source_name: &str,
    source: &str,
    function_marker: &str,
    signed_read_marker: &str,
) {
    let function_offset = source
        .find(function_marker)
        .unwrap_or_else(|| panic!("{source_name} missing function marker {function_marker}"));
    let function_body = &source[function_offset..];
    let gate_offset = function_body
        .find("ensure_legacy_signed_read_only_allowed")
        .unwrap_or_else(|| {
            panic!("{source_name}:{function_marker} missing legacy signed read-only gate")
        });
    let signed_read_offset = function_body.find(signed_read_marker).unwrap_or_else(|| {
        panic!("{source_name}:{function_marker} missing signed read marker {signed_read_marker}")
    });
    assert!(
        gate_offset < signed_read_offset,
        "{source_name}:{function_marker} must check legacy signed read-only gate before {signed_read_marker}"
    );
}
fn assert_stop_loss_guard_before(
    source_name: &str,
    source: &str,
    function_marker: &str,
    mutation_marker: &str,
) {
    let function_offset = source
        .find(function_marker)
        .unwrap_or_else(|| panic!("{source_name} missing function marker {function_marker}"));
    let function_body = &source[function_offset..];
    let guard_offset = function_body
        .find("ensure_entry_stop_loss_present")
        .unwrap_or_else(|| panic!("{source_name}:{function_marker} missing stop-loss guard"));
    let mutation_offset = function_body.find(mutation_marker).unwrap_or_else(|| {
        panic!("{source_name}:{function_marker} missing mutation marker {mutation_marker}")
    });
    assert!(
        guard_offset < mutation_offset,
        "{source_name}:{function_marker} must check stop-loss before {mutation_marker}"
    );
}
#[test]
fn live_strategy_startup_uses_candle_service_for_quant_core_source() {
    let root = repo_root();
    let strategy_data_path = root
        .join("crates")
        .join("services")
        .join("src")
        .join("strategy")
        .join("strategy_data_service.rs");
    let strategy_data = fs::read_to_string(&strategy_data_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", strategy_data_path.display(), error)
    });
    assert!(strategy_data.contains("use crate::market::get_confirmed_candles_for_backtest;"));
    assert!(strategy_data.contains("get_confirmed_candles_for_backtest"));
    assert!(!strategy_data.contains("CandlesModel::new()"));
    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));
    assert!(
        bootstrap.contains("use rust_quant_services::market::get_confirmed_candles_for_backtest;")
    );
    assert!(bootstrap.contains("get_confirmed_candles_for_backtest(&inst_id"));
    assert!(!bootstrap.contains("CandlesModel::new()"));
}
#[test]
fn kline_backtest_and_sync_paths_do_not_keep_legacy_candle_table_fallbacks() {
    let market_mod = read_repo_file(&["crates", "services", "src", "market", "mod.rs"]);
    assert!(market_mod.contains("get_quant_core_sharded_candles_for_backtest"));
    assert!(
        !market_mod.contains("fetch_candles_from_postgres(dto)"),
        "backtest candle loading must read quant_core symbol/timeframe sharded tables only"
    );

    let binance_websocket = read_repo_file(&[
        "crates",
        "services",
        "src",
        "market",
        "binance_websocket.rs",
    ]);
    assert!(
        !binance_websocket.contains("LegacyCompatTables"),
        "websocket candle persistence must not keep legacy table fallback"
    );

    let candles_job = read_repo_file(&[
        "crates",
        "orchestration",
        "src",
        "jobs",
        "data",
        "candles_job.rs",
    ]);
    assert!(
        !candles_job.contains("SqlxCandleRepository"),
        "scheduled candle sync must not keep legacy repository fallback"
    );

    let kline_sync_section = read_repo_file(&[
        "crates",
        "rust-quant-cli",
        "src",
        "app",
        "internal_server",
        "kline_sync_section.rs",
    ]);
    assert!(
        !kline_sync_section.contains("SqlxCandleRepository"),
        "internal kline sync must not keep legacy repository fallback"
    );
}
#[test]
fn external_market_sync_defaults_to_sharded_market_context_tables() {
    let external_market_sync = read_repo_file(&[
        "crates",
        "services",
        "src",
        "market",
        "external_market_sync_service.rs",
    ]);
    assert!(
        external_market_sync.contains("ShardedExternalMarketSnapshotRepository"),
        "external market sync must default to sharded market context tables"
    );
    assert!(
        !external_market_sync.contains("SqlxExternalMarketSnapshotRepository::new(pool)"),
        "external market sync must not default to external_market_snapshots"
    );

    let dune_market_sync = read_repo_file(&[
        "crates",
        "services",
        "src",
        "market",
        "dune_market_sync_service.rs",
    ]);
    assert!(
        dune_market_sync.contains("ShardedExternalMarketSnapshotRepository"),
        "dune market sync must default to sharded market context tables"
    );
    assert!(
        !dune_market_sync.contains("SqlxExternalMarketSnapshotRepository::new(pool)"),
        "dune market sync must not default to external_market_snapshots"
    );
}
#[test]
fn live_strategy_quant_core_smoke_script_is_safe_and_bounded() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_live_strategy_quant_core_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));
    assert!(script.contains("CANDLE_SOURCE:=\"quant_core\""));
    assert!(script.contains("STRATEGY_CONFIG_SOURCE:=\"quant_core\""));
    assert!(script.contains("IS_RUN_REAL_STRATEGY=true"));
    assert!(script.contains("IS_OPEN_SOCKET=false"));
    assert!(script.contains("IS_BACK_TEST=false"));
    assert!(script.contains("IS_RUN_SYNC_DATA_JOB=false"));
    assert!(script.contains("STRATEGY_SIGNAL_DISPATCH_MODE:=\"web\""));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=true"));
    assert!(script.contains("EXIT_AFTER_REAL_STRATEGY_ONESHOT=true"));
    assert!(script.contains("RUN_EXECUTION_WORKER_AFTER_STRATEGY:=\"true\""));
    assert!(script.contains("SMOKE_TIMEOUT_SECS"));
    assert!(script.contains("./scripts/dev/ddl_smoke.sh"));
    assert!(script.contains("cargo run --bin rust_quant"));
    assert!(script.contains("./scripts/dev/run_execution_worker_dry_run.sh"));
    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));
    assert!(bootstrap.contains("EXIT_AFTER_REAL_STRATEGY_ONESHOT"));
    assert!(bootstrap.contains("实时策略 one-shot 已完成"));
}
#[test]
fn startup_guide_live_strategy_examples_use_web_dispatch_and_warn_legacy_direct_gate() {
    let guide = read_repo_file(&["docs", "STARTUP_GUIDE.md"]);
    assert!(guide.contains("STRATEGY_SIGNAL_DISPATCH_MODE=web"));
    assert!(guide.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
    assert!(guide.contains("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"));
    assert!(guide.contains("LEGACY_SIGNED_READ_ONLY_CONFIRM"));
    assert!(guide.contains("I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS"));
    assert!(
        guide.contains("rust_quan_web")
            && guide.contains("execution task")
            && guide.contains("legacy direct")
    );
}
#[test]
fn execution_worker_from_env_requires_non_empty_internal_secret() {
    let worker_source = read_repo_file(&[
        "crates",
        "services",
        "src",
        "rust_quan_web",
        "execution_worker.rs",
    ]);
    assert!(worker_source.contains("fn required_internal_secret_from_env() -> Result<String>"));
    assert!(worker_source
        .contains("EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required"));
    assert!(worker_source.contains("let internal_secret = required_internal_secret_from_env()?;"));
    assert!(
        !worker_source.contains(
            ".unwrap_or_default();\n        let config = ExecutionWorkerConfig::from_env();"
        ),
        "ExecutionWorker::from_env must not default the internal secret to an empty string"
    );
}
#[test]
fn execution_worker_does_not_keep_live_mode_switch_gates() {
    let worker_source = read_repo_file(&[
        "crates",
        "services",
        "src",
        "rust_quan_web",
        "execution_worker.rs",
    ]);
    let worker_orchestration = read_repo_file(&[
        "crates",
        "services",
        "src",
        "rust_quan_web",
        "execution_worker_orchestration_section.rs",
    ]);
    assert!(
        !worker_source.contains("validate_live_worker_scope"),
        "execution worker must not keep a target-scope live switch gate"
    );
    assert!(
        !worker_source.contains("reconciliation_only_mode"),
        "execution worker must not keep reconciliation-only mode switch"
    );
    assert!(worker_source.contains("fn validate_runtime_scope(&self) -> Result<()>"));
    let audit_ready_start = worker_source
        .find("pub async fn verify_live_audit_ready(&self)")
        .expect("verify_live_audit_ready must exist");
    let audit_ready_body = &worker_source[audit_ready_start..];
    assert!(
        audit_ready_body.find("self.validate_runtime_scope()?")
            < audit_ready_body.find("self.ensure_live_audit_repository()?"),
        "verify_live_audit_ready must validate runtime scope before audit readiness"
    );
    let run_once_start = worker_orchestration
        .find("pub async fn run_once(&self)")
        .expect("run_once must exist");
    let run_once_body = &worker_orchestration[run_once_start..];
    assert!(
        run_once_body.find("self.validate_runtime_scope()?")
            < run_once_body.find("self.ensure_live_audit_repository()?"),
        "run_once must validate runtime scope before checkpointing, leasing, or audit readiness"
    );
}
#[test]
fn legacy_signed_read_only_queries_require_confirmation_gate() {
    let legacy_order_service = read_repo_file(&[
        "crates",
        "execution",
        "src",
        "order_manager",
        "order_service.rs",
    ]);
    let risk_account_job = read_repo_file(&["crates", "risk", "src", "account", "account_job.rs"]);
    let risk_position_service =
        read_repo_file(&["crates", "risk", "src", "position", "position_service.rs"]);
    let risk_signed_read_guard =
        read_repo_file(&["crates", "risk", "src", "legacy_signed_read_only.rs"]);
    let infrastructure_okx_adapter = read_repo_file(&[
        "crates",
        "infrastructure",
        "src",
        "exchanges",
        "okx_adapter.rs",
    ]);
    let okx_order_service = read_repo_file(&[
        "crates",
        "services",
        "src",
        "exchange",
        "okx_order_service.rs",
    ]);
    for source in [
        &legacy_order_service,
        &risk_signed_read_guard,
        &infrastructure_okx_adapter,
        &okx_order_service,
    ] {
        assert!(source.contains("LEGACY_SIGNED_READ_ONLY_CONFIRM"));
        assert!(source.contains("I_UNDERSTAND_LEGACY_SIGNED_READ_ONLY_ACCOUNT_READS"));
    }
    assert_signed_read_gate_before(
        "order_service.rs",
        &legacy_order_service,
        "pub async fn get_pending_orders",
        "OkxTrade::from_env",
    );
    assert_signed_read_gate_before(
        "order_service.rs",
        &legacy_order_service,
        "pub async fn get_order_detail",
        "OkxTrade::from_env",
    );
    assert_signed_read_gate_before(
        "order_service.rs",
        &legacy_order_service,
        "pub async fn sync_order_history",
        "OkxTrade::from_env",
    );
    assert_signed_read_gate_before(
        "order_service.rs",
        &legacy_order_service,
        "pub async fn sync_order_history_archive",
        "OkxTrade::from_env",
    );
    assert_signed_read_gate_before(
        "account_job.rs",
        &risk_account_job,
        "pub async fn get_account_balance",
        "OkxAccount::from_env",
    );
    assert_signed_read_gate_before(
        "position_service.rs",
        &risk_position_service,
        "pub async fn get_position_list",
        "OkxAccount::from_env",
    );
    assert_signed_read_gate_before(
        "okx_adapter.rs",
        &infrastructure_okx_adapter,
        "impl OkxAccountAdapter",
        "OkxAccount::from_env",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "async fn get_algo_orders_raw",
        "create_okx_client",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn get_positions",
        "create_okx_client",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn get_max_available_size",
        "create_okx_client",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn get_order_details",
        "create_okx_client",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn inspect_auto_close_by_order",
        ".get_order_details(",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn get_trade_available_equity",
        "create_okx_client",
    );
    assert_signed_read_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn get_funding_available_balance",
        "create_okx_client",
    );
}
#[test]
fn legacy_direct_live_strategy_mutations_require_confirmation_gate() {
    let main = read_repo_file(&[
        "crates",
        "services",
        "src",
        "strategy",
        "strategy_execution_service.rs",
    ]);
    let live_close = read_repo_file(&[
        "crates",
        "services",
        "src",
        "strategy",
        "strategy_execution_service",
        "live_close_algo_section.rs",
    ]);
    let external_flat = read_repo_file(&[
        "crates",
        "services",
        "src",
        "strategy",
        "strategy_execution_service",
        "external_flat_section.rs",
    ]);
    let legacy_swap_order = read_repo_file(&[
        "crates",
        "execution",
        "src",
        "order_manager",
        "swap_order_service.rs",
    ]);
    let okx_order_service = read_repo_file(&[
        "crates",
        "services",
        "src",
        "exchange",
        "okx_order_service.rs",
    ]);
    let okx_stop_loss_amender = read_repo_file(&[
        "crates",
        "risk",
        "src",
        "realtime",
        "okx_stop_loss_amender.rs",
    ]);
    let helpers = read_repo_file(&[
        "crates",
        "services",
        "src",
        "strategy",
        "strategy_execution_service",
        "live_helpers.rs",
    ]);
    assert!(helpers.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
    assert!(helpers.contains("I_UNDERSTAND_LEGACY_DIRECT_LIVE_ORDERS"));
    assert_legacy_gate_before(
        "strategy_execution_service.rs",
        &main,
        "async fn execute_order_internal",
        "execute_order_from_signal(",
    );
    assert_legacy_gate_before(
        "strategy_execution_service.rs",
        &main,
        "async fn execute_order_internal",
        "close_position(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "async fn sync_close_algos",
        "cancel_close_algos(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "async fn sync_close_algos",
        "place_close_algo(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "async fn cancel_cached_close_algos",
        "cancel_close_algos(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "pub async fn compensate_close_algos_on_start",
        "cancel_close_algos(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "pub async fn compensate_close_algos_on_start",
        "place_close_algo(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "async fn close_position_internal",
        "close_position(",
    );
    assert_legacy_gate_before(
        "live_close_algo_section.rs",
        &live_close,
        "async fn close_position_internal",
        "cancel_close_algos(",
    );
    assert_legacy_gate_before(
        "external_flat_section.rs",
        &external_flat,
        "async fn rebalance_trade_bucket_after_close",
        "transfer_between_accounts(",
    );
    assert!(legacy_swap_order.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
    assert_legacy_gate_before(
        "swap_order_service.rs",
        &legacy_swap_order,
        "pub async fn ready_to_order",
        "OkxAccount::from_env()",
    );
    assert_legacy_gate_before(
        "swap_order_service.rs",
        &legacy_swap_order,
        "pub async fn place_order_spot",
        "OkxTrade::from_env()?.place_order",
    );
    assert_legacy_gate_before(
        "swap_order_service.rs",
        &legacy_swap_order,
        "pub async fn close_position",
        "OkxTrade::from_env()?.close_position",
    );
    assert_legacy_gate_before(
        "swap_order_service.rs",
        &legacy_swap_order,
        "pub async fn order_swap",
        "OkxTrade::from_env()",
    );
    for (function_marker, risk_marker) in [
        ("pub async fn ready_to_order", "OkxAccount::from_env()"),
        ("pub async fn order_swap", "OkxTrade::from_env()"),
    ] {
        let function_offset = legacy_swap_order.find(function_marker).unwrap_or_else(|| {
            panic!("swap_order_service.rs missing function marker {function_marker}")
        });
        let function_body = &legacy_swap_order[function_offset..];
        let guard_offset = function_body
            .find("ensure_max_loss_percent_ratio")
            .unwrap_or_else(|| {
                panic!("swap_order_service.rs:{function_marker} missing max loss ratio guard")
            });
        let risk_offset = function_body.find(risk_marker).unwrap_or_else(|| {
            panic!("swap_order_service.rs:{function_marker} missing risk marker {risk_marker}")
        });
        assert!(
            guard_offset < risk_offset,
            "swap_order_service.rs:{function_marker} must validate max_loss_percent before {risk_marker}"
        );
    }
    assert!(okx_order_service.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn place_order",
        "trade.place_order",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn place_order_with_algo_orders",
        "trade.place_order",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn place_order_with_stop_loss",
        "place_order_with_algo_orders(",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn close_position",
        "trade.close_position",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn cancel_close_algos",
        "send_request(Method::POST",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn place_close_algo",
        "send_request(Method::POST",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn transfer_between_accounts",
        "asset\n            .transfer",
    );
    assert_legacy_gate_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn execute_order_from_signal",
        "place_order_with_algo_orders(",
    );
    assert_stop_loss_guard_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn place_order",
        "trade.place_order",
    );
    assert_stop_loss_guard_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn place_order_with_algo_orders",
        "trade.place_order",
    );
    assert_stop_loss_guard_before(
        "okx_order_service.rs",
        &okx_order_service,
        "pub async fn execute_order_from_signal",
        "place_order_with_algo_orders(",
    );
    assert!(okx_stop_loss_amender.contains("LEGACY_DIRECT_LIVE_ORDER_CONFIRM"));
    assert_legacy_gate_before(
        "okx_stop_loss_amender.rs",
        &okx_stop_loss_amender,
        "async fn move_stop_loss_to_price",
        "send_request(Method::POST",
    );
}
#[test]
fn live_strategy_max_loss_percent_is_validated_before_stop_loss_generation() {
    let signal_payload = read_repo_file(&[
        "crates",
        "services",
        "src",
        "strategy",
        "strategy_signal_payload.rs",
    ]);
    let live_helpers = read_repo_file(&[
        "crates",
        "services",
        "src",
        "strategy",
        "strategy_execution_service",
        "live_helpers.rs",
    ]);
    assert!(signal_payload.contains("fn validate_max_loss_percent"));
    assert!(signal_payload.contains("max_loss_percent < 1.0"));
    let payload_selector = signal_payload
        .find("fn select_strategy_signal_stop_loss")
        .expect("strategy signal payload stop-loss selector must exist");
    let payload_body = &signal_payload[payload_selector..];
    let payload_guard = payload_body
        .find("validate_max_loss_percent(risk_config.max_loss_percent)?")
        .expect("strategy signal payload must validate max_loss_percent");
    let payload_candidates = payload_body
        .find("build_stop_loss_candidates")
        .expect("strategy signal payload must build stop candidates");
    assert!(
        payload_guard < payload_candidates,
        "strategy signal payload must validate max_loss_percent before stop-loss candidates"
    );
    let helper_builder = live_helpers
        .find("fn build_stop_loss_candidates")
        .expect("live helper stop-loss candidate builder must exist");
    let helper_body = &live_helpers[helper_builder..];
    let helper_guard = helper_body
        .find("strategy_signal_payload::validate_max_loss_percent(max_loss_percent)?")
        .expect("live helper must validate max_loss_percent");
    let helper_stop = helper_body
        .find("let max_loss_stop")
        .expect("live helper must compute max_loss_stop");
    assert!(
        helper_guard < helper_stop,
        "live helper must validate max_loss_percent before max_loss_stop"
    );
}
#[test]
fn forced_signal_quant_core_smoke_script_drives_web_execution_loop() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_forced_signal_quant_core_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));
    assert!(script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL:=\"buy\""));
    assert!(script.contains("POSTGRES_CONTAINER:=\"quant_core_postgres\""));
    assert!(script.contains("POSTGRES_CONTAINER=\"${POSTGRES_CONTAINER}\""));
    assert!(script.contains("RESTORE_DEMO_CREDENTIAL"));
    assert!(script.contains("signed_exchange_preflight_passed"));
    assert!(script.contains("Local forced-signal dry-run fixture"));
    assert!(script.contains("last_check_code = 'local_smoke'"));
    assert!(script.contains("seal_forced_signal_fixture_cipher"));
    assert!(script.contains("WEB_BACKEND_ENV"));
    assert!(script.contains("load_web_backend_credential_encryption_env"));
    assert!(script.contains("API_CREDENTIAL_ENCRYPTION_KEY"));
    assert!(script.contains("crypto.createCipheriv('aes-256-gcm'"));
    assert!(script.contains("CredentialEnvelopeContext"));
    assert!(!script.contains("'v4:local_aes256gcm:forced-signal-fixture-api-key'"));
    assert!(!script.contains("'v4:local_aes256gcm:forced-signal-fixture-api-secret'"));
    assert!(script.contains("RESTORE_DEMO_RISK_SNAPSHOT"));
    assert!(script.contains("user_execution_risk_snapshots"));
    assert!(script.contains("fixture_signed_read_only_preflight"));
    assert!(script.contains("account_equity_usdt"));
    assert!(script.contains("ON CONFLICT (buyer_email, exchange, symbol)"));
    assert!(script.contains("RESTORE_DEMO_POSITION"));
    assert!(script.contains("user_position_snapshots"));
    assert!(script.contains("snapshot_source = 'forced_signal_position_clear'"));
    assert!(script.contains("quantity = 0"));
    assert!(script.contains("reset_forced_signal_open_tasks"));
    assert!(script.contains("forced_signal_smoke_reset"));
    assert!(script.contains("task_status = 'blocked'"));
    assert!(script.contains("TRADE_SIGNAL_SMOKE_STRATEGY_SLUG=vegas"));
    assert!(script.contains("EXECUTION_DEMO_STRATEGY_TITLE=\"Vegas Strategy Smoke\""));
    assert!(script.contains("TRADE_SIGNAL_SMOKE_SYMBOL=ETH-USDT-SWAP"));
    assert!(script.contains("LIVE_STRATEGY_ONLY_INST_IDS=ETH-USDT-SWAP"));
    assert!(script.contains("LIVE_STRATEGY_ONLY_PERIODS=4H"));
    assert!(script.contains("WEB_SEED_SCRIPT=\"${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh\""));
    assert!(script.contains("STRATEGY_SIGNAL_DISPATCH_MODE=web"));
    assert!(script.contains("EXECUTION_WORKER_DRY_RUN=true"));
    assert!(script.contains("RUN_EXECUTION_WORKER_AFTER_STRATEGY=false"));
    assert!(script.contains("RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX"));
    assert!(script.contains("BASE_SIGNAL_ID=\"$(query_web_scalar"));
    assert!(script.contains("id > ${BASE_SIGNAL_ID}"));
    assert!(script.contains("Expected a fresh rust_quant forced strategy signal"));
    assert!(script.contains("EXECUTION_WORKER_TARGET_TASK_IDS=\"${NEW_TASK_ID}\""));
    assert!(script.contains("EXECUTION_WORKER_LEASE_LIMIT=1"));
    assert!(script.contains("EXECUTION_WORKER_TASK_TYPES=execute_signal"));
    assert!(script.contains("EXECUTION_WORKER_TASK_STATUSES=pending"));
    assert!(script.contains("exchange_order_results"));
    assert!(script.contains("execution_task_attempts"));
    assert!(script.contains("order_status = 'dry_run'"));
    assert!(script.contains("task_status IN ('completed', 'pending_protection_sync')"));
    assert!(script.contains("./scripts/dev/run_live_strategy_quant_core_smoke.sh"));
    assert!(script.contains("./scripts/dev/run_execution_worker_dry_run.sh"));
}
#[test]
fn binance_websocket_quant_core_smoke_script_prepares_sync_then_live_wait() {
    let root = repo_root();
    let script_path = root
        .join("scripts")
        .join("dev")
        .join("run_binance_websocket_quant_core_smoke.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", script_path.display(), error));
    assert!(script.contains("SYNC_ONLY_PERIODS:=\"1m\""));
    assert!(script.contains("SYNC_LATEST_ONLY=true"));
    assert!(script.contains("IS_RUN_REAL_STRATEGY=true"));
    assert!(script.contains("IS_OPEN_SOCKET=true"));
    assert!(script.contains("RUST_QUANT_SMOKE_FORCE_SIGNAL=buy"));
    assert!(script.contains("WEB_SEED_SCRIPT=\"${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh\""));
    assert!(script.contains("seed_execution_demo_combo.sh"));
    assert!(script.contains("strategy_signal_inbox"));
    assert!(script.contains("run_execution_worker_dry_run.sh"));
}
#[test]
fn websocket_runtime_keeps_preheated_configs_for_ws_target_derivation() {
    let root = repo_root();
    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));
    assert!(bootstrap.contains("✅ 策略已预热并进入等待"));
    assert!(bootstrap.contains("started_configs.push(config.clone());\n            continue;"));
}
#[test]
fn live_strategy_runtime_can_be_filtered_for_single_probe_target() {
    let root = repo_root();
    let bootstrap_path = root
        .join("crates")
        .join("rust-quant-cli")
        .join("src")
        .join("app")
        .join("bootstrap.rs");
    let bootstrap = fs::read_to_string(&bootstrap_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", bootstrap_path.display(), error));
    assert!(bootstrap.contains("LIVE_STRATEGY_ONLY_INST_IDS"));
    assert!(bootstrap.contains("LIVE_STRATEGY_ONLY_PERIODS"));
    assert!(bootstrap.contains("filter_live_strategy_configs"));
    assert!(bootstrap.contains("实时策略过滤后剩余"));
}
