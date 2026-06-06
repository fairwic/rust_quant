use std::{fs, path::PathBuf};

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
        .join("replay_market_velocity_okx_signal_candidate.sh")
}

fn read_replay_script() -> String {
    let path = script_path();
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read {}: {}", path.display(), error);
    })
}

#[test]
fn market_velocity_okx_signal_replay_script_passes_bash_syntax_check() {
    let output = std::process::Command::new("bash")
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

#[test]
fn market_velocity_okx_signal_replay_defaults_to_payload_dry_run() {
    let script = read_replay_script();

    assert!(script.contains("MARKET_VELOCITY_SIGNAL_REPLAY_APPLY"));
    assert!(script.contains("MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM"));
    assert!(script.contains("I_UNDERSTAND_THIS_CREATES_WEB_EXECUTION_TASK"));
    assert!(script.contains("mode=dry_run"));
    assert!(script.contains("mode=apply"));
    assert!(!script.contains("EXECUTION_WORKER_DRY_RUN=false"));
    assert!(!script.contains("EXECUTION_WORKER_LIVE_ORDER_CONFIRM"));
    assert!(!script.contains("cargo run"));
    assert!(!script.contains("/api/v5/trade/order"));
}

#[test]
fn market_velocity_okx_signal_replay_matches_core_promotion_gate() {
    let script = read_replay_script();

    assert!(script.contains("event_type IN ('rank_velocity', 'top_entry')"));
    assert!(script.contains("delta_rank >= 3"));
    assert!(script.contains("new_rank > 0"));
    assert!(script.contains("new_rank <= 50"));
    assert!(script.contains("lower(price_direction) = 'up'"));
    assert!(script.contains("current_price IS NOT NULL"));
    assert!(script.contains("lower(exchange) = 'okx'"));
    assert!(script.contains("NOT LIKE 'LINKUSDT%'"));
}

#[test]
fn market_velocity_okx_signal_replay_posts_strategy_signal_contract_only_after_confirm() {
    let script = read_replay_script();

    assert!(script.contains("/api/commerce/internal/strategy-signals"));
    assert!(script.contains("x-alpha-execution-secret"));
    assert!(script.contains("rust_quant:market_velocity:"));
    assert!(script.contains("\"source_signal_type\": \"market_velocity\""));
    assert!(script.contains("\"side\": \"buy\""));
    assert!(script.contains("\"position_side\": \"long\""));
    assert!(script.contains("\"trade_side\": \"open\""));
    assert!(script.contains("\"order_type\": \"market\""));
    assert!(script.contains("\"protective_stop_loss_required\": true"));
    assert!(script.contains("selected_stop_loss_price"));
}

#[test]
fn market_velocity_okx_signal_replay_can_create_retry_external_id_for_failed_task() {
    let script = read_replay_script();

    assert!(script.contains("MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX"));
    assert!(script.contains("require_safe_external_id_suffix"));
    assert!(script.contains(
        "'rust_quant:market_velocity:' || id::text || '${replay_external_id_suffix_sql}'"
    ));
    assert!(script.contains("replay_external_id_suffix="));
    assert!(script.contains("rank_event_id"));
    assert!(!script.contains("UPDATE execution_tasks"));
}

#[test]
fn market_velocity_okx_signal_replay_preflights_web_task_generation_gates() {
    let script = read_replay_script();

    assert!(script.contains("WEB_POSTGRES_DB"));
    assert!(script.contains("strategy_combo_subscriptions"));
    assert!(script.contains("combo_risk_settings"));
    assert!(script.contains("user_execution_risk_snapshots"));
    assert!(script.contains("expires_at >= NOW()"));
    assert!(script.contains("execution_tasks"));
    assert!(script.contains("task_status NOT IN"));
    assert!(script.contains("user_position_snapshots"));
    assert!(script.contains("quantity > 0"));
    assert!(script.contains("web_task_generation_preflight"));
}
