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
const BACKTEST_INITIAL_R_MIGRATION: &str = include_str!(
    "../../../../migrations/20260719120500_add_back_test_detail_initial_r_contract.sql"
);
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
fn postgres_quant_core_ddl_uses_unbounded_strategy_signal_log_payload() {
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("strategy_result TEXT NOT NULL"),
        "strategy_job_signal_log.strategy_result must be TEXT so large Vegas signal JSON can be audited"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains(
            "ALTER TABLE IF EXISTS strategy_job_signal_log\n    ALTER COLUMN strategy_result TYPE TEXT"
        ),
        "quant_core schema ensure must widen existing strategy_job_signal_log.strategy_result columns"
    );
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
        "market_rank_events.live_handoff_state",
        "market_rank_events.live_handoff_blocker_code",
        "market_rank_events.live_handoff_blocker_detail",
        "market_rank_events.live_handoff_last_evaluated_at",
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
        POSTGRES_QUANT_CORE_DDL.contains("chk_market_rank_events_live_handoff_state"),
        "postgres quant_core DDL must constrain live handoff state separately from notification state"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("idx_market_rank_events_live_handoff_last_evaluated_at"),
        "postgres quant_core DDL must index live handoff diagnostics by latest evaluation time"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL
            .contains("CREATE TABLE IF NOT EXISTS market_velocity_live_handoff_states"),
        "postgres quant_core DDL must create strategy-scoped live handoff state table"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("uidx_market_velocity_live_handoff_states_contract"),
        "postgres quant_core DDL must keep one live handoff state per rank event and strategy contract"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("idx_market_velocity_live_handoff_states_pending"),
        "postgres quant_core DDL must index pending strategy-scoped live handoff scans"
    );
    for column in [
        "market_velocity_live_handoff_states.id",
        "market_velocity_live_handoff_states.rank_event_id",
        "market_velocity_live_handoff_states.strategy_slug",
        "market_velocity_live_handoff_states.strategy_preset",
        "market_velocity_live_handoff_states.entry_rule_version",
        "market_velocity_live_handoff_states.handoff_state",
        "market_velocity_live_handoff_states.blocker_code",
        "market_velocity_live_handoff_states.blocker_detail",
        "market_velocity_live_handoff_states.last_evaluated_at",
        "market_velocity_live_handoff_states.created_at",
        "market_velocity_live_handoff_states.updated_at",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON COLUMN {column}")),
            "postgres quant_core DDL must comment column {column}"
        );
    }
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
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("CREATE TABLE IF NOT EXISTS market_velocity_episodes"),
        "postgres quant_core DDL must create market_velocity_episodes"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("COMMENT ON TABLE market_velocity_episodes"),
        "postgres quant_core DDL must comment market_velocity_episodes"
    );
    assert!(
        POSTGRES_QUANT_CORE_DDL.contains("uidx_market_velocity_episodes_active_key"),
        "postgres quant_core DDL must enforce one active market velocity episode per rank signal key"
    );
    for column in [
        "market_velocity_episodes.id",
        "market_velocity_episodes.exchange",
        "market_velocity_episodes.symbol",
        "market_velocity_episodes.event_type",
        "market_velocity_episodes.timeframe",
        "market_velocity_episodes.status",
        "market_velocity_episodes.started_at",
        "market_velocity_episodes.last_seen_at",
        "market_velocity_episodes.first_old_rank",
        "market_velocity_episodes.latest_old_rank",
        "market_velocity_episodes.latest_new_rank",
        "market_velocity_episodes.best_new_rank",
        "market_velocity_episodes.latest_delta_rank",
        "market_velocity_episodes.max_delta_rank",
        "market_velocity_episodes.hit_count",
        "market_velocity_episodes.volume_24h_quote",
        "market_velocity_episodes.current_price",
        "market_velocity_episodes.previous_price",
        "market_velocity_episodes.price_change_pct",
        "market_velocity_episodes.price_direction",
        "market_velocity_episodes.technical_snapshot_status",
        "market_velocity_episodes.last_rank_event_id",
        "market_velocity_episodes.last_escalated_at",
        "market_velocity_episodes.created_at",
        "market_velocity_episodes.updated_at",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON COLUMN {column}")),
            "postgres quant_core DDL must comment column {column}"
        );
    }
}

#[test]
fn market_rank_snapshot_restore_query_samples_target_scans_instead_of_full_window() {
    let repository_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/repositories/fund_monitoring_repository.rs");
    let source = fs::read_to_string(&repository_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", repository_path.display(), error));
    assert!(
        source.contains("restore_targets"),
        "market rank snapshot restore should select target scan times instead of loading every row in the retention window"
    );
    assert!(
        source.contains("DISTINCT ON (target_at)"),
        "market rank snapshot restore should select the latest scan at or before each target horizon"
    );
    assert!(
        source.contains("CROSS JOIN LATERAL"),
        "market rank snapshot restore should use indexed per-target lookup instead of joining every historical snapshot before each target"
    );
    assert!(
        source.contains("ORDER BY snapshots.captured_at DESC\n                    LIMIT 1"),
        "market rank snapshot restore should stop after the latest scan time for each target"
    );
    assert!(
        !source.contains("JOIN market_rank_snapshots snapshots\n                  ON snapshots.exchange = $1\n                 AND snapshots.captured_at <= restore_targets.target_at"),
        "market rank snapshot restore must not join each target against all earlier snapshot rows"
    );
    assert!(
        !source.contains(
            "AND captured_at >= $2\n            ORDER BY captured_at ASC, rank ASC, symbol ASC"
        ),
        "market rank snapshot restore must not fetch the full 25h snapshot window"
    );
}

#[test]
fn market_rank_snapshot_prune_query_is_exchange_scoped_for_index_use() {
    let repository_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/repositories/fund_monitoring_repository.rs");
    let source = fs::read_to_string(&repository_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {}", repository_path.display(), error));
    assert!(
        source.contains("DELETE FROM market_rank_snapshots\n            WHERE exchange = $1\n              AND captured_at < $2"),
        "market rank snapshot pruning must scope by exchange so it can use the exchange/captured_at index"
    );
}

#[test]
fn backtest_detail_ddl_freezes_initial_risk_contract() {
    for column in [
        "initial_stop_price DOUBLE PRECISION",
        "initial_risk_amount DOUBLE PRECISION",
        "net_profit_r DOUBLE PRECISION",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(column),
            "back_test_detail DDL must include {column}"
        );
    }
    for column in [
        "back_test_detail.initial_stop_price",
        "back_test_detail.initial_risk_amount",
        "back_test_detail.net_profit_r",
    ] {
        assert!(
            POSTGRES_QUANT_CORE_DDL.contains(&format!("COMMENT ON COLUMN {column}")),
            "back_test_detail risk column must be documented: {column}"
        );
    }
}

#[test]
/// 生产迁移必须幂等补齐初始止损/R 字段及其数据库注释，不能只修改基线建表 SQL。
fn backtest_initial_r_contract_has_a_production_migration() {
    assert!(BACKTEST_INITIAL_R_MIGRATION.contains("ALTER TABLE IF EXISTS back_test_detail"));
    for column in [
        "ADD COLUMN IF NOT EXISTS initial_stop_price DOUBLE PRECISION",
        "ADD COLUMN IF NOT EXISTS initial_risk_amount DOUBLE PRECISION",
        "ADD COLUMN IF NOT EXISTS net_profit_r DOUBLE PRECISION",
    ] {
        assert!(
            BACKTEST_INITIAL_R_MIGRATION.contains(column),
            "production migration must add {column}"
        );
    }
    for column in [
        "back_test_detail.initial_stop_price",
        "back_test_detail.initial_risk_amount",
        "back_test_detail.net_profit_r",
    ] {
        assert!(
            BACKTEST_INITIAL_R_MIGRATION.contains(&format!("COMMENT ON COLUMN {column}")),
            "production migration must document {column}"
        );
    }
}
