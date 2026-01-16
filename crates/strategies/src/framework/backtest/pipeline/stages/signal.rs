//! SignalStage - 信号生成阶段

use crate::framework::backtest::adapter::IndicatorStrategyBacktest;
use crate::framework::backtest::pipeline::{BacktestContext, BacktestStage, StageResult};
use crate::CandleItem;

/// 信号生成阶段
///
/// 调用策略生成交易信号
pub struct SignalStage<S: IndicatorStrategyBacktest> {
    strategy: S,
    indicator_combine: S::IndicatorCombine,
    candle_buffer: Vec<CandleItem>,
    min_data_length: usize,
    capacity: usize,
}

impl<S: IndicatorStrategyBacktest> SignalStage<S> {
    pub fn new(strategy: S) -> Self {
        let indicator_combine = strategy.init_indicator_combine();
        let min_data_length = strategy.min_data_length();
        let window_size = min_data_length;
        let capacity = if window_size > 0 {
            (window_size * 2).max(1024)
        } else {
            1024
        };
        Self {
            strategy,
            indicator_combine,
            candle_buffer: Vec::with_capacity(capacity),
            min_data_length,
            capacity,
        }
    }
}

impl<S: IndicatorStrategyBacktest + Send + Sync> BacktestStage for SignalStage<S>
where
    S::IndicatorCombine: Send + Sync,
    S::IndicatorValues: Send + Sync,
{
    fn name(&self) -> &'static str {
        "SignalStage"
    }

    fn process(&mut self, ctx: &mut BacktestContext) -> StageResult {
        // 添加当前K线到缓冲区
        self.candle_buffer.push(ctx.candle.clone());

        // 构建指标值
        let mut indicator_values =
            S::build_indicator_values(&mut self.indicator_combine, &ctx.candle);

        // ⚠️ 严格对齐 engine.rs 的逻辑：
        // 1) 先缓冲数据并计算指标
        // 2) 缓冲不足直接跳过
        // 3) 缓冲满足后调用策略（即使 i < 500 也会调用，但结果会被丢弃）
        // 4) i < 500 时跳过后续阶段（不产生信号、不记录过滤原因）

        // 检查是否有足够数据（engine.rs: if candle_buffer.len() < window_size { continue; }）
        if self.candle_buffer.len() < self.min_data_length {
            return StageResult::Skip;
        }

        // 必须只传递最后 min_data_length 个 K 线（engine.rs: current_slice = last window_size）
        let start_index = self
            .candle_buffer
            .len()
            .saturating_sub(self.min_data_length);
        let current_slice = &self.candle_buffer[start_index..];

        let signal =
            self.strategy
                .generate_signal(current_slice, &mut indicator_values, &ctx.risk_config);

        // 预热期跳过（engine.rs: if i < 500 { continue; }）
        if ctx.candle_index < 500 {
            // 管理缓冲区大小对齐 legacy 行为
            if self.candle_buffer.len() >= self.capacity {
                let remove_count = self
                    .candle_buffer
                    .len()
                    .saturating_sub(self.min_data_length);
                if remove_count > 0 {
                    self.candle_buffer.drain(0..remove_count);
                }
            }
            return StageResult::Skip;
        }

        // 保存信号和过滤原因（i >= 500 才会进入后续阶段）
        if !signal.filter_reasons.is_empty() {
            ctx.is_signal_filtered = true;
            ctx.filter_reasons = signal.filter_reasons.clone();
        }

        ctx.signal = Some(signal);

        // 管理缓冲区大小 (Sliding Window)
        // 对齐 engine.rs：当缓冲达到 capacity 时，剔除最前面多余部分，保留 window_size
        if self.candle_buffer.len() >= self.capacity {
            let remove_count = self
                .candle_buffer
                .len()
                .saturating_sub(self.min_data_length);
            if remove_count > 0 {
                self.candle_buffer.drain(0..remove_count);
            }
        }

        StageResult::Continue
    }
}
