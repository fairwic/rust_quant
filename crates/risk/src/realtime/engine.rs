use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::debug;

use rust_quant_domain::BasicRiskConfig;

use super::{
    BreakevenStopLossService, MarketCandle, PositionSnapshot, RealtimeRiskEvent, StopLossAmender,
    StrategyRiskConfigSnapshot,
};

/// 实时风控引擎（事件驱动）
///
/// 当前内置：
/// - 1.5R 触发后移动止损到开仓价（保本）
pub struct RealtimeRiskEngine<A: StopLossAmender> {
    breakeven: BreakevenStopLossService<A>,
    risk_cache: Arc<RwLock<HashMap<(i64, String), BasicRiskConfig>>>,
}

impl<A: StopLossAmender> RealtimeRiskEngine<A> {
    pub fn new(amender: Arc<A>) -> Self {
        Self {
            breakeven: BreakevenStopLossService::new(amender),
            risk_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 运行事件循环（上层负责把 K线/持仓/配置事件推送进 rx）
    pub async fn run(&self, mut rx: mpsc::Receiver<RealtimeRiskEvent>) {
        while let Some(ev) = rx.recv().await {
            match ev {
                RealtimeRiskEvent::RiskConfig(cfg) => {
                    self.on_risk_config(cfg).await;
                }
                RealtimeRiskEvent::Position(pos) => {
                    self.on_position(pos).await;
                }
                RealtimeRiskEvent::Candle(c) => {
                    self.on_candle(c).await;
                }
            }
        }
    }

    async fn on_risk_config(&self, cfg: StrategyRiskConfigSnapshot) {
        {
            let mut guard = self.risk_cache.write().await;
            guard.insert(
                (cfg.strategy_config_id, cfg.inst_id.clone()),
                cfg.risk.clone(),
            );
        }
        self.breakeven.upsert_risk_config(cfg).await;
    }

    async fn on_position(&self, pos: PositionSnapshot) {
        let risk = {
            let guard = self.risk_cache.read().await;
            guard
                .get(&(pos.strategy_config_id, pos.inst_id.clone()))
                .cloned()
                .unwrap_or_else(BasicRiskConfig::default)
        };

        debug!(
            "收到持仓更新: strategy_config_id={}, inst_id={}, open={}, side={:?}",
            pos.strategy_config_id, pos.inst_id, pos.is_open, pos.pos_side
        );

        self.breakeven.upsert_position(pos, risk).await;
    }

    async fn on_candle(&self, candle: MarketCandle) {
        self.breakeven.on_candle(candle).await;
    }
}
