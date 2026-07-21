use super::super::MarketVelocityLiveReadinessConfig;
use super::{ExecutionWorker, ExecutionWorkerConfig, ExecutionWorkerLane};
use std::sync::{Mutex, OnceLock};
const EXECUTION_WORKER_ENV_KEYS: &[&str] = &[
    "RUST_QUAN_WEB_BASE_URL",
    "QUANT_WEB_BASE_URL",
    "EXECUTION_EVENT_SECRET",
    "RUST_QUAN_WEB_INTERNAL_SECRET",
    "MARKET_VELOCITY_LIVE_READINESS_TASK_ID",
    "EXECUTION_WORKER_LEASE_LIMIT",
    "EXECUTION_WORKER_DRY_RUN",
    "EXECUTION_WORKER_CONFIRMATION_MODE",
    "EXECUTION_WORKER_REPORT_REPLAY_MODE",
    "QUANT_CORE_DATABASE_URL",
    "QUANT_CORE_POSTGRES_URL",
    "POSTGRES_QUANT_CORE_DATABASE_URL",
];
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
struct EnvSnapshot {
    /// 列表数据。
    values: Vec<(&'static str, Option<String>)>,
}
impl EnvSnapshot {
    fn capture(keys: &[&'static str]) -> Self {
        Self {
            values: keys
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect(),
        }
    }
}
impl Drop for EnvSnapshot {
    fn drop(&mut self) {
        for (key, value) in &self.values {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
fn configure_base_worker_env() {
    std::env::set_var("RUST_QUAN_WEB_BASE_URL", "http://127.0.0.1:18000");
    std::env::set_var("EXECUTION_EVENT_SECRET", "local-test-secret");
    std::env::remove_var("EXECUTION_WORKER_DRY_RUN");
    std::env::remove_var("QUANT_CORE_DATABASE_URL");
    std::env::remove_var("QUANT_CORE_POSTGRES_URL");
    std::env::remove_var("POSTGRES_QUANT_CORE_DATABASE_URL");
}
#[test]
fn live_worker_from_env_requires_persistent_audit_repository() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("live worker must fail closed when audit repository is not configured"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("QUANT_CORE_DATABASE_URL is required for live execution audit"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn dry_run_worker_from_env_does_not_require_live_audit_repository() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
    let worker = ExecutionWorker::from_env()
        .expect("dry-run worker should start without live audit repository");
    assert!(
        worker.config.dry_run,
        "EXECUTION_WORKER_DRY_RUN=true must select the dry-run gateway path"
    );
}
#[test]
fn worker_from_env_requires_internal_secret_before_web_internal_calls() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::remove_var("EXECUTION_EVENT_SECRET");
    std::env::remove_var("RUST_QUAN_WEB_INTERNAL_SECRET");
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("worker must fail closed when internal secret is not configured"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn market_velocity_live_readiness_from_env_requires_internal_secret_before_web_call() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::remove_var("EXECUTION_EVENT_SECRET");
    std::env::remove_var("RUST_QUAN_WEB_INTERNAL_SECRET");
    std::env::set_var("MARKET_VELOCITY_LIVE_READINESS_TASK_ID", "42");
    let error = match MarketVelocityLiveReadinessConfig::from_env() {
        Ok(_) => panic!("live readiness must fail closed when internal secret is not configured"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_EVENT_SECRET or RUST_QUAN_WEB_INTERNAL_SECRET is required"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn worker_from_env_rejects_invalid_execution_mode_booleans_before_path_selection() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    for key in [
        "EXECUTION_WORKER_DRY_RUN",
        "EXECUTION_WORKER_REPORT_REPLAY_MODE",
        "EXECUTION_WORKER_CONFIRMATION_MODE",
    ] {
        configure_base_worker_env();
        std::env::set_var(key, "treu");
        let error = match ExecutionWorker::from_env() {
            Ok(_) => panic!("{key} must reject invalid boolean values before worker startup"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains(key),
            "unexpected error for {key}: {error:#}"
        );
        std::env::remove_var(key);
    }
}

#[test]
fn fixed_worker_lanes_override_legacy_modes_and_remain_mutually_exclusive() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
    std::env::set_var("EXECUTION_WORKER_CONFIRMATION_MODE", "true");
    std::env::set_var("EXECUTION_WORKER_REPORT_REPLAY_MODE", "true");

    for (lane, expected_confirmation, expected_report_replay) in [
        (ExecutionWorkerLane::Execution, false, false),
        (ExecutionWorkerLane::Confirmation, true, false),
        (ExecutionWorkerLane::ReportReplay, false, true),
    ] {
        let config = ExecutionWorkerConfig::from_env_for_lane(lane);
        assert_eq!(config.lane(), lane);
        assert_eq!(config.confirmation_mode, expected_confirmation);
        assert_eq!(config.report_replay_mode, expected_report_replay);
        assert!(
            !(config.confirmation_mode && config.report_replay_mode),
            "a fixed worker lane must never enable two state-machine consumers"
        );
    }
}

#[test]
fn legacy_worker_keeps_report_replay_precedence_when_both_modes_are_enabled() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
    std::env::set_var("EXECUTION_WORKER_CONFIRMATION_MODE", "true");
    std::env::set_var("EXECUTION_WORKER_REPORT_REPLAY_MODE", "true");

    let config = ExecutionWorkerConfig::from_env();
    assert_eq!(config.lane(), ExecutionWorkerLane::ReportReplay);
}

#[test]
fn report_replay_lane_does_not_require_a_live_exchange_gateway() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "false");

    assert!(
        ExecutionWorkerConfig::from_env_for_lane(ExecutionWorkerLane::Execution)
            .requires_live_exchange_gateway()
    );
    assert!(
        ExecutionWorkerConfig::from_env_for_lane(ExecutionWorkerLane::Confirmation)
            .requires_live_exchange_gateway()
    );
    assert!(
        !ExecutionWorkerConfig::from_env_for_lane(ExecutionWorkerLane::ReportReplay)
            .requires_live_exchange_gateway()
    );
}
#[test]
fn worker_from_env_rejects_zero_lease_limit_before_leasing() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_LEASE_LIMIT", "0");
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("worker must reject zero lease limit before leasing"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_WORKER_LEASE_LIMIT must be greater than zero"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn worker_from_env_rejects_invalid_lease_limit_before_leasing() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_LEASE_LIMIT", "abc");
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("worker must reject invalid lease limit before leasing"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_WORKER_LEASE_LIMIT must be greater than zero"),
        "unexpected error: {error:#}"
    );
}
