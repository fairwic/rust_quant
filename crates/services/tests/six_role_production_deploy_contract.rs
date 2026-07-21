use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_RUNTIME_SERVICES: [&str; 6] = [
    "quant-core-control-api",
    "quant-core-market-worker",
    "quant-core-signal-worker",
    "quant-core-account-worker",
    "quant-core-execution-worker",
    "quant-core-reconciliation-worker",
];

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

fn default_compose_services(compose: &str) -> Vec<String> {
    let mut services = Vec::new();
    let mut in_services = false;
    for line in compose.lines() {
        if line == "services:" {
            in_services = true;
            continue;
        }
        if in_services && !line.starts_with(' ') && !line.is_empty() {
            break;
        }
        if !in_services || !line.starts_with("  ") || line.starts_with("    ") {
            continue;
        }
        let service = line.trim().trim_end_matches(':');
        let block = compose_service_block(compose, service);
        if !block.contains("profiles:") {
            services.push(service.to_string());
        }
    }
    services
}

#[test]
fn production_runtime_defaults_to_six_supervised_roles() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let mut actual = default_compose_services(&compose);
    let mut expected = DEFAULT_RUNTIME_SERVICES.map(str::to_string).to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);

    let expected_commands = [
        ("quant-core-control-api", "quant_core_control_api"),
        ("quant-core-market-worker", "quant_core_market_worker"),
        ("quant-core-signal-worker", "quant_core_signal_worker"),
        ("quant-core-account-worker", "quant_core_account_worker"),
        ("quant-core-execution-worker", "quant_core_execution_worker"),
        (
            "quant-core-reconciliation-worker",
            "quant_core_reconciliation_worker",
        ),
    ];
    for (service, command) in expected_commands {
        let block = compose_service_block(&compose, service);
        assert!(block.contains(command), "{service} must run {command}");
        assert!(
            block.contains("restart: unless-stopped")
                && block.contains("stop_grace_period: 30s")
                && block.contains("healthcheck:"),
            "{service} must expose a stable process-level lifecycle contract"
        );
    }
}

#[test]
fn market_and_signal_roles_keep_capabilities_and_strategy_scopes_separate() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let market = compose_service_block(&compose, "quant-core-market-worker");
    for required in [
        "MARKET_VELOCITY_SIGNAL_DISPATCH_MODE: disabled",
        r#"EXCHANGE_LISTING_SIGNAL_SUBMIT: "0""#,
        "MARKET_WORKER_KLINE_SCAN_INTERVAL_SECS",
        "MARKET_WORKER_RECENT_REPAIR_INTERVAL_SECS",
        "MARKET_WORKER_RECENT_REPAIR_MAX_SYMBOLS",
    ] {
        assert!(
            market.contains(required),
            "market-worker missing `{required}`"
        );
    }
    for forbidden in [
        "EXECUTION_EVENT_SECRET",
        "RUST_QUAN_WEB_BASE_URL",
        "OKX_API_KEY",
        "EXECUTION_WORKER_",
    ] {
        assert!(
            !market.contains(forbidden),
            "market-worker must not receive `{forbidden}` capability"
        );
    }

    let signal = compose_service_block(&compose, "quant-core-signal-worker");
    for required in [
        "LIVE_STRATEGY_ONLY_TYPES: vegas,vegas_universal_4h",
        "LIVE_STRATEGY_VEGAS_ONLY_INST_IDS",
        "LIVE_STRATEGY_VEGAS_UNIVERSAL_4H_ONLY_INST_IDS",
        "MARKET_VELOCITY_LIVE_HANDOFF_LANES",
        r#"MARKET_VELOCITY_LIVE_HANDOFF_RUN_ONCE: "false""#,
    ] {
        assert!(
            signal.contains(required),
            "signal-worker missing `{required}`"
        );
    }
    assert!(!signal.contains("EXECUTION_WORKER_"));
}

#[test]
fn legacy_runtime_and_research_jobs_are_not_default_services() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    for legacy in [
        "quant-core-internal-server",
        "quant-core-exchange-symbol-sync-worker",
        "quant-core-vegas-eth-4h-worker",
        "quant-core-vegas-universal-4h-worker",
        "quant-core-market-velocity-radar",
        "quant-core-all-market-candle-volume-monitor",
        "quant-core-execution-confirmation-worker",
        "quant-core-execution-report-replay-worker",
    ] {
        assert!(
            compose_service_block(&compose, legacy).contains("- legacy-runtime"),
            "legacy runtime `{legacy}` must be opt-in only"
        );
    }
    for job in [
        "quant-core-schema-ensure",
        "quant-core-market-velocity-candle-backfill-scheduler",
        "quant-core-strategy-4h-candle-backfill-scheduler",
        "quant-core-market-velocity-paper-observation-scheduler",
        "quant-core-market-velocity-kline15m-paper-observation-scheduler",
        "quant-core-market-velocity-breakdown-short-paper-observation-scheduler",
    ] {
        assert!(
            compose_service_block(&compose, job).contains("profiles:"),
            "job `{job}` must not be a default long-running service"
        );
    }
}

#[test]
fn image_deploy_and_verification_contract_use_the_same_six_roles() {
    let dockerfile = read_repo_file("Dockerfile.runtime");
    let promote_entry = read_repo_file("scripts/deploy/promote_stable.sh");
    let rollback_entry = read_repo_file("scripts/deploy/rollback.sh");
    let deploy_core = read_repo_file("scripts/deploy/deploy_core.sh");
    let deploy_remote = read_repo_file("scripts/deploy/deploy_core_remote.sh");
    let runtime_services = read_repo_file("scripts/deploy/runtime-services.txt");
    let verify = read_repo_file("scripts/deploy/verify_production.sh");
    let workflow = read_repo_file(".github/workflows/cicd.yml");
    let deploy_surface = format!("{deploy_core}\n{deploy_remote}");

    for service in DEFAULT_RUNTIME_SERVICES {
        assert!(verify.contains(service), "verifier missing `{service}`");
    }
    for binary in [
        "quant_core_control_api",
        "quant_core_market_worker",
        "quant_core_signal_worker",
        "quant_core_account_worker",
        "quant_core_execution_worker",
        "quant_core_reconciliation_worker",
    ] {
        assert!(dockerfile.contains(&format!("--bin {binary}")));
        assert!(
            dockerfile.contains(&format!(
                "COPY --from=builder /app/rust_quant/bin/{binary} /usr/local/bin/{binary}"
            )),
            "runtime image must copy `{binary}`"
        );
    }
    assert_eq!(
        runtime_services.lines().collect::<Vec<_>>(),
        DEFAULT_RUNTIME_SERVICES
    );
    assert!(promote_entry.contains("deploy_core.sh\" promote"));
    assert!(rollback_entry.contains("deploy_core.sh\" rollback"));
    assert!(promote_entry.lines().count() <= 10 && rollback_entry.lines().count() <= 10);
    assert!(deploy_core.contains("DEPLOY_SERVICES is no longer supported"));
    assert!(!workflow.contains("DEPLOY_SERVICES:"));
    assert!(deploy_surface.contains("require_control_api_deploy_service"));
    assert!(deploy_surface.contains("require_exact_six_role_services"));
    assert!(deploy_surface.contains("quant-core-market-velocity-live-handoff,"));
    assert!(deploy_surface.contains("assert_services_process_stable"));
    assert!(!deploy_surface.contains("failed readiness"));
    assert!(deploy_surface
        .contains("runtime-services.txt must contain exactly the six Core runtime roles"));
    assert!(deploy_surface.contains("runtime-services.txt must include quant-core-control-api"));
    assert!(!deploy_surface.contains("--profile observation-scheduler"));
    assert!(!deploy_surface.contains("--profile live-handoff-scheduler"));
    for cutover_contract in [
        "DEPLOY_SIX_ROLE_CUTOVER_CONFIRM",
        "replace-legacy-runtime-with-six-roles",
        "six-role-cutover.previous_topology",
        "six-role-cutover.rollback_to_legacy",
    ] {
        assert!(
            deploy_surface.contains(cutover_contract),
            "promotion must preserve one-time cutover guard `{cutover_contract}`"
        );
    }
    assert!(deploy_surface.contains("six-role-cutover.previous_topology"));
    assert!(deploy_surface.contains("six-role-cutover.rollback_to_legacy"));
    assert!(deploy_surface.contains("command: [rust_quant]"));
    assert!(deploy_surface.contains("DEPLOY_OBSOLETE_SERVICES:-quant-core-vegas-eth-4h-live"));
    assert!(workflow.contains("DEPLOY_SIX_ROLE_CUTOVER_CONFIRM"));
    assert!(workflow.contains("Verify production runtime"));
    assert!(workflow.contains("./scripts/deploy/verify_production.sh \"${{ github.sha }}\""));
    for evidence in [
        "org.opencontainers.image.revision",
        "docker inspect",
        "docker logs --since",
        "execution_worker_checkpoints",
        "VERIFICATION=PASS",
    ] {
        assert!(verify.contains(evidence));
    }
    for forbidden_mutation in [
        "docker rm",
        "docker restart",
        "docker compose up",
        "curl -X",
    ] {
        assert!(!verify.contains(forbidden_mutation));
    }
    assert!(workflow.contains("market_velocity_production_deploy_contract"));
    assert!(workflow.contains("six_role_production_deploy_contract"));
}

#[test]
fn deploy_core_rejects_dynamic_runtime_topology_before_ssh() {
    let output = Command::new("bash")
        .arg(repo_root().join("scripts/deploy/deploy_core.sh"))
        .arg("rollback")
        .env("DEPLOY_SSH_USER", "contract-user")
        .env("DEPLOY_SSH_HOST", "contract.invalid")
        .env("SERVER_APP_PATH", "/tmp/contract-app")
        .env("DEPLOY_SERVICES", "quant-core-control-api")
        .output()
        .expect("deploy contract should execute the local argument guard");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("DEPLOY_SERVICES is no longer supported"),
        "dynamic topology must fail before any SSH call"
    );
}
