//! Pipeline执行器

use super::context::BacktestContext;
use super::stage::{BacktestStage, StageResult};
use crate::framework::backtest::types::{BackTestResult, BasicRiskStrategyConfig, TradingState};
use crate::CandleItem;

/// Pipeline执行器
///
/// 管理Stage链的执行
pub struct PipelineRunner {
    stages: Vec<Box<dyn BacktestStage>>,
}

impl PipelineRunner {
    /// 创建新的Pipeline
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// 添加Stage
    pub fn add_stage<S: BacktestStage + 'static>(mut self, stage: S) -> Self {
        self.stages.push(Box::new(stage));
        self
    }

    /// 执行Pipeline处理单根K线
    ///
    /// # 返回
    /// - `StageResult`: 最终执行结果
    pub fn process_candle(&mut self, ctx: &mut BacktestContext) -> StageResult {
        for stage in &mut self.stages {
            let result = stage.process(ctx);
            match result {
                StageResult::Continue => continue,
                StageResult::Skip | StageResult::Exit { .. } => return result,
            }
        }
        StageResult::Continue
    }

    /// 执行完整回测
    ///
    /// # 参数
    /// - `candles`: K线数据
    /// - `inst_id`: 交易对标识
    /// - `risk_config`: 风控配置
    /// - `min_data_length`: 最小数据长度（用于指标计算预热）
    ///
    /// # 返回
    /// - `BackTestResult`: 回测结果
    pub fn run(
        &mut self,
        candles: &[CandleItem],
        inst_id: &str,
        risk_config: BasicRiskStrategyConfig,
        min_data_length: usize,
    ) -> BackTestResult {
        let mut trading_state = TradingState::default();

        // 我们需要从头开始遍历，以便 SignalStage 可以构建完整的 K 线缓冲区
        // 具体的预热逻辑 (min_data_length 和 i < 500) 由 SignalStage 内部控制
        for (i, candle) in candles.iter().enumerate() {
            // 创建/更新上下文
            let mut ctx = BacktestContext::new(
                candle.clone(),
                i,
                inst_id.to_string(),
                risk_config.clone(),
                trading_state.clone(),
            );

            // 执行Pipeline
            let _result = self.process_candle(&mut ctx);

            // 同步状态
            trading_state = ctx.trading_state;
        }

        // 生成结果
        let win_rate = if trading_state.wins + trading_state.losses > 0 {
            trading_state.wins as f64 / (trading_state.wins + trading_state.losses) as f64
        } else {
            0.0
        };

        BackTestResult {
            funds: trading_state.funds,
            win_rate,
            open_trades: trading_state.open_position_times,
            trade_records: trading_state.trade_records,
            filtered_signals: Vec::new(), // TODO: 从FilterStage收集
        }
    }
}

impl Default for PipelineRunner {
    fn default() -> Self {
        Self::new()
    }
}
