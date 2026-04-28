use std::fs;
use std::path::Path;

const ACTIVE_ENTRYPOINTS: &[&str] = &[
    "crates/services/src/market/binance_websocket.rs",
    "scripts/optimize_vegas_batch.py",
    "scripts/visualize_backtest_detail.py",
    "scripts/analyze_high_vol_loss.py",
    "scripts/visualize_backtest_plotly.py",
];

const FORBIDDEN_TOKENS: &[&str] = &[
    "LegacyMysql",
    "pymysql",
    "mysql://",
    "mysql_exec(",
    "mysql_query(",
    "MYSQL_CMD",
];

#[test]
fn active_runtime_and_backtest_entrypoints_do_not_reference_mysql_tokens() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");

    let mut violations = Vec::new();
    for relative_path in ACTIVE_ENTRYPOINTS {
        let file_path = repo_root.join(relative_path);
        let source = fs::read_to_string(&file_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", file_path.display(), error));

        for token in FORBIDDEN_TOKENS {
            if source.contains(token) {
                violations.push(format!("{relative_path} contains {token}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "active Postgres entrypoints still contain MySQL tokens:\n{}",
        violations.join("\n")
    );
}

#[test]
fn workspace_sqlx_dependency_uses_explicit_postgres_features_only() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let manifest_path = repo_root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", manifest_path.display(), error));
    let sqlx_config = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("sqlx = "))
        .expect("workspace sqlx dependency is declared");

    assert!(
        sqlx_config.contains("default-features = false"),
        "workspace sqlx dependency must disable default features so MySQL is not enabled implicitly"
    );
    assert!(
        !sqlx_config.contains("\"mysql\""),
        "workspace sqlx dependency must not enable the mysql feature"
    );
}
