use super::super::MarketVelocityLiveReadinessConfig;
use super::{ExecutionWorker, ExecutionWorkerConfig};
use std::sync::{Mutex, OnceLock};
const EXECUTION_WORKER_ENV_KEYS: &[&str] = &[
    "RUST_QUAN_WEB_BASE_URL",
    "QUANT_WEB_BASE_URL",
    "EXECUTION_EVENT_SECRET",
    "RUST_QUAN_WEB_INTERNAL_SECRET",
    "EXECUTION_WORKER_DRY_RUN",
    "EXECUTION_WORKER_LEASE_LIMIT",
    "EXECUTION_WORKER_TARGET_TASK_IDS",
    "EXECUTION_WORKER_CONFIRMATION_MODE",
    "EXECUTION_WORKER_REPORT_REPLAY_MODE",
    "EXECUTION_WORKER_RECONCILIATION_ONLY",
    "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
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
    std::env::remove_var("QUANT_CORE_DATABASE_URL");
    std::env::remove_var("QUANT_CORE_POSTGRES_URL");
    std::env::remove_var("POSTGRES_QUANT_CORE_DATABASE_URL");
}
#[test]
fn live_worker_from_env_requires_persistent_audit_repository() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "false");
    std::env::set_var("EXECUTION_WORKER_TARGET_TASK_IDS", "42");
    std::env::remove_var("EXECUTION_WORKER_RECONCILIATION_ONLY");
    std::env::set_var(
        "EXECUTION_WORKER_LIVE_ORDER_CONFIRM",
        "I_UNDERSTAND_LIVE_ORDERS",
    );
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
fn dry_run_worker_from_env_allows_missing_audit_repository() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
    std::env::remove_var("EXECUTION_WORKER_TARGET_TASK_IDS");
    std::env::remove_var("EXECUTION_WORKER_RECONCILIATION_ONLY");
    ExecutionWorker::from_env().expect("dry-run worker should allow no audit database");
}
#[test]
fn worker_from_env_requires_internal_secret_before_web_internal_calls() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::remove_var("EXECUTION_EVENT_SECRET");
    std::env::remove_var("RUST_QUAN_WEB_INTERNAL_SECRET");
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
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
    std::env::set_var("EXECUTION_WORKER_TARGET_TASK_IDS", "42");
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
        "EXECUTION_WORKER_RECONCILIATION_ONLY",
        "EXECUTION_WORKER_REPORT_REPLAY_MODE",
        "EXECUTION_WORKER_CONFIRMATION_MODE",
    ] {
        configure_base_worker_env();
        std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
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
fn market_velocity_live_readiness_rejects_invalid_worker_mode_booleans() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
    std::env::set_var("EXECUTION_WORKER_TARGET_TASK_IDS", "42");
    std::env::set_var("EXECUTION_WORKER_REPORT_REPLAY_MODE", "treu");
    let error = match MarketVelocityLiveReadinessConfig::from_env() {
        Ok(_) => panic!("live readiness must reject invalid worker mode booleans"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_WORKER_REPORT_REPLAY_MODE"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn dry_run_env_parsing_fails_safe_for_invalid_values() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "treu");
    assert!(
        ExecutionWorkerConfig::from_env().dry_run,
        "invalid EXECUTION_WORKER_DRY_RUN must not disable dry-run"
    );
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", " true ");
    assert!(
        ExecutionWorkerConfig::from_env().dry_run,
        "whitespace-padded true should still be dry-run"
    );
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", " false ");
    assert!(
        !ExecutionWorkerConfig::from_env().dry_run,
        "only explicit false/off/0 values should disable dry-run"
    );
}
#[test]
fn worker_from_env_rejects_invalid_target_task_ids_before_leasing() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
    std::env::set_var("EXECUTION_WORKER_TARGET_TASK_IDS", "42,abc");
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("worker must reject invalid target task id tokens before leasing"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_WORKER_TARGET_TASK_IDS must contain only positive task ids"),
        "unexpected error: {error:#}"
    );
    std::env::set_var("EXECUTION_WORKER_TARGET_TASK_IDS", "42,0,-7");
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("worker must reject non-positive target task ids before leasing"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_WORKER_TARGET_TASK_IDS must contain only positive task ids"),
        "unexpected error: {error:#}"
    );
}
#[test]
fn worker_from_env_rejects_zero_lease_limit_before_leasing() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
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
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "true");
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
#[test]
fn reconciliation_only_worker_from_env_requires_target_task_ids_for_signed_read_only() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);
    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "false");
    std::env::remove_var("EXECUTION_WORKER_TARGET_TASK_IDS");
    std::env::set_var("EXECUTION_WORKER_RECONCILIATION_ONLY", "true");
    let error = match ExecutionWorker::from_env() {
        Ok(_) => panic!("signed read-only reconciliation worker must be scoped to target task ids"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("EXECUTION_WORKER_TARGET_TASK_IDS"),
        "unexpected error: {error:#}"
    );
    std::env::set_var("EXECUTION_WORKER_TARGET_TASK_IDS", "42");
    ExecutionWorker::from_env()
        .expect("scoped reconciliation-only worker should allow no mutation audit database");
}
