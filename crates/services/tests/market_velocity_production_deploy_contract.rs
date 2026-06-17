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
    let promote = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback = read_repo_file("scripts/deploy/rollback.sh");

    for service in [
        "quant-core-schema-ensure:",
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
        "MARKET_VELOCITY_CREATE_TASK_APPLY: ${MARKET_VELOCITY_CREATE_TASK_APPLY:-false}",
        "MARKET_VELOCITY_CREATE_TASK_CONFIRM: ${MARKET_VELOCITY_CREATE_TASK_CONFIRM:-}",
        "MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY: ${MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY:-false}",
        "MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM: ${MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM:-}",
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
            "COPY --from=builder /app/rust_quant/target/release/market_velocity_candle_backfill /usr/local/bin/market_velocity_candle_backfill"
        ),
        "runtime image must include the Rust-native Market Velocity candle backfill binary"
    );
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/target/release/market_velocity_live_handoff /usr/local/bin/market_velocity_live_handoff"
        ),
        "runtime image must include the Rust-native Market Velocity live handoff binary"
    );
    assert!(
        dockerfile.contains(
            "COPY --from=builder /app/rust_quant/target/release/quant_core_schema_ensure /usr/local/bin/quant_core_schema_ensure"
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
    assert!(
        workflow.contains("market_velocity_production_deploy_contract"),
        "CI verify must run the production deploy contract"
    );
    let default_deploy_services = "quant-core-market-velocity-radar,quant-core-market-velocity-paper-observation-scheduler,quant-core-market-velocity-live-handoff-scheduler,quant-core-execution-worker";
    for deploy_script in [&promote, &rollback] {
        assert!(
            deploy_script.contains(default_deploy_services),
            "default Core deployment must run the live handoff scheduler so Market Velocity reaches the production handoff node"
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
            "MARKET_VELOCITY_CREATE_TASK_APPLY",
            "MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY",
            "MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED",
            "MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED",
            "EXECUTION_WORKER_DRY_RUN",
        ] {
            assert!(
                deploy_script.contains(safety_flag),
                "default deploy/rollback must include runtime safety flag `{safety_flag}` in diagnostics"
            );
        }
        assert!(
            deploy_script.contains("remove_conflicting_named_containers"),
            "default deploy/rollback must remove stale fixed-name containers left by failed deployments"
        );
        assert!(
            deploy_script.contains(r#"--filter "name=^/${service}$""#),
            "stale container cleanup must target exact service container names only"
        );
    }
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
        "cargo test -p rust-quant-services live_order_confirmation --lib -- --nocapture",
    ] {
        assert!(
            workflow.contains(rust_native_contract),
            "production CI must run Rust-native contract `{rust_native_contract}`"
        );
    }
}

#[test]
fn market_velocity_live_signal_defaults_use_production_stop_reentry_preset() {
    let compose = read_repo_file("docker-compose.deploy.yml");

    for required in [
        "MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-10}",
        "MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK: ${MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK:-30}",
        "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.025}",
        "MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R: ${MARKET_VELOCITY_SIGNAL_TAKE_PROFIT_R:-2.4}",
        "MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS: ${MARKET_VELOCITY_SIGNAL_MAX_HOLDING_HOURS:-48}",
        "MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET: ${MARKET_VELOCITY_SIGNAL_STRATEGY_PRESET:-stop_reentry_025sl_24r_v1}",
        "MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION: ${MARKET_VELOCITY_SIGNAL_ENTRY_RULE_VERSION:-rank_radar_4h_trend_15m_stop_reentry_025sl_24r_v1}",
        "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-1.5}",
        "MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH: ${MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH:-true}",
        "MARKET_VELOCITY_ENTRY_CANDLE_OKX_REST_BASE: ${MARKET_VELOCITY_ENTRY_CANDLE_OKX_REST_BASE:-https://www.okx.com}",
        "MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS: ${MARKET_VELOCITY_ENTRY_CANDLE_REQUEST_SLEEP_MS:-0}",
        "MARKET_VELOCITY_SIGNAL_AUTOMATION_MODE: ${MARKET_VELOCITY_SIGNAL_AUTOMATION_MODE:-signal_only}",
        "MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED: ${MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED:-false}",
        "MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED: ${MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED:-true}",
        "EXECUTION_WORKER_TARGET_TASK_IDS: ${EXECUTION_WORKER_TARGET_TASK_IDS:-}",
    ] {
        assert!(
            compose.contains(required),
            "deploy compose must contain `{required}`"
        );
    }

    assert!(
        !compose.contains(
            "MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT: ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.02}"
        ),
        "deploy compose must not keep the old 2% stop default for Market Velocity live signal"
    );
    assert!(
        !compose.contains(
            "MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT: ${MARKET_VELOCITY_ENTRY_MAX_AVERAGE_DISTANCE_PCT:-3.0}"
        ),
        "deploy compose must not keep the old 3% chase filter for Market Velocity live signal"
    );
    assert!(
        !compose.contains("MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK: ${MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK:-3}"),
        "deploy compose must not loosen Market Velocity live candidates below the backtested rank delta gate"
    );
    assert!(
        !compose.contains("MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK: ${MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK:-50}"),
        "deploy compose must not keep the old wider top-rank window for Market Velocity live signal"
    );
}
