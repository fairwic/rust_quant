use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const LEGACY_SDK_REVISION: &str = "4d83997af344a3ffc1cc7e2444f1f8385e054dcd";
const PERSISTENCE_EXCLUSION: &str =
    "WEBSOCKET_CANDLE_PERSIST_EXCLUDED_TARGETS: ${LEGACY_CANDLE_PERSIST_EXCLUDED_TARGETS:-}";

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
fn every_legacy_eth_4h_websocket_writer_accepts_the_same_default_off_exclusion() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    for service in [
        "quant-core-signal-worker",
        "quant-core-vegas-eth-4h-worker",
        "quant-core-vegas-universal-4h-worker",
    ] {
        let block = compose_service_block(&compose, service);
        assert!(
            block.contains(PERSISTENCE_EXCLUSION),
            "{service} must accept the shared exact-target persistence exclusion"
        );
    }
}

#[test]
fn strategy_backfill_can_exclude_eth_without_disabling_other_4h_symbols() {
    let compose = read_repo_file("docker-compose.deploy.yml");
    let block = compose_service_block(&compose, "quant-core-strategy-4h-candle-backfill-scheduler");
    assert!(block.contains("--enabled-strategy-symbols"));
    assert!(block.contains("--exclude-symbols"));
    assert!(block.contains("${STRATEGY_4H_CANDLE_BACKFILL_EXCLUDED_SYMBOLS:-}"));
}

#[test]
fn legacy_ci_uses_one_exact_compatible_sdk_revision_for_verify_and_image_build() {
    let workflow = read_repo_file(".github/workflows/cicd.yml");
    let pinned_ref = format!("ref: {LEGACY_SDK_REVISION}");
    assert_eq!(
        workflow.matches(&pinned_ref).count(),
        2,
        "verify and image build must resolve the same legacy-compatible SDK"
    );
}

#[test]
fn scoped_handoff_deploys_only_the_current_three_legacy_writers_by_digest() {
    let workflow = read_repo_file(".github/workflows/eth-4h-market-handoff.yml");
    let local = read_repo_file("scripts/deploy/deploy_eth_4h_market_handoff.sh");
    let remote = read_repo_file("scripts/deploy/deploy_eth_4h_market_handoff_remote.sh");
    let core_workflow = read_repo_file(".github/workflows/cicd.yml");

    for required in [
        "prepare-legacy-eth-usdt-swap-4h-handoff",
        "verify-legacy-eth-usdt-swap-4h-handoff",
        "rollback-legacy-eth-usdt-swap-4h-before-ownership",
        "environment:",
        "name: production",
        "source_revision",
    ] {
        assert!(workflow.contains(required), "workflow missing `{required}`");
    }
    assert!(local.contains("deploy_eth_4h_market_handoff_remote.sh"));
    assert!(remote.contains("ETH-USDT-SWAP:4H"));
    assert!(remote.contains("STRATEGY_4H_CANDLE_BACKFILL_EXCLUDED_SYMBOLS=\"ETH-USDT-SWAP\""));
    assert!(remote.contains("quant-core-signal-worker unexpectedly exists"));
    assert!(remote.contains("@sha256:[0-9a-f]{64}"));
    assert!(remote.contains("org.opencontainers.image.revision"));
    assert!(remote.contains("market-ownership-transferred"));
    assert!(core_workflow.contains("vars.LEGACY_SIX_ROLE_AUTO_DEPLOY_ENABLED == 'true'"));
}

#[test]
fn scoped_handoff_rejects_a_wrong_confirmation_before_ssh() {
    let output = Command::new("bash")
        .arg(repo_root().join("scripts/deploy/deploy_eth_4h_market_handoff.sh"))
        .arg("verify")
        .env("DEPLOY_SSH_USER", "contract-user")
        .env("DEPLOY_SSH_HOST", "contract.invalid")
        .env("SERVER_APP_PATH", "/tmp/contract-app")
        .env("DEPLOY_ETH_4H_HANDOFF_CONFIRM", "wrong")
        .output()
        .expect("scoped handoff guard should execute");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid DEPLOY_ETH_4H_HANDOFF_CONFIRM")
    );
}
