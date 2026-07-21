use super::super::types::TradeSide;
use super::position::{close_position, partial_close_position};
use super::types::{BasicRiskStrategyConfig, SignalResult, TradePosition, TradingState};
use crate::CandleItem;
// ============================================================================
// 出场上下文结构（减少参数传递和重复计算）
// ============================================================================
/// 出场检查上下文，封装常用数据避免重复计算
struct ExitContext {
    /// 交易方向。
    side: TradeSide,
    /// 交易入场信息。
    entry: f64,
    /// 数量。
    qty: f64,
    /// 不利价格（触发止损用）：Long=low, Short=high
    adverse_price: f64,
    /// 有利价格（触发止盈用）：Long=high, Short=low
    favorable_price: f64,
    /// 当前价格 (Close)
    current_price: f64,
    /// K线时间戳
    candle_ts: i64,
}
impl ExitContext {
    /// 初始化new，确保风控依赖和内部状态可直接使用。
    fn new(position: &TradePosition, candle: &CandleItem) -> Self {
        let side = position.trade_side;
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
            current_price: candle.c,
            candle_ts: candle.ts,
            side,
        }
    }
    /// 计算利润
    fn profit(&self, exit_price: f64) -> f64 {
        match self.side {
            TradeSide::Long => (exit_price - self.entry) * self.qty,
            TradeSide::Short => (self.entry - exit_price) * self.qty,
        }
    }
    /// 检查止盈是否触发
    fn is_take_profit_hit(&self, target: f64) -> bool {
        match self.side {
            TradeSide::Long => self.favorable_price >= target,
            TradeSide::Short => self.favorable_price <= target,
        }
    }
    /// 检查止盈是否触发（严格模式，用于某些需要 > 而非 >= 的场景）
    fn is_take_profit_hit_strict(&self, target: f64) -> bool {
        match self.side {
            TradeSide::Long => self.favorable_price > target,
            TradeSide::Short => self.favorable_price < target,
        }
    }
    /// 检查止损是否触发
    fn is_stop_loss_hit(&self, target: f64) -> bool {
        match self.side {
            TradeSide::Long => self.adverse_price <= target,
            TradeSide::Short => self.adverse_price >= target,
        }
    }
    /// 计算止损价格
    fn stop_loss_price(&self, loss_pct: f64) -> f64 {
        match self.side {
            TradeSide::Long => self.entry * (1.0 - loss_pct),
            TradeSide::Short => self.entry * (1.0 + loss_pct),
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
    /// 触发部分平仓，返回价格、原因和按当前仓位计算的平仓比例
    PartialExit {
        price: f64,
        reason: &'static str,
        close_ratio: f64,
    },
    /// 未触发
    None,
}
// ============================================================================
// 出场目标计算（供实盘同步）
// ============================================================================
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExitTargets {
    /// 止损；为空时使用默认值或表示不限制。
    pub stop_loss: Option<f64>,
    /// 止盈；为空时使用默认值或表示不限制。
    pub take_profit: Option<f64>,
    /// 原因说明。
    pub stop_reason: Option<String>,
    /// 原因说明。
    pub take_reason: Option<String>,
}
#[cfg(test)]
/// 封装当前函数，减少风控调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn compute_effective_max_loss(
    position: &TradePosition,
    ctx: &ExitContext,
    max_loss_pct: f64,
    dynamic_max_loss: bool,
) -> f64 {
    compute_effective_max_loss_with_config(
        position,
        ctx,
        max_loss_pct,
        dynamic_max_loss,
        &BasicRiskStrategyConfig::default(),
    )
}
/// 计算 交易执行与风控 指标，保持公式和边界处理集中可审计。
fn compute_effective_max_loss_with_config(
    position: &TradePosition,
    ctx: &ExitContext,
    max_loss_pct: f64,
    dynamic_max_loss: bool,
    risk_config: &BasicRiskStrategyConfig,
) -> f64 {
    // 高波动动态降损：
    // - 入场K线振幅 > 3% 且方向不利时，收紧到 3%
    // - 否则沿用原逻辑：K线振幅 > 5% 时收紧到 4.5%
    let mut effective_max_loss = max_loss_pct;
    if dynamic_max_loss {
        let mut tightened_by_entry = false;
        if let (Some(entry_amp), Some(entry_close_pos)) = (
            position.entry_kline_amplitude,
            position.entry_kline_close_pos,
        ) {
            let entry_amp_threshold = risk_config.dynamic_entry_amp_threshold.unwrap_or(0.03);
            let entry_loss_percent = risk_config.dynamic_entry_loss_percent.unwrap_or(0.03);
            if entry_amp > entry_amp_threshold {
                let dir_mismatch = match ctx.side {
                    TradeSide::Long => entry_close_pos < 0.5,
                    TradeSide::Short => entry_close_pos > 0.5,
                };
                let require_mismatch = risk_config
                    .dynamic_entry_require_direction_mismatch
                    .unwrap_or(true);
                if !require_mismatch || dir_mismatch {
                    effective_max_loss = effective_max_loss.min(entry_loss_percent);
                    tightened_by_entry = true;
                }
            }
        }
        if !tightened_by_entry {
            let range_pct = (ctx.favorable_price - ctx.adverse_price).abs() / ctx.entry.max(1e-9);
            let range_threshold = risk_config.dynamic_range_threshold.unwrap_or(0.05);
            let range_loss_percent = risk_config.dynamic_range_loss_percent.unwrap_or(0.045);
            if range_pct > range_threshold {
                effective_max_loss = effective_max_loss.min(range_loss_percent);
            }
        }
    }
    effective_max_loss
}
/// 选择 交易执行与风控 的最佳候选结果，避免选择规则分散在调用方。
fn select_tightest_stop(side: TradeSide, candidates: &[f64]) -> Option<f64> {
    let values: Vec<f64> = candidates
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .collect();
    if values.is_empty() {
        return None;
    }
    Some(match side {
        TradeSide::Long => *values
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap(),
        TradeSide::Short => *values
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap(),
    })
}

/// 计算入场时的有效保护价，只使用入场时已经可见的配置和信号止损。
pub fn compute_initial_stop_price(
    position: &TradePosition,
    risk: &BasicRiskStrategyConfig,
) -> Option<f64> {
    if !position.open_price.is_finite() || position.open_price <= 0.0 {
        return None;
    }
    let mut effective_max_loss = risk.max_loss_percent;
    if risk.dynamic_max_loss.unwrap_or(true) {
        if let (Some(entry_amp), Some(entry_close_pos)) = (
            position.entry_kline_amplitude,
            position.entry_kline_close_pos,
        ) {
            let threshold = risk.dynamic_entry_amp_threshold.unwrap_or(0.03);
            let require_mismatch = risk
                .dynamic_entry_require_direction_mismatch
                .unwrap_or(true);
            let direction_mismatch = match position.trade_side {
                TradeSide::Long => entry_close_pos < 0.5,
                TradeSide::Short => entry_close_pos > 0.5,
            };
            if entry_amp > threshold && (!require_mismatch || direction_mismatch) {
                effective_max_loss =
                    effective_max_loss.min(risk.dynamic_entry_loss_percent.unwrap_or(0.03));
            }
        }
    }
    if !effective_max_loss.is_finite() || effective_max_loss <= 0.0 {
        return None;
    }
    let max_loss_stop = match position.trade_side {
        TradeSide::Long => position.open_price * (1.0 - effective_max_loss),
        TradeSide::Short => position.open_price * (1.0 + effective_max_loss),
    };
    let mut candidates = vec![max_loss_stop];
    if let Some(signal_stop) = position.signal_kline_stop_close_price {
        candidates.push(signal_stop);
    }
    select_tightest_stop(position.trade_side, &candidates)
}
/// 选择 交易执行与风控 的最佳候选结果，避免选择规则分散在调用方。
fn select_nearest_tp(side: TradeSide, entry: f64, candidates: &[f64]) -> Option<f64> {
    let values: Vec<f64> = candidates
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .filter(|v| match side {
            TradeSide::Long => *v > entry,
            TradeSide::Short => *v < entry,
        })
        .collect();
    if values.is_empty() {
        return None;
    }
    Some(match side {
        TradeSide::Long => *values
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap(),
        TradeSide::Short => *values
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap(),
    })
}
/// 计算 交易执行与风控 指标，保持公式和边界处理集中可审计。
pub fn compute_current_targets(
    position: &TradePosition,
    candle: &CandleItem,
    risk: &BasicRiskStrategyConfig,
) -> ExitTargets {
    let ctx = ExitContext::new(position, candle);
    let effective_max_loss = compute_effective_max_loss_with_config(
        position,
        &ctx,
        risk.max_loss_percent,
        risk.dynamic_max_loss.unwrap_or(true),
        risk,
    );
    let max_loss_stop = ctx.stop_loss_price(effective_max_loss);
    let mut stop_candidates = vec![max_loss_stop];
    if let Some(px) = position.signal_kline_stop_close_price {
        stop_candidates.push(px);
    }
    if let Some(px) = position.move_stop_open_price {
        stop_candidates.push(px);
    }
    let stop_loss = select_tightest_stop(ctx.side, &stop_candidates);
    let mut tp_candidates = Vec::new();
    if let Some(px) = position.atr_take_profit_level_3 {
        tp_candidates.push(px);
    }
    if let Some(px) = position.atr_take_ratio_profit_price {
        tp_candidates.push(px);
    }
    if let Some(px) = position.fixed_take_profit_price {
        tp_candidates.push(px);
    }
    match ctx.side {
        TradeSide::Long => {
            if let Some(px) = position.long_signal_take_profit_price {
                tp_candidates.push(px);
            }
        }
        TradeSide::Short => {
            if let Some(px) = position.short_signal_take_profit_price {
                tp_candidates.push(px);
            }
        }
    }
    let take_profit = select_nearest_tp(ctx.side, ctx.entry, &tp_candidates);
    ExitTargets {
        stop_loss,
        take_profit,
        stop_reason: None,
        take_reason: None,
    }
}
// ============================================================================
// 止损检查函数
// ============================================================================
/// 在信号止损和最大亏损止损之间选择离入场价更近的保护价，避免同棒穿越时按更差价格成交。
fn check_base_protective_stop(
    ctx: &ExitContext,
    position: &TradePosition,
    risk_config: &BasicRiskStrategyConfig,
) -> ExitResult {
    let effective_max_loss = compute_effective_max_loss_with_config(
        position,
        ctx,
        risk_config.max_loss_percent,
        risk_config.dynamic_max_loss.unwrap_or(true),
        risk_config,
    );
    let max_loss_stop = ctx.stop_loss_price(effective_max_loss);
    let signal_stop = position.signal_kline_stop_close_price;
    let mut candidates = vec![max_loss_stop];
    if let Some(price) = signal_stop {
        candidates.push(price);
    }
    let Some(selected) = select_tightest_stop(ctx.side, &candidates) else {
        return ExitResult::None;
    };
    if signal_stop.is_some_and(|price| price == selected) {
        return check_signal_kline_stop(ctx, Some(selected));
    }
    if ctx.is_stop_loss_hit(selected) {
        ExitResult::Exit {
            price: selected,
            reason: "最大亏损止损",
        }
    } else {
        ExitResult::None
    }
}
/// 检查信号K线止损
fn check_signal_kline_stop(ctx: &ExitContext, stop_price: Option<f64>) -> ExitResult {
    if ctx.candle_ts >= 1766952000000 && ctx.candle_ts <= 1767052800000 {
        println!(
            "RISK_CK_CLOSE: Side={:?}, StopPrice={:?}, CandleClose={:?}, Start={:?}",
            ctx.side, stop_price, ctx.current_price, ctx.entry
        );
    }
    if let Some(price) = stop_price {
        // Debug Log
        if ctx.candle_ts >= 1766952000000 && ctx.candle_ts <= 1767052800000 {
            println!(
                "RISK_CK_CLOSE: Side={:?}, StopPrice={:?}, CandleClose={:?}, Start={:?}",
                ctx.side, price, ctx.current_price, ctx.entry
            );
        }
    }
    match stop_price {
        Some(price) => match ctx.side {
            TradeSide::Long => {
                // Check Low price (Standard)
                if ctx.adverse_price <= price {
                    ExitResult::Exit {
                        price,
                        reason: "Signal_Kline_Stop_Loss",
                    }
                } else {
                    ExitResult::None
                }
            }
            TradeSide::Short => {
                // Check High price (Standard)
                if ctx.adverse_price >= price {
                    ExitResult::Exit {
                        price,
                        reason: "Signal_Kline_Stop_Loss",
                    }
                } else {
                    ExitResult::None
                }
            }
        },
        _ => ExitResult::None,
    }
}
/// 检查三级ATR系统的移动止损
fn check_atr_trailing_stop(ctx: &ExitContext, position: &TradePosition) -> ExitResult {
    match position.move_stop_open_price {
        Some(stop_price) if ctx.is_stop_loss_hit(stop_price) => ExitResult::ExitDynamic {
            price: stop_price,
            reason: if position.profit_protection_armed {
                "空头盈利保护止损".to_string()
            } else {
                format!("移动止损(触发级别:{})", position.reached_take_profit_level)
            },
        },
        _ => ExitResult::None,
    }
}
// ============================================================================
// 三级止盈系统
// ============================================================================
/// 更新三级ATR止盈系统的级别和移动止损线
/// 返回是否触发第三级完全平仓
fn normalized_close_ratio(value: Option<f64>) -> Option<f64> {
    value
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .map(|ratio| ratio.min(1.0))
}

/// 更新三级ATR止盈系统的级别；配置了平仓比例时会返回部分平仓动作。
fn update_atr_tiered_levels(
    ctx: &ExitContext,
    position: &mut TradePosition,
    risk_config: &BasicRiskStrategyConfig,
) -> ExitResult {
    let Some(level_1) = position.atr_take_profit_level_1 else {
        return ExitResult::None;
    };
    let current_level = position.reached_take_profit_level;
    // 第三级：5倍ATR，完全平仓
    if let Some(level_3) = position.atr_take_profit_level_3 {
        if current_level < 3 && ctx.is_take_profit_hit(level_3) {
            return ExitResult::Exit {
                price: level_3,
                reason: "三级止盈(5倍ATR)-完全平仓",
            };
        }
    }
    // 第二级：2倍ATR，移动止损到第一级止盈价
    if let Some(level_2) = position.atr_take_profit_level_2 {
        if current_level < 2 && ctx.is_take_profit_hit(level_2) {
            position.reached_take_profit_level = 2;
            position.move_stop_open_price = Some(level_1);
            if let Some(close_ratio) =
                normalized_close_ratio(risk_config.tiered_take_profit_level_2_close_ratio)
            {
                return ExitResult::PartialExit {
                    price: level_2,
                    reason: "分批止盈(级别2)",
                    close_ratio,
                };
            }
        }
    }
    // 第一级：1.5倍ATR，移动止损到开仓价
    if current_level < 1 && ctx.is_take_profit_hit(level_1) {
        position.reached_take_profit_level = 1;
        position.move_stop_open_price = Some(ctx.entry);
        if let Some(close_ratio) =
            normalized_close_ratio(risk_config.tiered_take_profit_level_1_close_ratio)
        {
            return ExitResult::PartialExit {
                price: level_1,
                reason: "分批止盈(级别1)",
                close_ratio,
            };
        }
    }
    ExitResult::None
}

/// 在当前 K 线完成后武装版本化盈利保护；新止损只会在下一次风险检查中生效。
fn update_profit_protection(ctx: &ExitContext, position: &mut TradePosition) {
    if position.profit_protection_armed {
        return;
    }
    let (Some(trigger), Some(stop)) = (
        position.profit_protection_trigger_price,
        position.profit_protection_stop_price,
    ) else {
        return;
    };
    if trigger.is_finite() && stop.is_finite() && ctx.is_take_profit_hit(trigger) {
        position.move_stop_open_price = Some(stop);
        position.profit_protection_armed = true;
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
    let _ratio = match ratio {
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
// ============================================================================
// 公共检查链（供 check_risk_config 和 check_risk_config_with_r_system 复用）
// ============================================================================
/// 止损检查链（优先级从高到低）
/// 检查顺序：
/// 1. 最大损失止损
/// 2. 单K振幅固定止损(1R)
/// 3. 移动止损（三级ATR系统/保本止损）
/// 4. 信号K线止损
fn run_stop_loss_checks(
    ctx: &ExitContext,
    risk_config: &BasicRiskStrategyConfig,
    position: &TradePosition,
) -> ExitResult {
    // 1. 信号K线止损 (最高优先级：优先遵从各策略的特定止损逻辑)
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_base_protective_stop(ctx, position, risk_config)
    {
        return result;
    }
    // 2. 移动止损（三级ATR系统）
    check_atr_trailing_stop(ctx, position)
}
/// 止盈检查链（优先级从高到低）
/// 检查顺序：
/// 1. 三级ATR止盈
/// 2. ATR比例止盈
/// 3. 固定信号线比例止盈
/// 4. 动态止盈
fn run_take_profit_checks(
    ctx: &ExitContext,
    risk_config: &BasicRiskStrategyConfig,
    position: &mut TradePosition,
) -> ExitResult {
    // 首次回测的 fixed 目标是已有目标与 R 上限的最近价，必须先检查；
    // 否则同根 K 线同时穿越多个目标时，检查顺序会虚增成交价。
    let result = check_fixed_take_profit(ctx, position.fixed_take_profit_price);
    if matches!(result, ExitResult::Exit { .. }) {
        return result;
    }
    if position.fixed_take_profit_only {
        return ExitResult::None;
    }
    // 1. 三级ATR止盈（同时更新级别）
    let result = update_atr_tiered_levels(ctx, position, risk_config);
    if matches!(
        result,
        ExitResult::Exit { .. } | ExitResult::PartialExit { .. }
    ) {
        return result;
    }
    // 2. ATR比例止盈
    let result = check_atr_ratio_take_profit(
        ctx,
        risk_config.atr_take_profit_ratio,
        position.atr_take_ratio_profit_price,
    );
    if matches!(result, ExitResult::Exit { .. }) {
        return result;
    }
    // 3. 动态止盈（做多/做空）
    let result = check_dynamic_take_profit(
        ctx,
        position.long_signal_take_profit_price,
        position.short_signal_take_profit_price,
    );
    if matches!(result, ExitResult::Exit { .. }) {
        return result;
    }
    // 真实止盈都未触发时，才在本根完成后武装下一根 K 线使用的盈利保护。
    update_profit_protection(ctx, position);
    ExitResult::None
}
// ============================================================================
// 主函数
// ============================================================================
/// 风险管理检查入口
/// # 优先级原则
/// **同一K线内，止损永远优先于止盈**
/// ## 检查顺序
/// ### 止损（优先级高）
/// 1. 最大损失止损 - 资金保护
/// 2. 移动止损 - 三级ATR系统
/// 3. 信号K线止损 - 技术止损
/// ### 止盈
/// 4. 三级ATR止盈 - 5倍ATR完全平仓
/// 5. ATR比例止盈
/// 6. 固定信号线比例止盈
/// 7. 动态止盈 - 指标动态止盈
/// 8. 逆势回调止盈
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
    // 止损检查（优先级最高）
    let stop_result = run_stop_loss_checks(&ctx, risk_config, &trade_position);
    if matches!(
        stop_result,
        ExitResult::Exit { .. } | ExitResult::ExitDynamic { .. }
    ) {
        return finalize_exit(
            trading_state,
            trade_position,
            candle,
            signal,
            &ctx,
            stop_result,
        );
    }
    // 止盈检查
    let tp_result = run_take_profit_checks(&ctx, risk_config, &mut trade_position);
    if matches!(tp_result, ExitResult::Exit { .. }) {
        return finalize_exit(
            trading_state,
            trade_position,
            candle,
            signal,
            &ctx,
            tp_result,
        );
    }
    if let ExitResult::PartialExit { .. } = tp_result {
        return finalize_partial_exit(
            trading_state,
            trade_position,
            candle,
            signal,
            &ctx,
            tp_result,
        );
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
    /// 提供默认参数，保证 交易执行与风控 在未显式配置时仍有稳定初始值。
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
/// 在标准风控的基础上，增加：
/// - R系统移动止损：根据盈利R倍数动态调整止损
/// - 时间止损：根据持仓时间和盈亏状态决定是否平仓
/// # 参数
/// - `risk_config`: 基础风控配置
/// - `r_risk_config`: R系统风控配置
/// - `r_runtime`: R系统运行时状态（可变）
/// - `trading_state`: 交易状态
/// - `signal`: 信号结果
/// - `candle`: 当前K线
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
    // 1. 入场基础保护止损（信号止损与最大亏损中取更紧者）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_base_protective_stop(&ctx, &trade_position, risk_config)
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
    // 4. 移动止损（三级ATR系统）
    if let result @ ExitResult::Exit { .. } | result @ ExitResult::ExitDynamic { .. } =
        check_atr_trailing_stop(&ctx, &trade_position)
    {
        r_runtime.r_state = None;
        return finalize_exit(trading_state, trade_position, candle, signal, &ctx, result);
    }
    // ========================================================================
    // 止盈检查（复用公共检查链）
    // ========================================================================
    let tp_result = run_take_profit_checks(&ctx, risk_config, &mut trade_position);
    if matches!(tp_result, ExitResult::Exit { .. }) {
        r_runtime.r_state = None;
        return finalize_exit(
            trading_state,
            trade_position,
            candle,
            signal,
            &ctx,
            tp_result,
        );
    }
    if let ExitResult::PartialExit { .. } = tp_result {
        return finalize_partial_exit(
            trading_state,
            trade_position,
            candle,
            signal,
            &ctx,
            tp_result,
        );
    }
    // 更新仓位状态
    trading_state.trade_position = Some(trade_position);
    trading_state
}
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
        ExitResult::PartialExit { .. } => return trading_state,
        ExitResult::None => return trading_state,
    };
    trade_position.close_price = Some(price);
    trading_state.trade_position = Some(trade_position);
    let profit = ctx.profit(price);
    close_position(&mut trading_state, candle, signal, &reason, profit);
    trading_state
}

/// 执行部分平仓并保留更新后的移动止损状态。
fn finalize_partial_exit(
    mut trading_state: TradingState,
    trade_position: TradePosition,
    candle: &CandleItem,
    signal: &SignalResult,
    ctx: &ExitContext,
    result: ExitResult,
) -> TradingState {
    let ExitResult::PartialExit {
        price,
        reason,
        close_ratio,
    } = result
    else {
        return trading_state;
    };
    let closing_quantity = (ctx.qty * close_ratio).min(ctx.qty).max(0.0);
    trading_state.trade_position = Some(trade_position);
    partial_close_position(
        &mut trading_state,
        candle,
        signal,
        reason,
        price,
        closing_quantity,
    );
    trading_state
}
#[cfg(test)]
mod tests;
