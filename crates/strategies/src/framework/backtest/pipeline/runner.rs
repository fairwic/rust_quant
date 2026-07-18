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
    /// 列表数据。
    stages: Vec<Box<dyn BacktestStage>>,
}
impl PipelineRunner {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }
    /// 添加Stage
    pub fn add_stage<S: BacktestStage + 'static>(mut self, stage: S) -> Self {
        self.stages.push(Box::new(stage));
        self
    }
    /// 按顺序执行每个回测阶段，遇到停止或错误结果时立即返回。
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
    /// # 参数
    /// - `candles`: K线数据
    /// - `inst_id`: 交易对标识
    /// - `risk_config`: 风控配置
    /// - `min_data_length`: 最小数据长度（用于指标计算预热）
    /// # 返回
    /// - `BackTestResult`: 回测结果
    pub fn run(
        &mut self,
        candles: &[CandleItem],
        inst_id: &str,
        risk_config: BasicRiskStrategyConfig,
        min_data_length: usize,
    ) -> BackTestResult {
        // warm-up 长度由 SignalStage 根据策略指标处理，Runner 只负责 candle 顺序和阶段调度，
        // 否则这里提前截断会让过滤信号、审计轨迹和 legacy 对齐结果缺失前置样本。
        let _ = min_data_length;
        if candles.is_empty() {
            return BackTestResult::default();
        }
        // BacktestContext 是整次回测的唯一可变状态容器；资金、持仓、过滤信号和审计轨迹
        // 都从这里推进，避免多个 stage 各自维护副本导致收盘结算不一致。
        let mut ctx = BacktestContext::new(
            candles[0].clone(),
            0,
            inst_id.to_string(),
            risk_config,
            TradingState::default(),
        );
        let collect_dynamic_config_logs =
            !rust_quant_core::config::env_is_true("BACKTEST_FAST_MODE", false);
        let mut dynamic_config_logs: Vec<DynamicConfigLog> = Vec::new();
        // 按原始 K 线顺序逐根推进，保证信号、过滤和成交记录共享同一个时间轴；
        // reset_for_next_candle 只切换当前 candle，不清空跨 candle 的持仓状态。
        for (i, candle) in candles.iter().enumerate() {
            if i > 0 {
                ctx.reset_for_next_candle(candle.clone(), i);
            }
            let _result = self.process_candle(&mut ctx);
            // 动态参数调整属于解释性证据，不参与成交计算，但需要随 candle 保存，
            // 否则回放某笔交易时无法判断当时使用的是哪一份策略配置；快速迭代模式
            // 只关闭这类逐 K 线诊断产物，不改变信号与成交状态推进。
            if collect_dynamic_config_logs {
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
        }
        // 最后一根 K 线后统一收尾未平仓和 shadow 记录，避免统计口径把仍在场内的仓位漏掉。
        if let Some(last_candle) = candles.last() {
            ctx.shadow_manager.finalize(last_candle);
        }
        finalize_trading_state(&mut ctx.trading_state, candles);
        let win_rate = calculate_win_rate(ctx.trading_state.wins, ctx.trading_state.losses);
        // ctx 在这里被拆开，结果对象只暴露回测产物，不继续泄漏 pipeline 内部运行态。
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
