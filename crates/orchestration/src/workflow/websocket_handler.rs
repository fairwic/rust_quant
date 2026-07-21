use super::strategy_runner;
use rust_quant_domain::{StrategyConfig, StrategyType, Timeframe};
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info};
/// WebSocket 策略处理器
///
/// 负责处理 WebSocket 接收到的 K 线数据，并触发相应的策略执行
pub struct WebsocketStrategyHandler {
    /// 配置service，用于配置运行参数。
    config_service: Arc<StrategyConfigService>,
    /// 执行服务。
    execution_service: Arc<StrategyExecutionService>,
    /// 当前 WebSocket 行情源交易所，用于避免同周期配置跨交易所重复执行。
    market_exchange: Option<String>,
    /// 当前 worker 启动成功的策略类型，防止 K 线触发时串载其他运行角色的配置。
    runtime_strategy_types: HashSet<StrategyType>,
    /// 启动预热成功的精确配置 ID；多策略类型共享行情连接时用它阻止 scope 笛卡尔积串载。
    runtime_config_ids: HashSet<i64>,
}
impl WebsocketStrategyHandler {
    /// 创建新的处理器实例
    pub fn new(
        config_service: Arc<StrategyConfigService>,
        execution_service: Arc<StrategyExecutionService>,
        market_exchange: Option<String>,
        runtime_strategy_types: HashSet<StrategyType>,
        runtime_config_ids: HashSet<i64>,
    ) -> Self {
        Self {
            config_service,
            execution_service,
            market_exchange,
            runtime_strategy_types,
            runtime_config_ids,
        }
    }
    /// 处理 K 线数据
    pub async fn handle(&self, inst_id: String, time_interval: String, snap: CandlesEntity) {
        let config_service = self.config_service.clone();
        let execution_service = self.execution_service.clone();
        let market_exchange = self.market_exchange.clone();
        let runtime_strategy_types = &self.runtime_strategy_types;
        let runtime_config_ids = &self.runtime_config_ids;
        let candle_ts = snap.ts;
        info!(
            "🎯 K线确认触发策略检查: inst_id={}, time_interval={}, ts={}",
            inst_id, time_interval, candle_ts
        );
        // 调用方已经把该回调派发到独立任务，这里直接执行，避免每根 K 线重复 spawn。
        let timeframe = match Timeframe::from_str(&time_interval) {
            Ok(tf) => tf,
            Err(_) => {
                error!("❌ 无效的时间周期: {}", time_interval);
                return;
            }
        };
        // 单角色 worker 直接按策略类型查询，避免把同交易对、同周期的其他策略载入本进程。
        let strategy_type_selector = single_runtime_strategy_type(runtime_strategy_types);
        let configs = match config_service
            .load_configs(&inst_id, &time_interval, strategy_type_selector)
            .await
        {
            Ok(configs) => configs,
            Err(e) => {
                error!(
                    "❌ 加载策略配置失败: inst_id={}, time_interval={}, error={}",
                    inst_id, time_interval, e
                );
                return;
            }
        };
        let configs = filter_configs_for_runtime(
            configs,
            market_exchange.as_deref(),
            runtime_strategy_types,
            runtime_config_ids,
        );
        if configs.is_empty() {
            info!(
                "⚠️  未找到启用的策略配置: inst_id={}, time_interval={}",
                inst_id, time_interval
            );
            return;
        }
        info!("✅ 找到 {} 个策略配置，开始执行", configs.len());
        for config in configs {
            let strategy_type = config.strategy_type;
            let config_id = config.id;
            if let Err(e) = strategy_runner::execute_strategy(
                &inst_id,
                timeframe,
                strategy_type,
                Some(config_id),
                None,
                Some(snap.clone()),
                &config_service,
                &execution_service,
            )
            .await
            {
                error!(
                    "❌ 策略执行失败: inst_id={}, time_interval={}, strategy={:?}, error={}",
                    inst_id, time_interval, strategy_type, e
                );
            } else {
                info!(
                    "✅ 策略执行完成: inst_id={}, time_interval={}, strategy={:?}",
                    inst_id, time_interval, strategy_type
                );
            }
        }
    }
}

/// 按当前 WebSocket 行情源和 worker 运行角色过滤策略配置。
fn filter_configs_for_runtime(
    configs: Vec<StrategyConfig>,
    market_exchange: Option<&str>,
    runtime_strategy_types: &HashSet<StrategyType>,
    runtime_config_ids: &HashSet<i64>,
) -> Vec<StrategyConfig> {
    let market_exchange = market_exchange
        .map(str::trim)
        .filter(|exchange| !exchange.is_empty());

    configs
        .into_iter()
        .filter(|config| {
            let exchange_matches = market_exchange.map_or(true, |market_exchange| {
                config
                    .exchange
                    .as_deref()
                    .map(str::trim)
                    .filter(|exchange| {
                        !exchange.is_empty() && !exchange.eq_ignore_ascii_case("all")
                    })
                    .map(|exchange| exchange.eq_ignore_ascii_case(market_exchange))
                    .unwrap_or(true)
            });
            let strategy_type_matches = runtime_strategy_types.is_empty()
                || runtime_strategy_types.contains(&config.strategy_type);
            // 启动阶段没有明确准入的 config ID 时必须执行零策略，不能退化为“全部允许”。
            let config_id_matches = runtime_config_ids.contains(&config.id);
            exchange_matches && strategy_type_matches && config_id_matches
        })
        .collect()
}

/// 单策略角色可在查询阶段精确筛选；多角色进程则在返回后按集合过滤。
fn single_runtime_strategy_type(runtime_strategy_types: &HashSet<StrategyType>) -> Option<&str> {
    if runtime_strategy_types.len() == 1 {
        runtime_strategy_types
            .iter()
            .next()
            .map(StrategyType::as_str)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_quant_domain::{StrategyConfig, StrategyStatus};

    fn test_config(id: i64, exchange: Option<&str>) -> StrategyConfig {
        StrategyConfig {
            id,
            version: "default".to_string(),
            strategy_type: StrategyType::BearShortStack,
            exchange: exchange.map(ToOwned::to_owned),
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::M5,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({}),
            status: StrategyStatus::Running,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        }
    }

    #[test]
    fn filters_runtime_configs_to_current_market_exchange() {
        let okx = test_config(1, Some("okx"));
        let binance = test_config(2, Some("binance"));

        let filtered = filter_configs_for_runtime(
            vec![okx, binance],
            Some("okx"),
            &HashSet::new(),
            &HashSet::from([1, 2]),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 1);
        assert_eq!(filtered[0].exchange.as_deref(), Some("okx"));
    }

    #[test]
    fn dedicated_worker_excludes_universal_strategy_config() {
        let mut dedicated = test_config(1, Some("okx"));
        dedicated.strategy_type = StrategyType::Vegas;
        let mut universal = test_config(2, Some("okx"));
        universal.strategy_type = StrategyType::VegasUniversal4h;

        let runtime_types = HashSet::from([StrategyType::Vegas]);
        let filtered = filter_configs_for_runtime(
            vec![dedicated, universal],
            Some("okx"),
            &runtime_types,
            &HashSet::from([1, 2]),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].strategy_type, StrategyType::Vegas);
        assert_eq!(single_runtime_strategy_type(&runtime_types), Some("vegas"));
    }

    #[test]
    fn universal_worker_excludes_dedicated_strategy_config() {
        let mut dedicated = test_config(1, Some("okx"));
        dedicated.strategy_type = StrategyType::Vegas;
        let mut universal = test_config(2, Some("okx"));
        universal.strategy_type = StrategyType::VegasUniversal4h;

        let runtime_types = HashSet::from([StrategyType::VegasUniversal4h]);
        let filtered = filter_configs_for_runtime(
            vec![dedicated, universal],
            Some("okx"),
            &runtime_types,
            &HashSet::from([1, 2]),
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].strategy_type, StrategyType::VegasUniversal4h);
        assert_eq!(
            single_runtime_strategy_type(&runtime_types),
            Some("vegas_universal_4h")
        );
    }

    #[test]
    fn shared_socket_executes_only_configs_admitted_during_startup() {
        let mut eth_vegas = test_config(11, Some("okx"));
        eth_vegas.strategy_type = StrategyType::Vegas;
        let mut btc_vegas = test_config(12, Some("okx"));
        btc_vegas.strategy_type = StrategyType::Vegas;
        let mut btc_universal = test_config(13, Some("okx"));
        btc_universal.strategy_type = StrategyType::VegasUniversal4h;

        let filtered = filter_configs_for_runtime(
            vec![eth_vegas, btc_vegas, btc_universal],
            Some("okx"),
            &HashSet::from([StrategyType::Vegas, StrategyType::VegasUniversal4h]),
            &HashSet::from([11, 13]),
        );

        assert_eq!(
            filtered.iter().map(|config| config.id).collect::<Vec<_>>(),
            vec![11, 13]
        );
    }

    #[test]
    fn empty_startup_admission_set_executes_no_strategy() {
        let filtered = filter_configs_for_runtime(
            vec![test_config(1, Some("okx"))],
            Some("okx"),
            &HashSet::new(),
            &HashSet::new(),
        );

        assert!(filtered.is_empty());
    }
}
