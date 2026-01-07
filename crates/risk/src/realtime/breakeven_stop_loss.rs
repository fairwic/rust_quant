use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use rust_quant_common::CandleItem;
use rust_quant_domain::enums::PositionSide;
use rust_quant_domain::BasicRiskConfig;

use super::{MarketCandle, PositionSnapshot, StopLossAmender, StrategyRiskConfigSnapshot};

/// 达到 1.5R 后将止损移动到开仓价（保本）
///
/// R 的定义：\( R = |entry - initial_stop_loss| \)
/// - Long: 触发阈值 = entry + 1.5R
/// - Short: 触发阈值 = entry - 1.5R
///
/// 注意：
/// - 仅当策略风险配置启用 `atr_take_profit_ratio` 时开启本规则（作为“动态止盈策略启用”的开关）。
/// - 若 `initial_stop_loss` 缺失，退化使用 `risk.max_loss_percent` 推算初始止损。
pub struct BreakevenStopLossService<A: StopLossAmender> {
    amender: Arc<A>,
    inner: Arc<RwLock<InnerState>>,
}

#[derive(Debug, Clone)]
struct StrategyKey {
    strategy_config_id: i64,
    inst_id: String,
}

impl StrategyKey {
    fn new(strategy_config_id: i64, inst_id: String) -> Self {
        Self {
            strategy_config_id,
            inst_id,
        }
    }
}

impl std::hash::Hash for StrategyKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.strategy_config_id.hash(state);
        self.inst_id.hash(state);
    }
}

impl PartialEq for StrategyKey {
    fn eq(&self, other: &Self) -> bool {
        self.strategy_config_id == other.strategy_config_id && self.inst_id == other.inst_id
    }
}

impl Eq for StrategyKey {}

#[derive(Debug, Clone)]
struct PositionRuntimeState {
    snapshot: PositionSnapshot,
    risk: BasicRiskConfig,
    moved_to_breakeven: bool,
}

#[derive(Default)]
struct InnerState {
    positions: HashMap<StrategyKey, PositionRuntimeState>,
}

impl<A: StopLossAmender> BreakevenStopLossService<A> {
    pub fn new(amender: Arc<A>) -> Self {
        Self {
            amender,
            inner: Arc::new(RwLock::new(InnerState::default())),
        }
    }

    /// 更新策略风险配置（热更新）
    pub async fn upsert_risk_config(&self, cfg: StrategyRiskConfigSnapshot) {
        let mut guard = self.inner.write().await;
        let key = StrategyKey::new(cfg.strategy_config_id, cfg.inst_id.clone());

        if let Some(st) = guard.positions.get_mut(&key) {
            st.risk = cfg.risk;
            debug!(
                "更新风控配置: strategy_config_id={}, inst_id={}",
                cfg.strategy_config_id, cfg.inst_id
            );
        } else {
            // 未有持仓时也允许提前缓存风险配置：用默认空持仓占位意义不大，这里选择忽略即可
            debug!(
                "收到风控配置但当前无持仓，忽略缓存: strategy_config_id={}, inst_id={}",
                cfg.strategy_config_id, cfg.inst_id
            );
        }
    }

    /// 更新持仓快照
    pub async fn upsert_position(&self, snapshot: PositionSnapshot, risk: BasicRiskConfig) {
        let mut guard = self.inner.write().await;
        let key = StrategyKey::new(snapshot.strategy_config_id, snapshot.inst_id.clone());

        if !snapshot.is_open {
            guard.positions.remove(&key);
            debug!(
                "清理持仓状态: strategy_config_id={}, inst_id={}",
                snapshot.strategy_config_id, snapshot.inst_id
            );
            return;
        }

        let moved = guard
            .positions
            .get(&key)
            .map(|s| s.moved_to_breakeven)
            .unwrap_or(false);

        guard.positions.insert(
            key,
            PositionRuntimeState {
                snapshot,
                risk,
                moved_to_breakeven: moved,
            },
        );
    }

    /// 处理 K线更新（可在 confirm=1 时调用）
    pub async fn on_candle(&self, market_candle: MarketCandle) {
        // 只在确认K线时触发，可以减少噪声（上层若传入未确认K线，这里不强制拦截）
        let inst_id = market_candle.inst_id.clone();
        let candle: CandleItem = market_candle.candle;

        let candidates = {
            let guard = self.inner.read().await;
            guard
                .positions
                .iter()
                .filter(|(k, st)| {
                    k.inst_id == inst_id && st.snapshot.is_open && !st.moved_to_breakeven
                })
                .map(|(k, st)| (k.clone(), st.clone()))
                .collect::<Vec<_>>()
        };

        if candidates.is_empty() {
            return;
        }

        for (key, st) in candidates {
            if !Self::is_enabled(&st.risk) {
                continue;
            }

            let ord_id = match st.snapshot.ord_id.as_deref() {
                Some(v) if !v.is_empty() => v,
                _ => {
                    warn!(
                        "缺少 ord_id，无法移动止损: strategy_config_id={}, inst_id={}",
                        key.strategy_config_id, key.inst_id
                    );
                    continue;
                }
            };

            let (triggered, breakeven_price) = match st.snapshot.pos_side {
                PositionSide::Long => {
                    let threshold = Self::breakeven_trigger_threshold(
                        PositionSide::Long,
                        st.snapshot.entry_price,
                        st.snapshot.initial_stop_loss,
                        st.risk.max_loss_percent,
                    );
                    (candle.h >= threshold, st.snapshot.entry_price)
                }
                PositionSide::Short => {
                    let threshold = Self::breakeven_trigger_threshold(
                        PositionSide::Short,
                        st.snapshot.entry_price,
                        st.snapshot.initial_stop_loss,
                        st.risk.max_loss_percent,
                    );
                    (candle.l <= threshold, st.snapshot.entry_price)
                }
                PositionSide::Both => {
                    // 当前系统的风控规则只覆盖单向持仓
                    continue;
                }
            };

            if !triggered {
                continue;
            }

            info!(
                "触发保本移动止损条件: strategy_config_id={}, inst_id={}, pos_side={:?}, entry={}",
                key.strategy_config_id, key.inst_id, st.snapshot.pos_side, st.snapshot.entry_price
            );

            let res = self
                .amender
                .move_stop_loss_to_price(&key.inst_id, ord_id, breakeven_price)
                .await;

            match res {
                Ok(_) => {
                    let mut guard = self.inner.write().await;
                    if let Some(s) = guard.positions.get_mut(&key) {
                        s.moved_to_breakeven = true;
                    }
                }
                Err(e) => {
                    warn!(
                        "移动止损失败(稍后会在后续K线重试): strategy_config_id={}, inst_id={}, ord_id={}, err={}",
                        key.strategy_config_id, key.inst_id, ord_id, e
                    );
                }
            }
        }
    }

    fn is_enabled(risk: &BasicRiskConfig) -> bool {
        risk.atr_take_profit_ratio.unwrap_or(0.0) > 0.0
    }

    /// 计算 1.5R 触发阈值
    fn breakeven_trigger_threshold(
        side: PositionSide,
        entry_price: f64,
        initial_stop_loss: Option<f64>,
        max_loss_percent: f64,
    ) -> f64 {
        let fallback_sl = match side {
            PositionSide::Long => entry_price * (1.0 - max_loss_percent),
            PositionSide::Short => entry_price * (1.0 + max_loss_percent),
            PositionSide::Both => entry_price,
        };
        let sl = initial_stop_loss.unwrap_or(fallback_sl);
        let r = (entry_price - sl).abs();
        let trigger_r = 1.5_f64;

        match side {
            PositionSide::Long => entry_price + trigger_r * r,
            PositionSide::Short => entry_price - trigger_r * r,
            PositionSide::Both => entry_price,
        }
    }
}
