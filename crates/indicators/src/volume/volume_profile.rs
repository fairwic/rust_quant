use rust_quant_common::CandleItem;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
const DEFAULT_LOOKBACK: usize = 48;
const DEFAULT_PRICE_BINS: usize = 24;
const DEFAULT_VALUE_AREA_RATIO: f64 = 0.70;
const MAX_PRICE_BINS: usize = 200;
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct VolumeProfileValue {
    /// pointofcontrol，用于交易策略计算。
    pub point_of_control: f64,
    /// 值area最高，用于交易策略计算。
    pub value_area_high: f64,
    /// 值area最低，用于交易策略计算。
    pub value_area_low: f64,
    /// 数量数值。
    pub total_volume: f64,
    /// 成交量分布的价格分箱数量。
    pub price_bin_count: usize,
    /// 收盘binvolume 比例。
    pub close_bin_volume_ratio: f64,
    /// 距离 POC 的百分比。
    pub distance_to_poc_pct: f64,
    /// 收盘above值area，用于交易策略计算。
    pub close_above_value_area: bool,
    /// 收盘below值area，用于交易策略计算。
    pub close_below_value_area: bool,
    /// 收盘inside值area，用于交易策略计算。
    pub close_inside_value_area: bool,
    /// 收盘on最高成交量node，用于交易策略计算。
    pub close_on_high_volume_node: bool,
    /// 收盘on最低成交量node，用于交易策略计算。
    pub close_on_low_volume_node: bool,
}
#[derive(Debug, Clone)]
pub struct VolumeProfileIndicator {
    /// K 线。
    candles: VecDeque<CandleItem>,
    /// lookback，用于交易策略计算。
    lookback: usize,
    /// 价格bins，用于交易策略计算。
    price_bins: usize,
    /// valuearea 比例。
    value_area_ratio: f64,
}
impl Default for VolumeProfileIndicator {
    /// 提供默认的集中实现，避免回测策略调用方重复处理相同细节。
    fn default() -> Self {
        Self::new(
            DEFAULT_LOOKBACK,
            DEFAULT_PRICE_BINS,
            DEFAULT_VALUE_AREA_RATIO,
        )
    }
}
impl VolumeProfileIndicator {
    /// 初始化new，确保回测策略依赖和内部状态可直接使用。
    pub fn new(lookback: usize, price_bins: usize, value_area_ratio: f64) -> Self {
        let value_area_ratio = if value_area_ratio.is_finite() {
            value_area_ratio.clamp(0.0, 1.0)
        } else {
            DEFAULT_VALUE_AREA_RATIO
        };
        Self {
            candles: VecDeque::new(),
            lookback: lookback.max(1),
            price_bins: price_bins.max(1).min(MAX_PRICE_BINS),
            value_area_ratio,
        }
    }
    pub fn lookback(&self) -> usize {
        self.lookback
    }
    /// 推进指标到下一根 K 线，并返回最新计算结果。
    pub fn next(&mut self, candle: &CandleItem) -> VolumeProfileValue {
        self.candles.push_back(candle.clone());
        while self.candles.len() > self.lookback {
            self.candles.pop_front();
        }
        calculate_profile(
            self.candles.iter(),
            self.price_bins,
            self.value_area_ratio,
            candle.c(),
        )
    }
}
/// 封装当前函数，减少回测策略调用方重复实现相同细节。
/// 当前函数完成参数检查、流程切分与结果封装，确保上层可安全复用。
/// 保留现有接口风格，优先保障可读性、可追踪性与可维护性。
fn calculate_profile<'a>(
    candles: impl Iterator<Item = &'a CandleItem> + Clone,
    price_bins: usize,
    value_area_ratio: f64,
    close: f64,
) -> VolumeProfileValue {
    let mut min_price = f64::INFINITY;
    let mut max_price = f64::NEG_INFINITY;
    for candle in candles.clone() {
        if is_valid_candle(candle) {
            min_price = min_price.min(candle.l().min(candle.h()));
            max_price = max_price.max(candle.l().max(candle.h()));
        }
    }
    if !min_price.is_finite() || !max_price.is_finite() || max_price <= min_price {
        return empty_value(close, price_bins);
    }
    let bin_width = (max_price - min_price) / price_bins as f64;
    if bin_width <= 0.0 || !bin_width.is_finite() {
        return empty_value(close, price_bins);
    }
    let mut volumes = vec![0.0; price_bins];
    for candle in candles {
        if !is_valid_candle(candle) {
            continue;
        }
        distribute_candle_volume(candle, min_price, max_price, bin_width, &mut volumes);
    }
    let total_volume = volumes.iter().sum::<f64>();
    if total_volume <= 0.0 || !total_volume.is_finite() {
        return empty_value(close, price_bins);
    }
    let poc_index = max_volume_index(&volumes);
    let point_of_control = bin_mid(min_price, max_price, bin_width, poc_index, price_bins);
    let (value_area_low_index, value_area_high_index) =
        value_area_indexes(&volumes, poc_index, total_volume * value_area_ratio);
    let value_area_low = bin_low(min_price, bin_width, value_area_low_index);
    let value_area_high = bin_high(
        min_price,
        max_price,
        bin_width,
        value_area_high_index,
        price_bins,
    );
    let close_index = price_to_bin(close, min_price, max_price, bin_width, price_bins);
    let close_bin_volume = volumes[close_index];
    let average_bin_volume = total_volume / price_bins as f64;
    VolumeProfileValue {
        point_of_control,
        value_area_high,
        value_area_low,
        total_volume,
        price_bin_count: price_bins,
        close_bin_volume_ratio: close_bin_volume / total_volume,
        distance_to_poc_pct: if point_of_control > 0.0 {
            (close - point_of_control) / point_of_control
        } else {
            0.0
        },
        close_above_value_area: close > value_area_high,
        close_below_value_area: close < value_area_low,
        close_inside_value_area: close >= value_area_low && close <= value_area_high,
        close_on_high_volume_node: close_bin_volume >= average_bin_volume * 1.25,
        close_on_low_volume_node: close_bin_volume <= average_bin_volume * 0.75,
    }
}
/// 判断 回测与策略研究 条件是否满足，给上层流程提供布尔决策。
fn is_valid_candle(candle: &CandleItem) -> bool {
    candle.h().is_finite()
        && candle.l().is_finite()
        && candle.c().is_finite()
        && candle.v().is_finite()
        && candle.h() >= candle.l()
        && candle.v() > 0.0
}
/// 封装分配K 线成交量，减少回测策略调用方重复实现相同细节。
fn distribute_candle_volume(
    candle: &CandleItem,
    min_price: f64,
    max_price: f64,
    bin_width: f64,
    volumes: &mut [f64],
) {
    let low = candle.l().min(candle.h());
    let high = candle.l().max(candle.h());
    let candle_range = high - low;
    if candle_range <= 0.0 {
        let index = price_to_bin(candle.c(), min_price, max_price, bin_width, volumes.len());
        volumes[index] += candle.v();
        return;
    }
    for index in 0..volumes.len() {
        let current_bin_low = bin_low(min_price, bin_width, index);
        let current_bin_high = bin_high(min_price, max_price, bin_width, index, volumes.len());
        let overlap = high.min(current_bin_high) - low.max(current_bin_low);
        if overlap > 0.0 {
            volumes[index] += candle.v() * (overlap / candle_range);
        }
    }
}
/// 计算最大成交量索引，并把公式边界留在回测策略内部。
fn max_volume_index(volumes: &[f64]) -> usize {
    volumes
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
        .unwrap_or(0)
}
/// 封装价值区域索引，减少回测策略调用方重复实现相同细节。
fn value_area_indexes(volumes: &[f64], poc_index: usize, target_volume: f64) -> (usize, usize) {
    let mut low_index = poc_index;
    let mut high_index = poc_index;
    let mut accumulated = volumes[poc_index];
    while accumulated < target_volume && (low_index > 0 || high_index + 1 < volumes.len()) {
        let left_volume = if low_index > 0 {
            Some(volumes[low_index - 1])
        } else {
            None
        };
        let right_volume = if high_index + 1 < volumes.len() {
            Some(volumes[high_index + 1])
        } else {
            None
        };
        match (left_volume, right_volume) {
            (Some(left), Some(right)) if left > right => {
                low_index -= 1;
                accumulated += left;
            }
            (Some(_), Some(right)) => {
                high_index += 1;
                accumulated += right;
            }
            (Some(left), None) => {
                low_index -= 1;
                accumulated += left;
            }
            (None, Some(right)) => {
                high_index += 1;
                accumulated += right;
            }
            (None, None) => break,
        }
    }
    (low_index, high_index)
}
/// 封装价格to分箱，减少回测策略调用方重复实现相同细节。
fn price_to_bin(
    price: f64,
    min_price: f64,
    max_price: f64,
    bin_width: f64,
    price_bins: usize,
) -> usize {
    if price <= min_price {
        return 0;
    }
    if price >= max_price {
        return price_bins - 1;
    }
    ((price - min_price) / bin_width).floor() as usize
}
fn bin_low(min_price: f64, bin_width: f64, index: usize) -> f64 {
    min_price + bin_width * index as f64
}
/// 封装分箱上边界，减少回测策略调用方重复实现相同细节。
fn bin_high(
    min_price: f64,
    max_price: f64,
    bin_width: f64,
    index: usize,
    price_bins: usize,
) -> f64 {
    if index + 1 == price_bins {
        max_price
    } else {
        min_price + bin_width * (index + 1) as f64
    }
}
/// 封装分箱中点，减少回测策略调用方重复实现相同细节。
fn bin_mid(min_price: f64, max_price: f64, bin_width: f64, index: usize, price_bins: usize) -> f64 {
    (bin_low(min_price, bin_width, index)
        + bin_high(min_price, max_price, bin_width, index, price_bins))
        / 2.0
}
/// 封装空价值，减少回测策略调用方重复实现相同细节。
fn empty_value(close: f64, price_bins: usize) -> VolumeProfileValue {
    VolumeProfileValue {
        point_of_control: close,
        value_area_high: close,
        value_area_low: close,
        price_bin_count: price_bins,
        close_inside_value_area: true,
        ..VolumeProfileValue::default()
    }
}
