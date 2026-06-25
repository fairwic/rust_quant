//! 策略执行服务
//!
//! 协调策略分析、风控检查、订单创建的完整业务流程
use super::live_decision::{apply_live_decision, approx_eq_opt};
use super::strategy_signal_payload::{self, StrategySignalPayloadBuildOptions};
use crate::rust_quan_web::{ExecutionTaskClient, ExecutionTaskConfig, StrategySignalSubmitRequest};
use anyhow::{anyhow, Result};
use dashmap::DashMap;
use okx::dto::account_dto::Position as OkxPosition;
use okx::enums::account_enums::AccountType;
use redis::AsyncCommands;
use rust_quant_common::CandleItem;
use rust_quant_core::cache::get_redis_connection;
use rust_quant_domain::entities::SwapOrder;
use rust_quant_domain::traits::SwapOrderRepository;
use rust_quant_domain::{OrderSide, PositionSide, StrategyConfig};
use rust_quant_strategies::framework::backtest::{
    compute_current_targets, BasicRiskStrategyConfig, ExitTargets, TradingState,
};
use rust_quant_strategies::framework::risk::{StopLossCalculator, StopLossSide};
use rust_quant_strategies::framework::types::TradeSide;
use rust_quant_strategies::strategy_common::SignalResult;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};
#[derive(Debug, Clone, Default, PartialEq)]
struct LiveExitTargets {
    /// 止损；为空时使用默认值或表示不限制。
    stop_loss: Option<f64>,
    /// 止盈；为空时使用默认值或表示不限制。
    take_profit: Option<f64>,
    /// 列表数据。
    algo_ids: Vec<String>,
    /// trade方向；为空时使用默认值或表示不限制。
    trade_side: Option<TradeSide>,
}
#[derive(Debug)]
enum CloseAlgoSyncResult {
    Placed(Vec<String>),
    Cleared,
    SkippedNoPosition,
}
#[cfg(test)]
#[derive(Default)]
struct GuardTestState {
    /// 开盘fail，用于交易策略计算。
    open_fail: AtomicBool,
    /// compensatefail，用于交易策略计算。
    compensate_fail: AtomicBool,
    /// hasalgo之后compensate。
    has_algo_after_compensate: AtomicBool,
    /// 收盘fail，用于交易策略计算。
    close_fail: AtomicBool,
    /// compensatecalls，用于交易策略计算。
    compensate_calls: AtomicUsize,
    /// 收盘calls，用于交易策略计算。
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
    /// 状态值。
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
    /// 判断K 线entitytoitem，给交易执行流程提供布尔结果。
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
    /// 封装环境变量enabled，减少交易执行调用方重复实现相同细节。
    fn env_enabled(key: &str) -> bool {
        match std::env::var(key) {
            Ok(v) => matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "y" | "on"
            ),
            Err(_) => false,
        }
    }
    /// 封装实盘tpslepsilon，减少交易执行调用方重复实现相同细节。
    fn live_tp_sl_epsilon() -> f64 {
        std::env::var("LIVE_TP_SL_EPSILON")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1e-6)
    }
    /// 封装环境变量positivef64，减少交易执行调用方重复实现相同细节。
    fn env_positive_f64(key: &str) -> Option<f64> {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
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
    /// 执行策略分析和交易流程
    /// 参考原始业务逻辑：src/trading/strategy/executor_common.rs::execute_order
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
        if Self::smoke_forced_signal_side_from_env().is_some() {
            let trigger_candle = snap_item.as_ref().ok_or_else(|| {
                anyhow!("RUST_QUANT_SMOKE_FORCE_SIGNAL requires a confirmed trigger candle")
            })?;
            if Self::apply_smoke_forced_signal_from_env(&mut signal, trigger_candle)? {
                warn!(
                    "已应用 smoke 强制策略信号: inst_id={}, period={}, should_buy={}, should_sell={}, ts={}",
                    inst_id, period, signal.should_buy, signal.should_sell, signal.ts
                );
            }
        }
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
    /// 执行 交易执行与风控 主流程，并把外部依赖调用、状态推进和错误返回串起来。
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
                    config.exchange.as_deref(),
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
            if Self::should_manage_local_close_algos_after_open() {
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
            } else {
                info!(
                    "Web 分发模式跳过本地交易所止盈止损委托同步: inst_id={}, config_id={}",
                    inst_id, config.id
                );
                self.live_exit_targets.insert(
                    config.id,
                    LiveExitTargets {
                        stop_loss: targets.stop_loss,
                        take_profit: targets.take_profit,
                        algo_ids: vec![],
                        trade_side: Some(side),
                    },
                );
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
            use rust_quant_core::database::get_db_pool;
            use rust_quant_infrastructure::SignalLogRepository;
            let repo = SignalLogRepository::new(get_db_pool().clone());
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
    /// 在经济事件发布前后的时间窗口内，市场波动剧烈，
    /// 不适合追涨追跌，应等待回调后再入场。
    /// # 默认窗口
    /// - 事件前 30 分钟开始生效
    /// - 事件后 60 分钟仍在影响中
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
    async fn execute_order_internal(
        &self,
        inst_id: &str,
        period: &str,
        signal: &SignalResult,
        risk_config: &rust_quant_domain::BasicRiskConfig,
        config_id: i64,
        strategy_type: &str,
        exchange: Option<&str>,
    ) -> Result<()> {
        #[cfg(test)]
        if self.guard_test_state.open_fail.load(Ordering::SeqCst) {
            return Err(anyhow!("mock open failed"));
        }
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
        let (order_side, position_side) = Self::trade_sides_from_signal(signal)?;
        let side = order_side.as_str();
        let pos_side = position_side.as_str();
        info!("交易方向: side={}, pos_side={}", side, pos_side);
        if Self::should_dispatch_strategy_signal_to_quant_web() {
            info!(
                "策略信号改由 rust_quan_web 分发执行任务: inst_id={}, period={}, config_id={}, side={}, pos_side={}",
                inst_id, period, config_id, side, pos_side
            );
            self.dispatch_strategy_signal_to_quant_web(
                inst_id,
                period,
                signal,
                risk_config,
                config_id,
                strategy_type,
                exchange,
                side,
                pos_side,
                &client_order_id,
            )
            .await?;
            return Ok(());
        }
        Self::ensure_legacy_direct_live_exchange_order_allowed()?;
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
        let opposite_position_side = PositionSide::from_order_side(order_side.opposite());
        let opposite_pos_side = opposite_position_side.as_str();
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
                let close_pos_side = if opposite_position_side == PositionSide::Long {
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
        let max_size_str = if order_side == OrderSide::Buy {
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
        let stop_candidates = Self::build_stop_loss_candidates(order_side, signal, risk_config)?;
        let stop_side = if order_side == OrderSide::Sell {
            StopLossSide::Short
        } else {
            StopLossSide::Long
        };
        let final_stop_loss = StopLossCalculator::select(stop_side, entry_price, &stop_candidates)
            .ok_or_else(|| anyhow!("无有效止损价"))?;
        let take_profit_trigger_px: Option<f64> = None;
        // 验证止损价格合理性
        if position_side == PositionSide::Short && entry_price > final_stop_loss {
            error!(
                "做空开仓价 > 止损价，不下单: entry={}, stop_loss={}",
                entry_price, final_stop_loss
            );
            return Err(anyhow!("止损价格不合理"));
        }
        if position_side == PositionSide::Long && entry_price < final_stop_loss {
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
include!("strategy_execution_service/live_close_algo_section.rs");
include!("strategy_execution_service/external_flat_section.rs");
include!("strategy_execution_service/live_helpers.rs");
#[cfg(test)]
mod tests {
    include!("strategy_execution_service/core_tests.rs");
    include!("strategy_execution_service/core_order_logic_tests.rs");
    include!("strategy_execution_service/order_tests.rs");
}
