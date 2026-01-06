//! R系统移动止损与分批止盈模块
//!
//! 基于第一性原理文档的量化定义：
//!
//! ## 移动止损规则
//! | 盈利阶段 | 止损调整 |
//! |----------|----------|
//! | 盈利 ≥ 1R | 止损移至保本位（入场价 ± 手续费） |
//! | 盈利 ≥ 1.5R | 止损移至 +0.5R 位置 |
//! | 盈利 ≥ 2R | 使用 ATR(14) × 1.0 跟踪止损 |
//! | 盈利 ≥ 3R | 使用 ATR(14) × 0.8 跟踪止损（收紧） |
//!
//! > R = 初始止损距离（风险单位）
//!
//! ## 分批止盈规则
//! | 到达目标 | 平仓比例 | 止损调整 |
//! |----------|----------|----------|
//! | 目标1 | 40% | 移至保本 |
//! | 目标2 | 30% | 移至目标1位置 |
//! | 目标3 | 剩余全部 | ATR跟踪 |
//!
//! ## 时间止损规则
//! | 持仓时间 | 盈亏状态 | 处理 |
//! |----------|----------|------|
//! | 12根K线 | 浮亏 | 减仓50% |
//! | 24根K线 | 盈亏平衡附近 | 平仓离场 |
//! | 48根K线 | 未达目标1 | 平仓，视为信号失效 |

use super::types::TradePosition;
use crate::framework::types::TradeSide;
use serde::{Deserialize, Serialize};

// ============================================================================
// R系统配置
// ============================================================================

/// R系统配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RSystemConfig {
    /// 第一级触发阈值（默认1R）
    pub level_1_trigger: f64,
    /// 第二级触发阈值（默认1.5R）
    pub level_2_trigger: f64,
    /// 第三级触发阈值（默认2R，开始ATR跟踪）
    pub level_3_trigger: f64,
    /// 第四级触发阈值（默认3R，收紧ATR）
    pub level_4_trigger: f64,
    /// ATR跟踪乘数（第三级）
    pub atr_multiplier_level_3: f64,
    /// ATR跟踪乘数（第四级，更紧）
    pub atr_multiplier_level_4: f64,
    /// 手续费率（用于计算保本位）
    pub fee_rate: f64,
}

impl Default for RSystemConfig {
    fn default() -> Self {
        Self {
            level_1_trigger: 1.0,
            level_2_trigger: 1.5,
            level_3_trigger: 2.0,
            level_4_trigger: 3.0,
            atr_multiplier_level_3: 1.0,
            atr_multiplier_level_4: 0.8,
            fee_rate: 0.0004, // 0.04% 双边
        }
    }
}

// ============================================================================
// 止损级别
// ============================================================================

/// 止损级别枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StopLossLevel {
    /// 初始止损
    #[default]
    Initial,
    /// 保本位（≥1R）
    BreakEven,
    /// +0.5R位置（≥1.5R）
    HalfR,
    /// ATR×1.0跟踪（≥2R）
    AtrTrailing1x,
    /// ATR×0.8收紧跟踪（≥3R）
    AtrTrailing08x,
}

impl StopLossLevel {
    /// 获取级别数值（用于比较）
    pub fn as_level(&self) -> u8 {
        match self {
            StopLossLevel::Initial => 0,
            StopLossLevel::BreakEven => 1,
            StopLossLevel::HalfR => 2,
            StopLossLevel::AtrTrailing1x => 3,
            StopLossLevel::AtrTrailing08x => 4,
        }
    }

    /// 从盈利R倍数计算应有的级别
    pub fn from_profit_r(profit_r: f64) -> Self {
        if profit_r >= 3.0 {
            StopLossLevel::AtrTrailing08x
        } else if profit_r >= 2.0 {
            StopLossLevel::AtrTrailing1x
        } else if profit_r >= 1.5 {
            StopLossLevel::HalfR
        } else if profit_r >= 1.0 {
            StopLossLevel::BreakEven
        } else {
            StopLossLevel::Initial
        }
    }
}

// ============================================================================
// R系统状态
// ============================================================================

/// R系统持仓状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RSystemState {
    /// 入场价格
    pub entry_price: f64,
    /// 初始止损价格
    pub initial_stop_price: f64,
    /// 当前止损价格
    pub current_stop_price: f64,
    /// 1R的价格距离
    pub one_r_distance: f64,
    /// 当前止损级别
    pub stop_level: StopLossLevel,
    /// 最高盈利R倍数（用于判断是否需要更新）
    pub max_profit_r: f64,
    /// 持仓方向
    pub side: TradeSide,
    /// 入场时的K线索引
    pub entry_bar_index: usize,
}

impl Default for RSystemState {
    fn default() -> Self {
        Self {
            entry_price: 0.0,
            initial_stop_price: 0.0,
            current_stop_price: 0.0,
            one_r_distance: 0.0,
            stop_level: StopLossLevel::Initial,
            max_profit_r: 0.0,
            side: TradeSide::Long,
            entry_bar_index: 0,
        }
    }
}

impl RSystemState {
    /// 创建新的R系统状态
    ///
    /// # 参数
    /// - `entry_price`: 入场价格
    /// - `initial_stop_price`: 初始止损价格
    /// - `side`: 持仓方向
    /// - `entry_bar_index`: 入场K线索引
    pub fn new(
        entry_price: f64,
        initial_stop_price: f64,
        side: TradeSide,
        entry_bar_index: usize,
    ) -> Self {
        let one_r_distance = (entry_price - initial_stop_price).abs();
        Self {
            entry_price,
            initial_stop_price,
            current_stop_price: initial_stop_price,
            one_r_distance,
            stop_level: StopLossLevel::Initial,
            max_profit_r: 0.0,
            side,
            entry_bar_index,
        }
    }

    /// 计算当前盈利R倍数
    ///
    /// # 参数
    /// - `current_price`: 当前价格
    pub fn calculate_profit_r(&self, current_price: f64) -> f64 {
        if self.one_r_distance <= 0.0 {
            return 0.0;
        }

        let profit_distance = match self.side {
            TradeSide::Long => current_price - self.entry_price,
            TradeSide::Short => self.entry_price - current_price,
        };

        profit_distance / self.one_r_distance
    }

    /// 计算指定R倍数对应的价格
    pub fn calculate_price_at_r(&self, r_multiple: f64) -> f64 {
        match self.side {
            TradeSide::Long => self.entry_price + self.one_r_distance * r_multiple,
            TradeSide::Short => self.entry_price - self.one_r_distance * r_multiple,
        }
    }

    /// 检查止损是否被触发
    ///
    /// # 参数
    /// - `low_price`: K线最低价（用于多头）
    /// - `high_price`: K线最高价（用于空头）
    pub fn is_stop_triggered(&self, low_price: f64, high_price: f64) -> bool {
        match self.side {
            TradeSide::Long => low_price <= self.current_stop_price,
            TradeSide::Short => high_price >= self.current_stop_price,
        }
    }
}

// ============================================================================
// R系统核心逻辑
// ============================================================================

/// 更新R系统移动止损
///
/// # 参数
/// - `state`: R系统状态（可变）
/// - `current_high`: 当前K线最高价
/// - `current_low`: 当前K线最低价
/// - `atr_value`: ATR值（用于ATR跟踪止损）
/// - `config`: R系统配置
///
/// # 返回
/// - `Option<f64>`: 新的止损价格（如果有更新）
pub fn update_r_system_trailing_stop(
    state: &mut RSystemState,
    current_high: f64,
    current_low: f64,
    atr_value: f64,
    config: &RSystemConfig,
) -> Option<f64> {
    // 计算当前盈利R倍数（使用有利价格）
    let favorable_price = match state.side {
        TradeSide::Long => current_high,
        TradeSide::Short => current_low,
    };
    let current_profit_r = state.calculate_profit_r(favorable_price);

    // 更新最高盈利记录
    if current_profit_r > state.max_profit_r {
        state.max_profit_r = current_profit_r;
    }

    // 根据盈利R倍数计算新止损
    let new_stop = calculate_new_stop_price(state, current_profit_r, atr_value, favorable_price, config);

    // 只能上调止损（多头）或下调止损（空头）
    let should_update = match state.side {
        TradeSide::Long => new_stop > state.current_stop_price,
        TradeSide::Short => new_stop < state.current_stop_price,
    };

    if should_update {
        state.current_stop_price = new_stop;
        state.stop_level = StopLossLevel::from_profit_r(current_profit_r);
        Some(new_stop)
    } else {
        None
    }
}

/// 计算新止损价格
fn calculate_new_stop_price(
    state: &RSystemState,
    profit_r: f64,
    atr_value: f64,
    favorable_price: f64,
    config: &RSystemConfig,
) -> f64 {
    if profit_r >= config.level_4_trigger {
        // ≥3R: ATR×0.8收紧跟踪
        calculate_atr_trailing_stop(state.side, favorable_price, atr_value, config.atr_multiplier_level_4)
    } else if profit_r >= config.level_3_trigger {
        // ≥2R: ATR×1.0跟踪
        calculate_atr_trailing_stop(state.side, favorable_price, atr_value, config.atr_multiplier_level_3)
    } else if profit_r >= config.level_2_trigger {
        // ≥1.5R: 止损移至+0.5R
        state.calculate_price_at_r(0.5)
    } else if profit_r >= config.level_1_trigger {
        // ≥1R: 止损移至保本位
        calculate_break_even_stop(state.entry_price, state.side, config.fee_rate)
    } else {
        // <1R: 保持初始止损
        state.initial_stop_price
    }
}

/// 计算ATR跟踪止损
fn calculate_atr_trailing_stop(
    side: TradeSide,
    favorable_price: f64,
    atr_value: f64,
    multiplier: f64,
) -> f64 {
    let atr_distance = atr_value * multiplier;
    match side {
        TradeSide::Long => favorable_price - atr_distance,
        TradeSide::Short => favorable_price + atr_distance,
    }
}

/// 计算保本位止损
fn calculate_break_even_stop(entry_price: f64, side: TradeSide, fee_rate: f64) -> f64 {
    // 保本位 = 入场价 ± 手续费
    let fee_offset = entry_price * fee_rate * 2.0; // 双边手续费
    match side {
        TradeSide::Long => entry_price + fee_offset,
        TradeSide::Short => entry_price - fee_offset,
    }
}

// ============================================================================
// 分批止盈系统
// ============================================================================

/// 分批止盈配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TieredTakeProfitConfig {
    /// 目标1 R倍数
    pub target_1_r: f64,
    /// 目标1 平仓比例
    pub target_1_close_ratio: f64,
    /// 目标2 R倍数
    pub target_2_r: f64,
    /// 目标2 平仓比例
    pub target_2_close_ratio: f64,
    /// 目标3 R倍数（完全平仓）
    pub target_3_r: f64,
}

impl Default for TieredTakeProfitConfig {
    fn default() -> Self {
        Self {
            target_1_r: 1.5,
            target_1_close_ratio: 0.4, // 40%
            target_2_r: 2.0,
            target_2_close_ratio: 0.3, // 30%
            target_3_r: 4.0,           // 剩余30%
        }
    }
}

/// 分批止盈状态
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TieredTakeProfitState {
    /// 目标1价格
    pub target_1_price: f64,
    /// 目标2价格
    pub target_2_price: f64,
    /// 目标3价格
    pub target_3_price: f64,
    /// 目标1是否已触达
    pub target_1_reached: bool,
    /// 目标2是否已触达
    pub target_2_reached: bool,
    /// 目标3是否已触达
    pub target_3_reached: bool,
    /// 已平仓比例
    pub closed_ratio: f64,
}

impl TieredTakeProfitState {
    /// 创建新的分批止盈状态
    pub fn new(r_state: &RSystemState, config: &TieredTakeProfitConfig) -> Self {
        Self {
            target_1_price: r_state.calculate_price_at_r(config.target_1_r),
            target_2_price: r_state.calculate_price_at_r(config.target_2_r),
            target_3_price: r_state.calculate_price_at_r(config.target_3_r),
            target_1_reached: false,
            target_2_reached: false,
            target_3_reached: false,
            closed_ratio: 0.0,
        }
    }
}

/// 分批止盈动作
#[derive(Debug, Clone, Copy)]
pub enum TakeProfitAction {
    /// 无动作
    None,
    /// 部分平仓
    PartialClose {
        /// 平仓比例（相对于当前持仓）
        ratio: f64,
        /// 平仓价格
        price: f64,
        /// 触发的目标级别
        level: u8,
    },
    /// 完全平仓
    FullClose {
        /// 平仓价格
        price: f64,
    },
}

/// 检查分批止盈
///
/// # 参数
/// - `tp_state`: 分批止盈状态（可变）
/// - `r_state`: R系统状态（可变，用于更新止损）
/// - `current_high`: 当前K线最高价
/// - `current_low`: 当前K线最低价
/// - `config`: 分批止盈配置
///
/// # 返回
/// - `TakeProfitAction`: 止盈动作
pub fn check_tiered_take_profit(
    tp_state: &mut TieredTakeProfitState,
    r_state: &mut RSystemState,
    current_high: f64,
    current_low: f64,
    config: &TieredTakeProfitConfig,
) -> TakeProfitAction {
    let favorable_price = match r_state.side {
        TradeSide::Long => current_high,
        TradeSide::Short => current_low,
    };

    // 检查目标3（完全平仓）
    let target_3_hit = match r_state.side {
        TradeSide::Long => favorable_price >= tp_state.target_3_price,
        TradeSide::Short => favorable_price <= tp_state.target_3_price,
    };

    if !tp_state.target_3_reached && target_3_hit {
        tp_state.target_3_reached = true;
        return TakeProfitAction::FullClose {
            price: tp_state.target_3_price,
        };
    }

    // 检查目标2
    let target_2_hit = match r_state.side {
        TradeSide::Long => favorable_price >= tp_state.target_2_price,
        TradeSide::Short => favorable_price <= tp_state.target_2_price,
    };

    if !tp_state.target_2_reached && target_2_hit {
        tp_state.target_2_reached = true;
        tp_state.closed_ratio += config.target_2_close_ratio;

        // 止损移至目标1价格
        r_state.current_stop_price = tp_state.target_1_price;
        r_state.stop_level = StopLossLevel::HalfR;

        return TakeProfitAction::PartialClose {
            ratio: config.target_2_close_ratio,
            price: tp_state.target_2_price,
            level: 2,
        };
    }

    // 检查目标1
    let target_1_hit = match r_state.side {
        TradeSide::Long => favorable_price >= tp_state.target_1_price,
        TradeSide::Short => favorable_price <= tp_state.target_1_price,
    };

    if !tp_state.target_1_reached && target_1_hit {
        tp_state.target_1_reached = true;
        tp_state.closed_ratio += config.target_1_close_ratio;

        // 止损移至保本位
        r_state.current_stop_price = r_state.entry_price;
        r_state.stop_level = StopLossLevel::BreakEven;

        return TakeProfitAction::PartialClose {
            ratio: config.target_1_close_ratio,
            price: tp_state.target_1_price,
            level: 1,
        };
    }

    TakeProfitAction::None
}

// ============================================================================
// 时间止损系统
// ============================================================================

/// 时间止损配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TimeStopConfig {
    /// 浮亏减仓阈值（K线数）
    pub loss_reduce_bars: usize,
    /// 盈亏平衡平仓阈值
    pub break_even_bars: usize,
    /// 信号失效阈值
    pub signal_invalid_bars: usize,
    /// 盈亏平衡判定范围（百分比）
    pub break_even_tolerance: f64,
}

impl Default for TimeStopConfig {
    fn default() -> Self {
        Self {
            loss_reduce_bars: 12,
            break_even_bars: 24,
            signal_invalid_bars: 48,
            break_even_tolerance: 0.002, // 0.2%
        }
    }
}

/// 时间止损动作
#[derive(Debug, Clone, Copy)]
pub enum TimeStopAction {
    /// 继续持有
    Hold,
    /// 减仓50%
    Reduce50 { reason: &'static str },
    /// 全部平仓
    CloseAll { reason: &'static str },
}

/// 检查时间止损
///
/// # 参数
/// - `r_state`: R系统状态
/// - `tp_state`: 分批止盈状态
/// - `current_bar_index`: 当前K线索引
/// - `current_price`: 当前价格
/// - `config`: 时间止损配置
///
/// # 返回
/// - `TimeStopAction`: 时间止损动作
pub fn check_time_stop(
    r_state: &RSystemState,
    tp_state: &TieredTakeProfitState,
    current_bar_index: usize,
    current_price: f64,
    config: &TimeStopConfig,
) -> TimeStopAction {
    let bars_held = current_bar_index.saturating_sub(r_state.entry_bar_index);
    let profit_ratio = match r_state.side {
        TradeSide::Long => (current_price - r_state.entry_price) / r_state.entry_price,
        TradeSide::Short => (r_state.entry_price - current_price) / r_state.entry_price,
    };

    // 规则1: 48根K线未达目标1 → 信号失效，平仓
    if bars_held >= config.signal_invalid_bars && !tp_state.target_1_reached {
        return TimeStopAction::CloseAll {
            reason: "信号失效(48K未达目标1)",
        };
    }

    // 规则2: 24根K线盈亏平衡附近 → 平仓
    if bars_held >= config.break_even_bars && profit_ratio.abs() < config.break_even_tolerance {
        return TimeStopAction::CloseAll {
            reason: "盈亏平衡超时(24K)",
        };
    }

    // 规则3: 12根K线浮亏 → 减仓50%
    if bars_held >= config.loss_reduce_bars && profit_ratio < 0.0 {
        return TimeStopAction::Reduce50 {
            reason: "浮亏减仓(12K)",
        };
    }

    TimeStopAction::Hold
}

// ============================================================================
// 与现有TradePosition集成
// ============================================================================

/// 从TradePosition创建R系统状态
pub fn create_r_state_from_position(
    position: &TradePosition,
    entry_bar_index: usize,
) -> Option<RSystemState> {
    let stop_price = position
        .signal_kline_stop_close_price
        .or(position.atr_stop_loss_price)?;

    Some(RSystemState::new(
        position.open_price,
        stop_price,
        position.trade_side.clone(),
        entry_bar_index,
    ))
}

/// 更新TradePosition的止损价格
pub fn update_position_stop_from_r_state(
    position: &mut TradePosition,
    r_state: &RSystemState,
) {
    // 更新移动止损价格
    position.move_stop_open_price = Some(r_state.current_stop_price);

    // 更新已触达的止盈级别
    position.reached_take_profit_level = r_state.stop_level.as_level();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_r_system_state_creation() {
        let state = RSystemState::new(100.0, 98.0, TradeSide::Long, 0);
        assert_eq!(state.one_r_distance, 2.0);
        assert_eq!(state.calculate_profit_r(102.0), 1.0);
        assert_eq!(state.calculate_profit_r(104.0), 2.0);
    }

    #[test]
    fn test_stop_level_from_profit_r() {
        assert_eq!(StopLossLevel::from_profit_r(0.5), StopLossLevel::Initial);
        assert_eq!(StopLossLevel::from_profit_r(1.0), StopLossLevel::BreakEven);
        assert_eq!(StopLossLevel::from_profit_r(1.5), StopLossLevel::HalfR);
        assert_eq!(StopLossLevel::from_profit_r(2.5), StopLossLevel::AtrTrailing1x);
        assert_eq!(StopLossLevel::from_profit_r(3.5), StopLossLevel::AtrTrailing08x);
    }

    #[test]
    fn test_trailing_stop_update() {
        let mut state = RSystemState::new(100.0, 98.0, TradeSide::Long, 0);
        let config = RSystemConfig::default();

        // 价格上涨到102（1R盈利），止损应移至保本
        let new_stop = update_r_system_trailing_stop(&mut state, 102.0, 101.0, 1.0, &config);
        assert!(new_stop.is_some());
        assert!(state.current_stop_price > 98.0); // 止损已上移

        // 价格继续上涨到104（2R盈利）
        let new_stop = update_r_system_trailing_stop(&mut state, 104.0, 103.0, 1.0, &config);
        assert!(new_stop.is_some());
        assert_eq!(state.stop_level, StopLossLevel::AtrTrailing1x);
    }

    #[test]
    fn test_tiered_take_profit() {
        let r_state = RSystemState::new(100.0, 98.0, TradeSide::Long, 0);
        let config = TieredTakeProfitConfig::default();
        let mut tp_state = TieredTakeProfitState::new(&r_state, &config);

        // 验证目标价格计算
        assert_eq!(tp_state.target_1_price, 103.0); // 1.5R
        assert_eq!(tp_state.target_2_price, 104.0); // 2R
        assert_eq!(tp_state.target_3_price, 108.0); // 4R
    }

    #[test]
    fn test_time_stop() {
        let r_state = RSystemState::new(100.0, 98.0, TradeSide::Long, 0);
        let tp_state = TieredTakeProfitState::default();
        let config = TimeStopConfig::default();

        // 12根K线后浮亏
        let action = check_time_stop(&r_state, &tp_state, 12, 99.0, &config);
        assert!(matches!(action, TimeStopAction::Reduce50 { .. }));

        // 48根K线未达目标1
        let action = check_time_stop(&r_state, &tp_state, 48, 101.0, &config);
        assert!(matches!(action, TimeStopAction::CloseAll { .. }));
    }

    #[test]
    fn test_short_position_r_system() {
        let mut state = RSystemState::new(100.0, 102.0, TradeSide::Short, 0);
        let config = RSystemConfig::default();

        assert_eq!(state.one_r_distance, 2.0);
        assert_eq!(state.calculate_profit_r(98.0), 1.0); // 空头盈利

        // 价格下跌到98（1R盈利），止损应下移
        let new_stop = update_r_system_trailing_stop(&mut state, 99.0, 98.0, 1.0, &config);
        assert!(new_stop.is_some());
        assert!(state.current_stop_price < 102.0); // 空头止损下移
    }
}

