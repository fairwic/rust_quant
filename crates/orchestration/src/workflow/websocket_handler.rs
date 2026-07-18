use super::strategy_runner;
use rust_quant_domain::{StrategyConfig, Timeframe};
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};
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
}
impl WebsocketStrategyHandler {
    /// 创建新的处理器实例
    pub fn new(
        config_service: Arc<StrategyConfigService>,
        execution_service: Arc<StrategyExecutionService>,
        market_exchange: Option<String>,
    ) -> Self {
        Self {
            config_service,
            execution_service,
            market_exchange,
        }
    }
    /// 处理 K 线数据
    pub async fn handle(&self, inst_id: String, time_interval: String, snap: CandlesEntity) {
        let config_service = self.config_service.clone();
        let execution_service = self.execution_service.clone();
        let market_exchange = self.market_exchange.clone();
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
        // 查询该交易对和时间周期的所有启用策略
        let configs = match config_service
            .load_configs(&inst_id, &time_interval, None)
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
        let configs = filter_configs_for_market_exchange(configs, market_exchange.as_deref());
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

/// 按当前 WebSocket 行情源过滤策略配置，避免 OKX 行情触发 Binance 配置等跨交易所重复执行。
fn filter_configs_for_market_exchange(
    configs: Vec<StrategyConfig>,
    market_exchange: Option<&str>,
) -> Vec<StrategyConfig> {
    let Some(market_exchange) = market_exchange
        .map(str::trim)
        .filter(|exchange| !exchange.is_empty())
    else {
        return configs;
    };

    configs
        .into_iter()
        .filter(|config| {
            config
                .exchange
                .as_deref()
                .map(str::trim)
                .filter(|exchange| !exchange.is_empty() && !exchange.eq_ignore_ascii_case("all"))
                .map(|exchange| exchange.eq_ignore_ascii_case(market_exchange))
                .unwrap_or(true)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_quant_domain::{StrategyConfig, StrategyStatus, StrategyType};

    fn test_config(id: i64, exchange: Option<&str>) -> StrategyConfig {
        StrategyConfig {
            id,
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

        let filtered = filter_configs_for_market_exchange(vec![okx, binance], Some("okx"));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 1);
        assert_eq!(filtered[0].exchange.as_deref(), Some("okx"));
    }
}
