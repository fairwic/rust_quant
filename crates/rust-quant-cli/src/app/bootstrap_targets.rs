/// 判断环境变量开关istrue，为配置运行时流程提供明确的布尔结果。
fn env_flag_is_true(envs: &HashMap<String, String>, key: &str) -> bool {
    envs.get(key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}
fn should_skip_market_data_sync_from_map(envs: &HashMap<String, String>) -> bool {
    env_flag_is_true(envs, "SYNC_SKIP_MARKET_DATA")
}
fn should_run_funding_rate_sync_from_map(envs: &HashMap<String, String>) -> bool {
    env_flag_is_true(envs, "IS_RUN_FUNDING_RATE_JOB")
}
fn should_run_market_velocity_radar_from_map(envs: &HashMap<String, String>) -> bool {
    env_flag_is_true(envs, "IS_RUN_MARKET_VELOCITY_RADAR")
}
/// 判断 配置、基础设施和运行时 条件是否满足，给上层流程提供布尔决策。
fn should_exit_after_market_velocity_live_readiness_from_map(
    envs: &HashMap<String, String>,
) -> bool {
    env_flag_is_true(envs, "IS_RUN_MARKET_VELOCITY_LIVE_READINESS")
        && !env_flag_is_true(envs, "IS_OPEN_SOCKET")
        && !env_flag_is_true(envs, "IS_RUN_INTERNAL_SERVER")
}
/// 提供默认回测targets的集中实现，避免配置运行时调用方重复处理相同细节。
fn default_backtest_targets() -> Vec<(String, String)> {
    vec![
        // ("ETH-USDT-SWAP".to_string(), "15m".to_string()),
        ("ETH-USDT-SWAP".to_string(), "4H".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "1H".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "5m".to_string()),
        // ("ETH-USDT-SWAP".to_string(), "1Dutc".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "5m".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "15m".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "1H".to_string()),
        ("BTC-USDT-SWAP".to_string(), "4H".to_string()),
        // ("BTC-USDT-SWAP".to_string(), "1Dutc".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "5m".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "15m".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "1H".to_string()),
        ("SOL-USDT-SWAP".to_string(), "4H".to_string()),
        // ("SOL-USDT-SWAP".to_string(), "1Dutc".to_string()),
        ("BCH-USDT-SWAP".to_string(), "4H".to_string()),
    ]
}
/// 提供override周期fromCSV的集中实现，避免配置运行时调用方重复处理相同细节。
fn override_periods_from_csv(periods: Vec<String>, raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return periods;
    };
    let overridden = dedup_strings(
        raw.split(',')
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect(),
    );
    if overridden.is_empty() {
        periods
    } else {
        overridden
    }
}
/// 提供去重字符串的集中实现，避免配置运行时调用方重复处理相同细节。
fn dedup_strings(values: Vec<String>) -> Vec<String> {
    let mut set = BTreeSet::new();
    for value in values {
        if !value.is_empty() {
            set.insert(value);
        }
    }
    set.into_iter().collect()
}
/// 计算 配置、基础设施和运行时 指标，保持公式和边界处理集中可审计。
fn derive_ws_targets_from_configs(configs: &[StrategyConfig]) -> (Vec<String>, Vec<String>) {
    let inst_ids = dedup_strings(configs.iter().map(|cfg| cfg.symbol.clone()).collect());
    let periods = dedup_strings(
        configs
            .iter()
            .map(|cfg| cfg.timeframe.as_str().to_string())
            .collect(),
    );
    (inst_ids, periods)
}
/// 提供市场data交易所的集中实现，避免配置运行时调用方重复处理相同细节。
fn market_data_exchange() -> String {
    std::env::var("MARKET_DATA_EXCHANGE")
        .or_else(|_| std::env::var("DEFAULT_EXCHANGE"))
        .unwrap_or_else(|_| "okx".to_string())
        .trim()
        .to_ascii_lowercase()
}
/// 计算 配置、基础设施和运行时 指标，保持公式和边界处理集中可审计。
fn derive_market_data_exchange_from_configs(
    configs: &[StrategyConfig],
    fallback: Option<&str>,
) -> Option<String> {
    let exchanges = dedup_strings(
        configs
            .iter()
            .filter_map(|config| config.exchange.as_deref())
            .map(|exchange| exchange.trim().to_ascii_lowercase())
            .filter(|exchange| !exchange.is_empty() && exchange != "all")
            .collect(),
    );
    match exchanges.as_slice() {
        [] => fallback
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty()),
        [exchange] => Some(exchange.clone()),
        multiple => {
            warn!(
                "⚠️  检测到多交易所实时策略配置，当前 WebSocket 仅使用单一数据源: {:?}",
                multiple
            );
            fallback
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
        }
    }
}
