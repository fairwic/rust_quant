use super::*;
use chrono::Utc;
use rust_quant_domain::{StrategyConfig, StrategyStatus, Timeframe};
fn test_config(id: i64, symbol: &str, timeframe: Timeframe) -> StrategyConfig {
    StrategyConfig {
        id,
        version: "default".to_string(),
        strategy_type: StrategyType::Vegas,
        exchange: None,
        symbol: symbol.to_string(),
        timeframe,
        status: StrategyStatus::Running,
        parameters: serde_json::json!({}),
        risk_config: serde_json::json!({}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        backtest_start: None,
        backtest_end: None,
        description: None,
    }
}
#[test]
fn test_derive_ws_targets_from_configs_dedup() {
    let configs = vec![
        test_config(1, "BTC-USDT-SWAP", Timeframe::H4),
        test_config(2, "BTC-USDT-SWAP", Timeframe::H4),
        test_config(3, "ETH-USDT-SWAP", Timeframe::H1),
    ];
    let (inst_ids, periods) = derive_ws_targets_from_configs(&configs);
    assert_eq!(
        inst_ids,
        vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()]
    );
    assert_eq!(periods, vec!["1H".to_string(), "4H".to_string()]);
}
#[test]
fn test_derive_ws_targets_from_configs_empty() {
    let configs = vec![];
    let (inst_ids, periods) = derive_ws_targets_from_configs(&configs);
    assert!(inst_ids.is_empty());
    assert!(periods.is_empty());
}
#[test]
fn test_derive_market_data_exchange_from_configs_prefers_strategy_exchange() {
    let mut config = test_config(1, "ETH-USDT-SWAP", Timeframe::H4);
    config.exchange = Some("binance".to_string());
    assert_eq!(
        derive_market_data_exchange_from_configs(&[config], Some("okx")),
        Some("binance".to_string())
    );
}
#[test]
fn test_filter_live_strategy_configs_supports_exchange_filter() {
    let mut okx = test_config(1, "BTC-USDT-SWAP", Timeframe::M5);
    okx.exchange = Some("okx".to_string());
    let mut binance = test_config(2, "BTC-USDT-SWAP", Timeframe::M5);
    binance.exchange = Some("binance".to_string());
    let filtered = filter_live_strategy_configs_with_filters(
        vec![okx, binance],
        &std::collections::BTreeSet::from(["BTC-USDT-SWAP".to_string()]),
        &std::collections::BTreeSet::from(["5m".to_string()]),
        &std::collections::BTreeSet::from(["okx".to_string()]),
        &std::collections::BTreeSet::new(),
        "okx",
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].exchange.as_deref(), Some("okx"));
}

#[test]
fn test_filter_live_strategy_configs_supports_strategy_type_filter() {
    let legacy = test_config(1, "ETH-USDT-SWAP", Timeframe::H4);
    let mut universal = test_config(2, "ETH-USDT-SWAP", Timeframe::H4);
    universal.strategy_type = StrategyType::VegasUniversal4h;
    let filtered = filter_live_strategy_configs_with_filters(
        vec![legacy, universal],
        &std::collections::BTreeSet::new(),
        &std::collections::BTreeSet::from(["4H".to_string()]),
        &std::collections::BTreeSet::from(["okx".to_string()]),
        &std::collections::BTreeSet::from(["vegas_universal_4h".to_string()]),
        "okx",
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].strategy_type, StrategyType::VegasUniversal4h);
}
#[test]
fn test_filter_live_strategy_configs_skips_market_velocity_event_strategy() {
    let vegas = test_config(1, "ETH-USDT-SWAP", Timeframe::H4);
    let mut market_velocity = test_config(2, "all", Timeframe::M15);
    market_velocity.strategy_type = StrategyType::MarketVelocity;
    let filtered = filter_live_strategy_configs(vec![vegas, market_velocity]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].strategy_type, StrategyType::Vegas);
    assert_eq!(filtered[0].symbol, "ETH-USDT-SWAP");
}
#[test]
fn test_default_backtest_targets_keep_enabled_1h_symbols() {
    let targets = default_backtest_targets();
    assert_eq!(
        targets,
        vec![
            ("ETH-USDT-SWAP".to_string(), "1H".to_string()),
            ("BTC-USDT-SWAP".to_string(), "1H".to_string()),
            ("SOL-USDT-SWAP".to_string(), "1H".to_string()),
            ("BCH-USDT-SWAP".to_string(), "1H".to_string()),
        ]
    );
}
#[test]
fn test_override_periods_from_csv_replaces_default_periods() {
    let periods = vec!["4H".to_string(), "1H".to_string()];
    let overridden = override_periods_from_csv(periods, Some("1m,4H"));
    assert_eq!(overridden, vec!["1m".to_string(), "4H".to_string()]);
}
#[test]
fn test_override_periods_from_csv_keeps_defaults_when_empty() {
    let periods = vec!["4H".to_string(), "1H".to_string()];
    let overridden = override_periods_from_csv(periods.clone(), Some("  ,  "));
    assert_eq!(overridden, periods);
}
#[test]
fn test_parse_dune_sync_requests_from_map_single_job_env() {
    let mut envs = std::collections::HashMap::new();
    envs.insert("IS_RUN_DUNE_SYNC_JOB".to_string(), "true".to_string());
    envs.insert("DUNE_SYMBOL".to_string(), "ETH".to_string());
    envs.insert(
        "DUNE_START_TIME".to_string(),
        "2026-02-21T20:00:00Z".to_string(),
    );
    envs.insert(
        "DUNE_END_TIME".to_string(),
        "2026-02-22T00:00:00Z".to_string(),
    );
    envs.insert(
        "DUNE_TEMPLATE_PATH".to_string(),
        "docs/external_market_data/dune/hyperliquid_funding_basis.sql".to_string(),
    );
    envs.insert(
        "DUNE_METRIC_TYPE".to_string(),
        "hyperliquid_basis".to_string(),
    );
    envs.insert("DUNE_MIN_USD".to_string(), "100000".to_string());
    let requests = parse_dune_sync_requests_from_map(&envs).expect("should parse dune sync env");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].metric_type, "hyperliquid_basis");
    assert_eq!(requests[0].symbol, "ETH");
    assert_eq!(
        requests[0].template_path,
        "docs/external_market_data/dune/hyperliquid_funding_basis.sql"
    );
    assert_eq!(
        requests[0].params.get("start_time"),
        Some(&"2026-02-21T20:00:00Z".to_string())
    );
    assert_eq!(
        requests[0].params.get("min_usd"),
        Some(&"100000".to_string())
    );
}
#[test]
fn test_parse_dune_sync_requests_from_map_batch_jobs() {
    let mut envs = std::collections::HashMap::new();
    envs.insert("IS_RUN_DUNE_SYNC_JOB".to_string(), "true".to_string());
    envs.insert(
            "DUNE_TEMPLATE_JOBS".to_string(),
            "hyperliquid_basis|ETH|docs/external_market_data/dune/hyperliquid_funding_basis.sql|2026-02-21T20:00:00Z|2026-02-22T00:00:00Z|medium|100000;\
eth_whale_transfer|ETH|docs/external_market_data/dune/eth_whale_transfer.sql|2026-02-21T20:00:00Z|2026-02-22T00:00:00Z|large|250000"
                .to_string(),
        );
    let requests = parse_dune_sync_requests_from_map(&envs).expect("should parse batch jobs");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].metric_type, "hyperliquid_basis");
    assert_eq!(requests[0].performance, DuneQueryPerformance::Medium);
    assert_eq!(requests[1].metric_type, "eth_whale_transfer");
    assert_eq!(requests[1].performance, DuneQueryPerformance::Large);
    assert_eq!(
        requests[1].params.get("min_usd"),
        Some(&"250000".to_string())
    );
}
#[test]
fn test_should_skip_market_data_sync_from_map_enabled() {
    let mut envs = std::collections::HashMap::new();
    envs.insert("SYNC_SKIP_MARKET_DATA".to_string(), "true".to_string());
    assert!(should_skip_market_data_sync_from_map(&envs));
}
#[test]
fn test_should_skip_market_data_sync_from_map_disabled_by_default() {
    let envs = std::collections::HashMap::new();
    assert!(!should_skip_market_data_sync_from_map(&envs));
}
#[test]
fn test_should_run_funding_rate_sync_from_map_enabled() {
    let mut envs = std::collections::HashMap::new();
    envs.insert("IS_RUN_FUNDING_RATE_JOB".to_string(), "true".to_string());
    assert!(should_run_funding_rate_sync_from_map(&envs));
}
#[test]
fn test_should_run_funding_rate_sync_from_map_disabled_by_default() {
    let envs = std::collections::HashMap::new();
    assert!(!should_run_funding_rate_sync_from_map(&envs));
}
#[test]
fn test_should_run_market_velocity_radar_from_map_enabled() {
    let mut envs = std::collections::HashMap::new();
    envs.insert(
        "IS_RUN_MARKET_VELOCITY_RADAR".to_string(),
        "true".to_string(),
    );
    assert!(should_run_market_velocity_radar_from_map(&envs));
}
#[test]
fn test_should_run_market_velocity_radar_from_map_disabled_by_default() {
    let envs = std::collections::HashMap::new();
    assert!(!should_run_market_velocity_radar_from_map(&envs));
}
#[test]
fn test_market_velocity_live_readiness_exits_as_one_shot() {
    let mut envs = std::collections::HashMap::new();
    envs.insert(
        "IS_RUN_MARKET_VELOCITY_LIVE_READINESS".to_string(),
        "true".to_string(),
    );
    assert!(should_exit_after_market_velocity_live_readiness_from_map(
        &envs
    ));
}
#[test]
fn market_velocity_radar_starts_maintenance_scheduler_in_same_process() {
    let source = include_str!("bootstrap.rs");
    let radar_start = source
        .find("async fn run_market_velocity_radar_worker_from_env")
        .expect("radar worker entrypoint should exist");
    let websocket_start = source
        .find("/// WebSocket数据监听")
        .expect("next entrypoint should exist");
    let radar_entrypoint = &source[radar_start..websocket_start];
    assert!(
        radar_entrypoint.contains("start_core_maintenance_scheduler"),
        "radar worker should start a logical maintenance scheduler in the same process"
    );
    assert!(
        radar_entrypoint.contains("MarketRankSnapshotPruneJob"),
        "market rank snapshot pruning should be registered as a logical job, not a new container"
    );
}
#[test]
fn execution_worker_entrypoints_verify_live_audit_before_polling() {
    let source = include_str!("bootstrap.rs");
    let single_start = source
        .find("async fn run_execution_worker_from_env")
        .expect("single-run worker entrypoint should exist");
    let loop_start = source
        .find("async fn run_execution_worker_loop")
        .expect("loop worker entrypoint should exist");
    let sync_start = source
        .find("async fn run_exchange_symbol_sync_worker_from_env")
        .expect("next worker entrypoint should exist");
    let single_entrypoint = &source[single_start..loop_start];
    assert!(
        single_entrypoint
            .find("worker.verify_live_audit_ready().await?")
            .expect("single-run entrypoint should verify live audit readiness")
            < single_entrypoint
                .find("worker.run_once().await?")
                .expect("single-run entrypoint should run worker after readiness")
    );
    let loop_entrypoint = &source[loop_start..sync_start];
    assert!(
        loop_entrypoint
            .find("worker.verify_live_audit_ready().await?")
            .expect("loop entrypoint should verify live audit readiness")
            < loop_entrypoint
                .find("loop {")
                .expect("loop entrypoint should start polling after readiness")
    );
}
#[test]
fn execution_worker_poll_interval_never_allows_zero_second_loop() {
    let envs = std::collections::HashMap::new();
    assert_eq!(execution_worker_poll_interval_secs_from_map(&envs), 5);
    let mut envs = std::collections::HashMap::new();
    envs.insert(
        "EXECUTION_WORKER_POLL_INTERVAL_SECS".to_string(),
        "abc".to_string(),
    );
    assert_eq!(execution_worker_poll_interval_secs_from_map(&envs), 5);
    envs.insert(
        "EXECUTION_WORKER_POLL_INTERVAL_SECS".to_string(),
        "0".to_string(),
    );
    assert_eq!(execution_worker_poll_interval_secs_from_map(&envs), 1);
    envs.insert(
        "EXECUTION_WORKER_POLL_INTERVAL_SECS".to_string(),
        " 2 ".to_string(),
    );
    assert_eq!(execution_worker_poll_interval_secs_from_map(&envs), 2);
}
