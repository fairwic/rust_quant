use super::super::types::TradeSide;
use super::position::close_position;
use super::types::{BasicRiskStrategyConfig, SignalResult, TradePosition, TradingState};
use crate::CandleItem;

// ============================================================================
// 出场上下文结构（减少参数传递和重复计算）
// ============================================================================

/// 出场检查上下文，封装常用数据避免重复计算
struct ExitContext {
    side: TradeSide,
    entry: f64,
    qty: f64,
    /// 不利价格（触发止损用）：Long=low, Short=high
    adverse_price: f64,
    /// 有利价格（触发止盈用）：Long=high, Short=low
    favorable_price: f64,
}

impl ExitContext {
    fn new(position: &TradePosition, candle: &CandleItem) -> Self {
        let side = position.trade_side.clone();
        Self {
            entry: position.open_price,
            qty: position.position_nums,
            adverse_price: match side {
                TradeSide::Long => candle.l,
                TradeSide::Short => candle.h,
            },
            favorable_price: match side {
                TradeSide::Long => candle.h,
                TradeSide::Short => candle.l,
            },
            side,
        }
    }

    /// 计算利润
    #[inline]
    fn profit(&self, exit_price: f64) -> f64 {
        match self.side {
            TradeSide::Long => (exit_price - self.entry) * self.qty,
            TradeSide::Short => (self.entry - exit_price) * self.qty,
        }
    }

    /// 检查止盈是否触发
    #[inline]
    fn is_take_profit_hit(&self, target: f64) -> bool {
        match self.side {
            TradeSide::Long => self.favorable_price >= target,
            TradeSide::Short => self.favorable_price <= target,
        }
    }

    /// 检查止盈是否触发（严格模式，用于某些需要 > 而非 >= 的场景）
    #[inline]
    fn is_take_profit_hit_strict(&self, target: f64) -> bool {
        match self.side {
            TradeSide::Long => self.favorable_price > target,
            TradeSide::Short => self.favorable_price < target,
        }
    }

    /// 检查止损是否触发
    #[inline]
    fn is_stop_loss_hit(&self, target: f64) -> bool {
        match self.side {
            TradeSide::Long => self.adverse_price <= target,
            TradeSide::Short => self.adverse_price >= target,
        }
    }

    /// 计算止损价格
    #[inline]
    fn stop_loss_price(&self, loss_pct: f64) -> f64 {
        match self.side {
            TradeSide::Long => self.entry * (1.0 - loss_pct),
            TradeSide::Short => self.entry * (1.0 + loss_pct),
        }
    }

    /// 计算收益率
    #[inline]
    fn profit_pct(&self) -> f64 {
        match self.side {
            TradeSide::Long => (self.adverse_price - self.entry) / self.entry,
            TradeSide::Short => (self.entry - self.adverse_price) / self.entry,
        }
    }
}

// ============================================================================
// 出场结果
// ============================================================================

/// 出场检查结果
enum ExitResult {
    /// 触发出场，返回平仓价格和原因
    Exit { price: f64, reason: &'static str },
    /// 触发出场，返回平仓价格和动态原因
    ExitDynamic { price: f64, reason: String },
    /// 未触发
    None,
}

// ============================================================================
// 止损检查函数
// ============================================================================

/// 检查最大损失止损
fn check_max_loss_stop(ctx: &ExitContext, max_loss_pct: f64) -> ExitResult {
    if ctx.profit_pct() < -max_loss_pct {
        let stop_price = ctx.stop_loss_price(max_loss_pct);
        ExitResult::Exit {
            price: stop_price,
            reason: "最大亏损止损",
        }
    } else {
        ExitResult::None
    }
}

/// 检查单K振幅固定止损（1R）
fn check_one_k_line_diff_stop(
    ctx: &ExitContext,
    position: &TradePosition,
    enabled: Option<bool>,
) -> ExitResult {
    if enabled.unwrap_or(false) == false {
        return ExitResult::None;
    }
    let diff = position.signal_high_low_diff;
    if diff <= 0.0 {
        return ExitResult::None;
    }

    let stop_price = match ctx.side {
        TradeSide::Long => ctx.entry - diff,
        TradeSide::Short => ctx.entry + diff,
    };

    if ctx.is_stop_loss_hit(stop_price) {
        ExitResult::Exit {
            price: stop_price,
            reason: "单K振幅止损(1R)",
        }
    } else {
        ExitResult::None
    }
}

/// 检查信号K线止损
fn check_signal_kline_stop(ctx: &ExitContext, stop_price: Option<f64>) -> ExitResult {
    match stop_price {
        Some(price) if ctx.is_stop_loss_hit(price) => ExitResult::Exit {
            price,
            reason: "预止损-信号线失效",
        },
        _ => ExitResult::None,
    }
}

/// 检查三级ATR系统的移动止损
fn check_atr_trailing_stop(ctx: &ExitContext, position: &TradePosition) -> ExitResult {
    // 必须有三级止盈配置才有移动止损
    if position.atr_take_profit_level_1.is_none() {
        return ExitResult::None;
    }

    match position.move_stop_open_price {
        Some(stop_price) if ctx.is_stop_loss_hit(stop_price) => ExitResult::ExitDynamic {
            price: stop_price,
            reason: format!("移动止损(触发级别:{})", position.reached_take_profit_level),
        },
        _ => ExitResult::None,
    }
}

// ============================================================================
// 三级止盈系统
// ============================================================================

/// 更新三级ATR止盈系统的级别和移动止损线
/// 返回是否触发第三级完全平仓
fn update_atr_tiered_levels(ctx: &ExitContext, position: &mut TradePosition) -> ExitResult {
    let (level_1, level_2, level_3) = match (
        position.atr_take_profit_level_1,
        position.atr_take_profit_level_2,
        position.atr_take_profit_level_3,
    ) {
        (Some(l1), Some(l2), Some(l3)) => (l1, l2, l3),
        _ => return ExitResult::None,
    };

    let current_level = position.reached_take_profit_level;

    // 第三级：5倍ATR，完全平仓
    if current_level < 3 && ctx.is_take_profit_hit(level_3) {
        return ExitResult::Exit {
            price: level_3,
            reason: "三级止盈(5倍ATR)-完全平仓",
        };
    }

    // 第二级：2倍ATR，移动止损到第一级止盈价
    if current_level < 2 && ctx.is_take_profit_hit(level_2) {
        position.reached_take_profit_level = 2;
        position.move_stop_open_price = Some(level_1);
    }

    // 第一级：1.5倍ATR，移动止损到开仓价
    if current_level < 1 && ctx.is_take_profit_hit(level_1) {
        position.reached_take_profit_level = 1;
        position.move_stop_open_price = Some(ctx.entry);
    }

    ExitResult::None
}

/// 触发保本移动止损：价格到达预设触发价后，将止损抬到开仓价
fn activate_break_even_stop(
    risk_config: &BasicRiskStrategyConfig,
    ctx: &ExitContext,
    trade_position: &mut TradePosition,
) {
    if risk_config
        .is_move_stop_open_price_when_touch_price
        .unwrap_or(false)
        == false
    {
        return;
    }

    let Some(trigger_price) = trade_position.move_stop_open_price_when_touch_price else {
        return;
    };

    // 已经激活过
    if trade_position.move_stop_open_price.is_some() {
        return;
    }

    let is_hit = match ctx.side {
        TradeSide::Long => ctx.favorable_price >= trigger_price,
        TradeSide::Short => ctx.favorable_price <= trigger_price,
    };

    if is_hit {
        trade_position.move_stop_open_price = Some(trade_position.open_price);
    }
}

// ============================================================================
// 止盈检查函数
// ============================================================================

/// 检查ATR比例止盈
fn check_atr_ratio_take_profit(
    ctx: &ExitContext,
    ratio: Option<f64>,
    target_price: Option<f64>,
) -> ExitResult {
    let ratio = match ratio {
        Some(r) if r > 0.0 => r,
        _ => return ExitResult::None,
    };

    match target_price {
        Some(price) if ctx.is_take_profit_hit(price) => ExitResult::Exit {
            price,
            reason: "atr按收益比例止盈",
        },
        _ => ExitResult::None,
    }
}

/// 检查固定信号线比例止盈
fn check_fixed_take_profit(ctx: &ExitContext, target: Option<f64>) -> ExitResult {
    match target {
        // 使用严格模式 (> 而非 >=)，保持原有行为
        Some(price) if ctx.is_take_profit_hit_strict(price) => ExitResult::Exit {
            price,
            reason: "固定信号线比例止盈",
        },
        _ => ExitResult::None,
    }
}

/// 检查动态止盈（做多/做空通用）
fn check_dynamic_take_profit(
    ctx: &ExitContext,
    long_target: Option<f64>,
    short_target: Option<f64>,
) -> ExitResult {
    let (target, reason) = match ctx.side {
        TradeSide::Long => match long_target {
            Some(t) if ctx.favorable_price > t => (t, "做多触达指标动态止盈"),
            _ => return ExitResult::None,
        },
        TradeSide::Short => match short_target {
            Some(t) if ctx.favorable_price < t => (t, "做空触达指标动态止盈"),
            _ => return ExitResult::None,
        },
    };

    ExitResult::Exit {
        price: target,
        reason,
    }
}

/// 检查逆势回调止盈
fn check_counter_trend_take_profit(ctx: &ExitContext, target: Option<f64>) -> ExitResult {
    match target {
        Some(price) if ctx.is_take_profit_hit(price) => ExitResult::Exit {
            price,
            reason: "逆势回调止盈",
        },
        _ => ExitResult::None,
    }
}

// ============================================================================
// 主函数
// ============================================================================

/// 风险管理检查入口
///
/// # 优先级原则
/// **同一K线内，止损永远优先于止盈**
///
/// ## 检查顺序
/// ### 止损（优先级高）
/// 1. 最大损失止损 - 资金保护
/// 2. 保本移动止损激活 - 触及触发价后将止损抬到开仓价
/// 3. 单K振幅固定止损(1R) - 开仓K线振幅对称止损
/// 4. 移动止损 - 三级ATR系统/保本止损
/// 5. 信号K线止损 - 技术止损
///
/// ### 止盈
/// 6. 三级ATR止盈 - 5倍ATR完全平仓
/// 7. ATR比例止盈
/// 8. 固定信号线比例止盈
/// 9. 动态止盈 - 指标动态止盈
/// 10. 逆势回调止盈
pub fn check_risk_config(
    risk_config: &BasicRiskStrategyConfig,
    mut trading_state: TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) -> TradingState {
    let Some(ref position) = trading_state.trade_position else {
        return trading_state;
    };

    let mut trade_position = position.clone();
    let ctx = ExitContext::new(&trade_position, candle);

    // ========================================================================
    // 止损检查（优先级最高）
    // ========================================================================

    // 1. 最大损失止损
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_max_loss_stop(&ctx, risk_config.max_loss_percent)
    {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 2. 保本移动止损激活（非立即出场，更新止损线）
    activate_break_even_stop(risk_config, &ctx, &mut trade_position);

    // 3. 单K振幅固定止损（1R）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_one_k_line_diff_stop(
            &ctx,
            &trade_position,
            risk_config.is_one_k_line_diff_stop_loss,
        )
    {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 4. 移动止损（三级ATR系统/保本止损）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_atr_trailing_stop(&ctx, &trade_position)
    {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 5. 信号K线止损
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_signal_kline_stop(&ctx, trade_position.signal_kline_stop_close_price)
    {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // ========================================================================
    // 止盈检查
    // ========================================================================

    // 6. 三级ATR止盈（同时更新级别）
    if let result @ ExitResult::Exit { .. } = update_atr_tiered_levels(&ctx, &mut trade_position) {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 7. ATR比例止盈
    if let result @ ExitResult::Exit { .. } = check_atr_ratio_take_profit(
        &ctx,
        risk_config.atr_take_profit_ratio,
        trade_position.atr_take_ratio_profit_price,
    ) {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 8. 固定信号线比例止盈
    if let result @ ExitResult::Exit { .. } =
        check_fixed_take_profit(&ctx, trade_position.fixed_take_profit_price)
    {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 9. 动态止盈（做多/做空）
    if let result @ ExitResult::Exit { .. } = check_dynamic_take_profit(
        &ctx,
        trade_position.long_signal_take_profit_price,
        trade_position.short_signal_take_profit_price,
    ) {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 10. 逆势回调止盈
    if let result @ ExitResult::Exit { .. } = check_counter_trend_take_profit(
        &ctx,
        trade_position.counter_trend_pullback_take_profit_price,
    ) {
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 更新仓位状态（三级止盈系统可能修改了级别和移动止损）
    trading_state.trade_position = Some(trade_position);
    trading_state
}

// ============================================================================
// R系统增强风控（基于第一性原理）
// ============================================================================

use super::r_system::{
    check_time_stop, update_r_system_trailing_stop, RSystemConfig, RSystemState, TimeStopAction,
    TimeStopConfig,
};

/// R系统增强风控配置
#[derive(Debug, Clone)]
pub struct RSystemRiskConfig {
    /// 是否启用R系统移动止损
    pub enable_r_system: bool,
    /// 是否启用时间止损
    pub enable_time_stop: bool,
    /// R系统配置
    pub r_config: RSystemConfig,
    /// 时间止损配置
    pub time_config: TimeStopConfig,
}

impl Default for RSystemRiskConfig {
    fn default() -> Self {
        Self {
            enable_r_system: true,
            enable_time_stop: true,
            r_config: RSystemConfig::default(),
            time_config: TimeStopConfig::default(),
        }
    }
}

/// R系统运行时状态（需要在回测循环中维护）
#[derive(Debug, Clone, Default)]
pub struct RSystemRuntime {
    /// R系统状态
    pub r_state: Option<RSystemState>,
    /// 当前K线索引
    pub current_bar_index: usize,
    /// ATR值（需要从外部计算并传入）
    pub current_atr: f64,
}

/// R系统增强风控检查入口
///
/// 在标准风控的基础上，增加：
/// - R系统移动止损：根据盈利R倍数动态调整止损
/// - 时间止损：根据持仓时间和盈亏状态决定是否平仓
///
/// # 参数
/// - `risk_config`: 基础风控配置
/// - `r_risk_config`: R系统风控配置
/// - `r_runtime`: R系统运行时状态（可变）
/// - `trading_state`: 交易状态
/// - `signal`: 信号结果
/// - `candle`: 当前K线
///
/// # 返回
/// - `TradingState`: 更新后的交易状态
pub fn check_risk_config_with_r_system(
    risk_config: &BasicRiskStrategyConfig,
    r_risk_config: &RSystemRiskConfig,
    r_runtime: &mut RSystemRuntime,
    mut trading_state: TradingState,
    signal: &SignalResult,
    candle: &CandleItem,
) -> TradingState {
    let Some(ref position) = trading_state.trade_position else {
        return trading_state;
    };

    let mut trade_position = position.clone();
    let ctx = ExitContext::new(&trade_position, candle);

    // ========================================================================
    // 止损检查（优先级最高）
    // ========================================================================

    // 1. 最大损失止损（最高优先级）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_max_loss_stop(&ctx, risk_config.max_loss_percent)
    {
        r_runtime.r_state = None; // 平仓后清除R系统状态
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 2. R系统移动止损（新增）
    if r_risk_config.enable_r_system {
        if let Some(ref mut r_state) = r_runtime.r_state {
            // 更新R系统移动止损
            update_r_system_trailing_stop(
                r_state,
                candle.h,
                candle.l,
                r_runtime.current_atr,
                &r_risk_config.r_config,
            );

            // 检查是否触发R系统止损
            if r_state.is_stop_triggered(candle.l, candle.h) {
                let stop_price = r_state.current_stop_price;
                let reason = format!("R系统止损(级别:{:?})", r_state.stop_level);
                r_runtime.r_state = None;
                return finalize_exit(
                    trading_state,
                    trade_position,
                    candle,
                    signal,
                    &ctx,
                    ExitResult::ExitDynamic {
                        price: stop_price,
                        reason,
                    },
                );
            }

            // 同步R系统止损到仓位
            trade_position.move_stop_open_price = Some(r_state.current_stop_price);
            trade_position.reached_take_profit_level = r_state.stop_level.as_level();
        }
    }

    // 3. 时间止损（新增）
    if r_risk_config.enable_time_stop {
        if let Some(ref r_state) = r_runtime.r_state {
            // 构造分批止盈状态（简化版，只检查目标1是否达成）
            let tp_state = super::r_system::TieredTakeProfitState {
                target_1_reached: trade_position.reached_take_profit_level >= 1,
                ..Default::default()
            };

            let time_action = check_time_stop(
                r_state,
                &tp_state,
                r_runtime.current_bar_index,
                candle.c,
                &r_risk_config.time_config,
            );

            match time_action {
                TimeStopAction::CloseAll { reason } => {
                    r_runtime.r_state = None;
                    return finalize_exit(
                        trading_state,
                        trade_position,
                        candle,
                        signal,
                        &ctx,
                        ExitResult::Exit {
                            price: candle.c,
                            reason,
                        },
                    );
                }
                TimeStopAction::Reduce50 { reason } => {
                    // 减仓50%的逻辑需要在外部处理（此处仅记录）
                    // 可以通过设置一个标记让外部代码处理部分平仓
                    tracing::info!(
                        "时间止损触发减仓50%: {}, 当前仓位={}",
                        reason,
                        trade_position.position_nums
                    );
                }
                TimeStopAction::Hold => {}
            }
        }
    }

    // 4. 保本移动止损激活（原有逻辑）
    activate_break_even_stop(risk_config, &ctx, &mut trade_position);

    // 5. 单K振幅固定止损（1R）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_one_k_line_diff_stop(
            &ctx,
            &trade_position,
            risk_config.is_one_k_line_diff_stop_loss,
        )
    {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 6. 移动止损（三级ATR系统/保本止损）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_atr_trailing_stop(&ctx, &trade_position)
    {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 7. 信号K线止损
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_signal_kline_stop(&ctx, trade_position.signal_kline_stop_close_price)
    {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // ========================================================================
    // 止盈检查
    // ========================================================================

    // 8. 三级ATR止盈（同时更新级别）
    if let result @ ExitResult::Exit { .. } = update_atr_tiered_levels(&ctx, &mut trade_position) {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 9. ATR比例止盈
    if let result @ ExitResult::Exit { .. } = check_atr_ratio_take_profit(
        &ctx,
        risk_config.atr_take_profit_ratio,
        trade_position.atr_take_ratio_profit_price,
    ) {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 10. 固定信号线比例止盈
    if let result @ ExitResult::Exit { .. } =
        check_fixed_take_profit(&ctx, trade_position.fixed_take_profit_price)
    {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 11. 动态止盈（做多/做空）
    if let result @ ExitResult::Exit { .. } = check_dynamic_take_profit(
        &ctx,
        trade_position.long_signal_take_profit_price,
        trade_position.short_signal_take_profit_price,
    ) {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 12. 逆势回调止盈
    if let result @ ExitResult::Exit { .. } = check_counter_trend_take_profit(
        &ctx,
        trade_position.counter_trend_pullback_take_profit_price,
    ) {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }

    // 更新仓位状态
    trading_state.trade_position = Some(trade_position);
    trading_state
}

/// 初始化R系统状态（在开仓时调用）
///
/// # 参数
/// - `position`: 当前仓位
/// - `bar_index`: 当前K线索引
///
/// # 返回
/// - `Option<RSystemState>`: R系统状态
pub fn init_r_system_state(position: &TradePosition, bar_index: usize) -> Option<RSystemState> {
    super::r_system::create_r_state_from_position(position, bar_index)
}

/// 执行平仓并返回最终状态
fn finalize_exit(
    mut trading_state: TradingState,
    mut trade_position: TradePosition,
    candle: &CandleItem,
    signal: &SignalResult,
    ctx: &ExitContext,
    result: ExitResult,
) -> TradingState {
    let (price, reason) = match result {
        ExitResult::Exit { price, reason } => (price, reason.to_string()),
        ExitResult::ExitDynamic { price, reason } => (price, reason),
        ExitResult::None => return trading_state,
    };

    trade_position.close_price = Some(price);
    trading_state.trade_position = Some(trade_position);

    let profit = ctx.profit(price);
    close_position(&mut trading_state, candle, signal, &reason, profit);
    trading_state
}
