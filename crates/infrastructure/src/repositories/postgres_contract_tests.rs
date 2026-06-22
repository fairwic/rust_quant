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

const POSTGRES_QUANT_CORE_DDL: &str = include_str!("../../../../sql/postgres_quant_core.sql");

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

#[test]
fn postgres_quant_core_ddl_contains_live_strategy_order_contract() {
    for table in [
        "swap_orders",
        "exchange_apikey_config",
        "exchange_apikey_strategy_relation",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
            "postgres quant_core DDL must create {table}"
        );
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON TABLE {table}")),
            "postgres quant_core DDL must comment table {table}"
        );
    }

    for column in [
        "swap_orders.id",
        "swap_orders.strategy_id",
        "swap_orders.in_order_id",
        "swap_orders.out_order_id",
        "swap_orders.strategy_type",
        "swap_orders.period",
        "swap_orders.inst_id",
        "swap_orders.side",
        "swap_orders.pos_size",
        "swap_orders.pos_side",
        "swap_orders.tag",
        "swap_orders.platform_type",
        "swap_orders.detail",
        "swap_orders.created_at",
        "swap_orders.update_at",
        "exchange_apikey_config.id",
        "exchange_apikey_config.exchange_name",
        "exchange_apikey_config.api_key",
        "exchange_apikey_config.api_secret",
        "exchange_apikey_config.passphrase",
        "exchange_apikey_config.is_sandbox",
        "exchange_apikey_config.is_enabled",
        "exchange_apikey_config.description",
        "exchange_apikey_config.create_user_id",
        "exchange_apikey_config.create_time",
        "exchange_apikey_config.update_time",
        "exchange_apikey_config.is_deleted",
        "exchange_apikey_strategy_relation.id",
        "exchange_apikey_strategy_relation.strategy_config_id",
        "exchange_apikey_strategy_relation.api_config_id",
        "exchange_apikey_strategy_relation.priority",
        "exchange_apikey_strategy_relation.is_enabled",
        "exchange_apikey_strategy_relation.is_deleted",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON COLUMN {column}")),
            "postgres quant_core DDL must comment column {column}"
        );
    }
}

#[test]
fn postgres_quant_core_ddl_contains_market_velocity_radar_contract() {
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("CREATE TABLE IF NOT EXISTS market_rank_events"),
        "postgres quant_core DDL must create market_rank_events"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("COMMENT ON TABLE market_rank_events"),
        "postgres quant_core DDL must comment market_rank_events"
    );

    for column in [
        "market_rank_events.id",
        "market_rank_events.exchange",
        "market_rank_events.symbol",
        "market_rank_events.event_type",
        "market_rank_events.timeframe",
        "market_rank_events.old_rank",
        "market_rank_events.new_rank",
        "market_rank_events.delta_rank",
        "market_rank_events.volume_24h_quote",
        "market_rank_events.current_price",
        "market_rank_events.previous_price",
        "market_rank_events.price_change_pct",
        "market_rank_events.price_direction",
        "market_rank_events.technical_timeframe",
        "market_rank_events.technical_period",
        "market_rank_events.technical_close_price",
        "market_rank_events.technical_ma_value",
        "market_rank_events.technical_ema_value",
        "market_rank_events.technical_ma_distance_pct",
        "market_rank_events.technical_ema_distance_pct",
        "market_rank_events.technical_ma_state",
        "market_rank_events.technical_ema_state",
        "market_rank_events.technical_candle_count",
        "market_rank_events.technical_snapshot_at",
        "market_rank_events.technical_snapshot_status",
        "market_rank_events.detected_at",
        "market_rank_events.source",
        "market_rank_events.notification_state",
        "market_rank_events.created_at",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON COLUMN {column}")),
            "postgres quant_core DDL must comment column {column}"
        );
    }
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("chk_market_rank_events_price_direction"),
        "postgres quant_core DDL must constrain market rank event price direction"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("idx_market_rank_events_radar_exchange_recent"),
        "postgres quant_core DDL must index recent radar event lookups by exchange and time"
    );

    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("CREATE TABLE IF NOT EXISTS market_rank_snapshots"),
        "postgres quant_core DDL must create market_rank_snapshots"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("COMMENT ON TABLE market_rank_snapshots"),
        "postgres quant_core DDL must comment market_rank_snapshots"
    );
    for column in [
        "market_rank_snapshots.id",
        "market_rank_snapshots.exchange",
        "market_rank_snapshots.symbol",
        "market_rank_snapshots.rank",
        "market_rank_snapshots.price",
        "market_rank_snapshots.volume_24h_quote",
        "market_rank_snapshots.captured_at",
        "market_rank_snapshots.created_at",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON COLUMN {column}")),
            "postgres quant_core DDL must comment column {column}"
        );
    }
}
