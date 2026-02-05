use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info};

use rust_quant_domain::Timeframe;
use rust_quant_market::models::CandlesEntity;
use rust_quant_services::strategy::{StrategyConfigService, StrategyExecutionService};
use tokio::sync::mpsc;

use rust_quant_risk::realtime::{MarketCandle, RealtimeRiskEvent};

use super::strategy_runner;

/// WebSocket 策略处理器
///
/// 负责处理 WebSocket 接收到的 K 线数据，并触发相应的策略执行
pub struct WebsocketStrategyHandler {
    config_service: Arc<StrategyConfigService>,
    execution_service: Arc<StrategyExecutionService>,
    realtime_risk_tx: Option<mpsc::Sender<RealtimeRiskEvent>>,
}

impl WebsocketStrategyHandler {
    /// 创建新的处理器实例
    pub fn new(
        config_service: Arc<StrategyConfigService>,
        execution_service: Arc<StrategyExecutionService>,
    ) -> Self {
        Self {
            config_service,
            execution_service,
            realtime_risk_tx: None,
        }
    }

    /// 注入实时风控事件通道（用于推送确认K线）
    pub fn with_realtime_risk_sender(mut self, tx: mpsc::Sender<RealtimeRiskEvent>) -> Self {
        self.realtime_risk_tx = Some(tx);
        self
    }

    /// 处理 K 线数据
    pub async fn handle(&self, inst_id: String, time_interval: String, snap: CandlesEntity) {
        let config_service = self.config_service.clone();
        let execution_service = self.execution_service.clone();
        let realtime_risk_tx = self.realtime_risk_tx.clone();
        let candle_ts = snap.ts;

        info!(
            "🎯 K线确认触发策略检查: inst_id={}, time_interval={}, ts={}",
            inst_id, time_interval, candle_ts
        );

        // 异步执行策略检查，避免阻塞 WebSocket 线程
        tokio::spawn(async move {
            // 解析时间周期
            let timeframe = match Timeframe::from_str(&time_interval) {
                Ok(tf) => tf,
                Err(_) => {
                    error!("❌ 无效的时间周期: {}", time_interval);
                    return;
                }
            };

            // 推送确认K线给实时风控（全局只需一次：在 handler 入口层做）
            if let Some(tx) = realtime_risk_tx {
                match MarketCandle::try_from_entity(inst_id.clone(), &snap) {
                    Ok(mc) => {
                        let _ = tx.send(RealtimeRiskEvent::Candle(mc)).await;
                    }
                    Err(e) => {
                        error!("❌ 转换 MarketCandle 失败: inst_id={}, err={}", inst_id, e);
                    }
                }
            }

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

            if configs.is_empty() {
                info!(
                    "⚠️  未找到启用的策略配置: inst_id={}, time_interval={}",
                    inst_id, time_interval
                );
                return;
            }

            info!("✅ 找到 {} 个策略配置，开始执行", configs.len());

            // 执行每个策略
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
        });
    }
}
