use super::{
    entry_confirmation, ComputedCandle, MarketVelocityEventBacktestArgs,
    MarketVelocityTradeDirection, RadarEvent, MS_15M,
};
use chrono::{SecondsFormat, TimeZone, Utc};
use std::collections::VecDeque;

/// 历史趋势背景；同时满足多空或历史不足时都按中性处理，避免事后挑方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoricalTrendContext {
    /// 两个背景均未成立，或多空背景同时成立而无法唯一归因。
    Neutral,
    /// 当前信号 K 线之前的历史价格处于上涨背景；具体交易方向由策略模式决定。
    PriorUp,
    /// 当前信号 K 线之前的历史价格处于下跌背景；具体交易方向由策略模式决定。
    PriorDown,
}

/// 单个 symbol/run 内的一次性趋势状态；方向翻转不能绕过中性重置重新武装。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OneShotTrendLifecycle {
    /// 尚无互斥趋势背景，可以等待下一次首次成立。
    Neutral,
    /// 趋势背景已经首次成立，但尚未消费有效极端量 setup。
    Armed(HistoricalTrendContext),
    /// 当前趋势背景的首个有效 setup 已消费，后续同背景不得重复发出。
    Consumed(HistoricalTrendContext),
    /// 背景未先回到中性就直接换向，必须继续等待中性才能重新武装。
    AwaitNeutral,
}

/// 将生命周期与连续中性计数绑定，避免短暂阈值穿越提前解除冷却。
struct OneShotTrendState {
    /// 当前趋势 setup 的消费阶段。
    lifecycle: OneShotTrendLifecycle,
    /// 离开方向背景后连续观察到的中性 15m K 线数量。
    neutral_streak: usize,
    /// 回到 neutral 前要求的连续中性 K 线数量，V1 默认为 1。
    reset_confirm_candles: usize,
}

#[derive(Debug, Clone, Copy)]
struct HistoricalPoint {
    /// 已完成 15m K 线开盘价，用作 192 根净变化的起点。
    open: f64,
    /// 已完成 15m K 线收盘价，用作净变化终点和回归样本。
    close: f64,
}

/// 维护固定 x 轴的滚动线性回归，避免逐根重复扫描 96 根历史。
struct RollingRegression {
    /// 回归窗口长度，单位为已完成 15m K 线根数。
    period: usize,
    /// 按时间升序保存窗口内收盘价。
    closes: VecDeque<f64>,
    /// 窗口内 y 合计，用于常数时间更新协方差。
    sum_y: f64,
    /// 窗口内 y 平方和，用于常数时间更新方差。
    sum_y2: f64,
    /// 固定 x=0..period-1 与收盘价的乘积和。
    sum_xy: f64,
}

impl RollingRegression {
    /// 创建尚未形成完整回归窗口的滚动统计。
    fn new(period: usize) -> Self {
        Self {
            period,
            closes: VecDeque::with_capacity(period.saturating_add(1)),
            sum_y: 0.0,
            sum_y2: 0.0,
            sum_xy: 0.0,
        }
    }

    /// 推入一根已完成 K 线的收盘价，并在满窗时移除最早样本。
    fn push(&mut self, close: f64) {
        if self.closes.len() == self.period {
            let previous_sum_y = self.sum_y;
            let removed = self.closes.pop_front().unwrap_or(0.0);
            self.sum_y -= removed;
            self.sum_y2 -= removed * removed;
            self.sum_xy -= previous_sum_y - removed;
        }
        let x = self.closes.len() as f64;
        self.closes.push_back(close);
        self.sum_y += close;
        self.sum_y2 += close * close;
        self.sum_xy += x * close;
    }

    /// 仅当完整窗口的斜率方向、首尾方向与最小 R² 一致时返回趋势方向。
    fn direction(&self, min_r_squared: f64) -> Option<std::cmp::Ordering> {
        if self.closes.len() != self.period || self.period < 2 {
            return None;
        }
        let n = self.period as f64;
        let sum_x = n * (n - 1.0) / 2.0;
        let sum_x2 = n * (n - 1.0) * (2.0 * n - 1.0) / 6.0;
        let covariance = self.sum_xy - sum_x * self.sum_y / n;
        let variance_x = sum_x2 - sum_x * sum_x / n;
        let variance_y = self.sum_y2 - self.sum_y * self.sum_y / n;
        if variance_x <= 0.0 || variance_y <= 0.0 {
            return None;
        }
        let r_squared = covariance * covariance / (variance_x * variance_y);
        if !r_squared.is_finite() || r_squared < min_r_squared {
            return None;
        }
        let first = *self.closes.front()?;
        let last = *self.closes.back()?;
        if covariance > 0.0 && last > first {
            Some(std::cmp::Ordering::Greater)
        } else if covariance < 0.0 && last < first {
            Some(std::cmp::Ordering::Less)
        } else {
            None
        }
    }
}

/// 同时维护 192 根净幅与 96 根回归趋势，输出互斥历史背景。
struct HistoricalTrendTracker {
    /// 净变化窗口长度，单位为已完成 15m K 线根数。
    net_lookback: usize,
    /// 计算净变化所需的滚动开收盘样本。
    history: VecDeque<HistoricalPoint>,
    /// 计算持续趋势所需的独立滚动回归窗口。
    regression: RollingRegression,
}

impl HistoricalTrendTracker {
    /// 按冻结参数创建净变化和持续趋势两个并行历史窗口。
    fn new(args: &MarketVelocityEventBacktestArgs) -> Self {
        let regression_period = args.entry_min_opposite_duration_candles.unwrap_or(0);
        Self {
            net_lookback: args.entry_opposite_move_lookback_candles,
            history: VecDeque::with_capacity(
                args.entry_opposite_move_lookback_candles.saturating_add(1),
            ),
            regression: RollingRegression::new(regression_period),
        }
    }

    /// 只读取当前信号 K 线之前已推入的历史，输出互斥趋势背景。
    fn context(&self, args: &MarketVelocityEventBacktestArgs) -> HistoricalTrendContext {
        let mut prior_up = false;
        let mut prior_down = false;
        if let Some(minimum) = args.entry_min_opposite_net_move_pct {
            if self.history.len() == self.net_lookback {
                let first = self.history.front();
                let last = self.history.back();
                if let (Some(first), Some(last)) = (first, last) {
                    if first.open.is_finite() && first.open > 0.0 && last.close.is_finite() {
                        let move_pct = (last.close - first.open) / first.open * 100.0;
                        prior_up |= move_pct >= minimum;
                        prior_down |= move_pct <= -minimum;
                    }
                }
            }
        }
        if args.entry_min_opposite_duration_candles.is_some() {
            match self
                .regression
                .direction(args.entry_opposite_duration_min_r_squared)
            {
                Some(std::cmp::Ordering::Greater) => prior_up = true,
                Some(std::cmp::Ordering::Less) => prior_down = true,
                _ => {}
            }
        }
        match (prior_up, prior_down) {
            (true, false) => HistoricalTrendContext::PriorUp,
            (false, true) => HistoricalTrendContext::PriorDown,
            _ => HistoricalTrendContext::Neutral,
        }
    }

    /// 在当前信号判断结束后推入该根 K 线，防止同棒历史条件泄漏。
    fn push(&mut self, candle: &ComputedCandle) {
        if self.history.len() == self.net_lookback {
            self.history.pop_front();
        }
        self.history.push_back(HistoricalPoint {
            open: candle.candle.open,
            close: candle.candle.close,
        });
        self.regression.push(candle.candle.close);
    }
}

/// 一次性状态扫描统计，用于核对去重前后频率是否来自同一组 15m 信号。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OneShotTrendScanStats {
    /// 本次扫描中从中性首次进入 armed 的次数。
    pub(super) armed_episodes: usize,
    /// armed、consumed 或等待中性状态实际回到中性的次数。
    pub(super) neutral_resets: usize,
    /// 研究窗口内满足趋势、量比和振幅条件的 setup 数，尚未应用一次性消费。
    pub(super) valid_setups_before_dedup: usize,
    /// 研究窗口内通过一次性消费后实际发出的 setup 数。
    pub(super) emitted_setups: usize,
}

/// 单个 symbol 的一次性趋势扫描结果。
pub(super) struct OneShotTrendScan {
    /// 使用稳定合成 ID 构建的研究事件，不对应归档事件表主键。
    pub(super) events: Vec<RadarEvent>,
    /// 用于审计状态压缩效果的扫描计数。
    pub(super) stats: OneShotTrendScanStats,
}

impl OneShotTrendState {
    /// 创建单个 symbol/run 独享的趋势状态，禁止并行回测共享可变计数。
    fn new(reset_confirm_candles: usize) -> Self {
        Self {
            lifecycle: OneShotTrendLifecycle::Neutral,
            neutral_streak: 0,
            reset_confirm_candles,
        }
    }

    /// 只有连续中性达到冻结窗口后才允许新方向武装，直接翻转仍进入等待状态。
    fn observe(&mut self, context: HistoricalTrendContext) -> (bool, bool) {
        let previous = self.lifecycle;
        let mut reset = false;
        // 中性尚未确认期间保留原消费身份；当前 context 为中性又会阻止产生新 setup，
        // 因此这里只延迟重置，不会把冷却期误当成仍可交易的方向背景。
        self.lifecycle = match (previous, context) {
            (OneShotTrendLifecycle::Neutral, HistoricalTrendContext::Neutral) => {
                self.neutral_streak = 0;
                OneShotTrendLifecycle::Neutral
            }
            (OneShotTrendLifecycle::Neutral, directional) => {
                self.neutral_streak = 0;
                OneShotTrendLifecycle::Armed(directional)
            }
            (OneShotTrendLifecycle::Armed(current), next)
            | (OneShotTrendLifecycle::Consumed(current), next)
                if next == current =>
            {
                self.neutral_streak = 0;
                previous
            }
            (OneShotTrendLifecycle::Armed(_), HistoricalTrendContext::Neutral)
            | (OneShotTrendLifecycle::Consumed(_), HistoricalTrendContext::Neutral)
            | (OneShotTrendLifecycle::AwaitNeutral, HistoricalTrendContext::Neutral) => {
                self.neutral_streak = self.neutral_streak.saturating_add(1);
                if self.neutral_streak >= self.reset_confirm_candles {
                    self.neutral_streak = 0;
                    reset = true;
                    OneShotTrendLifecycle::Neutral
                } else {
                    previous
                }
            }
            (OneShotTrendLifecycle::Armed(_), _)
            | (OneShotTrendLifecycle::Consumed(_), _)
            | (OneShotTrendLifecycle::AwaitNeutral, _) => {
                self.neutral_streak = 0;
                OneShotTrendLifecycle::AwaitNeutral
            }
        };
        (
            matches!(
                (self.lifecycle, previous),
                (
                    OneShotTrendLifecycle::Armed(_),
                    OneShotTrendLifecycle::Neutral
                )
            ),
            reset,
        )
    }

    /// 第一个有效 setup 消费当前趋势状态；其后即使未成交也必须等待中性重置。
    fn consume(&mut self, context: HistoricalTrendContext) -> bool {
        if self.lifecycle == OneShotTrendLifecycle::Armed(context) {
            self.lifecycle = OneShotTrendLifecycle::Consumed(context);
            true
        } else {
            false
        }
    }
}

/// 极端量 K 线按实体反向；十字/小实体则按更长影线表达的拒绝方向反向入场。
pub(super) fn extreme_volume_contrarian_direction(
    candle: &ComputedCandle,
) -> MarketVelocityTradeDirection {
    let raw = &candle.candle;
    let range = raw.high - raw.low;
    let fallback = if raw.close > raw.open {
        MarketVelocityTradeDirection::Short
    } else {
        MarketVelocityTradeDirection::Long
    };
    if !range.is_finite() || range <= 0.0 {
        return fallback;
    }
    let body_ratio = (raw.close - raw.open).abs() / range;
    if body_ratio >= 0.20 {
        return fallback;
    }
    let upper_wick = raw.high - raw.open.max(raw.close);
    let lower_wick = raw.open.min(raw.close) - raw.low;
    if upper_wick > lower_wick {
        MarketVelocityTradeDirection::Short
    } else if lower_wick > upper_wick {
        MarketVelocityTradeDirection::Long
    } else {
        fallback
    }
}

/// 极端量延续研究只按已完成 K 线实体方向交易，不用影线替十字/小实体猜方向。
fn extreme_volume_continuation_direction(candle: &ComputedCandle) -> MarketVelocityTradeDirection {
    if candle.candle.close > candle.candle.open {
        MarketVelocityTradeDirection::Long
    } else {
        MarketVelocityTradeDirection::Short
    }
}

/// 从单个 symbol 的已完成 15m K 线直接构建一次性趋势信号，不读取事件归档表。
pub(super) fn scan_one_shot_trend_events(
    symbol: &str,
    candles: &[ComputedCandle],
    args: &MarketVelocityEventBacktestArgs,
) -> OneShotTrendScan {
    let mut state = OneShotTrendState::new(args.entry_opposite_trend_reset_confirm_candles);
    let mut trend = HistoricalTrendTracker::new(args);
    let mut stats = OneShotTrendScanStats::default();
    let mut events = Vec::new();
    let event_start_ms = args.event_start_ms.unwrap_or(i64::MIN);
    let event_end_ms = args.event_end_ms.unwrap_or(i64::MAX);

    for (idx, computed) in candles.iter().enumerate() {
        // 15m K 线只有在收盘时才生产可见，因此事件时间必须落在起始时间加 15 分钟。
        let event_ts = computed.candle.ts.saturating_add(MS_15M);
        if event_ts > event_end_ms {
            break;
        }
        let completed_count = idx + 1;
        let context = trend.context(args);
        let (armed, reset) = state.observe(context);
        stats.armed_episodes += usize::from(armed);
        stats.neutral_resets += usize::from(reset);
        trend.push(computed);

        if computed.candle.close == computed.candle.open {
            continue;
        }
        let direction = if args.entry_extreme_volume_continuation {
            extreme_volume_continuation_direction(computed)
        } else {
            extreme_volume_contrarian_direction(computed)
        };
        let required_context = match (args.entry_extreme_volume_continuation, direction) {
            (true, MarketVelocityTradeDirection::Long) => HistoricalTrendContext::PriorUp,
            (true, MarketVelocityTradeDirection::Short) => HistoricalTrendContext::PriorDown,
            (false, MarketVelocityTradeDirection::Long) => HistoricalTrendContext::PriorDown,
            (false, MarketVelocityTradeDirection::Short) => HistoricalTrendContext::PriorUp,
            (_, MarketVelocityTradeDirection::Both) => HistoricalTrendContext::Neutral,
        };
        if context != required_context {
            continue;
        }
        let visible = &candles[..completed_count];
        let (entry_ok, _) = entry_confirmation(visible, event_ts, direction, args);
        if !entry_ok {
            continue;
        }
        if event_ts >= event_start_ms {
            stats.valid_setups_before_dedup += 1;
        }
        // 窗口开始前的首个 setup 也要消费状态，否则会在窗口边界伪造一次重新武装。
        if !state.consume(context) || event_ts < event_start_ms {
            continue;
        }
        stats.emitted_setups += 1;
        events.push(RadarEvent {
            id: synthetic_event_id(symbol, event_ts),
            exchange: "okx".to_string(),
            symbol: symbol.to_string(),
            ts: event_ts,
            detected_at: Utc
                .timestamp_millis_opt(event_ts)
                .single()
                .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
                .unwrap_or_else(|| event_ts.to_string()),
            new_rank: 0,
            delta_rank: 0,
            current_price: computed.candle.close,
            price_change_pct: (computed.candle.close - computed.candle.open) / computed.candle.open
                * 100.0,
        });
    }
    OneShotTrendScan { events, stats }
}

/// 为只读 K 线研究生成可复现事件 ID；它不写库，也不冒充归档事件表主键。
fn synthetic_event_id(symbol: &str, detected_ms: i64) -> i64 {
    let mut hash = 17_i64;
    for byte in symbol.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(i64::from(*byte));
    }
    let symbol_component = (hash.unsigned_abs() % 1_000_000) as i64;
    symbol_component
        .saturating_mul(10_000_000)
        .saturating_add((detected_ms / MS_15M).rem_euclid(10_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::market_velocity_event_backtest::directional_reversal::opposite_net_move_filter_reason;
    use crate::app::market_velocity_event_backtest::BacktestCandle;

    fn computed(open: f64, high: f64, low: f64, close: f64) -> ComputedCandle {
        ComputedCandle {
            candle: BacktestCandle {
                ts: 0,
                open,
                high,
                low,
                close,
                volume: 10.0,
            },
            sma: None,
            ema: None,
            previous_volume_avg: None,
            previous_range_avg: None,
            rsi14: None,
            bollinger_middle: None,
            bollinger_upper: None,
            bollinger_lower: None,
            bollinger_bandwidth_pct: None,
            macd_line: None,
            macd_signal_line: None,
            macd_histogram: None,
        }
    }

    #[test]
    fn state_consumes_only_first_setup_until_neutral_reset() {
        let mut state = OneShotTrendState::new(1);
        state.observe(HistoricalTrendContext::PriorUp);
        assert!(state.consume(HistoricalTrendContext::PriorUp));
        assert!(!state.consume(HistoricalTrendContext::PriorUp));
        state.observe(HistoricalTrendContext::Neutral);
        state.observe(HistoricalTrendContext::PriorUp);
        assert!(state.consume(HistoricalTrendContext::PriorUp));
    }

    #[test]
    fn continuation_direction_follows_completed_candle_body() {
        assert_eq!(
            extreme_volume_continuation_direction(&computed(100.0, 106.0, 99.0, 105.0)),
            MarketVelocityTradeDirection::Long
        );
        assert_eq!(
            extreme_volume_continuation_direction(&computed(100.0, 101.0, 94.0, 95.0)),
            MarketVelocityTradeDirection::Short
        );
    }

    #[test]
    fn direct_direction_flip_cannot_rearm_without_neutral() {
        let mut state = OneShotTrendState::new(1);
        state.observe(HistoricalTrendContext::PriorUp);
        assert!(state.consume(HistoricalTrendContext::PriorUp));
        state.observe(HistoricalTrendContext::PriorDown);
        assert!(!state.consume(HistoricalTrendContext::PriorDown));
        state.observe(HistoricalTrendContext::Neutral);
        state.observe(HistoricalTrendContext::PriorDown);
        assert!(state.consume(HistoricalTrendContext::PriorDown));
    }

    #[test]
    fn stable_reset_requires_all_consecutive_neutral_candles() {
        let mut state = OneShotTrendState::new(8);
        state.observe(HistoricalTrendContext::PriorUp);
        assert!(state.consume(HistoricalTrendContext::PriorUp));

        for _ in 0..7 {
            let (_, reset) = state.observe(HistoricalTrendContext::Neutral);
            assert!(!reset);
        }
        assert!(!state.consume(HistoricalTrendContext::PriorUp));
        let (_, reset) = state.observe(HistoricalTrendContext::Neutral);
        assert!(reset);
        state.observe(HistoricalTrendContext::PriorUp);
        assert!(state.consume(HistoricalTrendContext::PriorUp));
    }

    #[test]
    fn directional_context_breaks_partial_neutral_reset_streak() {
        let mut state = OneShotTrendState::new(3);
        state.observe(HistoricalTrendContext::PriorUp);
        assert!(state.consume(HistoricalTrendContext::PriorUp));
        state.observe(HistoricalTrendContext::Neutral);
        state.observe(HistoricalTrendContext::Neutral);
        state.observe(HistoricalTrendContext::PriorUp);

        let (_, first_reset) = state.observe(HistoricalTrendContext::Neutral);
        let (_, second_reset) = state.observe(HistoricalTrendContext::Neutral);
        assert!(!first_reset);
        assert!(!second_reset);
        let (_, third_reset) = state.observe(HistoricalTrendContext::Neutral);
        assert!(third_reset);
    }

    #[test]
    fn small_body_uses_dominant_wick_rejection_direction() {
        let upper_rejection = computed(100.0, 112.0, 98.0, 101.0);
        let lower_rejection = computed(100.0, 102.0, 88.0, 99.0);
        assert_eq!(
            extreme_volume_contrarian_direction(&upper_rejection),
            MarketVelocityTradeDirection::Short
        );
        assert_eq!(
            extreme_volume_contrarian_direction(&lower_rejection),
            MarketVelocityTradeDirection::Long
        );
    }

    #[test]
    fn rolling_context_matches_frozen_history_filter_at_every_candle() {
        let args = MarketVelocityEventBacktestArgs {
            entry_opposite_move_lookback_candles: 12,
            entry_min_opposite_net_move_pct: Some(5.0),
            entry_min_opposite_duration_candles: Some(8),
            entry_opposite_duration_min_r_squared: 0.60,
            ..MarketVelocityEventBacktestArgs::default()
        };
        let closes = (0..80)
            .map(|idx| match idx {
                0..=24 => 100.0 + idx as f64,
                25..=49 => 125.0 - (idx - 24) as f64 * 1.4,
                _ => 90.0 + (idx - 49) as f64 * 0.3 + (idx % 3) as f64,
            })
            .collect::<Vec<_>>();
        let candles = closes
            .iter()
            .enumerate()
            .map(|(idx, close)| {
                let mut value = computed(*close - 0.2, *close + 0.5, *close - 0.5, *close);
                value.candle.ts = idx as i64 * MS_15M;
                value
            })
            .collect::<Vec<_>>();
        let mut tracker = HistoricalTrendTracker::new(&args);

        for (idx, candle) in candles.iter().enumerate() {
            let completed_count = idx + 1;
            let prior_down = opposite_net_move_filter_reason(
                &candles,
                completed_count,
                MarketVelocityTradeDirection::Long,
                &args,
            )
            .is_none();
            let prior_up = opposite_net_move_filter_reason(
                &candles,
                completed_count,
                MarketVelocityTradeDirection::Short,
                &args,
            )
            .is_none();
            let expected = match (prior_up, prior_down) {
                (true, false) => HistoricalTrendContext::PriorUp,
                (false, true) => HistoricalTrendContext::PriorDown,
                _ => HistoricalTrendContext::Neutral,
            };
            assert_eq!(tracker.context(&args), expected, "candle index {idx}");
            tracker.push(candle);
        }
    }
}
