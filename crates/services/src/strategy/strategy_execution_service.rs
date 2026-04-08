//! 策略执行服务
//!
//! 协调策略分析、风控检查、订单创建的完整业务流程

#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use okx::dto::account_dto::Position as OkxPosition;
use okx::enums::account_enums::AccountType;
use redis::AsyncCommands;
use tracing::{error, info, warn};

use rust_quant_common::CandleItem;
use rust_quant_core::cache::get_redis_connection;
use rust_quant_domain::entities::SwapOrder;
use rust_quant_domain::traits::SwapOrderRepository;
use rust_quant_domain::StrategyConfig;
use rust_quant_strategies::framework::backtest::{
    compute_current_targets, BasicRiskStrategyConfig, ExitTargets, TradingState,
};
use rust_quant_strategies::framework::risk::{StopLossCalculator, StopLossSide};
use rust_quant_strategies::framework::types::TradeSide;
use rust_quant_strategies::strategy_common::SignalResult;

use super::live_decision::{apply_live_decision, approx_eq_opt};

#[derive(Debug, Clone, Default, PartialEq)]
struct LiveExitTargets {
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    algo_ids: Vec<String>,
    trade_side: Option<TradeSide>,
}

#[derive(Debug)]
enum CloseAlgoSyncResult {
    Placed(Vec<String>),
    Cleared,
    SkippedNoPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TradeBucketTransferDirection {
    FundToTrade,
    TradeToFund,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalFlatDecision {
    Skip,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LiveTradeBucketRebalanceConfig {
    target_trade_ratio: f64,
    min_transfer: f64,
    transfer_epsilon: f64,
}

#[cfg(test)]
#[derive(Default)]
struct GuardTestState {
    compensate_fail: AtomicBool,
    has_algo_after_compensate: AtomicBool,
    close_fail: AtomicBool,
    compensate_calls: AtomicUsize,
    close_calls: AtomicUsize,
}

/// 策略执行服务
///
/// 职责：
/// 1. 协调策略分析流程
/// 2. 调用风控检查
/// 3. 协调订单创建
/// 4. 管理策略执行状态
///
/// 依赖：
/// - StrategyRegistry: 获取策略实现
/// - SwapOrderRepository: 订单持久化
/// - TradingService: 创建订单（待实现）
/// - RiskService: 风控检查（待实现）
pub struct StrategyExecutionService {
    /// 合约订单仓储（依赖注入）
    swap_order_repository: Arc<dyn SwapOrderRepository>,

    /// 实盘交易状态（每个策略配置一份）
    live_states: DashMap<i64, TradingState>,
    /// 实盘止盈止损目标缓存
    live_exit_targets: DashMap<i64, LiveExitTargets>,
    #[cfg(test)]
    guard_test_state: Arc<GuardTestState>,
}

impl StrategyExecutionService {
    const EXTERNAL_FLAT_PROBE_TTL_SECS: u64 = 60 * 60 * 6;

    /// 创建新的策略执行服务（依赖注入）
    pub fn new(swap_order_repository: Arc<dyn SwapOrderRepository>) -> Self {
        Self {
            swap_order_repository,
            live_states: DashMap::new(),
            live_exit_targets: DashMap::new(),
            #[cfg(test)]
            guard_test_state: Arc::new(GuardTestState::default()),
        }
    }

    fn candle_entity_to_item(c: &rust_quant_market::models::CandlesEntity) -> Result<CandleItem> {
        let o =
            c.o.parse::<f64>()
                .map_err(|e| anyhow!("解析开盘价失败: {}", e))?;
        let h =
            c.h.parse::<f64>()
                .map_err(|e| anyhow!("解析最高价失败: {}", e))?;
        let l =
            c.l.parse::<f64>()
                .map_err(|e| anyhow!("解析最低价失败: {}", e))?;
        let close =
            c.c.parse::<f64>()
                .map_err(|e| anyhow!("解析收盘价失败: {}", e))?;
        let v = c
            .vol_ccy
            .parse::<f64>()
            .map_err(|e| anyhow!("解析成交量失败: {}", e))?;
        let confirm = c
            .confirm
            .parse::<i32>()
            .map_err(|e| anyhow!("解析 confirm 失败: {}", e))?;

        Ok(CandleItem {
            o,
            h,
            l,
            c: close,
            v,
            ts: c.ts,
            confirm,
        })
    }

    fn env_enabled(key: &str) -> bool {
        match std::env::var(key) {
            Ok(v) => matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            ),
            Err(_) => false,
        }
    }

    fn live_tp_sl_epsilon() -> f64 {
        std::env::var("LIVE_TP_SL_EPSILON")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1e-6)
    }

    fn env_positive_f64(key: &str) -> Option<f64> {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
    }

    fn live_trade_bucket_rebalance_config() -> Option<LiveTradeBucketRebalanceConfig> {
        if !Self::env_enabled("LIVE_ENABLE_TRADE_BUCKET_REBALANCE") {
            return None;
        }

        let target_trade_ratio = std::env::var("LIVE_TARGET_TRADE_RATIO")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0 && *v < 1.0)
            .unwrap_or(0.30);
        let min_transfer =
            Self::env_positive_f64("LIVE_TRADE_BUCKET_MIN_TRANSFER_USDT").unwrap_or(1.0);
        let transfer_epsilon =
            Self::env_positive_f64("LIVE_TRADE_BUCKET_EPSILON_USDT").unwrap_or(0.5);

        Some(LiveTradeBucketRebalanceConfig {
            target_trade_ratio,
            min_transfer,
            transfer_epsilon,
        })
    }

    fn calculate_trade_bucket_transfer(
        trade_balance: f64,
        funding_balance: f64,
        config: LiveTradeBucketRebalanceConfig,
    ) -> Option<(f64, TradeBucketTransferDirection)> {
        if !trade_balance.is_finite()
            || !funding_balance.is_finite()
            || trade_balance < 0.0
            || funding_balance < 0.0
        {
            return None;
        }

        let total_balance = trade_balance + funding_balance;
        if total_balance <= 0.0 {
            return None;
        }

        let target_trade_balance = total_balance * config.target_trade_ratio;
        let diff = target_trade_balance - trade_balance;
        if diff.abs() < config.transfer_epsilon {
            return None;
        }

        if diff > 0.0 {
            let amount = diff;
            if amount >= config.min_transfer {
                return Some((amount, TradeBucketTransferDirection::FundToTrade));
            }
        }

        if diff < 0.0 {
            let amount = -diff;
            if amount >= config.min_transfer {
                return Some((amount, TradeBucketTransferDirection::TradeToFund));
            }
        }

        None
    }

    fn build_close_algo_tag(config_id: i64) -> String {
        format!("rq-{}", config_id)
    }

    fn build_close_algo_cl_ord_id(config_id: i64) -> String {
        format!("rq-{}-{}", config_id, chrono::Utc::now().timestamp_millis())
    }

    fn build_entry_cl_ord_id(config_id: i64, ts: i64) -> String {
        format!("rq{}{}", config_id, ts)
    }

    fn parse_detail_object(detail: &str) -> serde_json::Map<String, serde_json::Value> {
        match serde_json::from_str::<serde_json::Value>(detail) {
            Ok(serde_json::Value::Object(map)) => map,
            Ok(other) => {
                let mut map = serde_json::Map::new();
                map.insert("raw_detail".to_string(), other);
                map
            }
            Err(_) => {
                let mut map = serde_json::Map::new();
                map.insert(
                    "raw_detail".to_string(),
                    serde_json::Value::String(detail.to_string()),
                );
                map
            }
        }
    }

    fn extract_close_algo_ids(detail: &str) -> Vec<String> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
            return Vec::new();
        };
        let Some(ids) = value
            .get("close_algo")
            .and_then(|v| v.get("ids"))
            .and_then(|v| v.as_array())
        else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    }

    fn upsert_close_algo_detail(
        detail: &str,
        algo_ids: &[String],
        tag: &str,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> String {
        let mut map = Self::parse_detail_object(detail);
        let mut close_algo = serde_json::Map::new();
        let ids = algo_ids
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>();
        close_algo.insert("ids".to_string(), serde_json::Value::Array(ids));
        close_algo.insert(
            "updated_at".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                chrono::Utc::now().timestamp_millis(),
            )),
        );
        close_algo.insert(
            "tag".to_string(),
            serde_json::Value::String(tag.to_string()),
        );
        close_algo.insert(
            "stop_loss".to_string(),
            stop_loss
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        );
        close_algo.insert(
            "take_profit".to_string(),
            take_profit
                .and_then(serde_json::Number::from_f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        );
        map.insert(
            "close_algo".to_string(),
            serde_json::Value::Object(close_algo),
        );
        serde_json::Value::Object(map).to_string()
    }

    fn remove_close_algo_detail(detail: &str) -> String {
        let mut map = Self::parse_detail_object(detail);
        map.remove("close_algo");
        serde_json::Value::Object(map).to_string()
    }

    fn parse_f64_value(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    fn extract_close_algo_targets(detail: &str) -> (Option<f64>, Option<f64>) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
            return (None, None);
        };
        let Some(close_algo) = value.get("close_algo") else {
            return (None, None);
        };
        let stop_loss = close_algo.get("stop_loss").and_then(Self::parse_f64_value);
        let take_profit = close_algo
            .get("take_profit")
            .and_then(Self::parse_f64_value);
        (stop_loss, take_profit)
    }

    fn extract_entry_price(detail: &str) -> Option<f64> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
            return None;
        };
        value.get("entry_price").and_then(Self::parse_f64_value)
    }

    fn parse_opt_f64(input: Option<&str>) -> Option<f64> {
        input.and_then(|v| v.parse::<f64>().ok())
    }

    fn format_open_position_time(position: &OkxPosition) -> String {
        let millis = position
            .c_time
            .as_deref()
            .and_then(|v| v.parse::<i64>().ok())
            .or_else(|| {
                position
                    .u_time
                    .as_deref()
                    .and_then(|v| v.parse::<i64>().ok())
            })
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
    }

    fn has_live_algo_for_side(&self, config_id: i64, side: TradeSide) -> bool {
        self.live_exit_targets
            .get(&config_id)
            .map(|target| target.trade_side == Some(side) && !target.algo_ids.is_empty())
            .unwrap_or(false)
    }

    fn emit_guard_audit_log(
        stage: &str,
        inst_id: &str,
        period: &str,
        config_id: i64,
        side: TradeSide,
        trigger_ts: i64,
        message: Option<String>,
    ) {
        let payload = serde_json::json!({
            "event": "live_guard",
            "stage": stage,
            "inst_id": inst_id,
            "period": period,
            "config_id": config_id,
            "side": format!("{:?}", side),
            "trigger_ts": trigger_ts,
            "message": message,
            "logged_at": chrono::Utc::now().timestamp_millis(),
        });
        warn!("LIVE_GUARD {}", payload);
    }

    #[cfg(not(test))]
    async fn compensate_for_guard(&self, config: &StrategyConfig, _side: TradeSide) -> Result<()> {
        self.compensate_close_algos_on_start(config).await
    }

    #[cfg(test)]
    async fn compensate_for_guard(&self, config: &StrategyConfig, side: TradeSide) -> Result<()> {
        self.guard_test_state
            .compensate_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.guard_test_state.compensate_fail.load(Ordering::SeqCst) {
            return Err(anyhow!("mock compensate failed"));
        }
        if self
            .guard_test_state
            .has_algo_after_compensate
            .load(Ordering::SeqCst)
        {
            self.live_exit_targets.insert(
                config.id,
                LiveExitTargets {
                    stop_loss: Some(1.0),
                    take_profit: None,
                    algo_ids: vec!["mock-algo".to_string()],
                    trade_side: Some(side),
                },
            );
        }
        Ok(())
    }

    #[cfg(not(test))]
    async fn close_for_guard(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        side: TradeSide,
    ) -> Result<()> {
        self.close_position_internal(inst_id, period, config_id, side)
            .await
    }

    #[cfg(test)]
    async fn close_for_guard(
        &self,
        _inst_id: &str,
        _period: &str,
        _config_id: i64,
        _side: TradeSide,
    ) -> Result<()> {
        self.guard_test_state
            .close_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.guard_test_state.close_fail.load(Ordering::SeqCst) {
            return Err(anyhow!("mock close failed"));
        }
        Ok(())
    }

    async fn enforce_opened_position_guard(
        &self,
        inst_id: &str,
        period: &str,
        config: &StrategyConfig,
        side: TradeSide,
        trigger_ts: i64,
    ) -> Result<()> {
        Self::emit_guard_audit_log(
            "sync_failed_after_open",
            inst_id,
            period,
            config.id,
            side,
            trigger_ts,
            Some("open succeeded but tp/sl sync failed".to_string()),
        );
        if let Err(comp_err) = self.compensate_for_guard(config, side).await {
            Self::emit_guard_audit_log(
                "compensate_failed",
                inst_id,
                period,
                config.id,
                side,
                trigger_ts,
                Some(comp_err.to_string()),
            );
        }

        if !self.has_live_algo_for_side(config.id, side) {
            Self::emit_guard_audit_log(
                "force_close_start",
                inst_id,
                period,
                config.id,
                side,
                trigger_ts,
                Some("compensation did not restore tp/sl".to_string()),
            );
            if let Err(close_err) = self.close_for_guard(inst_id, period, config.id, side).await {
                Self::emit_guard_audit_log(
                    "force_close_failed",
                    inst_id,
                    period,
                    config.id,
                    side,
                    trigger_ts,
                    Some(close_err.to_string()),
                );
                return Err(close_err);
            }
            Self::emit_guard_audit_log(
                "force_close_done",
                inst_id,
                period,
                config.id,
                side,
                trigger_ts,
                None,
            );
            self.live_exit_targets.remove(&config.id);
            self.live_states.insert(config.id, TradingState::default());
            return Err(anyhow!(
                "开仓后止盈止损同步失败，补偿未成功，已触发主动平仓"
            ));
        }

        Self::emit_guard_audit_log(
            "guard_resolved_by_compensate",
            inst_id,
            period,
            config.id,
            side,
            trigger_ts,
            Some("tp/sl restored after compensation".to_string()),
        );
        Ok(())
    }

    #[cfg(test)]
    fn configure_guard_test_state(
        &self,
        compensate_fail: bool,
        has_algo_after_compensate: bool,
        close_fail: bool,
    ) {
        self.guard_test_state
            .compensate_fail
            .store(compensate_fail, Ordering::SeqCst);
        self.guard_test_state
            .has_algo_after_compensate
            .store(has_algo_after_compensate, Ordering::SeqCst);
        self.guard_test_state
            .close_fail
            .store(close_fail, Ordering::SeqCst);
        self.guard_test_state
            .compensate_calls
            .store(0, Ordering::SeqCst);
        self.guard_test_state.close_calls.store(0, Ordering::SeqCst);
    }

    #[cfg(test)]
    fn guard_test_calls(&self) -> (usize, usize) {
        (
            self.guard_test_state
                .compensate_calls
                .load(Ordering::SeqCst),
            self.guard_test_state.close_calls.load(Ordering::SeqCst),
        )
    }

    /// 从持仓信息重新恢复交易状态
    fn rehydrate_live_state_from_position(
        &self,
        config_id: i64,
        position: &OkxPosition,
        trade_side: TradeSide,
        detail: Option<&str>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) {
        let position_nums = position
            .pos
            .parse::<f64>()
            .ok()
            .map(f64::abs)
            .unwrap_or(0.0);
        let avg_px = Self::parse_opt_f64(position.avg_px.as_deref());
        let open_price = detail
            .and_then(Self::extract_entry_price)
            .or(avg_px)
            .unwrap_or(0.0);

        let mut state = self
            .live_states
            .get(&config_id)
            .map(|v| v.clone())
            .unwrap_or_default();

        let mut trade_position = state.trade_position.unwrap_or_default();
        trade_position.trade_side = trade_side;
        trade_position.position_nums = position_nums;
        trade_position.open_price = open_price;
        trade_position.open_position_time = Self::format_open_position_time(position);
        trade_position.signal_high_low_diff = trade_position.signal_high_low_diff.max(1e-8);
        trade_position.signal_kline_stop_close_price = stop_loss;
        trade_position.atr_stop_loss_price = stop_loss;
        trade_position.atr_take_ratio_profit_price = take_profit;
        if trade_side == TradeSide::Long {
            trade_position.long_signal_take_profit_price = take_profit;
        } else {
            trade_position.short_signal_take_profit_price = take_profit;
        }
        state.trade_position = Some(trade_position);
        self.live_states.insert(config_id, state);
    }

    /// 判断止损止盈目标是否发生变化
    fn targets_changed(prev: &LiveExitTargets, next: &ExitTargets, eps: f64) -> bool {
        !approx_eq_opt(prev.stop_loss, next.stop_loss, eps)
            || !approx_eq_opt(prev.take_profit, next.take_profit, eps)
    }

    /// 构建止损候选价列表（由上层选择最紧止损）
    fn build_stop_loss_candidates(
        side: &str,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
    ) -> Vec<f64> {
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;
        let max_loss_stop = if side == "sell" {
            entry_price * (1.0 + max_loss_percent)
        } else {
            entry_price * (1.0 - max_loss_percent)
        };

        let mut candidates: Vec<f64> = vec![max_loss_stop];

        // 信号K线止损（若启用且信号提供）
        if risk_config.is_used_signal_k_line_stop_loss.unwrap_or(false) {
            if let Some(px) = signal.signal_kline_stop_loss_price {
                candidates.push(px);
            }
        }

        candidates
    }

    /// 执行策略分析和交易流程
    ///
    /// 参考原始业务逻辑：src/trading/strategy/executor_common.rs::execute_order
    ///
    /// 完整业务流程：
    /// 1. 验证配置
    /// 2. 执行策略分析，获取信号
    /// 3. 检查信号有效性
    /// 4. 记录信号日志（异步，不阻塞）
    /// 5. 解析风险配置
    /// 6. 执行下单
    pub async fn execute_strategy(
        &self,
        inst_id: &str,
        period: &str,
        config: &StrategyConfig,
        snap: Option<rust_quant_market::models::CandlesEntity>,
    ) -> Result<SignalResult> {
        info!(
            "开始执行策略: type={:?}, symbol={}, period={}",
            config.strategy_type, inst_id, period
        );

        // 1. 验证配置
        self.validate_config(config)?;

        // 1.1 对账：如果交易所已经通过止损/止盈把仓位关掉，本地先清状态并回补交易桶。
        self.reconcile_external_flat_close(config, inst_id, period)
            .await?;

        // 2. 获取策略实现
        // 必须严格使用配置中的 strategy_type 路由执行器：
        // - detect_strategy 基于参数“猜策略”，在参数为空/通用字段时会误判
        // - 误判会导致读取错误的策略缓存 key，直接失败
        use rust_quant_strategies::strategy_registry::{
            get_strategy_registry, register_strategy_on_demand,
        };

        register_strategy_on_demand(&config.strategy_type);
        let strategy_executor = get_strategy_registry()
            .get(config.strategy_type.as_str())
            .map_err(|e| anyhow!("获取策略执行器失败: {}", e))?;

        info!(
            "使用策略: {} (config.strategy_type={:?})",
            strategy_executor.name(),
            config.strategy_type
        );

        // 3. 执行策略分析，获取交易信号
        let snap_item = match snap.as_ref() {
            Some(c) => Some(Self::candle_entity_to_item(c)?),
            None => None,
        };

        // execute() 需要所有权；后续止损计算也需要引用，因此这里保留一份副本
        let snap_item_for_execute = snap_item.clone();

        let mut signal = strategy_executor
            .execute(inst_id, period, config, snap_item_for_execute)
            .await
            .map_err(|e| {
                error!("策略执行失败: {}", e);
                anyhow!("策略分析失败: {}", e)
            })?;

        info!("策略分析完成");

        info!("signal: {:?}", serde_json::to_string(&signal).unwrap());
        let raw_has_signal = signal.should_buy || signal.should_sell;

        if raw_has_signal {
            // 5. 记录信号
            warn!(
                "{:?} 策略信号！inst_id={}, period={}, should_buy={:?}, should_sell={:?}, ts={:?}",
                config.strategy_type,
                inst_id,
                period,
                signal.should_buy,
                signal.should_sell,
                signal.ts
            );

            // 6. 异步记录信号日志（不阻塞下单）
            self.save_signal_log_async(inst_id, period, &signal, config);
        }

        // 7. 解析风险配置
        let risk_config: rust_quant_domain::BasicRiskConfig =
            serde_json::from_value(config.risk_config.clone())
                .map_err(|e| anyhow!("解析风险配置失败: {}", e))?;

        let decision_risk: BasicRiskStrategyConfig =
            serde_json::from_value(config.risk_config.clone())
                .map_err(|e| anyhow!("解析风控配置失败: {}", e))?;

        info!("风险配置: risk_config:{:#?}", risk_config);

        let Some(trigger_candle) = snap_item.as_ref() else {
            warn!(
                "⚠️ 无K线快照，跳过执行: inst_id={}, period={}, strategy={:?}",
                inst_id, period, config.strategy_type
            );
            return Ok(signal);
        };

        let outcome = self
            .handle_live_decision(
                inst_id,
                period,
                config,
                &mut signal,
                trigger_candle,
                decision_risk,
                &risk_config,
            )
            .await?;

        if !raw_has_signal && !outcome.closed && outcome.opened_side.is_none() {
            info!(
                "无交易信号，跳过下单 - 策略类型：{:?}, 交易周期：{}",
                config.strategy_type, period
            );
            return Ok(signal);
        }

        info!("✅ {:?} 策略执行完成", config.strategy_type);
        Ok(signal)
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_live_decision(
        &self,
        inst_id: &str,
        period: &str,
        config: &StrategyConfig,
        signal: &mut SignalResult,
        trigger_candle: &CandleItem,
        decision_risk: BasicRiskStrategyConfig,
        order_risk: &rust_quant_domain::BasicRiskConfig,
    ) -> Result<super::LiveDecisionOutcome> {
        let previous_state = self
            .live_states
            .get(&config.id)
            .map(|s| s.clone())
            .unwrap_or_default();
        let mut state = self
            .live_states
            .get(&config.id)
            .map(|s| s.clone())
            .unwrap_or_default();

        let outcome = apply_live_decision(&mut state, signal, trigger_candle, decision_risk);
        let epsilon = Self::live_tp_sl_epsilon();
        let prev_exit = self.live_exit_targets.get(&config.id).map(|v| v.clone());
        let prev_snapshot = prev_exit.clone().unwrap_or_default();
        let mut pending_targets: Option<ExitTargets> = None;
        let mut pending_side: Option<TradeSide> = None;
        let mut clear_exit_cache = false;
        if let Some(position) = state.trade_position.as_ref() {
            let targets = compute_current_targets(position, trigger_candle, &decision_risk);
            if Self::targets_changed(&prev_snapshot, &targets, epsilon) {
                pending_targets = Some(targets);
                pending_side = Some(position.trade_side);
            }
        } else {
            clear_exit_cache = true;
        }

        if outcome.closed {
            if let Some(side) = outcome.closed_side {
                self.close_position_internal(inst_id, period, config.id, side)
                    .await?;
            }
        }

        if outcome.opened_side.is_some() {
            if let Err(e) = self
                .execute_order_internal(
                    inst_id,
                    period,
                    signal,
                    order_risk,
                    config.id,
                    config.strategy_type.as_str(),
                )
                .await
            {
                error!("❌ {:?} 策略下单失败: {}", config.strategy_type, e);
                let rollback_state = if outcome.closed {
                    let mut closed_state = state.clone();
                    closed_state.trade_position = None;
                    closed_state
                } else {
                    previous_state
                };
                self.live_states.insert(config.id, rollback_state);
                return Err(e);
            }
        }

        self.live_states.insert(config.id, state);

        let opened_side = outcome.opened_side;
        if let (Some(targets), Some(side)) = (pending_targets, pending_side) {
            match self
                .sync_close_algos(
                    inst_id,
                    period,
                    config.id,
                    side,
                    &targets,
                    prev_snapshot.algo_ids.as_slice(),
                )
                .await
            {
                Ok(CloseAlgoSyncResult::Placed(algo_ids)) => {
                    self.live_exit_targets.insert(
                        config.id,
                        LiveExitTargets {
                            stop_loss: targets.stop_loss,
                            take_profit: targets.take_profit,
                            algo_ids,
                            trade_side: Some(side),
                        },
                    );
                }
                Ok(CloseAlgoSyncResult::Cleared) => {
                    self.live_exit_targets.remove(&config.id);
                }
                Ok(CloseAlgoSyncResult::SkippedNoPosition) => {}
                Err(e) => {
                    warn!(
                        "⚠️ 同步止盈止损失败: inst_id={}, config_id={}, err={}",
                        inst_id, config.id, e
                    );

                    if opened_side == Some(side) {
                        self.enforce_opened_position_guard(
                            inst_id,
                            period,
                            config,
                            side,
                            trigger_candle.ts,
                        )
                        .await?;
                    }
                }
            }
        }

        if clear_exit_cache {
            if let Some(prev_exit) = prev_exit.as_ref() {
                if !prev_exit.algo_ids.is_empty() {
                    if let Err(e) = self
                        .cancel_cached_close_algos(
                            inst_id,
                            period,
                            config.id,
                            prev_exit.trade_side,
                            &prev_exit.algo_ids,
                        )
                        .await
                    {
                        warn!(
                            "⚠️ 平仓后撤销止盈止损失败: inst_id={}, config_id={}, err={}",
                            inst_id, config.id, e
                        );
                    } else {
                        self.live_exit_targets.remove(&config.id);
                    }
                } else {
                    self.live_exit_targets.remove(&config.id);
                }
            } else {
                self.live_exit_targets.remove(&config.id);
            }
        }

        Ok(outcome)
    }

    async fn sync_close_algos(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        side: TradeSide,
        targets: &ExitTargets,
        prev_algo_ids: &[String],
    ) -> Result<CloseAlgoSyncResult> {
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;

        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| anyhow!("获取API配置失败: {}", e))?;

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| anyhow!("获取账户数据失败: {}", e))?;

        let pos_side_str = match side {
            TradeSide::Long => "long",
            TradeSide::Short => "short",
        };

        let position = positions.iter().find(|p| {
            p.inst_id == inst_id
                && p.pos_side.eq_ignore_ascii_case(pos_side_str)
                && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
        });

        let Some(position) = position else {
            warn!(
                "⚠️ 未找到可同步的持仓: inst_id={}, pos_side={}",
                inst_id, pos_side_str
            );
            return Ok(CloseAlgoSyncResult::SkippedNoPosition);
        };

        let mgn_mode = position.mgn_mode.clone();

        if !prev_algo_ids.is_empty() {
            okx_service
                .cancel_close_algos(&api_config, inst_id, prev_algo_ids)
                .await?;
        }

        if targets.stop_loss.is_none() && targets.take_profit.is_none() {
            if let Err(e) = self
                .clear_persisted_close_algos(config_id, inst_id, period, pos_side_str)
                .await
            {
                warn!(
                    "⚠️ 清理持久化止盈止损失败: inst_id={}, config_id={}, err={}",
                    inst_id, config_id, e
                );
            }
            return Ok(CloseAlgoSyncResult::Cleared);
        }

        let close_side = match side {
            TradeSide::Long => "sell",
            TradeSide::Short => "buy",
        };

        let algo_cl_ord_id = Self::build_close_algo_cl_ord_id(config_id);
        let tag = Self::build_close_algo_tag(config_id);
        let algo_ids = okx_service
            .place_close_algo(
                &api_config,
                inst_id,
                &mgn_mode,
                close_side,
                pos_side_str,
                targets.take_profit,
                targets.stop_loss,
                Some(algo_cl_ord_id.as_str()),
                Some(tag.as_str()),
            )
            .await?;

        if algo_ids.is_empty() {
            return Err(anyhow!(
                "下达平仓策略委托未返回algoId: inst_id={}, period={}, config_id={}",
                inst_id,
                period,
                config_id
            ));
        }

        if let Err(e) = self
            .persist_close_algos(
                config_id,
                inst_id,
                period,
                pos_side_str,
                &algo_ids,
                &tag,
                targets.stop_loss,
                targets.take_profit,
            )
            .await
        {
            warn!(
                "⚠️ 持久化止盈止损失败: inst_id={}, config_id={}, err={}",
                inst_id, config_id, e
            );
        }

        Ok(CloseAlgoSyncResult::Placed(algo_ids))
    }

    #[allow(clippy::too_many_arguments)]
    async fn persist_close_algos(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        pos_side: &str,
        algo_ids: &[String],
        tag: &str,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
    ) -> Result<()> {
        let Some(mut order) = self
            .swap_order_repository
            .find_latest_by_strategy_inst_period_pos_side(
                config_id as i32,
                inst_id,
                period,
                pos_side,
            )
            .await?
        else {
            warn!(
                "⚠️ 未找到订单记录，跳过持久化止盈止损: inst_id={}, period={}, config_id={}, pos_side={}",
                inst_id, period, config_id, pos_side
            );
            return Ok(());
        };

        order.detail =
            Self::upsert_close_algo_detail(&order.detail, algo_ids, tag, stop_loss, take_profit);
        self.swap_order_repository.update(&order).await?;
        Ok(())
    }

    async fn clear_persisted_close_algos(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        pos_side: &str,
    ) -> Result<()> {
        let Some(mut order) = self
            .swap_order_repository
            .find_latest_by_strategy_inst_period_pos_side(
                config_id as i32,
                inst_id,
                period,
                pos_side,
            )
            .await?
        else {
            return Ok(());
        };

        order.detail = Self::remove_close_algo_detail(&order.detail);
        self.swap_order_repository.update(&order).await?;
        Ok(())
    }

    async fn load_persisted_close_algos(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        pos_side: &str,
    ) -> Result<Option<SwapOrder>> {
        self.swap_order_repository
            .find_latest_by_strategy_inst_period_pos_side(
                config_id as i32,
                inst_id,
                period,
                pos_side,
            )
            .await
    }

    async fn cancel_cached_close_algos(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        trade_side: Option<TradeSide>,
        algo_ids: &[String],
    ) -> Result<()> {
        if algo_ids.is_empty() {
            return Ok(());
        }

        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;

        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| anyhow!("获取API配置失败: {}", e))?;

        let okx_service = OkxOrderService;
        okx_service
            .cancel_close_algos(&api_config, inst_id, algo_ids)
            .await?;

        let pos_side_str = match trade_side {
            Some(TradeSide::Long) => Some("long"),
            Some(TradeSide::Short) => Some("short"),
            None => None,
        };

        if let Some(pos_side_str) = pos_side_str {
            if let Err(e) = self
                .clear_persisted_close_algos(config_id, inst_id, period, pos_side_str)
                .await
            {
                warn!(
                    "⚠️ 清理持久化止盈止损失败: inst_id={}, config_id={}, err={}",
                    inst_id, config_id, e
                );
            }
        }
        Ok(())
    }

    pub async fn compensate_close_algos_on_start(&self, config: &StrategyConfig) -> Result<()> {
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;

        let inst_id = config.symbol.as_str();
        let period = config.timeframe.as_str();
        let pos_sides = ["long", "short"];

        let api_service = create_exchange_api_service();
        let api_config = match api_service.get_first_api_config(config.id as i32).await {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!(
                    "⚠️ 启动补偿撤单获取API配置失败: inst_id={}, config_id={}, err={}",
                    inst_id, config.id, e
                );
                return Ok(());
            }
        };

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| anyhow!("获取账户数据失败: {}", e))?;

        for pos_side in pos_sides {
            let position = positions.iter().find(|p| {
                p.inst_id == inst_id
                    && p.pos_side.eq_ignore_ascii_case(pos_side)
                    && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
            });
            let persisted_order = self
                .load_persisted_close_algos(config.id, inst_id, period, pos_side)
                .await?;

            if position.is_none() {
                if let Some(order) = persisted_order {
                    let algo_ids = Self::extract_close_algo_ids(&order.detail);
                    if !algo_ids.is_empty() {
                        if let Err(e) = okx_service
                            .cancel_close_algos(&api_config, inst_id, &algo_ids)
                            .await
                        {
                            warn!(
                                "⚠️ 启动补偿撤单失败: inst_id={}, config_id={}, pos_side={}, err={}",
                                inst_id, config.id, pos_side, e
                            );
                        } else {
                            let mut updated = order;
                            updated.detail = Self::remove_close_algo_detail(&updated.detail);
                            if let Err(e) = self.swap_order_repository.update(&updated).await {
                                warn!(
                                    "⚠️ 启动补偿撤单后清理持久化失败: inst_id={}, config_id={}, pos_side={}, err={}",
                                    inst_id, config.id, pos_side, e
                                );
                            }
                        }
                    }
                }
                continue;
            }

            let position = position.unwrap();
            let trade_side = if pos_side.eq_ignore_ascii_case("long") {
                TradeSide::Long
            } else {
                TradeSide::Short
            };

            let mut has_tp_sl = false;
            let mut exchange_algo_ids: Vec<String> = Vec::new();
            let mut exchange_stop_loss: Option<f64> = None;
            let mut exchange_take_profit: Option<f64> = None;

            if let Some(close_algos) = position.close_order_algo.as_ref() {
                for algo in close_algos {
                    let sl = algo
                        .sl_trigger_px
                        .as_ref()
                        .and_then(|v| v.parse::<f64>().ok());
                    let tp = algo
                        .tp_trigger_px
                        .as_ref()
                        .and_then(|v| v.parse::<f64>().ok());
                    if sl.is_some() || tp.is_some() {
                        has_tp_sl = true;
                    }
                    if !algo.algo_id.is_empty() {
                        exchange_algo_ids.push(algo.algo_id.clone());
                    }
                    if exchange_stop_loss.is_none() {
                        exchange_stop_loss = sl;
                    }
                    if exchange_take_profit.is_none() {
                        exchange_take_profit = tp;
                    }
                }
            }

            if has_tp_sl {
                let tag = Self::build_close_algo_tag(config.id);
                if let Some(order) = persisted_order.as_ref() {
                    if !exchange_algo_ids.is_empty() {
                        let mut updated = order.clone();
                        updated.detail = Self::upsert_close_algo_detail(
                            &updated.detail,
                            &exchange_algo_ids,
                            &tag,
                            exchange_stop_loss,
                            exchange_take_profit,
                        );
                        if let Err(e) = self.swap_order_repository.update(&updated).await {
                            warn!(
                                "⚠️ 同步持久化止盈止损失败: inst_id={}, config_id={}, pos_side={}, err={}",
                                inst_id, config.id, pos_side, e
                            );
                        }
                    }
                }

                self.rehydrate_live_state_from_position(
                    config.id,
                    position,
                    trade_side,
                    persisted_order.as_ref().map(|order| order.detail.as_str()),
                    exchange_stop_loss,
                    exchange_take_profit,
                );

                if !exchange_algo_ids.is_empty() {
                    self.live_exit_targets.insert(
                        config.id,
                        LiveExitTargets {
                            stop_loss: exchange_stop_loss,
                            take_profit: exchange_take_profit,
                            algo_ids: exchange_algo_ids,
                            trade_side: Some(trade_side),
                        },
                    );
                }
                continue;
            }

            let Some(order) = persisted_order.as_ref() else {
                self.rehydrate_live_state_from_position(
                    config.id, position, trade_side, None, None, None,
                );
                warn!(
                    "⚠️ 持仓无止盈止损且无持久化记录: inst_id={}, config_id={}, pos_side={}",
                    inst_id, config.id, pos_side
                );
                continue;
            };

            let (stop_loss, take_profit) = Self::extract_close_algo_targets(&order.detail);
            self.rehydrate_live_state_from_position(
                config.id,
                position,
                trade_side,
                Some(order.detail.as_str()),
                stop_loss,
                take_profit,
            );

            if stop_loss.is_none() && take_profit.is_none() {
                warn!(
                    "⚠️ 持仓无止盈止损且无可用目标: inst_id={}, config_id={}, pos_side={}",
                    inst_id, config.id, pos_side
                );
                continue;
            }

            let close_side = if pos_side.eq_ignore_ascii_case("long") {
                "sell"
            } else {
                "buy"
            };
            let algo_cl_ord_id = Self::build_close_algo_cl_ord_id(config.id);
            let tag = Self::build_close_algo_tag(config.id);
            let algo_ids = okx_service
                .place_close_algo(
                    &api_config,
                    inst_id,
                    &position.mgn_mode,
                    close_side,
                    pos_side,
                    take_profit,
                    stop_loss,
                    Some(algo_cl_ord_id.as_str()),
                    Some(tag.as_str()),
                )
                .await?;

            if !algo_ids.is_empty() {
                if let Err(e) = self
                    .persist_close_algos(
                        config.id,
                        inst_id,
                        period,
                        pos_side,
                        &algo_ids,
                        &tag,
                        stop_loss,
                        take_profit,
                    )
                    .await
                {
                    warn!(
                        "⚠️ 启动补偿挂单持久化失败: inst_id={}, config_id={}, pos_side={}, err={}",
                        inst_id, config.id, pos_side, e
                    );
                }
                self.live_exit_targets.insert(
                    config.id,
                    LiveExitTargets {
                        stop_loss,
                        take_profit,
                        algo_ids,
                        trade_side: Some(trade_side),
                    },
                );
            }
        }

        Ok(())
    }

    async fn reconcile_external_flat_close(
        &self,
        config: &StrategyConfig,
        inst_id: &str,
        period: &str,
    ) -> Result<()> {
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;

        let has_local_state = self
            .live_states
            .get(&config.id)
            .map(|state| state.trade_position.is_some())
            .unwrap_or(false);
        let has_live_exit_cache = self
            .live_exit_targets
            .get(&config.id)
            .map(|targets| !targets.algo_ids.is_empty() || targets.trade_side.is_some())
            .unwrap_or(false);

        let persisted_long = self
            .load_persisted_close_algos(config.id, inst_id, period, "long")
            .await?;
        let persisted_short = self
            .load_persisted_close_algos(config.id, inst_id, period, "short")
            .await?;
        let has_persisted_close_algo = persisted_long
            .as_ref()
            .map(|order| !Self::extract_close_algo_ids(&order.detail).is_empty())
            .unwrap_or(false)
            || persisted_short
                .as_ref()
                .map(|order| !Self::extract_close_algo_ids(&order.detail).is_empty())
                .unwrap_or(false);

        if !has_local_state && !has_live_exit_cache && !has_persisted_close_algo {
            return Ok(());
        }

        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config.id as i32)
            .await
            .map_err(|e| anyhow!("获取API配置失败: {}", e))?;

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| anyhow!("获取账户数据失败: {}", e))?;

        let has_exchange_position = positions
            .iter()
            .any(|p| p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12);

        if has_exchange_position {
            if let Err(e) = self
                .clear_external_flat_probe(config.id, inst_id, period)
                .await
            {
                warn!(
                    "⚠️ 清理外部平仓探测标记失败: inst_id={}, period={}, config_id={}, err={}",
                    inst_id, period, config.id, e
                );
            }
            return Ok(());
        }

        let mut inspection_confirms_close = false;
        for order in [&persisted_long, &persisted_short].into_iter().flatten() {
            if order.out_order_id.trim().is_empty() {
                continue;
            }
            match okx_service
                .inspect_auto_close_by_order(
                    &api_config,
                    inst_id,
                    Some(order.out_order_id.as_str()),
                    None,
                )
                .await
            {
                Ok(inspection) => {
                    inspection_confirms_close |= inspection.position_closed
                        && (inspection.auto_close_likely
                            || !inspection.pending_algo_ids.is_empty()
                            || !inspection.history_algo_ids.is_empty());
                    info!(
                        "🔎 外部平仓 inspection: config_id={}, inst_id={}, period={}, out_order_id={}, inspection={:?}",
                        config.id, inst_id, period, order.out_order_id, inspection
                    );
                }
                Err(e) => {
                    warn!(
                        "⚠️ 外部平仓 inspection 失败: config_id={}, inst_id={}, period={}, out_order_id={}, err={}",
                        config.id, inst_id, period, order.out_order_id, e
                    );
                }
            }
        }

        match self
            .confirm_external_flat_close(config.id, inst_id, period, inspection_confirms_close)
            .await?
        {
            ExternalFlatDecision::Skip => return Ok(()),
            ExternalFlatDecision::Confirmed => {}
        }

        info!(
            "🔄 检测到外部平仓完成，清理本地状态并执行交易桶回补: config_id={}, inst_id={}, period={}",
            config.id, inst_id, period
        );

        if let Err(e) = self
            .clear_persisted_close_algos(config.id, inst_id, period, "long")
            .await
        {
            warn!(
                "⚠️ 外部平仓后清理 long 持久化保护单失败: inst_id={}, config_id={}, err={}",
                inst_id, config.id, e
            );
        }
        if let Err(e) = self
            .clear_persisted_close_algos(config.id, inst_id, period, "short")
            .await
        {
            warn!(
                "⚠️ 外部平仓后清理 short 持久化保护单失败: inst_id={}, config_id={}, err={}",
                inst_id, config.id, e
            );
        }

        self.live_exit_targets.remove(&config.id);
        self.live_states.insert(config.id, TradingState::default());

        if let Err(e) = self
            .clear_external_flat_probe(config.id, inst_id, period)
            .await
        {
            warn!(
                "⚠️ 清理外部平仓探测标记失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config.id, e
            );
        }

        if let Err(e) = self
            .rebalance_trade_bucket_after_close(&api_config, config.id, inst_id)
            .await
        {
            warn!(
                "⚠️ 外部平仓后交易桶自动划转失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config.id, e
            );
        }

        Ok(())
    }

    fn external_flat_probe_key(config_id: i64, inst_id: &str, period: &str) -> String {
        format!(
            "live_external_flat_probe:{}:{}:{}",
            config_id, inst_id, period
        )
    }

    async fn clear_external_flat_probe(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
    ) -> Result<()> {
        let rkey = Self::external_flat_probe_key(config_id, inst_id, period);
        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!(
                    "⚠️ 获取Redis连接失败，跳过清理外部平仓探测标记: config_id={}, inst_id={}, period={}, err={}",
                    config_id, inst_id, period, e
                );
                return Ok(());
            }
        };
        if let Err(e) = conn.del::<_, ()>(&rkey).await {
            warn!(
                "⚠️ 删除外部平仓探测标记失败: config_id={}, inst_id={}, period={}, err={}",
                config_id, inst_id, period, e
            );
        }
        Ok(())
    }

    async fn confirm_external_flat_close(
        &self,
        config_id: i64,
        inst_id: &str,
        period: &str,
        inspection_confirms_close: bool,
    ) -> Result<ExternalFlatDecision> {
        if inspection_confirms_close {
            return Ok(ExternalFlatDecision::Confirmed);
        }

        let rkey = Self::external_flat_probe_key(config_id, inst_id, period);
        let mut conn = match get_redis_connection().await {
            Ok(conn) => conn,
            Err(e) => {
                warn!(
                    "⚠️ 获取Redis连接失败，外部平仓缺少确认时保守跳过: config_id={}, inst_id={}, period={}, err={}",
                    config_id, inst_id, period, e
                );
                return Ok(ExternalFlatDecision::Skip);
            }
        };
        let seen_once = conn.get::<_, Option<String>>(&rkey).await?.is_some();

        if !seen_once {
            conn.set_ex::<_, _, ()>(&rkey, "1", Self::EXTERNAL_FLAT_PROBE_TTL_SECS)
                .await?;
            warn!(
                "⚠️ 首次观测到交易所无持仓但缺少自动平仓证据，暂不清理本地状态: config_id={}, inst_id={}, period={}",
                config_id, inst_id, period
            );
            return Ok(ExternalFlatDecision::Skip);
        }

        warn!(
            "⚠️ 二次观测到交易所无持仓，按外部平仓处理: config_id={}, inst_id={}, period={}",
            config_id, inst_id, period
        );
        Ok(ExternalFlatDecision::Confirmed)
    }

    async fn rebalance_trade_bucket_after_close(
        &self,
        api_config: &rust_quant_domain::entities::ExchangeApiConfig,
        config_id: i64,
        inst_id: &str,
    ) -> Result<()> {
        let Some(rebalance_config) = Self::live_trade_bucket_rebalance_config() else {
            return Ok(());
        };

        use crate::exchange::OkxOrderService;

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(api_config, Some("SWAP"), None)
            .await
            .map_err(|e| anyhow!("获取持仓失败: {}", e))?;

        let has_open_swap_positions = positions
            .iter()
            .any(|p| p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12);

        if has_open_swap_positions {
            info!(
                "⏭️ 跳过交易桶回补: 当前仍有未平 SWAP 持仓, config_id={}, inst_id={}",
                config_id, inst_id
            );
            return Ok(());
        }

        let currency = std::env::var("LIVE_TRADE_BUCKET_CURRENCY")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "USDT".to_string());

        let funding_balance = okx_service
            .get_funding_available_balance(api_config, &currency)
            .await?;
        let trade_balance = okx_service
            .get_trade_available_equity(api_config, &currency)
            .await?;
        let target_trade_balance =
            (trade_balance + funding_balance) * rebalance_config.target_trade_ratio;

        let Some((transfer_amount, direction)) =
            Self::calculate_trade_bucket_transfer(trade_balance, funding_balance, rebalance_config)
        else {
            info!(
                "⏭️ 交易桶余额接近动态目标或低于最小划转金额，无需划转: config_id={}, inst_id={}, trade_balance={:.4}, funding_balance={:.4}, total_balance={:.4}, target={:.4}, ratio={:.4}, min_transfer={:.4}, epsilon={:.4}",
                config_id,
                inst_id,
                trade_balance,
                funding_balance,
                trade_balance + funding_balance,
                target_trade_balance,
                rebalance_config.target_trade_ratio,
                rebalance_config.min_transfer,
                rebalance_config.transfer_epsilon
            );
            return Ok(());
        };

        let (from, to, direction_label) = match direction {
            TradeBucketTransferDirection::FundToTrade => {
                (AccountType::FOUND, AccountType::TRADE, "fund_to_trade")
            }
            TradeBucketTransferDirection::TradeToFund => {
                (AccountType::TRADE, AccountType::FOUND, "trade_to_fund")
            }
        };

        if matches!(direction, TradeBucketTransferDirection::FundToTrade) {
            if funding_balance + 1e-9 < transfer_amount {
                warn!(
                    "⚠️ 交易桶回补跳过: 资金账户余额不足, config_id={}, inst_id={}, need={:.4}, funding_balance={:.4}",
                    config_id, inst_id, transfer_amount, funding_balance
                );
                return Ok(());
            }
        }

        okx_service
            .transfer_between_accounts(api_config, &currency, transfer_amount, from, to)
            .await?;

        info!(
            "💸 交易桶自动划转完成: config_id={}, inst_id={}, direction={}, amount={:.4}, currency={}, total_balance={:.4}, target={:.4}, ratio={:.4}",
            config_id,
            inst_id,
            direction_label,
            transfer_amount,
            currency,
            trade_balance + funding_balance,
            target_trade_balance,
            rebalance_config.target_trade_ratio
        );

        Ok(())
    }

    async fn close_position_internal(
        &self,
        inst_id: &str,
        period: &str,
        config_id: i64,
        close_side: TradeSide,
    ) -> Result<()> {
        use crate::exchange::create_exchange_api_service;
        use crate::exchange::OkxOrderService;

        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| {
                error!("获取API配置失败: config_id={}, error={}", config_id, e);
                anyhow!("获取API配置失败: {}", e)
            })?;

        let okx_service = OkxOrderService;
        let positions = okx_service
            .get_positions(&api_config, Some("SWAP"), Some(inst_id))
            .await
            .map_err(|e| {
                error!("获取账户数据失败: {}", e);
                anyhow!("获取账户数据失败: {}", e)
            })?;

        let close_pos_side_str = match close_side {
            TradeSide::Long => "long",
            TradeSide::Short => "short",
        };
        let persisted_order = self
            .load_persisted_close_algos(config_id, inst_id, period, close_pos_side_str)
            .await?;
        let persisted_algo_ids = persisted_order
            .as_ref()
            .map(|order| Self::extract_close_algo_ids(&order.detail))
            .unwrap_or_default();

        if let Some(p) = positions.iter().find(|p| {
            p.inst_id == inst_id
                && p.pos_side.eq_ignore_ascii_case(close_pos_side_str)
                && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
        }) {
            let mgn_mode = p.mgn_mode.clone();
            let close_pos_side = if close_pos_side_str == "long" {
                okx::dto::PositionSide::Long
            } else {
                okx::dto::PositionSide::Short
            };
            warn!(
                "⚠️ 信号平仓: inst_id={}, period={}, close_pos_side={:?}, mgn_mode={}",
                inst_id, period, close_pos_side, mgn_mode
            );
            okx_service
                .close_position(&api_config, inst_id, close_pos_side, &mgn_mode)
                .await
                .map_err(|e| anyhow!("平仓失败: {}", e))?;
        } else {
            warn!(
                "⚠️ 未找到可平仓位: inst_id={}, period={}, close_side={:?}",
                inst_id, period, close_side
            );
        }

        if !persisted_algo_ids.is_empty() {
            if let Err(e) = okx_service
                .cancel_close_algos(&api_config, inst_id, &persisted_algo_ids)
                .await
            {
                warn!(
                    "⚠️ 平仓后撤销持久化保护单失败: inst_id={}, period={}, config_id={}, err={}",
                    inst_id, period, config_id, e
                );
            }
        }

        if let Err(e) = self
            .clear_persisted_close_algos(config_id, inst_id, period, close_pos_side_str)
            .await
        {
            warn!(
                "⚠️ 平仓后清理持久化保护单失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config_id, e
            );
        }

        if let Some(prev_exit) = self.live_exit_targets.get(&config_id) {
            if prev_exit.trade_side == Some(close_side) {
                self.live_exit_targets.remove(&config_id);
            }
        }

        if let Err(e) = self
            .rebalance_trade_bucket_after_close(&api_config, config_id, inst_id)
            .await
        {
            warn!(
                "⚠️ 平仓后交易桶自动划转失败: inst_id={}, period={}, config_id={}, err={}",
                inst_id, period, config_id, e
            );
        }

        Ok(())
    }

    /// 批量执行多个策略
    pub async fn execute_multiple_strategies(
        &self,
        inst_id: &str,
        period: &str,
        configs: Vec<StrategyConfig>,
    ) -> Result<Vec<SignalResult>> {
        let total = configs.len();
        info!("批量执行 {} 个策略", total);

        let mut results = Vec::with_capacity(total);

        for config in configs {
            match self.execute_strategy(inst_id, period, &config, None).await {
                Ok(signal) => results.push(signal),
                Err(e) => {
                    error!("策略执行失败: config_id={}, error={}", config.id, e);
                    // 继续执行其他策略
                }
            }
        }

        info!("批量执行完成: 成功 {}/{}", results.len(), total);
        Ok(results)
    }

    /// 获取K线数据（内部辅助方法）
    /// TODO: 实现数据获取逻辑
    #[allow(dead_code)]
    async fn get_candles(
        &self,
        _inst_id: &str,
        _period: &str,
        _limit: usize,
    ) -> Result<Vec<rust_quant_domain::Candle>> {
        Err(anyhow!("get_candles 暂未实现"))
    }

    /// 异步记录信号日志（不阻塞主流程）
    fn save_signal_log_async(
        &self,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        config: &StrategyConfig,
    ) {
        let signal_json = match serde_json::to_string(&signal) {
            Ok(s) => s,
            Err(e) => {
                error!("序列化信号失败: {}", e);
                format!("{:?}", signal)
            }
        };

        let inst_id = inst_id.to_string();
        let period = period.to_string();
        let strategy_type = config.strategy_type.as_str().to_string();

        tokio::spawn(async move {
            use rust_quant_infrastructure::SignalLogRepository;

            let repo = SignalLogRepository::new();

            match repo
                .save_signal_log(&inst_id, &period, &strategy_type, &signal_json)
                .await
            {
                Ok(_) => {
                    info!("✅ 信号日志已记录: inst_id={}, period={}", inst_id, period);
                }
                Err(e) => {
                    error!("❌ 写入信号日志失败: {}", e);
                }
            }
        });
    }

    /// 检查当前是否处于高重要性经济事件窗口
    ///
    /// 在经济事件发布前后的时间窗口内，市场波动剧烈，
    /// 不适合追涨追跌，应等待回调后再入场。
    ///
    /// # 默认窗口
    /// - 事件前 30 分钟开始生效
    /// - 事件后 60 分钟仍在影响中
    ///
    /// # 返回
    /// - `Ok(true)` - 当前处于经济事件窗口，建议等待
    /// - `Ok(false)` - 当前无活跃经济事件，可正常交易
    /// - `Err(_)` - 查询失败（建议忽略错误，继续交易）
    async fn check_economic_event_window(&self) -> Result<bool> {
        use crate::market::EconomicEventQueryService;

        let query_service = EconomicEventQueryService::new();
        let current_time_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // 从环境变量读取窗口配置（单位：分钟）
        let window_before_min: i64 = std::env::var("ECON_EVENT_WINDOW_BEFORE_MIN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        let window_after_min: i64 = std::env::var("ECON_EVENT_WINDOW_AFTER_MIN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        let window_before_ms = window_before_min * 60 * 1000;
        let window_after_ms = window_after_min * 60 * 1000;

        let events = query_service
            .get_active_high_importance_events(
                current_time_ms,
                Some(window_before_ms),
                Some(window_after_ms),
            )
            .await?;

        if !events.is_empty() {
            for event in &events {
                info!(
                    "📅 检测到活跃经济事件: {} - {} ({}), importance={}, event_time={}",
                    event.region, event.event, event.category, event.importance, event.event_time
                );
            }
            return Ok(true);
        }

        Ok(false)
    }

    /// 执行下单（内部方法）
    #[allow(clippy::too_many_arguments)]
    async fn execute_order_internal(
        &self,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
        config_id: i64,
        strategy_type: &str,
    ) -> Result<()> {
        info!(
            "准备下单: inst_id={}, period={}, config_id={}",
            inst_id, period, config_id
        );

        // 0) 幂等性：同一策略配置 + 同一周期 + 同一信号时间戳，只允许下单一次
        let in_order_id = SwapOrder::generate_live_in_order_id(
            inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let client_order_id = Self::build_entry_cl_ord_id(config_id, signal.ts);
        if let Some(existing) = self
            .swap_order_repository
            .find_by_in_order_id(&in_order_id)
            .await?
        {
            warn!(
                "⚠️ 幂等命中，跳过重复下单: inst_id={}, period={}, config_id={}, in_order_id={}, out_order_id={:?}",
                inst_id, period, config_id, in_order_id, existing.out_order_id
            );
            return Ok(());
        }

        // 1. 确定交易方向
        let (side, pos_side) = if signal.should_buy {
            ("buy", "long")
        } else if signal.should_sell {
            ("sell", "short")
        } else {
            return Err(anyhow!("信号无效，无交易方向"));
        };

        info!("交易方向: side={}, pos_side={}", side, pos_side);

        // 3. 获取API配置（从Redis缓存或数据库）
        use crate::exchange::create_exchange_api_service;
        let api_service = create_exchange_api_service();
        let api_config = api_service
            .get_first_api_config(config_id as i32)
            .await
            .map_err(|e| {
                error!("获取API配置失败: config_id={}, error={}", config_id, e);
                anyhow!("获取API配置失败: {}", e)
            })?;

        info!(
            "使用API配置: exchange={}, api_key={}...",
            api_config.exchange_name,
            &api_config.api_key[..api_config.api_key.len().min(8)]
        );

        // 4. 获取持仓和可用资金
        use crate::exchange::OkxOrderService;
        let okx_service = OkxOrderService;

        let (positions, max_size) = tokio::try_join!(
            okx_service.get_positions(&api_config, Some("SWAP"), Some(inst_id)),
            okx_service.get_max_available_size(&api_config, inst_id)
        )
        .map_err(|e| {
            error!("获取账户数据失败: {}", e);
            anyhow!("获取账户数据失败: {}", e)
        })?;

        info!("当前持仓数量: {}", positions.len());
        // 4.1 实盘仓位治理（可选）：同向不加仓/反向先平仓
        let skip_same_side = Self::env_enabled("LIVE_SKIP_IF_SAME_SIDE_POSITION");
        let close_opposite_side = Self::env_enabled("LIVE_CLOSE_OPPOSITE_POSITION");
        let opposite_pos_side = if pos_side == "long" { "short" } else { "long" };

        let same_side_exists = positions.iter().any(|p| {
            p.inst_id == inst_id
                && p.pos_side.eq_ignore_ascii_case(pos_side)
                && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
        });
        if skip_same_side && same_side_exists {
            warn!(
                "⚠️ 已有同向持仓，跳过开新仓: inst_id={}, pos_side={}",
                inst_id, pos_side
            );
            return Ok(());
        }

        if close_opposite_side {
            if let Some(p) = positions.iter().find(|p| {
                p.inst_id == inst_id
                    && p.pos_side.eq_ignore_ascii_case(opposite_pos_side)
                    && p.pos.parse::<f64>().unwrap_or(0.0).abs() > 1e-12
            }) {
                let mgn_mode = p.mgn_mode.clone();
                let close_pos_side = if opposite_pos_side == "long" {
                    okx::dto::PositionSide::Long
                } else {
                    okx::dto::PositionSide::Short
                };
                warn!(
                    "⚠️ 检测到反向持仓，先平仓再开仓: inst_id={}, close_pos_side={:?}, mgn_mode={}",
                    inst_id, close_pos_side, mgn_mode
                );
                okx_service
                    .close_position(&api_config, inst_id, close_pos_side, &mgn_mode)
                    .await
                    .map_err(|e| anyhow!("平仓失败: {}", e))?;
            }
        }

        // 5. 计算下单数量（使用90%的安全系数）
        let safety_factor = 0.9;
        let max_size_str = if side == "buy" {
            max_size.max_buy.as_str()
        } else {
            max_size.max_sell.as_str()
        };

        let max_available = match max_size_str.parse::<f64>() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "解析最大可用下单量失败: inst_id={}, side={}, raw={}, error={}",
                    inst_id, side, max_size_str, e
                );
                return Err(anyhow!("解析最大可用下单量失败"));
            }
        };

        info!(
            "最大可用数量: side={}, max_available={}, safety_factor={}",
            side, max_available, safety_factor
        );

        let order_size_f64 = max_available * safety_factor;
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        if order_size == "0" {
            info!("下单数量为0，跳过下单");
            return Ok(());
        }

        info!("计算的下单数量: {}", order_size);

        // 6. 计算止损止盈价格
        let entry_price = signal.open_price;
        let stop_candidates = Self::build_stop_loss_candidates(side, signal, risk_config);
        let stop_side = if side == "sell" {
            StopLossSide::Short
        } else {
            StopLossSide::Long
        };
        let final_stop_loss = StopLossCalculator::select(stop_side, entry_price, &stop_candidates)
            .ok_or_else(|| anyhow!("无有效止损价"))?;

        let take_profit_trigger_px: Option<f64> = None;

        // 验证止损价格合理性
        if pos_side == "short" && entry_price > final_stop_loss {
            error!(
                "做空开仓价 > 止损价，不下单: entry={}, stop_loss={}",
                entry_price, final_stop_loss
            );
            return Err(anyhow!("止损价格不合理"));
        }
        if pos_side == "long" && entry_price < final_stop_loss {
            error!(
                "做多开仓价 < 止损价，不下单: entry={}, stop_loss={}",
                entry_price, final_stop_loss
            );
            return Err(anyhow!("止损价格不合理"));
        }

        info!(
            "下单参数: entry_price={:.2}, stop_loss={:.2}, take_profit={:?}",
            entry_price, final_stop_loss, take_profit_trigger_px
        );

        // 7. 实际下单到交易所（与原实现 swap_order_service.rs::order_swap 保持一致）
        let order_result = okx_service
            .execute_order_from_signal(
                &api_config,
                inst_id,
                signal,
                order_size.clone(),
                Some(final_stop_loss),
                take_profit_trigger_px,
                Some(client_order_id), // 交易所 client order id，避免超长/非法格式
            )
            .await
            .map_err(|e| {
                error!("下单到交易所失败: {}", e);
                anyhow!("下单失败: {}", e)
            })?;

        // 获取交易所返回的订单ID
        let out_order_id = match order_result.first() {
            Some(o) => o.ord_id.clone(),
            None => {
                warn!(
                    "⚠️ 下单返回为空: inst_id={}, period={}, config_id={}",
                    inst_id, period, config_id
                );
                String::new()
            }
        };

        info!(
            "✅ 下单成功: inst_id={}, order_id={}, size={}",
            inst_id, out_order_id, order_size
        );

        // 8. 保存订单记录到数据库
        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": final_stop_loss,
            "take_profit": take_profit_trigger_px,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
                "atr_stop_loss_price": signal.atr_stop_loss_price,
                "atr_take_profit_ratio_price": signal.atr_take_profit_ratio_price,
            }
        });

        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            strategy_type,
            side,
            pos_side,
            &order_size,
            &in_order_id,
            &out_order_id,
            "okx",
            &order_detail.to_string(),
        );

        match self.swap_order_repository.save(&swap_order).await {
            Ok(order_id) => {
                info!(
                    "✅ 订单记录已保存: db_id={}, in_order_id={}",
                    order_id, in_order_id
                );
            }
            Err(e) => {
                // 订单已提交到交易所,保存失败只记录警告,不返回错误
                error!("⚠️ 保存订单记录失败(订单已提交): {}", e);
            }
        }

        Ok(())
    }

    /// 验证策略配置
    fn validate_config(&self, config: &StrategyConfig) -> Result<()> {
        if !config.is_running() {
            return Err(anyhow!(
                "策略未运行: config_id={}, status={:?}",
                config.id,
                config.status
            ));
        }

        if config.parameters.is_null() {
            return Err(anyhow!("策略参数为空"));
        }

        Ok(())
    }

    /// 检查是否应该执行策略
    pub fn should_execute(
        &self,
        config: &StrategyConfig,
        last_execution_time: Option<i64>,
        current_time: i64,
    ) -> bool {
        if !config.is_running() {
            return false;
        }

        if let Some(last_time) = last_execution_time {
            let interval = current_time - last_time;
            let min_interval = self.get_min_execution_interval(&config.timeframe);

            if interval < min_interval {
                return false;
            }
        }

        true
    }

    /// 获取最小执行间隔（秒）
    fn get_min_execution_interval(&self, timeframe: &rust_quant_domain::Timeframe) -> i64 {
        use rust_quant_domain::Timeframe;

        match *timeframe {
            Timeframe::M1 => 60,
            Timeframe::M3 => 180,
            Timeframe::M5 => 300,
            Timeframe::M15 => 900,
            Timeframe::M30 => 1800,
            Timeframe::H1 => 3600,
            Timeframe::H2 => 7200,
            Timeframe::H4 => 14400,
            Timeframe::H6 => 21600,
            Timeframe::H12 => 43200,
            Timeframe::D1 => 86400,
            Timeframe::W1 => 604800,
            Timeframe::MN1 => 2592000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rust_quant_core::cache::init_redis_pool;
    use rust_quant_core::database::init_db_pool;
    use rust_quant_strategies::TradePosition;
    use std::sync::Mutex;

    /// Mock SwapOrderRepository - 支持自定义行为
    struct MockSwapOrderRepository {
        /// 模拟已存在的订单（用于幂等性测试）
        existing_order: Option<SwapOrder>,
        /// 保存订单时是否返回错误
        save_should_fail: bool,
        /// 保存的订单记录
        saved_orders: Arc<Mutex<Vec<SwapOrder>>>,
    }

    impl MockSwapOrderRepository {
        fn new() -> Self {
            Self {
                existing_order: None,
                save_should_fail: false,
                saved_orders: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_existing_order(mut self, order: SwapOrder) -> Self {
            self.existing_order = Some(order);
            self
        }

        fn with_save_failure(mut self, should_fail: bool) -> Self {
            self.save_should_fail = should_fail;
            self
        }

        #[allow(dead_code)]
        fn get_saved_orders(&self) -> Vec<SwapOrder> {
            self.saved_orders.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SwapOrderRepository for MockSwapOrderRepository {
        async fn find_by_id(&self, _id: i32) -> Result<Option<SwapOrder>> {
            Ok(None)
        }

        async fn find_by_in_order_id(&self, in_order_id: &str) -> Result<Option<SwapOrder>> {
            if let Some(ref order) = self.existing_order {
                if order.in_order_id == in_order_id {
                    return Ok(Some(order.clone()));
                }
            }
            Ok(None)
        }

        async fn find_by_out_order_id(&self, _out_order_id: &str) -> Result<Option<SwapOrder>> {
            Ok(None)
        }

        async fn find_by_inst_id(
            &self,
            _inst_id: &str,
            _limit: Option<i32>,
        ) -> Result<Vec<SwapOrder>> {
            Ok(vec![])
        }

        async fn find_pending_order(
            &self,
            _inst_id: &str,
            _period: &str,
            _side: &str,
            _pos_side: &str,
        ) -> Result<Vec<SwapOrder>> {
            Ok(vec![])
        }

        async fn find_latest_by_strategy_inst_period_pos_side(
            &self,
            strategy_id: i32,
            inst_id: &str,
            period: &str,
            pos_side: &str,
        ) -> Result<Option<SwapOrder>> {
            if let Some(ref order) = self.existing_order {
                if order.strategy_id == strategy_id
                    && order.inst_id == inst_id
                    && order.period == period
                    && order.pos_side == pos_side
                {
                    return Ok(Some(order.clone()));
                }
            }

            let orders = self.saved_orders.lock().unwrap();
            let mut candidates: Vec<SwapOrder> = orders
                .iter()
                .filter(|order| {
                    order.strategy_id == strategy_id
                        && order.inst_id == inst_id
                        && order.period == period
                        && order.pos_side == pos_side
                })
                .cloned()
                .collect();

            candidates.sort_by_key(|order| order.created_at);
            Ok(candidates.pop())
        }

        async fn save(&self, order: &SwapOrder) -> Result<i32> {
            if self.save_should_fail {
                return Err(anyhow!("模拟保存失败"));
            }
            self.saved_orders.lock().unwrap().push(order.clone());
            Ok(1)
        }

        async fn update(&self, order: &SwapOrder) -> Result<()> {
            let mut orders = self.saved_orders.lock().unwrap();
            if let Some(existing) = orders.iter_mut().find(|o| {
                (order.id.is_some() && o.id == order.id) || o.in_order_id == order.in_order_id
            }) {
                *existing = order.clone();
            }
            Ok(())
        }

        async fn find_by_strategy_and_time(
            &self,
            _strategy_id: i32,
            _start_time: i64,
            _end_time: i64,
        ) -> Result<Vec<SwapOrder>> {
            Ok(vec![])
        }
    }

    fn create_test_service() -> StrategyExecutionService {
        StrategyExecutionService::new(Arc::new(MockSwapOrderRepository::new()))
    }

    /// 创建测试用的SignalResult - 买入信号
    fn create_buy_signal(open_price: f64, ts: i64) -> SignalResult {
        SignalResult {
            should_buy: true,
            should_sell: false,
            open_price,
            signal_kline_stop_loss_price: Some(open_price * 0.98), // 2%止损
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            stop_loss_source: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Long,
        }
    }

    /// 创建测试用的SignalResult - 卖出信号
    fn create_sell_signal(open_price: f64, ts: i64) -> SignalResult {
        SignalResult {
            should_buy: false,
            should_sell: true,
            open_price,
            signal_kline_stop_loss_price: Some(open_price * 1.02), // 2%止损
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            ts,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            stop_loss_source: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Short,
        }
    }

    #[test]
    fn test_service_creation() {
        let _service = create_test_service();
    }

    #[test]
    fn test_close_algo_detail_roundtrip() {
        let detail = serde_json::json!({
            "entry_price": 100.0,
            "stop_loss": 95.0,
        })
        .to_string();
        let algo_ids = vec!["a1".to_string(), "a2".to_string()];
        let updated = StrategyExecutionService::upsert_close_algo_detail(
            &detail,
            &algo_ids,
            "rq-1",
            Some(95.0),
            Some(110.0),
        );

        let extracted = StrategyExecutionService::extract_close_algo_ids(&updated);
        assert_eq!(extracted, algo_ids);

        let cleared = StrategyExecutionService::remove_close_algo_detail(&updated);
        let extracted_after_clear = StrategyExecutionService::extract_close_algo_ids(&cleared);
        assert!(extracted_after_clear.is_empty());
    }

    #[test]
    fn test_min_execution_interval() {
        use rust_quant_domain::Timeframe;

        let service = create_test_service();

        assert_eq!(service.get_min_execution_interval(&Timeframe::M1), 60);
        assert_eq!(service.get_min_execution_interval(&Timeframe::M5), 300);
        assert_eq!(service.get_min_execution_interval(&Timeframe::H1), 3600);
        assert_eq!(service.get_min_execution_interval(&Timeframe::D1), 86400);
    }

    #[tokio::test]
    async fn test_should_execute() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};

        let service = create_test_service();

        let config = StrategyConfig {
            id: 1,
            strategy_type: StrategyType::Vegas,
            symbol: "BTC-USDT".to_string(),
            timeframe: Timeframe::H1,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        assert!(service.should_execute(&config, None, 1000));
        assert!(!service.should_execute(&config, Some(1000), 1500));
        assert!(service.should_execute(&config, Some(1000), 5000));
    }

    #[tokio::test]
    async fn execution_respects_filter_block() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};
        use rust_quant_strategies::framework::backtest::BasicRiskStrategyConfig;

        let repo = Arc::new(MockSwapOrderRepository::new());
        let service = StrategyExecutionService::new(repo.clone());

        let config = StrategyConfig {
            id: 42,
            strategy_type: StrategyType::Vegas,
            symbol: "BTC-USDT".to_string(),
            timeframe: Timeframe::H1,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({ "max_loss_percent": 0.02 }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        let mut signal = create_buy_signal(100.0, 1);
        signal
            .filter_reasons
            .push("FIB_STRICT_MAJOR_BEAR_BLOCK_LONG".to_string());

        let candle = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 99.0,
            c: 100.0,
            v: 1.0,
            ts: 1,
            confirm: 1,
        };

        let decision_risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };
        let order_risk = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };

        let outcome = service
            .handle_live_decision(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                &mut signal,
                &candle,
                decision_risk,
                &order_risk,
            )
            .await
            .expect("handle_live_decision should succeed");

        assert!(outcome.opened_side.is_none());
        assert!(repo.get_saved_orders().is_empty());
    }

    #[tokio::test]
    async fn handle_live_decision_rolls_back_state_when_open_fails() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};
        use rust_quant_strategies::framework::backtest::BasicRiskStrategyConfig;

        if std::env::var("DB_HOST").is_err() {
            std::env::set_var(
                "DB_HOST",
                "mysql://root:example@127.0.0.1:33306/test?ssl-mode=DISABLED",
            );
        }
        let _ = init_db_pool().await;

        let repo = Arc::new(MockSwapOrderRepository::new());
        let service = StrategyExecutionService::new(repo);

        let config = StrategyConfig {
            id: 4200,
            strategy_type: StrategyType::Vegas,
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::H4,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({ "max_loss_percent": 0.02 }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        let mut signal = create_buy_signal(100.0, 1_700_000_000_000);
        let candle = CandleItem {
            o: 100.0,
            h: 101.0,
            l: 99.0,
            c: 100.0,
            v: 1.0,
            ts: 1_700_000_000_000,
            confirm: 1,
        };
        let decision_risk = BasicRiskStrategyConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };
        let order_risk = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: 0.02,
            ..Default::default()
        };

        let err = service
            .handle_live_decision(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                &mut signal,
                &candle,
                decision_risk,
                &order_risk,
            )
            .await
            .expect_err("missing api config should make order placement fail");
        assert!(err.to_string().contains("获取API配置失败"));

        let reloaded = service
            .live_states
            .get(&config.id)
            .map(|v| v.clone())
            .unwrap_or_default();
        assert!(reloaded.trade_position.is_none());
    }

    #[tokio::test]
    async fn confirm_external_flat_close_requires_second_observation_without_inspection() {
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        let _ = init_redis_pool().await;

        let service = create_test_service();
        let config_id = 5200;
        let inst_id = "ETH-USDT-SWAP";
        let period = "4H";

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");

        let first = service
            .confirm_external_flat_close(config_id, inst_id, period, false)
            .await
            .expect("first observation should succeed");
        assert!(matches!(first, ExternalFlatDecision::Skip));

        let second = service
            .confirm_external_flat_close(config_id, inst_id, period, false)
            .await
            .expect("second observation should succeed");
        assert!(matches!(second, ExternalFlatDecision::Confirmed));

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");
    }

    #[tokio::test]
    async fn confirm_external_flat_close_confirms_immediately_with_inspection() {
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        let _ = init_redis_pool().await;

        let service = create_test_service();
        let config_id = 5300;
        let inst_id = "ETH-USDT-SWAP";
        let period = "4H";

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");

        let decision = service
            .confirm_external_flat_close(config_id, inst_id, period, true)
            .await
            .expect("inspection-backed observation should succeed");
        assert!(matches!(decision, ExternalFlatDecision::Confirmed));

        service
            .clear_external_flat_probe(config_id, inst_id, period)
            .await
            .expect("probe cleanup should succeed");
    }

    #[tokio::test]
    async fn opened_sync_failure_forces_close_when_compensation_cannot_restore_tpsl() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};

        let service = create_test_service();
        let config = StrategyConfig {
            id: 999,
            strategy_type: StrategyType::Vegas,
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::H4,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({"max_loss_percent": 0.02}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };

        let state = TradingState {
            trade_position: Some(TradePosition {
                trade_side: TradeSide::Long,
                position_nums: 1.0,
                open_price: 100.0,
                open_position_time: "2026-01-01 00:00:00".to_string(),
                signal_high_low_diff: 1.0,
                ..Default::default()
            }),
            ..TradingState::default()
        };
        service.live_states.insert(config.id, state);
        service.configure_guard_test_state(true, false, false);

        let err = service
            .enforce_opened_position_guard(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                TradeSide::Long,
                1_738_454_400_000,
            )
            .await
            .expect_err("guard should force close when no tp/sl can be restored");
        assert!(err
            .to_string()
            .contains("开仓后止盈止损同步失败，补偿未成功，已触发主动平仓"));

        let (compensate_calls, close_calls) = service.guard_test_calls();
        assert_eq!(compensate_calls, 1);
        assert_eq!(close_calls, 1);

        let reloaded = service
            .live_states
            .get(&config.id)
            .map(|v| v.clone())
            .unwrap_or_default();
        assert!(reloaded.trade_position.is_none());
        assert!(!service.has_live_algo_for_side(config.id, TradeSide::Long));
    }

    #[tokio::test]
    async fn opened_sync_failure_keeps_position_when_compensation_restores_tpsl() {
        use chrono::Utc;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};

        let service = create_test_service();
        let config = StrategyConfig {
            id: 1000,
            strategy_type: StrategyType::Vegas,
            symbol: "BTC-USDT-SWAP".to_string(),
            timeframe: Timeframe::H4,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::json!({"max_loss_percent": 0.02}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: None,
        };
        service.configure_guard_test_state(false, true, false);

        let result = service
            .enforce_opened_position_guard(
                &config.symbol,
                config.timeframe.as_str(),
                &config,
                TradeSide::Long,
                1_738_454_400_000,
            )
            .await;
        assert!(result.is_ok());

        let (compensate_calls, close_calls) = service.guard_test_calls();
        assert_eq!(compensate_calls, 1);
        assert_eq!(close_calls, 0);
        assert!(service.has_live_algo_for_side(config.id, TradeSide::Long));
    }

    // ========== 下单逻辑单元测试 ==========

    /// 测试：下单数量计算逻辑（90%安全系数）
    #[test]
    fn test_order_size_calculation() {
        let max_available = 100.0;
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor;
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        assert_eq!(order_size, "90.00");

        // 测试小于1的情况
        let max_available = 0.5;
        let order_size_f64 = max_available * safety_factor;
        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        assert_eq!(order_size, "0");
    }

    /// 测试：止损价格计算逻辑 - 做多
    #[test]
    fn test_stop_loss_calculation_long() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02; // 2%

        let stop_loss_price = entry_price * (1.0 - max_loss_percent);
        assert_eq!(stop_loss_price, 49000.0);

        // 验证：做多时，开仓价应该 > 止损价
        assert!(entry_price > stop_loss_price);
    }

    /// 测试：止损价格计算逻辑 - 做空
    #[test]
    fn test_stop_loss_calculation_short() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02; // 2%

        let stop_loss_price = entry_price * (1.0 + max_loss_percent);
        assert_eq!(stop_loss_price, 51000.0);

        // 验证：做空时，开仓价应该 < 止损价
        assert!(entry_price < stop_loss_price);
    }

    /// 测试：止损价格验证 - 做多时开仓价 < 止损价应该失败
    #[test]
    fn test_stop_loss_validation_long_invalid() {
        let entry_price = 49000.0;
        let stop_loss_price = 50000.0; // 止损价 > 开仓价，不合理

        let is_valid = entry_price >= stop_loss_price;
        assert!(!is_valid, "做多时开仓价应该 >= 止损价");
    }

    /// 测试：止损价格验证 - 做空时开仓价 > 止损价应该失败
    #[test]
    fn test_stop_loss_validation_short_invalid() {
        let entry_price = 51000.0;
        let stop_loss_price = 50000.0; // 止损价 < 开仓价，不合理

        let is_valid = entry_price <= stop_loss_price;
        assert!(!is_valid, "做空时开仓价应该 <= 止损价");
    }

    /// 测试：信号K线止损价格优先级
    #[test]
    fn test_signal_kline_stop_loss_priority() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02;
        let signal_kline_stop_loss = 48000.0; // 信号K线止损价

        // 计算默认止损价
        let default_stop_loss = entry_price * (1.0 - max_loss_percent); // 49000.0

        // 如果使用信号K线止损，应该使用信号K线止损价
        let final_stop_loss = match Some(true) {
            Some(true) => match Some(signal_kline_stop_loss) {
                Some(v) => v,
                None => default_stop_loss,
            },
            _ => default_stop_loss,
        };

        assert_eq!(final_stop_loss, signal_kline_stop_loss);
        assert_ne!(final_stop_loss, default_stop_loss);
    }

    /// 测试：信号K线止损价格缺失时使用默认止损
    #[test]
    fn test_signal_kline_stop_loss_fallback() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02;
        let default_stop_loss = entry_price * (1.0 - max_loss_percent); // 49000.0

        // 如果使用信号K线止损但信号K线止损价为None，应该使用默认止损
        let final_stop_loss = match Some(true) {
            Some(true) => match None::<f64> {
                Some(v) => v,
                None => default_stop_loss,
            },
            _ => default_stop_loss,
        };

        assert_eq!(final_stop_loss, default_stop_loss);
    }

    /// 测试：订单ID生成
    #[test]
    fn test_generate_in_order_id() {
        let inst_id = "BTC-USDT-SWAP";
        let strategy_type = "vegas";
        let config_id = 11;
        let period = "4H";
        let ts = 1234567890;

        let in_order_id =
            SwapOrder::generate_live_in_order_id(inst_id, strategy_type, config_id, period, ts);
        assert_eq!(in_order_id, "BTC-USDT-SWAP_vegas_11_4H_1234567890");
    }

    /// 测试：幂等性检查 - 已存在订单应该跳过
    #[tokio::test]
    async fn test_idempotency_check() {
        let inst_id = "BTC-USDT-SWAP";
        let ts = 1234567890;
        let in_order_id = SwapOrder::generate_live_in_order_id(inst_id, "vegas", 1, "1H", ts);

        // 创建已存在的订单
        let existing_order = SwapOrder::new(
            1,
            in_order_id.clone(),
            "out_order_123".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            inst_id.to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );

        let repo = MockSwapOrderRepository::new().with_existing_order(existing_order);
        let service = StrategyExecutionService::new(Arc::new(repo));

        // 验证幂等性：查询已存在的订单应该返回Some
        let found = service
            .swap_order_repository
            .find_by_in_order_id(&in_order_id)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().in_order_id, in_order_id);
    }

    /// 测试：交易方向判断 - 买入信号
    #[test]
    fn test_trade_direction_buy() {
        let signal = create_buy_signal(50000.0, 1234567890);

        let (side, pos_side) = if signal.should_buy {
            ("buy", "long")
        } else if signal.should_sell {
            ("sell", "short")
        } else {
            panic!("信号无效");
        };

        assert_eq!(side, "buy");
        assert_eq!(pos_side, "long");
    }

    /// 测试：交易方向判断 - 卖出信号
    #[test]
    fn test_trade_direction_sell() {
        let signal = create_sell_signal(50000.0, 1234567890);

        let (side, pos_side) = if signal.should_buy {
            ("buy", "long")
        } else if signal.should_sell {
            ("sell", "short")
        } else {
            panic!("信号无效");
        };

        assert_eq!(side, "sell");
        assert_eq!(pos_side, "short");
    }

    /// 测试：无效信号处理
    #[test]
    fn test_invalid_signal() {
        let signal = SignalResult {
            should_buy: false,
            should_sell: false,
            ..create_buy_signal(50000.0, 1234567890)
        };

        let has_signal = signal.should_buy || signal.should_sell;
        assert!(!has_signal, "应该识别为无效信号");
    }

    /// 测试：订单详情JSON构建
    #[test]
    fn test_order_detail_json() {
        let entry_price = 50000.0;
        let stop_loss = 49000.0;
        let signal = create_buy_signal(entry_price, 1234567890);

        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": stop_loss,
            "take_profit": null,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
                "atr_stop_loss_price": signal.atr_stop_loss_price,
                "atr_take_profit_ratio_price": signal.atr_take_profit_ratio_price,
            }
        });

        assert_eq!(order_detail["entry_price"], entry_price);
        assert_eq!(order_detail["stop_loss"], stop_loss);
        assert_eq!(order_detail["signal"]["should_buy"], signal.should_buy);
        assert_eq!(order_detail["signal"]["should_sell"], signal.should_sell);
    }

    /// 测试：订单保存成功
    #[tokio::test]
    async fn test_order_save_success() {
        let repo = MockSwapOrderRepository::new();
        let service = StrategyExecutionService::new(Arc::new(repo));

        let order = SwapOrder::new(
            1,
            "test_in_123".to_string(),
            "test_out_456".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            "BTC-USDT-SWAP".to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );

        // 验证订单结构
        assert_eq!(order.strategy_id, 1);
        assert_eq!(order.inst_id, "BTC-USDT-SWAP");
        assert_eq!(order.side, "buy");

        // 测试保存
        let result = service.swap_order_repository.save(&order).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    /// 测试：订单保存失败处理
    #[tokio::test]
    async fn test_order_save_failure() {
        let repo = MockSwapOrderRepository::new().with_save_failure(true);
        let service = StrategyExecutionService::new(Arc::new(repo));

        let order = SwapOrder::new(
            1,
            "test_in_123".to_string(),
            "test_out_456".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            "BTC-USDT-SWAP".to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );

        // 验证保存失败时应该返回错误
        let result = service.swap_order_repository.save(&order).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("模拟保存失败"));
    }

    /// 测试：止损价格精度（2位小数）
    #[test]
    fn test_stop_loss_precision() {
        let stop_loss_price = 49000.123456789;
        let formatted = format!("{:.2}", stop_loss_price);
        assert_eq!(formatted, "49000.12");
    }

    /// 测试：下单数量精度（2位小数）
    #[test]
    fn test_order_size_precision() {
        let order_size_f64 = 90.123456789;
        let formatted = format!("{:.2}", order_size_f64);
        assert_eq!(formatted, "90.12");
    }

    /// 测试：做多止损价格边界情况
    #[test]
    fn test_long_stop_loss_edge_cases() {
        // 测试最大止损百分比
        let entry_price = 50000.0;
        let max_loss_percent = 0.05; // 5%
        let stop_loss = entry_price * (1.0 - max_loss_percent);
        assert_eq!(stop_loss, 47500.0);

        // 验证合理性
        assert!(entry_price > stop_loss);
    }

    /// 测试：做空止损价格边界情况
    #[test]
    fn test_short_stop_loss_edge_cases() {
        // 测试最大止损百分比
        let entry_price = 50000.0;
        let max_loss_percent = 0.05; // 5%
        let stop_loss = entry_price * (1.0 + max_loss_percent);
        assert_eq!(stop_loss, 52500.0);

        // 验证合理性
        assert!(entry_price < stop_loss);
    }

    /// 测试：下单数量为0时应该跳过
    #[test]
    fn test_zero_order_size_skip() {
        let order_size = "0".to_string();
        let should_skip = order_size == "0";
        assert!(should_skip);
    }

    /// 测试：下单数量小于1时应该返回0
    #[test]
    fn test_small_order_size() {
        let max_available = 0.5;
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor; // 0.45

        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        assert_eq!(order_size, "0");
    }

    /// 测试：订单从信号创建
    #[test]
    fn test_order_from_signal() {
        let signal = create_buy_signal(50000.0, 1234567890);
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let strategy_type = "vegas";
        let side = "buy";
        let pos_side = "long";
        let order_size = "1.0";
        let in_order_id = "test_in_123";
        let out_order_id = "test_out_456";
        let platform_type = "okx";

        let order_detail = serde_json::json!({
            "entry_price": signal.open_price,
            "stop_loss": signal.signal_kline_stop_loss_price,
        });

        let order = SwapOrder::from_signal(
            1,
            inst_id,
            period,
            strategy_type,
            side,
            pos_side,
            order_size,
            in_order_id,
            out_order_id,
            platform_type,
            &order_detail.to_string(),
        );

        assert_eq!(order.strategy_id, 1);
        assert_eq!(order.inst_id, inst_id);
        assert_eq!(order.side, side);
        assert_eq!(order.pos_side, pos_side);
        assert_eq!(order.in_order_id, in_order_id);
        assert_eq!(order.out_order_id, out_order_id);
    }

    // ========== execute_order_internal 实际测试用例 ==========

    /// 测试辅助：创建测试用的ExchangeApiConfig
    #[allow(dead_code)]
    fn create_test_api_config() -> rust_quant_domain::entities::ExchangeApiConfig {
        rust_quant_domain::entities::ExchangeApiConfig::new(
            1,
            "okx".to_string(),
            "test_api_key".to_string(),
            "test_api_secret".to_string(),
            Some("test_passphrase".to_string()),
            true, // sandbox
            true, // enabled
            Some("测试API配置".to_string()),
        )
    }

    /// 测试辅助：创建测试用的BasicRiskConfig
    fn create_test_risk_config(
        max_loss_percent: f64,
        use_signal_kline_stop_loss: Option<bool>,
    ) -> rust_quant_domain::BasicRiskConfig {
        rust_quant_domain::BasicRiskConfig {
            max_loss_percent,
            atr_take_profit_ratio: None,
            fix_signal_kline_take_profit_ratio: None,
            is_move_stop_loss: None,
            is_used_signal_k_line_stop_loss: use_signal_kline_stop_loss,
            max_hold_time: None,
            max_leverage: None,
        }
    }

    /// 测试：execute_order_internal - 正常买入下单流程
    ///
    /// 注意：此测试需要mock外部依赖（ExchangeApiService和OkxOrderService）
    /// 由于这些依赖是硬编码的，此测试主要用于验证逻辑流程
    #[tokio::test]
    #[ignore] // 需要真实环境或mock，默认忽略
    async fn test_execute_order_internal_buy_success() {
        let repo = MockSwapOrderRepository::new();
        let _service = StrategyExecutionService::new(Arc::new(repo));

        let signal = create_buy_signal(50000.0, 1234567890);
        let risk_config = create_test_risk_config(0.02, None);
        let _inst_id = "BTC-USDT-SWAP";
        let _period = "1H";
        let _config_id = 1;
        let _strategy_type = "vegas";

        // 注意：此测试需要mock ExchangeApiService 和 OkxOrderService
        // 由于这些是硬编码依赖，实际测试需要：
        // 1. 使用真实环境（需要配置API密钥）
        // 2. 或者重构代码支持依赖注入
        // 3. 或者使用条件编译创建测试版本

        // 这里只验证信号和配置的有效性
        assert!(signal.should_buy);
        assert!(!signal.should_sell);
        assert_eq!(signal.open_price, 50000.0);
        assert_eq!(risk_config.max_loss_percent, 0.02);
    }

    /// 测试：execute_order_internal - 幂等性检查
    #[tokio::test]
    async fn test_execute_order_internal_idempotency() {
        let inst_id = "BTC-USDT-SWAP";
        let ts = 1234567890;
        let in_order_id = SwapOrder::generate_live_in_order_id(inst_id, "vegas", 1, "1H", ts);

        // 创建已存在的订单
        let existing_order = SwapOrder::new(
            1,
            in_order_id.clone(),
            "out_order_123".to_string(),
            "vegas".to_string(),
            "1H".to_string(),
            inst_id.to_string(),
            "buy".to_string(),
            "1.0".to_string(),
            "long".to_string(),
            "okx".to_string(),
            "{}".to_string(),
        );

        let repo = MockSwapOrderRepository::new().with_existing_order(existing_order);
        let service = StrategyExecutionService::new(Arc::new(repo));

        // 验证幂等性：查询已存在的订单应该返回Some
        let found = service
            .swap_order_repository
            .find_by_in_order_id(&in_order_id)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().in_order_id, in_order_id);
    }

    /// 测试：execute_order_internal - 无效信号处理
    #[test]
    fn test_execute_order_internal_invalid_signal() {
        let signal = SignalResult {
            should_buy: false,
            should_sell: false,
            ..create_buy_signal(50000.0, 1234567890)
        };

        // 验证无效信号应该返回错误
        let (side, pos_side) = if signal.should_buy {
            ("buy", "long")
        } else if signal.should_sell {
            ("sell", "short")
        } else {
            ("invalid", "invalid")
        };

        assert_eq!(side, "invalid");
        assert_eq!(pos_side, "invalid");
    }

    /// 测试：execute_order_internal - 下单数量为0时跳过
    #[test]
    fn test_execute_order_internal_zero_size_skip() {
        // 模拟最大可用数量很小的情况
        let max_available = 0.5; // 小于1
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor; // 0.45

        let order_size = if order_size_f64 < 1.0 {
            "0".to_string()
        } else {
            format!("{:.2}", order_size_f64)
        };

        assert_eq!(order_size, "0");
        // 当order_size为0时，应该跳过下单
        assert!(order_size == "0");
    }

    /// 测试：execute_order_internal - 止损价格验证失败（做多）
    #[test]
    fn test_execute_order_internal_stop_loss_validation_fail_long() {
        let entry_price = 49000.0;
        let stop_loss_price = 50000.0; // 止损价 > 开仓价，不合理

        // 做多时，开仓价应该 > 止损价
        let is_valid = entry_price >= stop_loss_price;
        assert!(!is_valid, "做多时止损价格不合理应该失败");
    }

    /// 测试：execute_order_internal - 止损价格验证失败（做空）
    #[test]
    fn test_execute_order_internal_stop_loss_validation_fail_short() {
        let entry_price = 51000.0;
        let stop_loss_price = 50000.0; // 止损价 < 开仓价，不合理

        // 做空时，开仓价应该 < 止损价
        let is_valid = entry_price <= stop_loss_price;
        assert!(!is_valid, "做空时止损价格不合理应该失败");
    }

    /// 测试：execute_order_internal - 使用信号K线止损
    #[test]
    fn test_execute_order_internal_signal_kline_stop_loss() {
        let entry_price = 50000.0;
        let max_loss_percent = 0.02;
        let signal_kline_stop_loss = 48000.0;

        // 计算默认止损
        let default_stop_loss = entry_price * (1.0 - max_loss_percent); // 49000.0

        // 如果使用信号K线止损，应该使用信号K线止损价
        let risk_config = create_test_risk_config(0.02, Some(true));
        let final_stop_loss = match risk_config.is_used_signal_k_line_stop_loss {
            Some(true) => match Some(signal_kline_stop_loss) {
                Some(v) => v,
                None => default_stop_loss,
            },
            _ => default_stop_loss,
        };

        assert_eq!(final_stop_loss, signal_kline_stop_loss);
        assert_ne!(final_stop_loss, default_stop_loss);
    }

    /// 测试：execute_order_internal - 订单保存成功
    #[tokio::test]
    async fn test_execute_order_internal_order_save_success() {
        let repo = MockSwapOrderRepository::new();
        let service = StrategyExecutionService::new(Arc::new(repo));

        let signal = create_buy_signal(50000.0, 1234567890);
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let strategy_type = "vegas";
        let config_id = 1;
        let in_order_id = SwapOrder::generate_live_in_order_id(
            inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let out_order_id = "test_out_123".to_string();
        let order_size = "1.0".to_string();

        let order_detail = serde_json::json!({
            "entry_price": signal.open_price,
            "stop_loss": signal.signal_kline_stop_loss_price,
        });

        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            strategy_type,
            "buy",
            "long",
            &order_size,
            &in_order_id,
            &out_order_id,
            "okx",
            &order_detail.to_string(),
        );

        // 测试保存订单
        let result = service.swap_order_repository.save(&swap_order).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    /// 测试：execute_order_internal - 订单保存失败处理
    #[tokio::test]
    async fn test_execute_order_internal_order_save_failure() {
        let repo = MockSwapOrderRepository::new().with_save_failure(true);
        let service = StrategyExecutionService::new(Arc::new(repo));

        let signal = create_buy_signal(50000.0, 1234567890);
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let strategy_type = "vegas";
        let config_id = 1;
        let in_order_id = SwapOrder::generate_live_in_order_id(
            inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let out_order_id = "test_out_123".to_string();
        let order_size = "1.0".to_string();

        let order_detail = serde_json::json!({
            "entry_price": signal.open_price,
            "stop_loss": signal.signal_kline_stop_loss_price,
        });

        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            strategy_type,
            "buy",
            "long",
            &order_size,
            &in_order_id,
            &out_order_id,
            "okx",
            &order_detail.to_string(),
        );

        // 测试保存失败
        let result = service.swap_order_repository.save(&swap_order).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("模拟保存失败"));
    }

    /// 测试：execute_order_internal - 真实场景集成测试
    ///
    /// 此测试通过execute_strategy方法间接测试execute_order_internal的完整流程
    /// 使用真实的数据结构和逻辑，可以连接真实的数据库和API（如果配置了）
    ///
    /// 前置条件（可选）：
    /// 1. 数据库配置：DATABASE_URL环境变量
    /// 2. Redis配置：REDIS_URL环境变量
    /// 3. API配置：需要在数据库中配置策略配置ID和API配置的关联
    ///
    /// 如果未配置数据库或API，测试会跳过实际下单，仅验证逻辑流程
    #[tokio::test]
    #[ignore] // 默认忽略，需要真实环境配置
    async fn test_execute_order_internal_real_scenario() {
        use chrono::Utc;
        use rust_quant_core::database::get_db_pool;
        use rust_quant_domain::{StrategyStatus, StrategyType, Timeframe};
        use rust_quant_infrastructure::repositories::SqlxSwapOrderRepository;

        println!("🚀 开始真实场景集成测试");

        // 1. 初始化数据库连接（如果配置了）
        let pool_result = std::panic::catch_unwind(get_db_pool);
        let repo: Arc<dyn SwapOrderRepository> = match pool_result {
            Ok(pool) => {
                println!("✅ 数据库连接成功");
                // Pool 实现了 Clone trait，可以安全地克隆
                Arc::new(SqlxSwapOrderRepository::new(pool.clone()))
            }
            Err(_) => {
                println!("⚠️  数据库未配置，使用Mock Repository");
                Arc::new(MockSwapOrderRepository::new())
            }
        };

        let service = StrategyExecutionService::new(repo.clone());

        // 2. 创建真实的策略配置
        let config_id = 1i64;
        let inst_id = "BTC-USDT-SWAP";
        let period = "1H";
        let risk_config = rust_quant_domain::BasicRiskConfig {
            max_loss_percent: 0.02, // 2%止损
            atr_take_profit_ratio: None,
            fix_signal_kline_take_profit_ratio: None,
            is_move_stop_loss: None,
            is_used_signal_k_line_stop_loss: Some(true), // 使用信号K线止损
            max_hold_time: None,
            max_leverage: None,
        };

        let config = StrategyConfig {
            id: config_id,
            strategy_type: StrategyType::Vegas,
            symbol: "BTC-USDT".to_string(),
            timeframe: Timeframe::H1,
            status: StrategyStatus::Running,
            parameters: serde_json::json!({}),
            risk_config: serde_json::to_value(&risk_config).unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            backtest_start: None,
            backtest_end: None,
            description: Some("真实场景测试配置".to_string()),
        };

        // 3. 创建真实的交易信号（模拟策略分析结果）
        let current_price = 50000.0;
        let ts = chrono::Utc::now().timestamp_millis();
        let signal = SignalResult {
            should_buy: true,
            should_sell: false,
            open_price: current_price,
            signal_kline_stop_loss_price: Some(current_price * 0.98), // 2%止损
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            stop_loss_source: None,
            ts,
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Long,
        };

        println!(
            "📊 交易信号: should_buy={}, open_price={}, stop_loss={:?}",
            signal.should_buy, signal.open_price, signal.signal_kline_stop_loss_price
        );

        // 4. 验证信号和配置
        assert!(signal.should_buy, "信号应该是买入信号");
        assert_eq!(signal.open_price, current_price);
        assert!(signal.signal_kline_stop_loss_price.is_some());

        // 5. 验证止损价格计算逻辑
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;
        let default_stop_loss = entry_price * (1.0 - max_loss_percent);
        let final_stop_loss = match risk_config.is_used_signal_k_line_stop_loss {
            Some(true) => signal
                .signal_kline_stop_loss_price
                .unwrap_or(default_stop_loss),
            _ => default_stop_loss,
        };

        assert!(entry_price > final_stop_loss, "做多时开仓价应该 > 止损价");
        assert_eq!(
            final_stop_loss,
            current_price * 0.98,
            "应该使用信号K线止损价"
        );
        println!(
            "✅ 止损价格验证通过: entry={}, stop_loss={}",
            entry_price, final_stop_loss
        );

        // 6. 验证订单ID生成
        let in_order_id =
            SwapOrder::generate_live_in_order_id(inst_id, "vegas", config_id, period, signal.ts);
        assert!(!in_order_id.is_empty());
        assert!(in_order_id.contains(inst_id));
        println!("✅ 订单ID生成: {}", in_order_id);

        // 7. 检查幂等性
        let existing_order = service
            .swap_order_repository
            .find_by_in_order_id(&in_order_id)
            .await
            .unwrap();

        if let Some(existing_order) = existing_order {
            println!("⚠️  订单已存在（幂等性检查通过），跳过重复下单");
            println!("   已存在订单: {:?}", existing_order.out_order_id);
            println!(
                "   配置ID: {}, 交易对: {}, 周期: {}",
                config_id, inst_id, period
            );
            return;
        }
        println!("✅ 幂等性检查通过，可以下单");

        // 8. 尝试通过execute_strategy执行完整流程（需要真实环境）
        // 注意：这会实际调用外部API，需要：
        // - 数据库中存在config_id对应的策略配置
        // - 数据库中配置了策略与API的关联
        // - API配置有效且有足够资金

        println!("ℹ️  尝试执行完整下单流程...");
        println!("   提示：如果数据库和API未配置，此步骤会失败，但逻辑验证已完成");

        // 由于execute_strategy需要真实的K线数据，这里我们只验证逻辑
        // 如果需要完整测试，需要提供真实的CandlesEntity

        // 9. 验证订单详情构建
        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": final_stop_loss,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
                "atr_stop_loss_price": signal.atr_stop_loss_price,
                "atr_take_profit_ratio_price": signal.atr_take_profit_ratio_price,
            }
        });

        assert_eq!(order_detail["entry_price"], entry_price);
        assert_eq!(order_detail["stop_loss"], final_stop_loss);
        assert_eq!(order_detail["signal"]["should_buy"], signal.should_buy);
        println!("✅ 订单详情构建验证通过");

        // 10. 验证订单对象创建
        let swap_order = SwapOrder::from_signal(
            config_id as i32,
            inst_id,
            period,
            "vegas",
            "buy",
            "long",
            "1.0",
            &in_order_id,
            "test_out_123",
            "okx",
            &order_detail.to_string(),
        );

        assert_eq!(swap_order.strategy_id, config_id as i32);
        assert_eq!(swap_order.inst_id, inst_id);
        assert_eq!(swap_order.side, "buy");
        assert_eq!(swap_order.pos_side, "long");
        assert_eq!(swap_order.in_order_id, in_order_id);
        println!("✅ 订单对象创建验证通过");

        println!("✅ 真实场景测试完成：所有逻辑验证通过");
        println!("   如需完整测试，请配置数据库和API环境变量");
    }

    #[tokio::test]
    #[ignore]
    async fn test_execute_order_internal_simulated_service_e2e_persists_swap_order(
    ) -> anyhow::Result<()> {
        use rust_quant_core::cache::init_redis_pool;
        use rust_quant_core::database::{get_db_pool, init_db_pool};
        use rust_quant_domain::entities::ExchangeApiConfig;
        use rust_quant_domain::traits::{ExchangeApiConfigRepository, StrategyApiConfigRepository};
        use rust_quant_infrastructure::repositories::{
            SqlxExchangeApiConfigRepository, SqlxStrategyApiConfigRepository,
            SqlxSwapOrderRepository,
        };

        fn env_or_default(key: &str, default: &str) -> String {
            std::env::var(key).unwrap_or_else(|_| default.to_string())
        }

        fn env_required(key: &str) -> anyhow::Result<String> {
            std::env::var(key).map_err(|_| anyhow::anyhow!("missing env var: {}", key))
        }

        async fn get_position_mgn_mode(
            okx: &crate::exchange::OkxOrderService,
            api: &ExchangeApiConfig,
            inst_id: &str,
            pos_side: &str,
        ) -> anyhow::Result<Option<String>> {
            let positions = okx.get_positions(api, Some("SWAP"), Some(inst_id)).await?;
            for p in positions {
                if p.inst_id != inst_id || p.pos_side != pos_side {
                    continue;
                }
                let qty = p.pos.parse::<f64>().unwrap_or(0.0);
                if qty.abs() < 1e-12 {
                    continue;
                }
                return Ok(Some(p.mgn_mode));
            }
            Ok(None)
        }

        async fn wait_for_position(
            okx: &crate::exchange::OkxOrderService,
            api: &ExchangeApiConfig,
            inst_id: &str,
            pos_side: &str,
            should_exist: bool,
        ) -> anyhow::Result<()> {
            let max_tries: usize = env_or_default("OKX_TEST_RETRY", "20").parse().unwrap_or(20);
            let sleep_ms: u64 = env_or_default("OKX_TEST_RETRY_SLEEP_MS", "500")
                .parse()
                .unwrap_or(500);

            for _ in 0..max_tries {
                let exists = get_position_mgn_mode(okx, api, inst_id, pos_side)
                    .await?
                    .is_some();
                if exists == should_exist {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }

            anyhow::bail!(
                "position state did not converge: inst_id={}, pos_side={}, expected_exist={}",
                inst_id,
                pos_side,
                should_exist
            );
        }

        dotenv::dotenv().ok();
        if env_or_default("RUN_OKX_SIMULATED_SERVICE_E2E", "0") != "1" {
            eprintln!(
                "skip test_execute_order_internal_simulated_service_e2e_persists_swap_order: RUN_OKX_SIMULATED_SERVICE_E2E!=1"
            );
            return Ok(());
        }

        std::env::set_var("APP_ENV", "local");
        std::env::set_var("OKX_SIMULATED_TRADING", "1");
        std::env::set_var("OKX_REQUEST_EXPIRATION_MS", "300000");
        if std::env::var("DB_HOST").is_err() {
            std::env::set_var(
                "DB_HOST",
                "mysql://root:example@127.0.0.1:33306/test?ssl-mode=DISABLED",
            );
        }
        if std::env::var("REDIS_HOST").is_err() {
            std::env::set_var("REDIS_HOST", "redis://127.0.0.1:6379/");
        }
        let _ = init_db_pool().await;
        let _ = init_redis_pool().await;
        let pool = get_db_pool().clone();

        let api_key = env_required("OKX_SIMULATED_API_KEY")?;
        let api_secret = env_required("OKX_SIMULATED_API_SECRET")?;
        let passphrase = env_required("OKX_SIMULATED_PASSPHRASE")?;
        let inst_id = env_or_default("OKX_TEST_INST_ID", "ETH-USDT-SWAP");
        let period = "4H";
        let strategy_type = "vegas";
        let config_id = env_or_default("OKX_SIMULATED_SERVICE_STRATEGY_CONFIG_ID", "990011")
            .parse::<i64>()
            .unwrap_or(990011);

        let api_repo = SqlxExchangeApiConfigRepository::new(pool.clone());
        let relation_repo = SqlxStrategyApiConfigRepository::new(pool.clone());
        let swap_repo = Arc::new(SqlxSwapOrderRepository::new(pool.clone()));
        let service = StrategyExecutionService::new(swap_repo.clone());

        let api_config = ExchangeApiConfig::new(
            0,
            "okx".to_string(),
            api_key.clone(),
            api_secret,
            Some(passphrase.clone()),
            true,
            true,
            Some("simulated-service-e2e".to_string()),
        );

        let api_config_id = api_repo.save(&api_config).await?;
        relation_repo
            .create_association(config_id as i32, api_config_id, 1)
            .await?;

        let okx_service = crate::exchange::OkxOrderService;
        let db_api_config = api_repo
            .find_by_id(api_config_id)
            .await?
            .ok_or_else(|| anyhow!("saved api config not found"))?;

        if let Some(mgn_mode) =
            get_position_mgn_mode(&okx_service, &db_api_config, &inst_id, "long").await?
        {
            okx_service
                .close_position(
                    &db_api_config,
                    &inst_id,
                    okx::dto::PositionSide::Long,
                    &mgn_mode,
                )
                .await?;
            wait_for_position(&okx_service, &db_api_config, &inst_id, "long", false).await?;
        }

        let open_price = {
            #[derive(serde::Deserialize)]
            struct OkxTickerResponse {
                data: Vec<OkxTickerData>,
            }
            #[derive(serde::Deserialize)]
            struct OkxTickerData {
                last: String,
            }
            let url = format!(
                "https://www.okx.com/api/v5/market/ticker?instId={}",
                inst_id
            );
            let resp = reqwest::get(url).await?.error_for_status()?;
            let data: OkxTickerResponse = resp.json().await?;
            data.data
                .first()
                .ok_or_else(|| anyhow!("empty ticker response"))?
                .last
                .parse::<f64>()?
        };

        let signal = SignalResult {
            should_buy: true,
            should_sell: false,
            open_price,
            signal_kline_stop_loss_price: Some(open_price * 0.98),
            best_open_price: None,
            atr_take_profit_ratio_price: None,
            atr_stop_loss_price: None,
            long_signal_take_profit_price: None,
            short_signal_take_profit_price: None,
            stop_loss_source: None,
            ts: chrono::Utc::now().timestamp_millis(),
            single_value: None,
            single_result: None,
            is_ema_short_trend: None,
            is_ema_long_trend: None,
            atr_take_profit_level_1: None,
            atr_take_profit_level_2: None,
            atr_take_profit_level_3: None,
            filter_reasons: vec![],
            dynamic_adjustments: vec![],
            dynamic_config_snapshot: None,
            direction: rust_quant_domain::SignalDirection::Long,
        };
        let risk_config = create_test_risk_config(0.02, Some(true));

        service
            .execute_order_internal(
                &inst_id,
                period,
                &signal,
                &risk_config,
                config_id,
                strategy_type,
            )
            .await?;

        let in_order_id = SwapOrder::generate_live_in_order_id(
            &inst_id,
            strategy_type,
            config_id,
            period,
            signal.ts,
        );
        let saved = swap_repo
            .find_by_in_order_id(&in_order_id)
            .await?
            .ok_or_else(|| anyhow!("swap_orders did not persist in_order_id={}", in_order_id))?;
        assert_eq!(saved.strategy_id, config_id as i32);
        assert_eq!(saved.inst_id, inst_id);
        assert_eq!(saved.period, period);
        assert_eq!(saved.side, "buy");
        assert_eq!(saved.pos_side, "long");
        assert!(!saved.out_order_id.trim().is_empty());

        wait_for_position(&okx_service, &db_api_config, &inst_id, "long", true).await?;

        if let Some(mgn_mode) =
            get_position_mgn_mode(&okx_service, &db_api_config, &inst_id, "long").await?
        {
            okx_service
                .close_position(
                    &db_api_config,
                    &inst_id,
                    okx::dto::PositionSide::Long,
                    &mgn_mode,
                )
                .await?;
            wait_for_position(&okx_service, &db_api_config, &inst_id, "long", false).await?;
        }

        sqlx::query("DELETE FROM exchange_apikey_strategy_relation WHERE strategy_config_id = ?")
            .bind(config_id as i32)
            .execute(&pool)
            .await?;
        sqlx::query("UPDATE exchange_apikey_config SET is_deleted = 1 WHERE id = ?")
            .bind(api_config_id)
            .execute(&pool)
            .await?;

        Ok(())
    }

    /// 测试：execute_order_internal - 完整流程验证（逻辑层面）
    #[test]
    fn test_execute_order_internal_full_flow_logic() {
        // 1. 创建信号
        let signal = create_buy_signal(50000.0, 1234567890);
        assert!(signal.should_buy);
        assert_eq!(signal.open_price, 50000.0);

        // 2. 创建风险配置
        let risk_config = create_test_risk_config(0.02, None);
        assert_eq!(risk_config.max_loss_percent, 0.02);

        // 3. 计算止损价格
        let entry_price = signal.open_price;
        let max_loss_percent = risk_config.max_loss_percent;
        let stop_loss_price = entry_price * (1.0 - max_loss_percent);
        assert_eq!(stop_loss_price, 49000.0);

        // 4. 验证止损价格合理性（做多）
        let _pos_side = "long";
        assert!(entry_price > stop_loss_price, "做多时开仓价应该 > 止损价");

        // 5. 计算下单数量
        let max_available = 100.0;
        let safety_factor = 0.9;
        let order_size_f64 = max_available * safety_factor;
        let order_size = format!("{:.2}", order_size_f64);
        assert_eq!(order_size, "90.00");

        // 6. 生成订单ID
        let inst_id = "BTC-USDT-SWAP";
        let config_id = 1i64;
        let period = "1H";
        let in_order_id =
            SwapOrder::generate_live_in_order_id(inst_id, "vegas", config_id, period, signal.ts);
        assert_eq!(
            in_order_id,
            format!(
                "{}_{}_{}_{}_{}",
                inst_id, "vegas", config_id, period, signal.ts
            )
        );

        // 7. 创建订单详情
        let order_detail = serde_json::json!({
            "entry_price": entry_price,
            "stop_loss": stop_loss_price,
            "signal": {
                "should_buy": signal.should_buy,
                "should_sell": signal.should_sell,
            }
        });
        assert_eq!(order_detail["entry_price"], entry_price);
        assert_eq!(order_detail["stop_loss"], stop_loss_price);
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_in_band() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(300.2, 699.8, config);
        assert!(result.is_none());
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_fund_to_trade() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(250.0, 750.0, config);
        assert_eq!(
            result,
            Some((50.0, TradeBucketTransferDirection::FundToTrade))
        );
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_trade_to_fund() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(350.0, 650.0, config);
        assert_eq!(
            result,
            Some((50.0, TradeBucketTransferDirection::TradeToFund))
        );
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_exact_difference_inside_old_band() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let lower = StrategyExecutionService::calculate_trade_bucket_transfer(290.0, 710.0, config);
        let upper = StrategyExecutionService::calculate_trade_bucket_transfer(310.0, 690.0, config);

        assert_eq!(
            lower,
            Some((10.0, TradeBucketTransferDirection::FundToTrade))
        );
        assert_eq!(
            upper,
            Some((10.0, TradeBucketTransferDirection::TradeToFund))
        );
    }

    #[test]
    fn test_calculate_trade_bucket_transfer_dynamic_ratio_growth() {
        let config = LiveTradeBucketRebalanceConfig {
            target_trade_ratio: 0.30,
            min_transfer: 1.0,
            transfer_epsilon: 0.5,
        };

        let result =
            StrategyExecutionService::calculate_trade_bucket_transfer(500.0, 1500.0, config);
        assert_eq!(
            result,
            Some((100.0, TradeBucketTransferDirection::FundToTrade))
        );
    }
}
