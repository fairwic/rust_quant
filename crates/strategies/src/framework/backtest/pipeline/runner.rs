//! Pipeline执行器

use super::context::BacktestContext;
use super::stage::{BacktestStage, StageResult};
use crate::framework::backtest::position::finalize_trading_state;
use crate::framework::backtest::types::{
    BackTestResult, BasicRiskStrategyConfig, DynamicConfigLog, TradingState,
};
use crate::framework::backtest::utils::calculate_win_rate;
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
        let _ = min_data_length;

        if candles.is_empty() {
            return BackTestResult::default();
        }

        let mut ctx = BacktestContext::new(
            candles[0].clone(),
            0,
            inst_id.to_string(),
            risk_config,
            TradingState::default(),
        );

        let mut dynamic_config_logs: Vec<DynamicConfigLog> = Vec::new();

        // 从头遍历：SignalStage 负责管理 warm-up (min_data_length) 和 i < 500 的对齐逻辑
        for (i, candle) in candles.iter().enumerate() {
            if i > 0 {
                ctx.reset_for_next_candle(candle.clone(), i);
            }
            let _result = self.process_candle(&mut ctx);

            if let Some(signal) = &ctx.signal {
                if signal.dynamic_config_snapshot.is_some()
                    || !signal.dynamic_adjustments.is_empty()
                {
                    dynamic_config_logs.push(DynamicConfigLog {
                        ts: ctx.candle.ts,
                        adjustments: signal.dynamic_adjustments.clone(),
                        config_snapshot: signal.dynamic_config_snapshot.clone(),
                    });
                }
            }
        }

        // --- Finalize: 对齐 legacy engine 的收尾逻辑 ---
        if let Some(last_candle) = candles.last() {
            ctx.shadow_manager.finalize(last_candle);
        }
        finalize_trading_state(&mut ctx.trading_state, candles);

        let win_rate = calculate_win_rate(ctx.trading_state.wins, ctx.trading_state.losses);

        // move out of ctx
        let BacktestContext {
            trading_state,
            shadow_manager,
            audit_trail,
            ..
        } = ctx;

        BackTestResult {
            funds: trading_state.funds,
            win_rate,
            open_trades: trading_state.open_position_times,
            trade_records: trading_state.trade_records,
            filtered_signals: shadow_manager.into_filtered_signals(),
            dynamic_config_logs,
            audit_trail,
        }
    }
}

impl Default for PipelineRunner {
    fn default() -> Self {
        Self::new()
    }
}
