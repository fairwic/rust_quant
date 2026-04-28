use std::fs;
use std::path::Path;

const ACTIVE_REPOSITORY_FILES: &[&str] = &[
    "audit_repository.rs",
    "backtest_repository.rs",
    "economic_event_repository.rs",
    "exchange_api_config_repository.rs",
    "external_market_snapshot_repository.rs",
    "fund_monitoring_repository.rs",
    "funding_rate_repository.rs",
    "signal_log_repository.rs",
    "strategy_config_repository.rs",
    "swap_order_repository.rs",
];

const FORBIDDEN_TOKENS: &[&str] = &[
    concat!("Pool<", "My", "Sql>"),
    concat!("My", "Sql", "Pool"),
    concat!("sqlx::", "My", "Sql"),
    concat!("my", "sql::", "My", "SqlQueryResult"),
    concat!("QueryBuilder<", "My", "Sql>"),
    "sqlx::query!(",
    "sqlx::query_as!(",
    "ON DUPLICATE KEY",
    "DATE_SUB(",
    "last_insert_id()",
];

#[test]
fn active_repositories_do_not_use_mysql_runtime_tokens() {
    let repository_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/repositories");

    let mut violations = Vec::new();
    for file_name in ACTIVE_REPOSITORY_FILES {
        let file_path = repository_dir.join(file_name);
        let source = fs::read_to_string(&file_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", file_path.display(), error));

        for token in FORBIDDEN_TOKENS {
            if source.contains(token) {
                violations.push(format!("{} contains {}", file_name, token));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "repository Postgres migration contract violated:\n{}",
        violations.join("\n")
    );
}
