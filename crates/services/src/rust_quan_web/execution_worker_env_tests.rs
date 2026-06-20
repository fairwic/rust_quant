use super::ExecutionWorker;
use std::sync::{Mutex, OnceLock};

const EXECUTION_WORKER_ENV_KEYS: &[&str] = &[
    "RUST_QUAN_WEB_BASE_URL",
    "QUANT_WEB_BASE_URL",
    "EXECUTION_EVENT_SECRET",
    "EXECUTION_WORKER_DRY_RUN",
    "EXECUTION_WORKER_TARGET_TASK_IDS",
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
fn reconciliation_only_worker_from_env_allows_missing_audit_repository() {
    let _guard = env_lock().lock().expect("env lock poisoned");
    let _snapshot = EnvSnapshot::capture(EXECUTION_WORKER_ENV_KEYS);

    configure_base_worker_env();
    std::env::set_var("EXECUTION_WORKER_DRY_RUN", "false");
    std::env::remove_var("EXECUTION_WORKER_TARGET_TASK_IDS");
    std::env::set_var("EXECUTION_WORKER_RECONCILIATION_ONLY", "true");

    ExecutionWorker::from_env().expect("reconciliation-only worker should allow no audit database");
}
