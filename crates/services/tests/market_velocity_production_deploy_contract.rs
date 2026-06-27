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
    let dockerfile = read_repo_file("Dockerfile.runtime");
    let okx_websocket = read_repo_file("crates/market/src/streams/websocket_service.rs");
    let promote = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback = read_repo_file("scripts/deploy/rollback.sh");
    for service in [
        "quant-core-schema-ensure:",
        "quant-core-internal-server:",
        "quant-core-exchange-symbol-sync-worker:",
        "quant-core-vegas-eth-4h-worker:",
        "quant-core-market-velocity-radar:",
        "quant-core-market-velocity-candle-backfill-scheduler:",
        "quant-core-market-velocity-paper-observation-scheduler:",
        "quant-core-market-velocity-live-handoff:",
        "quant-core-market-velocity-live-handoff-scheduler:",
        "quant-core-execution-worker:",
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
        "LIVE_STRATEGY_ONLY_INST_IDS: ${VEGAS_ETH_4H_INST_ID:-ETH-USDT-SWAP}",
        "LIVE_STRATEGY_ONLY_PERIODS: ${VEGAS_ETH_4H_PERIOD:-4H}",
        "STRATEGY_SIGNAL_DISPATCH_MODE: ${VEGAS_STRATEGY_SIGNAL_DISPATCH_MODE:-web}",
        r#"IS_RUN_MARKET_VELOCITY_RADAR: "true""#,
        r#"MARKET_VELOCITY_RADAR_ONLY: "true""#,
        "market_velocity_candle_backfill",
        "--loop-interval-seconds",
        "MARKET_VELOCITY_CANDLE_BACKFILL_INTERVAL_SECS",
        r#"IS_RUN_EXECUTION_WORKER: "true""#,
        r#"EXECUTION_WORKER_ONLY: "true""#,
        "market_velocity_live_handoff",
        "EXECUTION_EVENT_SECRET: ${EXECUTION_EVENT_SECRET:?EXECUTION_EVENT_SECRET is required}",
        "MARKET_VELOCITY_LIVE_BUYER_EMAIL: ${MARKET_VELOCITY_LIVE_BUYER_EMAIL:-}",
        "MARKET_VELOCITY_LIVE_COMBO_ID: ${MARKET_VELOCITY_LIVE_COMBO_ID:-}",
        r#"MARKET_VELOCITY_LIVE_HANDOFF_RUN_ONCE: "false""#,
        "MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS: ${MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS:-60}",
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
        "quant-core-market-velocity-radar",
        "quant-core-market-velocity-paper-observation-scheduler",
        "quant-core-market-velocity-live-handoff-scheduler",
        "quant-core-execution-worker",
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
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/bin/market_velocity_candle_backfill /usr/local/bin/market_velocity_candle_backfill"
        ),
        "runtime image must include the Rust-native Market Velocity candle backfill binary"
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
    assert!(
        okx_websocket.contains("new_with_config(\n        &CONFIG.business_websocket_url,\n        None,")
            && !okx_websocket.contains("expect(\"未配置OKX_API_KEY\")"),
        "OKX candle websocket must use the business public endpoint without requiring global OKX private credentials"
    );
    assert!(
        workflow.contains("market_velocity_production_deploy_contract"),
        "CI verify must run the production deploy contract"
    );
    let default_deploy_services = "quant-core-internal-server,quant-core-exchange-symbol-sync-worker,quant-core-vegas-eth-4h-worker,quant-core-market-velocity-radar,quant-core-market-velocity-paper-observation-scheduler,quant-core-market-velocity-live-handoff-scheduler,quant-core-execution-worker";
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
            !deploy_script.contains(
                "quant-core-market-velocity-radar,quant-core-market-velocity-candle-backfill-scheduler"
            ),
            "default Core deployment must not reintroduce global candle backfill as a live prerequisite"
        );
        assert!(
            deploy_script.contains("--profile observation-scheduler")
                && deploy_script.contains("--profile live-handoff-scheduler")
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
fn market_velocity_live_signal_defaults_use_latest_hybrid_research_preset() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    for required in [
        "MARKET_VELOCITY_SIGNAL_DISPATCH_MODE: ${MARKET_VELOCITY_SIGNAL_DISPATCH_MODE:-disabled}",
        "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-20}",
        "MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MAX_DELTA_RANK:-40}",
        "MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_SIGNAL_MIN_PRICE_CHANGE_PCT:-5.0}",
        "MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT: ${MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT:-10.0}",
        "MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT:-0.0}",
        "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.04}",
        "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-1.8}",
        "MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS: ${MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS:-48}",
        "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET:-research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-rank_radar_4h15m_r04_18r_rcm_fvg_rt1_pb3_vol11_d20_40_p5_10_v2}",
        "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-5.0}",
        "MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO: ${MARKET_VELOCITY_ENTRY_MIN_VOLUME_RATIO:-1.1}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_MAX_SIGNAL_PULLBACK_PCT: ${MARKET_VELOCITY_SIGNAL_ENTRY_MAX_SIGNAL_PULLBACK_PCT:-3.0}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_TOLERANCE_PCT: ${MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_TOLERANCE_PCT:-0.3}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_AFTER_SIGNAL: ${MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_AFTER_SIGNAL:-true}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_MAX_WAIT_CANDLES: ${MARKET_VELOCITY_SIGNAL_ENTRY_RETEST_MAX_WAIT_CANDLES:-1}",
        "MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE: ${MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE:-m15_impulse_retrace}",
        "MARKET_VELOCITY_SIGNAL_FVG_LOOKBACK_CANDLES: ${MARKET_VELOCITY_SIGNAL_FVG_LOOKBACK_CANDLES:-40}",
        "MARKET_VELOCITY_SIGNAL_FVG_MAX_WAIT_CANDLES: ${MARKET_VELOCITY_SIGNAL_FVG_MAX_WAIT_CANDLES:-24}",
        "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT: ${MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_FILL_PCT:-20.0}",
        "MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_MIN_WAIT_CANDLES: ${MARKET_VELOCITY_SIGNAL_FVG_IMPULSE_RETRACE_MIN_WAIT_CANDLES:-0}",
        "MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST: ${MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST:-reclaim_ema}",
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
        "deploy compose must not keep the legacy momentum live preset after hybrid promotion"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-rank_radar_4h_trend_15m_momentum_03sl_20r_v5}"
        ),
        "deploy compose must not keep the legacy momentum live entry rule after hybrid promotion"
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
            "MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_SIGNAL_TREND_MIN_AVERAGE_DISTANCE_PCT:-4.0}"
        ),
        "deploy compose must not revert to the low-trade 4h distance gate"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE: ${MARKET_VELOCITY_SIGNAL_FVG_ENTRY_MODE:-off}"
        ),
        "deploy compose must not leave the live handoff runtime on the legacy non-FVG shell"
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
fn market_velocity_paper_observation_defaults_use_latest_hybrid_research_preset() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let observation_block =
        compose_service_block(&compose, "quant-core-market-velocity-paper-observation");
    let scheduler_block = compose_service_block(
        &compose,
        "quant-core-market-velocity-paper-observation-scheduler",
    );
    let expected_preset =
        "research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2";

    for service_block in [&observation_block, &scheduler_block] {
        assert!(
            service_block.contains("market_velocity_paper_observation"),
            "paper observation services must keep the Rust-native observation entrypoint"
        );
        assert!(
            service_block.contains("--paper-strategy-preset")
                && service_block.contains(expected_preset),
            "paper observation services must default to the latest verified hybrid reclaim FVG preset"
        );
        assert!(
            !service_block.contains("momentum_03sl_20r_v5"),
            "paper observation services must not keep the legacy momentum_03sl_20r_v5 preset after promotion"
        );
    }
}
