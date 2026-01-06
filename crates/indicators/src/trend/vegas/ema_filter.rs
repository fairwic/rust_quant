//! EMA距离过滤模块
//!
//! 基于第一性原理文档的量化定义：
//!
//! ## EMA距离量化
//! - 过远: abs(ema2 - ema4) / ema4 > 5% → 过滤特定信号
//! - 适中: 2% < abs(ema2 - ema4) / ema4 ≤ 5% → 正常交易
//! - 过近: abs(ema2 - ema4) / ema4 ≤ 2% → 震荡市
//!
//! ## 过滤规则
//! - 空头排列 + 距离过远 + 收盘价 > ema3 → **不做多**（假信号）
//! - 多头排列 + 距离过远 + 收盘价 < ema3 → **不做空**（假信号）

use super::signal::EmaSignalValue;
use serde::{Deserialize, Serialize};

/// EMA距离过滤配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmaDistanceConfig {
    /// 距离过远阈值（默认5%）
    pub too_far_threshold: f64,
    /// 震荡市阈值（默认2%）
    pub ranging_threshold: f64,
    /// 三线缠绕阈值（默认1%，用于判断震荡市）
    pub tangled_threshold: f64,
}

impl Default for EmaDistanceConfig {
    fn default() -> Self {
        Self {
            too_far_threshold: 0.05,   // 5%
            ranging_threshold: 0.02,   // 2%
            tangled_threshold: 0.01,   // 1%
        }
    }
}

/// EMA距离状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmaDistanceState {
    /// 距离过远（>5%）
    TooFar,
    /// 距离适中（2%-5%）
    Normal,
    /// 距离过近/震荡（<2%）
    Ranging,
    /// 三线缠绕（震荡市确认）
    Tangled,
}

/// EMA距离过滤结果
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EmaDistanceFilter {
    /// EMA2与EMA4的距离比例
    pub distance_ratio: f64,
    /// EMA2与EMA3的距离比例
    pub ema2_ema3_distance: f64,
    /// EMA3与EMA4的距离比例
    pub ema3_ema4_distance: f64,
    /// 距离状态
    pub state: EmaDistanceState,
    /// 是否应该过滤做多信号
    pub should_filter_long: bool,
    /// 是否应该过滤做空信号
    pub should_filter_short: bool,
    /// 是否处于震荡市
    pub is_ranging_market: bool,
    /// 是否三线缠绕
    pub is_tangled: bool,
}

impl Default for EmaDistanceState {
    fn default() -> Self {
        EmaDistanceState::Normal
    }
}

/// 计算EMA距离过滤
///
/// # 参数
/// - `close_price`: 当前收盘价
/// - `ema_values`: EMA指标值
/// - `config`: 过滤配置
///
/// # 返回
/// - `EmaDistanceFilter`: 过滤结果
///
/// # 示例
/// ```ignore
/// let filter = apply_ema_distance_filter(current_close, &ema_values, &EmaDistanceConfig::default());
/// if filter.should_filter_long {
///     // 不执行做多
/// }
/// ```
pub fn apply_ema_distance_filter(
    close_price: f64,
    ema_values: &EmaSignalValue,
    config: &EmaDistanceConfig,
) -> EmaDistanceFilter {
    let mut result = EmaDistanceFilter::default();

    // 避免除以0
    if ema_values.ema4_value <= 0.0 {
        return result;
    }

    // 计算EMA2与EMA4的距离比例
    result.distance_ratio =
        ((ema_values.ema2_value - ema_values.ema4_value) / ema_values.ema4_value).abs();

    // 计算EMA2与EMA3的距离
    if ema_values.ema3_value > 0.0 {
        result.ema2_ema3_distance =
            ((ema_values.ema2_value - ema_values.ema3_value) / ema_values.ema3_value).abs();
    }

    // 计算EMA3与EMA4的距离
    result.ema3_ema4_distance =
        ((ema_values.ema3_value - ema_values.ema4_value) / ema_values.ema4_value).abs();

    // 判断距离状态
    result.state = if result.distance_ratio > config.too_far_threshold {
        EmaDistanceState::TooFar
    } else if result.distance_ratio <= config.ranging_threshold {
        EmaDistanceState::Ranging
    } else {
        EmaDistanceState::Normal
    };

    // 判断三线缠绕（震荡市确认）
    // 条件：EMA2、EMA3、EMA4 两两距离都 < 1%
    result.is_tangled = result.ema2_ema3_distance < config.tangled_threshold
        && result.ema3_ema4_distance < config.tangled_threshold;

    if result.is_tangled {
        result.state = EmaDistanceState::Tangled;
    }

    // 判断是否震荡市
    result.is_ranging_market =
        result.state == EmaDistanceState::Ranging || result.state == EmaDistanceState::Tangled;

    // 应用过滤规则
    apply_filter_rules(close_price, ema_values, &mut result, config);

    result
}

/// 应用过滤规则
///
/// ## 规则
/// 1. 空头排列 + 距离过远 + 收盘价 > ema3 → 不做多
/// 2. 多头排列 + 距离过远 + 收盘价 < ema3 → 不做空
fn apply_filter_rules(
    close_price: f64,
    ema_values: &EmaSignalValue,
    result: &mut EmaDistanceFilter,
    _config: &EmaDistanceConfig,
) {
    let is_too_far = result.state == EmaDistanceState::TooFar;

    // 规则1: 空头排列 + 距离过远 + 收盘价 > ema3 → 不做多
    // 空头排列: ema2 < ema3 < ema4
    let is_bearish_trend = ema_values.ema2_value < ema_values.ema3_value
        && ema_values.ema3_value < ema_values.ema4_value;

    if is_bearish_trend && is_too_far && close_price > ema_values.ema3_value {
        result.should_filter_long = true;
    }

    // 规则2: 多头排列 + 距离过远 + 收盘价 < ema3 → 不做空
    // 多头排列: ema2 > ema3 > ema4
    let is_bullish_trend = ema_values.ema2_value > ema_values.ema3_value
        && ema_values.ema3_value > ema_values.ema4_value;

    if is_bullish_trend && is_too_far && close_price < ema_values.ema3_value {
        result.should_filter_short = true;
    }
}

/// EMA排列状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmaTrendAlignment {
    /// 多头排列: ema2 > ema3 > ema4
    Bullish,
    /// 空头排列: ema2 < ema3 < ema4
    Bearish,
    /// 混乱/过渡期
    Mixed,
}

/// 判断EMA排列状态
///
/// # 参数
/// - `ema_values`: EMA指标值
/// - `tolerance`: 允许的误差范围（用于判断"基本相等"的情况）
///
/// # 返回
/// - `EmaTrendAlignment`: 排列状态
pub fn get_ema_alignment(ema_values: &EmaSignalValue, tolerance: f64) -> EmaTrendAlignment {
    let e2 = ema_values.ema2_value;
    let e3 = ema_values.ema3_value;
    let e4 = ema_values.ema4_value;

    // 多头排列: e2 > e3 > e4
    let is_bullish = e2 > e3 * (1.0 + tolerance) && e3 > e4 * (1.0 + tolerance);

    // 空头排列: e2 < e3 < e4
    let is_bearish = e2 < e3 * (1.0 - tolerance) && e3 < e4 * (1.0 - tolerance);

    if is_bullish {
        EmaTrendAlignment::Bullish
    } else if is_bearish {
        EmaTrendAlignment::Bearish
    } else {
        EmaTrendAlignment::Mixed
    }
}

/// 计算挂单建议价格
///
/// 根据第一性原理文档：
/// - 空头排列 + 距离过远 → 在 ema4 下方 2% 位置挂空单
/// - 多头排列 + 距离过远 → 在 ema4 上方 2% 位置挂多单
///
/// # 参数
/// - `ema_values`: EMA指标值
/// - `filter_result`: EMA距离过滤结果
/// - `offset_ratio`: 偏移比例（默认2%）
///
/// # 返回
/// - `Option<(f64, bool)>`: (建议价格, 是否做多)，None表示无建议
pub fn calculate_pending_order_price(
    ema_values: &EmaSignalValue,
    filter_result: &EmaDistanceFilter,
    offset_ratio: f64,
) -> Option<(f64, bool)> {
    if filter_result.state != EmaDistanceState::TooFar {
        return None;
    }

    let alignment = get_ema_alignment(ema_values, 0.001);

    match alignment {
        EmaTrendAlignment::Bearish if filter_result.should_filter_long => {
            // 空头排列，距离过远，建议在ema4下方挂空单
            Some((ema_values.ema4_value * (1.0 - offset_ratio), false))
        }
        EmaTrendAlignment::Bullish if filter_result.should_filter_short => {
            // 多头排列，距离过远，建议在ema4上方挂多单
            Some((ema_values.ema4_value * (1.0 + offset_ratio), true))
        }
        _ => None,
    }
}

/// 成交量递减过滤
///
/// 根据第一性原理文档：
/// - 条件: 出现交易信号，但近3根K线成交量递减 Vol(n-2) > Vol(n-1) > Vol(n)
/// - 规则: 忽略做多/做空信号
///
/// # 参数
/// - `volumes`: 近N根K线的成交量（从旧到新）
///
/// # 返回
/// - `bool`: true表示应该过滤信号
pub fn check_volume_decreasing_filter(volumes: &[f64]) -> bool {
    if volumes.len() < 3 {
        return false;
    }

    let n = volumes.len();
    let v_n = volumes[n - 1]; // 当前
    let v_n1 = volumes[n - 2]; // 前一根
    let v_n2 = volumes[n - 3]; // 前两根

    // Vol(n-2) > Vol(n-1) > Vol(n) → 连续递减
    v_n2 > v_n1 && v_n1 > v_n
}

/// 批量计算近N根K线的成交量
///
/// # 参数
/// - `candles`: K线数据
/// - `count`: 需要的K线数量
///
/// # 返回
/// - `Vec<f64>`: 成交量列表
pub fn extract_recent_volumes(candles: &[rust_quant_common::CandleItem], count: usize) -> Vec<f64> {
    let start = candles.len().saturating_sub(count);
    candles[start..].iter().map(|c| c.v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_ema_values(e1: f64, e2: f64, e3: f64, e4: f64, e5: f64) -> EmaSignalValue {
        EmaSignalValue {
            ema1_value: e1,
            ema2_value: e2,
            ema3_value: e3,
            ema4_value: e4,
            ema5_value: e5,
            ..Default::default()
        }
    }

    #[test]
    fn test_distance_too_far() {
        // EMA2=100, EMA4=90 → 距离 11.1% > 5%
        let ema = create_ema_values(102.0, 100.0, 95.0, 90.0, 88.0);
        let config = EmaDistanceConfig::default();
        let filter = apply_ema_distance_filter(96.0, &ema, &config);

        assert_eq!(filter.state, EmaDistanceState::TooFar);
        assert!(filter.distance_ratio > 0.05);
    }

    #[test]
    fn test_distance_normal() {
        // EMA2=100, EMA4=97 → 距离 3.1%，在2%-5%之间
        let ema = create_ema_values(101.0, 100.0, 98.0, 97.0, 96.0);
        let config = EmaDistanceConfig::default();
        let filter = apply_ema_distance_filter(99.0, &ema, &config);

        assert_eq!(filter.state, EmaDistanceState::Normal);
    }

    #[test]
    fn test_distance_ranging() {
        // EMA2=100, EMA4=99 → 距离 1%，< 2%
        let ema = create_ema_values(100.5, 100.0, 99.5, 99.0, 98.5);
        let config = EmaDistanceConfig::default();
        let filter = apply_ema_distance_filter(99.8, &ema, &config);

        assert_eq!(filter.state, EmaDistanceState::Ranging);
        assert!(filter.is_ranging_market);
    }

    #[test]
    fn test_tangled_market() {
        // 三线缠绕：EMA2/3/4 两两距离 < 1%
        let ema = create_ema_values(100.0, 100.0, 99.8, 99.6, 99.4);
        let config = EmaDistanceConfig::default();
        let filter = apply_ema_distance_filter(99.9, &ema, &config);

        assert!(filter.is_tangled);
        assert!(filter.is_ranging_market);
    }

    #[test]
    fn test_filter_long_bearish_trend() {
        // 空头排列 + 距离过远 + 收盘价 > ema3 → 过滤做多
        // 空头排列: ema2 < ema3 < ema4
        let ema = create_ema_values(85.0, 88.0, 92.0, 100.0, 105.0);
        let config = EmaDistanceConfig::default();
        let filter = apply_ema_distance_filter(95.0, &ema, &config); // 收盘价 > ema3(92)

        assert!(filter.should_filter_long);
        assert!(!filter.should_filter_short);
    }

    #[test]
    fn test_filter_short_bullish_trend() {
        // 多头排列 + 距离过远 + 收盘价 < ema3 → 过滤做空
        // 多头排列: ema2 > ema3 > ema4
        let ema = create_ema_values(115.0, 112.0, 108.0, 100.0, 95.0);
        let config = EmaDistanceConfig::default();
        let filter = apply_ema_distance_filter(105.0, &ema, &config); // 收盘价 < ema3(108)

        assert!(!filter.should_filter_long);
        assert!(filter.should_filter_short);
    }

    #[test]
    fn test_volume_decreasing() {
        // Vol(n-2)=1000 > Vol(n-1)=800 > Vol(n)=600 → 递减
        let volumes = vec![1000.0, 800.0, 600.0];
        assert!(check_volume_decreasing_filter(&volumes));

        // 非递减
        let volumes2 = vec![800.0, 1000.0, 600.0];
        assert!(!check_volume_decreasing_filter(&volumes2));
    }

    #[test]
    fn test_ema_alignment() {
        // 多头排列
        let bullish = create_ema_values(110.0, 105.0, 100.0, 95.0, 90.0);
        assert_eq!(get_ema_alignment(&bullish, 0.001), EmaTrendAlignment::Bullish);

        // 空头排列
        let bearish = create_ema_values(90.0, 95.0, 100.0, 105.0, 110.0);
        assert_eq!(get_ema_alignment(&bearish, 0.001), EmaTrendAlignment::Bearish);

        // 混乱
        let mixed = create_ema_values(100.0, 105.0, 100.0, 103.0, 98.0);
        assert_eq!(get_ema_alignment(&mixed, 0.001), EmaTrendAlignment::Mixed);
    }
}

