use std::fs;
use std::path::{Path, PathBuf};
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("services crate should be under rust_quant/crates/services")
        .to_path_buf()
}
fn read_repo_file(path: &str) -> String {
    fs::read_to_string(repo_root().join(path)).expect(path)
}
fn compose_service_block(compose: &str, service: &str) -> String {
    let needle = format!("  {service}:");
    let mut found = false;
    let mut lines = Vec::new();
    for line in compose.lines() {
        if line == needle {
            found = true;
            continue;
        }
        if found && line.starts_with("  ") && !line.starts_with("    ") {
            break;
        }
        if found {
            lines.push(line);
        }
    }
    assert!(found, "compose must contain service block `{service}`");
    lines.join("\n")
}
#[test]
fn market_velocity_production_deploy_contract_is_compose_and_rust_native() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let workflow = read_repo_file(".github/workflows/cicd.yml");
    let production_gate = read_repo_file(".github/workflows/market_velocity_production_gate.yml");
    let dockerfile = read_repo_file("Dockerfile.runtime");
    let okx_websocket = read_repo_file("crates/market/src/streams/websocket_service.rs");
    let promote = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback = read_repo_file("scripts/deploy/rollback.sh");
    let verify = read_repo_file("scripts/deploy/verify_production.sh");
    for service in [
        "quant-core-schema-ensure:",
        "quant-core-internal-server:",
        "quant-core-exchange-symbol-sync-worker:",
        "quant-core-vegas-eth-4h-worker:",
        "quant-core-vegas-universal-4h-worker:",
        "quant-core-market-velocity-radar:",
        "quant-core-market-velocity-candle-backfill-scheduler:",
        "quant-core-market-velocity-kline-scanner-scheduler:",
        "quant-core-market-velocity-paper-observation-scheduler:",
        "quant-core-market-velocity-live-handoff:",
        "quant-core-market-velocity-live-handoff-scheduler:",
        "quant-core-market-velocity-breakdown-short-live-handoff-scheduler:",
        "quant-core-execution-worker:",
        "quant-core-execution-confirmation-worker:",
        "quant-core-execution-report-replay-worker:",
    ] {
        assert!(
            compose.contains(service),
            "deploy compose must define {service}"
        );
    }
    for rust_native_entrypoint in [
        r#"IS_RUN_EXCHANGE_SYMBOL_SYNC_WORKER: "true""#,
        r#"EXCHANGE_SYMBOL_SYNC_WORKER_ONLY: "true""#,
        r#"EXCHANGE_SYMBOL_SYNC_RUN_ONCE: "false""#,
        "EXCHANGE_SYMBOL_SOURCES: ${EXCHANGE_SYMBOL_SOURCES:-okx}",
        r#"IS_RUN_REAL_STRATEGY: "true""#,
        r#"IS_OPEN_SOCKET: "true""#,
        "LIVE_STRATEGY_ONLY_EXCHANGES: ${LIVE_STRATEGY_ONLY_EXCHANGES:-okx}",
        "LIVE_STRATEGY_ONLY_INST_IDS: ${LIVE_STRATEGY_ONLY_INST_IDS:-ETH-USDT-SWAP}",
        "LIVE_STRATEGY_ONLY_PERIODS: ${LIVE_STRATEGY_ONLY_PERIODS:-4H}",
        "MARKET_DATA_EXCHANGE: ${LIVE_STRATEGY_MARKET_DATA_EXCHANGE:-okx}",
        "DEFAULT_EXCHANGE: ${LIVE_STRATEGY_MARKET_DATA_EXCHANGE:-okx}",
        "STRATEGY_SIGNAL_DISPATCH_MODE: ${LIVE_STRATEGY_SIGNAL_DISPATCH_MODE:-web}",
        r#"IS_RUN_MARKET_VELOCITY_RADAR: "true""#,
        r#"MARKET_VELOCITY_RADAR_ONLY: "true""#,
        "market_velocity_candle_backfill",
        "--timeframes",
        "MARKET_VELOCITY_BACKFILL_TIMEFRAMES:-1m,5m,15m",
        "MARKET_VELOCITY_BACKFILL_REQUEST_SLEEP_MS:-500",
        "--loop-interval-seconds",
        "MARKET_VELOCITY_CANDLE_BACKFILL_INTERVAL_SECS",
        "market_velocity_kline_scanner",
        "MARKET_VELOCITY_KLINE_SCANNER_INTERVAL_SECS",
        "MARKET_VELOCITY_KLINE_SCANNER_MIN_PRICE_CHANGE_PCT",
        r#"IS_RUN_EXECUTION_WORKER: "true""#,
        r#"EXECUTION_WORKER_ONLY: "true""#,
        "market_velocity_live_handoff",
        "EXECUTION_EVENT_SECRET: ${EXECUTION_EVENT_SECRET:?EXECUTION_EVENT_SECRET is required}",
        "MARKET_VELOCITY_LIVE_BUYER_EMAIL: ${MARKET_VELOCITY_LIVE_BUYER_EMAIL:-}",
        "MARKET_VELOCITY_LIVE_COMBO_ID: ${MARKET_VELOCITY_LIVE_COMBO_ID:-}",
        r#"MARKET_VELOCITY_LIVE_HANDOFF_RUN_ONCE: "false""#,
        "MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS: ${MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS:-5}",
        "MARKET_VELOCITY_LIVE_HANDOFF_SIGNAL_TTL_MS: ${MARKET_VELOCITY_LIVE_HANDOFF_SIGNAL_TTL_MS:-10000}",
    ] {
        assert!(
            compose.contains(rust_native_entrypoint),
            "deploy compose must use Rust-native entrypoint `{rust_native_entrypoint}`"
        );
    }
    assert!(
        !compose.contains("REDIS_HOST: ${REDIS_HOST:-redis://127.0.0.1:6379/}"),
        "production containers must not default Redis to container-local 127.0.0.1"
    );
    assert!(
        compose.contains("REDIS_HOST: ${REDIS_HOST:-redis://redis:6379/}"),
        "production Redis default must use Docker DNS for the deployed redis container unless REDIS_HOST is explicitly set"
    );
    assert!(
        compose.contains(r#""host.docker.internal:host-gateway""#),
        "production compose must map host.docker.internal for Linux Docker deployments"
    );
    assert!(
        compose.contains("quant-core-external:")
            && compose.contains("name: ${QUANT_CORE_EXTERNAL_NETWORK:-bjd_server_default}")
            && compose.contains("external: true"),
        "production compose must attach Core services to the external app network that owns postgres and quant-web-backend"
    );
    assert!(
        !compose.contains(r#""postgres:host-gateway""#)
            && !compose.contains(r#""redis:host-gateway""#),
        "production compose must not override postgres/redis Docker DNS with host-gateway aliases"
    );
    for service in [
        "quant-core-internal-server",
        "quant-core-exchange-symbol-sync-worker",
        "quant-core-vegas-eth-4h-worker",
        "quant-core-vegas-universal-4h-worker",
        "quant-core-market-velocity-radar",
        "quant-core-market-velocity-candle-backfill-scheduler",
        "quant-core-market-velocity-kline-scanner-scheduler",
        "quant-core-market-velocity-paper-observation-scheduler",
        "quant-core-market-velocity-live-handoff-scheduler",
        "quant-core-market-velocity-breakdown-short-live-handoff-scheduler",
        "quant-core-execution-worker",
        "quant-core-execution-confirmation-worker",
        "quant-core-execution-report-replay-worker",
    ] {
        let service_block = compose_service_block(&compose, service);
        assert!(
            service_block.contains(r#""host.docker.internal:host-gateway""#),
            "default deployed service `{service}` must keep host.docker.internal available for host-reachable dependencies"
        );
        assert!(
            service_block.contains("networks:")
                && service_block.contains("- default")
                && service_block.contains("- quant-core-external"),
            "default deployed service `{service}` must join both the Core compose network and the external app network"
        );
    }
    let execution_worker = compose_service_block(&compose, "quant-core-execution-worker");
    assert!(
        execution_worker.contains(
            "EXECUTION_WORKER_TASK_TYPES: ${EXECUTION_WORKER_TASK_TYPES:-execute_signal,risk_control_close_candidate}"
        ),
        "normal execution worker must lease the task type emitted for risk-control closes"
    );
    let confirmation_worker =
        compose_service_block(&compose, "quant-core-execution-confirmation-worker");
    assert!(
        confirmation_worker.contains(r#"EXECUTION_WORKER_CONFIRMATION_MODE: "true""#)
            && confirmation_worker.contains(r#"EXECUTION_WORKER_REPORT_REPLAY_MODE: "false""#),
        "production must keep a dedicated confirmation worker for pending exchange orders"
    );
    let replay_worker =
        compose_service_block(&compose, "quant-core-execution-report-replay-worker");
    assert!(
        replay_worker.contains(r#"EXECUTION_WORKER_CONFIRMATION_MODE: "false""#)
            && replay_worker.contains(r#"EXECUTION_WORKER_REPORT_REPLAY_MODE: "true""#),
        "production must keep a dedicated report replay worker for failed Web writebacks"
    );
    assert!(
        !dockerfile.contains("--bins"),
        "runtime image must not compile research and backtest binaries that are absent from the final image"
    );
    let copied_production_binaries = dockerfile.lines().filter_map(|line| {
        line.trim()
            .strip_prefix("COPY --from=builder /app/rust_quant/bin/")
            .and_then(|paths| paths.split_whitespace().next())
    });
    for production_binary in copied_production_binaries {
        assert!(
            dockerfile.contains(&format!("--bin {production_binary}")),
            "runtime image must compile production binary `{production_binary}` explicitly"
        );
    }
    for read_only_evidence in [
        "org.opencontainers.image.revision",
        "docker inspect",
        "docker logs --since",
        "execution_worker_checkpoints",
        "VERIFICATION=PASS",
    ] {
        assert!(
            verify.contains(read_only_evidence),
            "production verifier must collect read-only evidence `{read_only_evidence}`"
        );
    }
    for forbidden_mutation in [
        "docker rm",
        "docker restart",
        "docker compose up",
        "curl -X",
    ] {
        assert!(
            !verify.contains(forbidden_mutation),
            "production verifier must stay read-only and exclude `{forbidden_mutation}`"
        );
    }
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/bin/market_velocity_candle_backfill /usr/local/bin/market_velocity_candle_backfill"
        ),
        "runtime image must include the Rust-native Market Velocity candle backfill binary"
    );
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/bin/market_velocity_kline_scanner /usr/local/bin/market_velocity_kline_scanner"
        ),
        "runtime image must include the Rust-native Market Velocity 15m K-line scanner binary"
    );
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/bin/market_velocity_event_backtest /usr/local/bin/market_velocity_event_backtest"
        ),
        "runtime image must include the read-only Market Velocity event backtest binary for production audit"
    );
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/bin/market_velocity_live_handoff /usr/local/bin/market_velocity_live_handoff"
        ),
        "runtime image must include the Rust-native Market Velocity live handoff binary"
    );
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/bin/quant_core_schema_ensure /usr/local/bin/quant_core_schema_ensure"
        ),
        "runtime image must include the Rust-native quant_core schema ensure binary"
    );
    let schema_ensure_block = compose_service_block(&compose, "quant-core-schema-ensure");
    assert!(
        schema_ensure_block.contains("quant_core_schema_ensure"),
        "schema ensure service must run the Rust-native schema ensure binary"
    );
    assert!(
        schema_ensure_block.contains("QUANT_CORE_DATABASE_URL: ${QUANT_CORE_DATABASE_URL:?QUANT_CORE_DATABASE_URL is required}")
            && schema_ensure_block.contains("DATABASE_URL: ${QUANT_CORE_DATABASE_URL:?QUANT_CORE_DATABASE_URL is required}"),
        "schema ensure service must target the quant_core database explicitly"
    );
    assert!(
        schema_ensure_block.contains("networks:")
            && schema_ensure_block.contains("- default")
            && schema_ensure_block.contains("- quant-core-external"),
        "schema ensure service must join the same networks as deployed Core services"
    );
    let internal_server_block = compose_service_block(&compose, "quant-core-internal-server");
    assert!(
        internal_server_block.contains(
            "RUST_QUAN_WEB_BASE_URL: ${RUST_QUAN_WEB_BASE_URL:?RUST_QUAN_WEB_BASE_URL is required}"
        ) && internal_server_block.contains(
            "EXECUTION_EVENT_SECRET: ${EXECUTION_EVENT_SECRET:?EXECUTION_EVENT_SECRET is required}"
        ),
        "Core internal server must be able to call Quant Web internal writeback APIs"
    );
    let exchange_symbol_sync_block =
        compose_service_block(&compose, "quant-core-exchange-symbol-sync-worker");
    assert!(
        exchange_symbol_sync_block.contains("REDIS_HOST: ${REDIS_HOST:-redis://redis:6379/}"),
        "exchange symbol sync worker must use the deployed Redis endpoint instead of falling back to localhost"
    );
    for service in [
        "quant-core-market-velocity-live-handoff",
        "quant-core-market-velocity-live-handoff-scheduler",
    ] {
        let service_block = compose_service_block(&compose, service);
        for required_live_default in [
            "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-0}",
            "MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK:-none}",
            "MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT:-0.0}",
            "MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT:-none}",
            "MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION: ${MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION:-false}",
            "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.04}",
            "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-0.52}",
            "MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS: ${MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS:-24}",
            "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET:-research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1}",
            "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1}",
            "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-14.0}",
            "MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO: ${MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO:-1.3}",
            "MARKET_VELOCITY_ENTRY_MIN_RSI: ${MARKET_VELOCITY_ENTRY_MIN_RSI:-50.0}",
            "MARKET_VELOCITY_ENTRY_MAX_RSI: ${MARKET_VELOCITY_ENTRY_MAX_RSI:-90.0}",
            "MARKET_VELOCITY_ENTRY_BOLLINGER_BREAKOUT: ${MARKET_VELOCITY_ENTRY_BOLLINGER_BREAKOUT:-true}",
            "MARKET_VELOCITY_ENTRY_MIN_RECENT_DRAWDOWN_PCT: ${MARKET_VELOCITY_ENTRY_MIN_RECENT_DRAWDOWN_PCT:-3.5}",
            "MARKET_VELOCITY_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES: ${MARKET_VELOCITY_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES:-12}",
            "MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE: ${MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE:-m15_impulse_retrace}",
            "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT: ${MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT:-50.0}",
            "MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST: ${MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST:-breakout_previous_high}",
        ] {
            assert!(
                service_block.contains(required_live_default),
                "`{service}` must default to the promoted 15m kline Market Velocity live preset field `{required_live_default}`"
            );
        }
        assert!(
            !service_block
                .contains("momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1")
                && !service_block
                    .contains("rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1"),
            "`{service}` must not keep the previous reclaim/MA live defaults"
        );
    }
    assert!(
        okx_websocket.contains("new_with_config(\n        &CONFIG.business_websocket_url,\n        None,")
            && !okx_websocket.contains("expect(\"未配置OKX_API_KEY\")"),
        "OKX candle websocket must use the business public endpoint without requiring global OKX private credentials"
    );
    assert!(
        workflow.contains("market_velocity_production_deploy_contract"),
        "CI verify must run the production deploy contract"
    );
    assert!(
        production_gate.contains(
            "Optional canary buyer email; leave empty to fan out by active Web subscriptions"
        ) && production_gate.contains(
            "Optional canary combo id; must be provided together with buyer_email"
        ),
        "production gate must make buyer/combo scope optional so Market Velocity can fan out by Web subscriptions"
    );
    assert!(
        !production_gate.contains("MARKET_VELOCITY_LIVE_BUYER_EMAIL:?buyer_email is required")
            && !production_gate.contains("MARKET_VELOCITY_LIVE_COMBO_ID:?combo_id is required"),
        "production gate must not require a single buyer/combo for the default fan-out path"
    );
    assert!(
        production_gate.contains("buyer_email and combo_id must be provided together")
            && production_gate.contains("Web fan-out resolves credentials per subscription"),
        "production gate must keep canary scope explicit and leave credentials to Web fan-out when unscoped"
    );
    let default_deploy_services = "quant-core-internal-server,quant-core-exchange-symbol-sync-worker,quant-core-vegas-eth-4h-worker,quant-core-vegas-universal-4h-worker,quant-core-market-velocity-radar,quant-core-market-velocity-candle-backfill-scheduler,quant-core-market-velocity-kline-scanner-scheduler,quant-core-market-velocity-paper-observation-scheduler,quant-core-market-velocity-kline15m-paper-observation-scheduler,quant-core-market-velocity-breakdown-short-paper-observation-scheduler,quant-core-market-velocity-live-handoff-scheduler,quant-core-market-velocity-breakdown-short-live-handoff-scheduler,quant-core-execution-worker,quant-core-execution-confirmation-worker,quant-core-execution-report-replay-worker";
    for deploy_script in [&promote, &rollback] {
        assert!(
            deploy_script.contains(default_deploy_services),
            "default Core deployment must run the live handoff scheduler so Market Velocity reaches the production handoff node"
        );
        assert!(
            deploy_script.contains("require_internal_server_deploy_service")
                && deploy_script.contains(
                    "DEPLOY_SERVICES must include quant-core-internal-server"
                ),
            "default deploy/rollback must fail fast if DEPLOY_SERVICES omits the Web-facing Core internal server"
        );
        assert!(
            deploy_script.contains("--profile observation-scheduler")
                && deploy_script.contains("--profile breakdown-short-paper-observation-scheduler")
                && deploy_script.contains("--profile live-handoff-scheduler")
                && deploy_script.contains("--profile breakdown-short-live-handoff-scheduler")
                && deploy_script.contains("--profile candle-backfill-scheduler")
                && deploy_script.contains("--profile kline-scanner-scheduler")
                && deploy_script.contains("--profile schema-ensure"),
            "default deploy/rollback must enable required production profiles explicitly"
        );
        assert!(
            deploy_script.contains("DEPLOY_COMPOSE_SOURCE_FILE"),
            "default deploy/rollback must upload the current repository compose file instead of trusting a stale remote copy"
        );
        assert!(
            deploy_script.contains("scp -P"),
            "default deploy/rollback must sync the current compose file to the production host"
        );
        assert!(
            deploy_script.contains("DEPLOY_COMPOSE_PROJECT_NAME"),
            "default deploy/rollback must allow the production compose project name to be pinned"
        );
        assert!(
            deploy_script.contains("--project-directory"),
            "default deploy/rollback must keep the compose project directory at SERVER_APP_PATH after uploading compose into .deploy"
        );
        assert!(
            deploy_script.contains("--project-name"),
            "default deploy/rollback must keep the compose project name stable after uploading compose into .deploy"
        );
        assert!(
            deploy_script.contains("ps --all -q"),
            "default deploy/rollback must inspect all containers so exited profile services produce useful diagnostics"
        );
        assert!(
            deploy_script.contains("logs --tail=120"),
            "default deploy/rollback must print service logs when a deployed service is not running"
        );
        assert!(
            deploy_script.contains(".State.Restarting")
                && deploy_script.contains(".RestartCount")
                && deploy_script.contains("DEPLOY_HEALTH_STABLE_SECS"),
            "default deploy/rollback must treat restarting containers and restart-count spikes as failed readiness"
        );
        assert!(
            deploy_script.contains("print_runtime_safety_flags"),
            "default deploy/rollback must print non-secret runtime safety flags after stable readiness"
        );
        assert!(
            deploy_script.contains("up -d --no-build --pull never"),
            "default deploy/rollback must not re-pull images after stale containers are removed"
        );
        for safety_flag in [
            "MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH",
            "LEGACY_DIRECT_LIVE_ORDER_CONFIRM",
            "LEGACY_SIGNED_READ_ONLY_CONFIRM",
            "RISK_BALANCE_LIVE_MUTATION_CONFIRM",
            "PROTECTIVE_OUTCOME_CONFIRM",
        ] {
            assert!(
                deploy_script.contains(safety_flag),
                "default deploy/rollback must include runtime safety flag `{safety_flag}` in diagnostics"
            );
        }
        for removed_flag in [
            "MARKET_VELOCITY_CREATE_TASK_APPLY",
            "MARKET_VELOCITY_CREATE_TASK_CONFIRM",
            "MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY",
            "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM",
            "MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED",
            "MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED",
            "EXECUTION_WORKER_DRY_RUN",
            "EXECUTION_WORKER_ALLOW_GLOBAL_LIVE_SCOPE",
            "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
            "EXECUTION_WORKER_RECONCILIATION_ONLY",
        ] {
            assert!(
                !deploy_script.contains(removed_flag),
                "default deploy/rollback must not keep removed runtime switch `{removed_flag}`"
            );
        }
        assert!(
            deploy_script.contains("assert_no_persistent_live_mutation_env_flags")
                && deploy_script.contains(".env")
                && deploy_script.contains("refusing deployment with persistent live mutation flag"),
            "default deploy/rollback must fail fast when persistent live mutation flags are present in env or .env"
        );
        assert!(
            deploy_script.contains("assert_no_pinned_redis_host_env")
                && deploy_script.contains(".env.deploy")
                && deploy_script.contains("refusing deployment with pinned Redis container IP"),
            "default deploy/rollback must fail fast when REDIS_HOST is pinned to a disposable container IP instead of Docker DNS"
        );
        assert!(
            deploy_script.contains("assert_no_legacy_market_velocity_dispatch_mode_override")
                && deploy_script.contains("MARKET_VELOCITY_SIGNAL_DISPATCH_MODE")
                && deploy_script.contains(".env.deploy")
                && deploy_script.contains("hybrid live handoff owns signal emission"),
            "default deploy/rollback must fail fast when persistent env files pin Market Velocity back to legacy direct Web dispatch"
        );
        assert!(
            deploy_script
                .rfind("assert_no_persistent_live_mutation_env_flags")
                .expect("live mutation env guard must be called")
                < deploy_script
                    .find("compose -f \"${override_file}\" up -d --no-build")
                    .expect("deploy script starts long-running services"),
            "default deploy/rollback must check persistent live mutation flags before starting services"
        );
        assert!(
            deploy_script
                .rfind("assert_no_legacy_market_velocity_dispatch_mode_override")
                .expect("market velocity dispatch mode guard must be called")
                < deploy_script
                    .find("compose -f \"${override_file}\" up -d --no-build")
                    .expect("deploy script starts long-running services"),
            "default deploy/rollback must reject legacy Market Velocity direct dispatch overrides before starting services"
        );
        assert!(
            deploy_script.contains("remove_conflicting_named_containers"),
            "default deploy/rollback must remove stale fixed-name containers left by failed deployments"
        );
        assert!(
            deploy_script.contains(r#"--filter "name=^/${service}$""#),
            "stale container cleanup must target exact service container names only"
        );
        assert!(
            !deploy_script.contains("--remove-orphans"),
            "default deploy/rollback must not use broad compose orphan cleanup because shared dependencies such as Redis can live outside the current Core deploy compose"
        );
        assert!(
            deploy_script.contains(
                "retired_services_csv=\"${DEPLOY_RETIRED_SERVICES:-quant-core-vegas-eth-4h-live}\""
            ) && deploy_script.contains("remove_retired_deployment_containers"),
            "default deploy/rollback must explicitly remove retired live worker containers without deleting unrelated compose services"
        );
    }
    assert!(
        !compose.contains("quant-core-vegas-eth-4h-live")
            && !compose.contains("VEGAS_QUANT_CORE_IMAGE")
            && compose.contains("quant-core-vegas-eth-4h-worker"),
        "production compose must not reintroduce the retired legacy Vegas live service; Vegas signals must flow through Web execution tasks and the unified execution worker"
    );
    assert!(
        promote.contains("run_schema_ensure")
            && promote.contains("quant-core-schema-ensure")
            && promote.find("run_schema_ensure").expect("schema ensure helper exists")
                < promote
                    .find("compose -f \"${override_file}\" up -d --no-build")
                    .expect("deploy script starts long-running services"),
        "promote must run the Rust-native schema ensure service before starting long-running workers"
    );
    assert!(
        promote.contains("run --rm --no-deps -T"),
        "schema ensure compose run must disable TTY in CI"
    );
    assert!(
        promote.contains("</dev/null"),
        "schema ensure compose run must close stdin so it cannot consume the remote deployment heredoc"
    );
    assert!(
        !workflow.contains("market_velocity_okx_task_creation_handoff_contract"),
        "production CI must not validate shell handoff contracts for Market Velocity"
    );
    assert!(
        !workflow.contains("market_velocity_okx_live_preflight_contract"),
        "production CI must not validate shell live preflight contracts for Market Velocity"
    );
    assert!(
        !workflow.contains("market_velocity_okx_scoped_live_worker_contract"),
        "production CI must not validate shell live worker contracts for Market Velocity"
    );
    assert!(
        !workflow
            .contains("cargo test -p rust-quant-services market_velocity_signal -- --nocapture"),
        "production CI must scope Market Velocity signal verification to Rust lib tests"
    );
    for rust_native_contract in [
        "cargo test -p rust-quant-cli market_velocity_live_handoff --lib -- --nocapture",
        "cargo check -p rust-quant-cli --bin market_velocity_live_handoff",
        "cargo check -p rust-quant-cli --bin quant_core_schema_ensure",
        "cargo test -p rust-quant-cli market_velocity_backfill --lib -- --nocapture",
        "cargo check -p rust-quant-cli --bin market_velocity_candle_backfill",
        "cargo test -p rust-quant-cli market_velocity_kline_scanner --lib -- --nocapture",
        "cargo check -p rust-quant-cli --bin market_velocity_kline_scanner",
        "cargo check -p rust-quant-cli --bin market_velocity_event_backtest",
        "cargo test -p rust-quant-services market_velocity_signal --lib -- --nocapture",
        "cargo test -p rust-quant-services strategy_signal --lib -- --nocapture",
        "cargo test -p rust-quant-services target_task --lib -- --nocapture",
    ] {
        assert!(
            workflow.contains(rust_native_contract),
            "production CI must run Rust-native contract `{rust_native_contract}`"
        );
    }
}
#[test]
fn market_velocity_live_signal_defaults_use_promoted_kline15m_preset() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    for required in [
        "MARKET_VELOCITY_SIGNAL_DISPATCH_MODE: ${MARKET_VELOCITY_SIGNAL_DISPATCH_MODE:-disabled}",
        "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-0}",
        "MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK:-none}",
        "MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT:-0.0}",
        "MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT:-none}",
        "MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION: ${MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION:-false}",
        "MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT:-0.0}",
        "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.04}",
        "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-0.52}",
        "MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS: ${MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS:-24}",
        "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET:-research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1}",
        "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-14.0}",
        "MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO: ${MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO:-1.3}",
        "MARKET_VELOCITY_ENTRY_MIN_RSI: ${MARKET_VELOCITY_ENTRY_MIN_RSI:-50.0}",
        "MARKET_VELOCITY_ENTRY_MAX_RSI: ${MARKET_VELOCITY_ENTRY_MAX_RSI:-90.0}",
        "MARKET_VELOCITY_ENTRY_BOLLINGER_BREAKOUT: ${MARKET_VELOCITY_ENTRY_BOLLINGER_BREAKOUT:-true}",
        "MARKET_VELOCITY_ENTRY_MIN_RECENT_DRAWDOWN_PCT: ${MARKET_VELOCITY_ENTRY_MIN_RECENT_DRAWDOWN_PCT:-3.5}",
        "MARKET_VELOCITY_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES: ${MARKET_VELOCITY_ENTRY_RECENT_DRAWDOWN_LOOKBACK_CANDLES:-12}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_MAX_SIGNAL_PULLBACK_PCT: ${MARKET_VELOCITY_SIGNAL_ENTRY_MAX_SIGNAL_PULLBACK_PCT:-}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_TOLERANCE_PCT: ${MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_TOLERANCE_PCT:-0.3}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_AFTER_SIGNAL: ${MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_AFTER_SIGNAL:-false}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_MAX_WAIT_CANDLES: ${MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_MAX_WAIT_CANDLES:-8}",
        "MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE: ${MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE:-m15_impulse_retrace}",
        "MARKET_VELOCITY_SIGNAL_FVG_LOOKBACK_CANDLES: ${MARKET_VELOCITY_SIGNAL_FVG_LOOKBACK_CANDLES:-40}",
        "MARKET_VELOCITY_SIGNAL_FVG_MAX_WAIT_CANDLES: ${MARKET_VELOCITY_SIGNAL_FVG_MAX_WAIT_CANDLES:-24}",
        "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT: ${MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT:-50.0}",
        "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_MIN_WAIT_CANDLES: ${MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_MIN_WAIT_CANDLES:-0}",
        "MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST: ${MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST:-breakout_previous_high}",
        "MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH: ${MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH:-true}",
        "MARKET_VELOCITY_ENTRY_CANDLE_OKX_REST_BASE: ${MARKET_VELOCITY_ENTRY_CANDLE_OKX_REST_BASE:-https://www.okx.com}",
        "MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS: ${MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS:-0}",
    ] {
        assert!(
            compose.contains(required),
            "deploy compose must contain `{required}`"
        );
    }
    for removed in [
        "MARKET_VELOCITY_SIGNAL_AUTOMATION_MODE",
        "MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED",
        "MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED",
        "MARKET_VELOCITY_CREATE_TASK_APPLY",
        "MARKET_VELOCITY_CREATE_TASK_CONFIRM",
        "MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY",
        "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM",
        "EXECUTION_WORKER_DRY_RUN",
        "EXECUTION_WORKER_TARGET_TASK_IDS",
        "EXECUTION_WORKER_ALLOW_GLOBAL_LIVE_SCOPE",
        "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
        "EXECUTION_WORKER_RECONCILIATION_ONLY",
    ] {
        assert!(
            !compose.contains(removed),
            "deploy compose must not keep removed runtime switch `{removed}`"
        );
    }
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_DISPATCH_MODE: ${MARKET_VELOCITY_SIGNAL_DISPATCH_MODE:-web}"
        ),
        "deploy compose must not let the radar bypass the hybrid live handoff shell with direct Web signal dispatch"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.02}"
        ),
        "deploy compose must not keep the old 2% stop default for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.025}"
        ),
        "deploy compose must not keep the old 2.5% stop default for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET:-momentum_03sl_20r_v5}"
        ),
        "deploy compose must not keep the legacy momentum live preset after stable promotion"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET:-momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1}"
        ),
        "deploy compose must not keep the previous reclaim/MA live preset after kline15m promotion"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-rank_radar_4h_trend_15m_momentum_03sl_20r_v5}"
        ),
        "deploy compose must not keep the legacy momentum live entry rule after hybrid promotion"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-rank_radar_4h15m_mom0375_17r_rcm_ma_pb_d18_42_p5_10_v1}"
        ),
        "deploy compose must not keep the previous reclaim/MA live entry rule after kline15m promotion"
    );
    assert!(
        !compose.contains("MARKET_VELOCITY_SYMBOL_BLOCKLIST: ${MARKET_VELOCITY_SYMBOL_BLOCKLIST:-"),
        "deploy compose must not default to a historical symbol blocklist"
    );
    assert!(
        !compose.contains("stop_reentry_03sl_30r_v3"),
        "deploy compose must not keep the overfit symbol-blocklist tuning preset"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-2.4}"
        ),
        "deploy compose must not keep the old 2.4R take-profit default for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-1.8}"
        ),
        "deploy compose must not keep the old 1.8R take-profit default for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-1.7}"
        ),
        "deploy compose must not keep the previous 1.7R take-profit default for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-1.5}"
        ),
        "deploy compose must not keep the overly narrow 1.5% entry-distance filter for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-3.0}"
        ),
        "deploy compose must not keep the pre-hybrid 3% entry-distance filter"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-10}"
        ),
        "deploy compose must not keep the weaker Market Velocity rank delta gate"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-15}"
        ),
        "deploy compose must not keep the older momentum rank delta gate"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-20}"
        ),
        "deploy compose must not keep the older hybrid rank delta gate"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT:-4.0}"
        ),
        "deploy compose must not revert to the low-trade 4h distance gate"
    );
    assert!(
        !compose.contains("MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK"),
        "deploy compose must not expose new_rank as a Market Velocity live signal parameter"
    );
}

#[test]
fn market_velocity_runtime_strategy_loader_prefers_promoted_preset_over_legacy_default_row() {
    let source = read_repo_file("crates/rust-quant-cli/src/app/market_velocity_strategy_config.rs");
    assert!(
        source.contains("preferred_market_velocity_signal_strategy_preset"),
        "runtime strategy loader must derive a promoted preset selector before falling back to legacy default rows"
    );
    assert!(
        source.contains("select_default_market_velocity_signal_config_row"),
        "runtime strategy loader must choose the default row in Rust so the promoted preset can outrank an older version=default record"
    );
    assert!(
        !source.contains("CASE version WHEN 'default' THEN 0 ELSE 1 END"),
        "runtime strategy loader must not hard-prioritize version=default over the promoted hybrid preset"
    );
}

#[test]
fn market_velocity_paper_observation_defaults_use_stable_production_preset() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let observation_block =
        compose_service_block(&compose, "quant-core-market-velocity-paper-observation");
    let scheduler_block = compose_service_block(
        &compose,
        "quant-core-market-velocity-paper-observation-scheduler",
    );
    let expected_preset = "momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1";

    for service_block in [&observation_block, &scheduler_block] {
        assert!(
            service_block.contains("market_velocity_paper_observation"),
            "paper observation services must keep the Rust-native observation entrypoint"
        );
        assert!(
            service_block.contains("--paper-strategy-preset")
                && service_block.contains(expected_preset),
            "paper observation services must default to the stable reclaim/MA/pullback preset"
        );
        assert!(
            !service_block.contains("momentum_03sl_20r_v5"),
            "paper observation services must not keep the legacy momentum_03sl_20r_v5 preset after promotion"
        );
    }
}

#[test]
fn market_velocity_kline15m_challenger_has_isolated_paper_scheduler() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let promote = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback = read_repo_file("scripts/deploy/rollback.sh");
    let service_name = "quant-core-market-velocity-kline15m-paper-observation-scheduler";
    let service_block = compose_service_block(&compose, service_name);

    assert!(
        service_block.contains("market_velocity_paper_observation"),
        "kline15m challenger must use the same observation-only binary"
    );
    assert!(
        service_block.contains("kline15m-paper-observation-scheduler"),
        "kline15m challenger must be behind an explicit opt-in compose profile"
    );
    assert!(
        service_block.contains("--paper-strategy-preset")
            && service_block
                .contains("research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1"),
        "kline15m challenger scheduler must run the 0.52R fvg50 preset"
    );
    assert!(
        service_block.contains("MARKET_VELOCITY_KLINE15M_PAPER_OBSERVATION_INTERVAL_SECS"),
        "kline15m challenger must have an independent observation interval knob"
    );
    for deploy_script in [promote, rollback] {
        assert!(
            deploy_script.contains(service_name)
                && deploy_script.contains("--profile kline15m-paper-observation-scheduler"),
            "kline15m paper scheduler must be managed by default deploys once enabled in production"
        );
    }
}

#[test]
fn market_velocity_breakdown_short_has_isolated_paper_scheduler_without_live_handoff() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let promote = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback = read_repo_file("scripts/deploy/rollback.sh");
    let service_name = "quant-core-market-velocity-breakdown-short-paper-observation-scheduler";
    let service_block = compose_service_block(&compose, service_name);

    assert!(
        service_block.contains("market_velocity_paper_observation"),
        "breakdown-short challenger must use the paper-only observation binary"
    );
    assert!(
        service_block.contains("breakdown-short-paper-observation-scheduler"),
        "breakdown-short challenger must be behind an explicit opt-in compose profile"
    );
    assert!(
        service_block.contains("--paper-strategy-preset")
            && service_block.contains(
                "research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6"
            ),
        "breakdown-short scheduler must run the paper-only short breakdown preset"
    );
    assert!(
        service_block.contains("MARKET_VELOCITY_BREAKDOWN_SHORT_PAPER_OBSERVATION_INTERVAL_SECS"),
        "breakdown-short challenger must have an independent observation interval knob"
    );
    assert!(
        service_block.contains("image: ${QUANT_CORE_CHALLENGER_IMAGE:-ghcr.io/fairwic/quant-core-worker:missing-breakdown-short-challenger-image}")
            && !service_block.contains("quant-core-worker:latest"),
        "breakdown-short paper scheduler must use an explicit challenger image or a failing sentinel instead of falling back to latest"
    );
    assert!(
        !service_block.contains("market_velocity_live_handoff")
            && !service_block.contains("MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED"),
        "breakdown-short paper scheduler must not enable live handoff or live-order flags"
    );
    for deploy_script in [promote, rollback] {
        assert!(
            deploy_script.contains(service_name)
                && deploy_script.contains("--profile breakdown-short-paper-observation-scheduler"),
            "breakdown-short paper scheduler must be managed by default deploys with its paper-only profile"
        );
    }
}

#[test]
fn market_velocity_breakdown_short_has_isolated_live_handoff_scheduler() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let promote = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback = read_repo_file("scripts/deploy/rollback.sh");
    let service_name = "quant-core-market-velocity-breakdown-short-live-handoff-scheduler";
    let service_block = compose_service_block(&compose, service_name);

    assert!(
        service_block.contains("market_velocity_live_handoff"),
        "breakdown-short live scheduler must use the live handoff binary"
    );
    assert!(
        service_block.contains("breakdown-short-live-handoff-scheduler"),
        "breakdown-short live scheduler must be behind its own compose profile"
    );
    assert!(
        service_block.contains("image: ${QUANT_CORE_IMAGE:-ghcr.io/fairwic/quant-core-worker:latest}")
            && !service_block.contains("QUANT_CORE_CHALLENGER_IMAGE"),
        "breakdown-short live scheduler must deploy the stable Core image, not the challenger image"
    );
    for required in [
        r#"MARKET_VELOCITY_LIVE_HANDOFF_RUN_ONCE: "false""#,
        "MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS: ${MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_HANDOFF_INTERVAL_SECS:-5}",
        "MARKET_VELOCITY_LIVE_HANDOFF_SIGNAL_TTL_MS: ${MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_HANDOFF_SIGNAL_TTL_MS:-10000}",
        "MARKET_VELOCITY_LIVE_BUYER_EMAIL: ${MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_BUYER_EMAIL:-}",
        "MARKET_VELOCITY_LIVE_COMBO_ID: ${MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_COMBO_ID:-}",
        "MARKET_VELOCITY_TASK_READINESS_CREDENTIAL_ID: ${MARKET_VELOCITY_BREAKDOWN_SHORT_TASK_READINESS_CREDENTIAL_ID:-}",
        "MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_LOOKBACK_HOURS:-24}",
        "MARKET_VELOCITY_LIVE_CANDIDATE_LIMIT: ${MARKET_VELOCITY_BREAKDOWN_SHORT_LIVE_CANDIDATE_LIMIT:-100}",
        "MARKET_VELOCITY_STRATEGY_SLUG: ${MARKET_VELOCITY_BREAKDOWN_SHORT_STRATEGY_SLUG:-market_velocity_breakdown_short}",
        "MARKET_VELOCITY_SIGNAL_TRADE_DIRECTION: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_TRADE_DIRECTION:-short}",
        "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_STRATEGY_PRESET:-research_momentum_short_04sl_10r_15m_support_breakdown_d5_100_pchg2_12_vol10_dist14_v6}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_ENTRY_RULE_VERSION:-rank_radar_15m_short_r04_10r_15msup_brkdn_d5_100_p2_12_vol10_d14_v6}",
        "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_MIN_DELTA_RANK:-5}",
        "MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_MAX_DELTA_RANK:-100}",
        "MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_MIN_PRICE_CHANGE_PCT:-2.0}",
        "MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_MAX_PRICE_CHANGE_PCT:-12.0}",
        "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_STOP_LOSS_PCT:-0.04}",
        "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_TAKE_PROFIT_R:-1.0}",
        "MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_MAX_HOLDING_HOURS:-24}",
        "MARKET_VELOCITY_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_REQUIRE_TECHNICAL_CONFIRMATION:-false}",
        "MARKET_VELOCITY_SIGNAL_REQUIRE_ENTRY_CONFIRMATION: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_REQUIRE_ENTRY_CONFIRMATION:-false}",
        "MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_BREAKDOWN_SHORT_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT:-0.0}",
        "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_BREAKDOWN_SHORT_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-14.0}",
        "MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO: ${MARKET_VELOCITY_BREAKDOWN_SHORT_ENTRY_MIN_VOLUME_RATIO:-1.0}",
        "MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST: ${MARKET_VELOCITY_BREAKDOWN_SHORT_ENTRY_TRIGGER_ALLOWLIST:-breakdown_range_low}",
        "MARKET_VELOCITY_ENTRY_CANDLE_MAX_STALENESS_MINUTES: ${MARKET_VELOCITY_BREAKDOWN_SHORT_ENTRY_CANDLE_MAX_STALENESS_MINUTES:-45}",
        "MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH: ${MARKET_VELOCITY_BREAKDOWN_SHORT_ENTRY_CANDLE_ON_DEMAND_REFRESH:-true}",
        "MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS: ${MARKET_VELOCITY_BREAKDOWN_SHORT_ENTRY_CANDLE_REQUEST_SLEEP_MS:-0}",
    ] {
        assert!(
            service_block.contains(required),
            "breakdown-short live scheduler must default exact v6 live field `{required}`"
        );
    }
    for forbidden in [
        "MARKET_VELOCITY_SIGNAL_DISPATCH_MODE",
        "MARKET_VELOCITY_SIGNAL_AUTOMATION_MODE",
        "MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED",
        "MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED",
        "MARKET_VELOCITY_ENTRY_MIN_RSI",
        "MARKET_VELOCITY_ENTRY_MAX_RSI",
        "MARKET_VELOCITY_ENTRY_BOLLINGER_BREAKOUT",
        "MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE",
    ] {
        assert!(
            !service_block.contains(forbidden),
            "breakdown-short live scheduler must not inherit unrelated live switch/filter `{forbidden}`"
        );
    }
    for deploy_script in [promote, rollback] {
        assert!(
            deploy_script.contains(service_name)
                && deploy_script.contains("--profile breakdown-short-live-handoff-scheduler"),
            "breakdown-short live scheduler must be managed by default deploys with its own profile"
        );
    }
}
