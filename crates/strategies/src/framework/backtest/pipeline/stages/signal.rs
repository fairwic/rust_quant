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
}

impl<S: IndicatorStrategyBacktest> SignalStage<S> {
    pub fn new(strategy: S) -> Self {
        let indicator_combine = strategy.init_indicator_combine();
        let min_data_length = strategy.min_data_length();
        Self {
            strategy,
            indicator_combine,
            candle_buffer: Vec::with_capacity(min_data_length + 100),
            min_data_length,
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
        // 1. 先缓冲数据 (calculate indicators)
        // 2. 检查全局索引是否满足预热要求 (i < 500)
        // 3. 检查缓冲区是否满足最小长度

        // 预热期跳过 (engine.rs line 74: if i < 500 { continue; })
        if ctx.candle_index < 500 {
            return StageResult::Skip;
        }

        // 检查是否有足够数据
        if self.candle_buffer.len() < self.min_data_length {
            return StageResult::Skip;
        }

        // 生成信号
        // ⚠️ 严格对齐 engine.rs logic:
        // let current_slice = &candle_buffer[candle_buffer.len() - window_size..];
        // 必须只传递最后 min_data_length 个 K 线，因为策略可能对输入长度敏感
        let start_index = self
            .candle_buffer
            .len()
            .saturating_sub(self.min_data_length);
        let current_slice = &self.candle_buffer[start_index..];

        let signal =
            self.strategy
                .generate_signal(current_slice, &mut indicator_values, &ctx.risk_config);

        // 保存信号和过滤原因
        if !signal.filter_reasons.is_empty() {
            ctx.is_signal_filtered = true;
            ctx.filter_reasons = signal.filter_reasons.clone();
        }

        ctx.signal = Some(signal);

        // 管理缓冲区大小 (Sliding Window)
        // 保持缓冲区大小为 min_data_length
        // 逻辑对齐 engine.rs line 99-102
        if self.candle_buffer.len() >= self.candle_buffer.capacity() {
            // engine.rs 使用：
            // let remove_count = candle_buffer.len() - window_size;
            // candle_buffer.drain(0..remove_count);
            // 这里稍微放宽一些容量检查，避免频繁内存操作，但为了逻辑严格一致，
            // 我们应当始终保持传给 strategy 的是最后 min_data_length 个
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
