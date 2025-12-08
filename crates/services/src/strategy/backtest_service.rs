//! 回测服务
//!
//! 负责回测日志和详情的保存，协调 BacktestLogRepository

use anyhow::Result;
use serde_json::json;
use std::env;
use tracing::info;

use rust_quant_analytics::calculate_performance_metrics;
use rust_quant_common::CandleItem;
use rust_quant_domain::entities::{BacktestDetail, BacktestLog, BacktestPerformanceMetrics};
use rust_quant_domain::traits::BacktestLogRepository;
use rust_quant_domain::StrategyType;
use rust_quant_strategies::strategy_common::{BackTestResult, BasicRiskStrategyConfig, TradeRecord};

/// 回测服务
///
/// 职责：
/// 1. 保存回测日志
/// 2. 保存回测详情
/// 3. 协调回测数据的持久化
///
/// 依赖：
/// - BacktestLogRepository: 回测数据访问接口
pub struct BacktestService {
    repository: Box<dyn BacktestLogRepository>,
}

impl BacktestService {
    /// 创建回测服务实例
    ///
    /// # 参数
    /// * `repository` - BacktestLogRepository 实现（通过依赖注入）
    pub fn new(repository: Box<dyn BacktestLogRepository>) -> Self {
        Self { repository }
    }

    /// 保存回测日志和详情
    ///
    /// # 参数
    /// * `inst_id` - 交易对
    /// * `time` - 时间周期
    /// * `strategy_config_string` - 策略配置 JSON 字符串
    /// * `back_test_result` - 回测结果
    /// * `mysql_candles` - K 线数据（用于统计）
    /// * `risk_strategy_config` - 风险配置
    /// * `strategy_name` - 策略名称
    ///
    /// # 返回
    /// * 回测日志 ID
    pub async fn save_backtest_log(
        &self,
        inst_id: &str,
        time: &str,
        strategy_config_string: Option<String>,
        back_test_result: BackTestResult,
        mysql_candles: &[CandleItem],
        risk_strategy_config: BasicRiskStrategyConfig,
        strategy_name: &str,
    ) -> Result<i64> {
        let mut log_entity = BacktestLog::new(
            strategy_name.to_string(),
            inst_id.to_string(),
            time.to_string(),
            back_test_result.win_rate.to_string(),
            back_test_result.funds.to_string(),
            back_test_result.open_trades as i32,
            strategy_config_string,
            json!(risk_strategy_config).to_string(),
            (back_test_result.funds - 100.0).to_string(),
            mysql_candles.first().map(|c| c.ts).unwrap_or_default(),
            mysql_candles.last().map(|c| c.ts).unwrap_or_default(),
            mysql_candles.len() as i32,
        );

        // 可选：写入前后可在此更新自定义胜率统计
        log_entity.one_bar_after_win_rate = 0.0;
        log_entity.two_bar_after_win_rate = 0.0;
        log_entity.three_bar_after_win_rate = 0.0;
        log_entity.four_bar_after_win_rate = 0.0;
        log_entity.five_bar_after_win_rate = 0.0;
        log_entity.ten_bar_after_win_rate = 0.0;

        let back_test_id = self.repository.insert_log(&log_entity).await?;

        // 如果启用了随机测试，则不保存详情和绩效指标
        if env::var("ENABLE_RANDOM_TEST").unwrap_or_default() != "true"
            && !back_test_result.trade_records.is_empty()
        {
            // 保存回测详情
            self.save_backtest_details(
                back_test_id,
                StrategyType::from_str(strategy_name).unwrap_or(StrategyType::Vegas),
                inst_id,
                time,
                back_test_result.trade_records.clone(),
            )
            .await?;

            // 计算并更新绩效指标
            let start_time = mysql_candles.first().map(|c| c.ts).unwrap_or_default();
            let end_time = mysql_candles.last().map(|c| c.ts).unwrap_or_default();

            self.update_performance_metrics(
                back_test_id,
                100.0, // 初始资金
                back_test_result.funds,
                &back_test_result.trade_records,
                start_time,
                end_time,
            )
            .await?;
        }

        info!(
            "回测日志保存成功: back_test_id={}, inst_id={}, period={}",
            back_test_id, inst_id, time
        );

        Ok(back_test_id)
    }

    /// 保存回测详情
    ///
    /// # 参数
    /// * `back_test_id` - 回测日志 ID
    /// * `strategy_type` - 策略类型
    /// * `inst_id` - 交易对
    /// * `time` - 时间周期
    /// * `trade_records` - 交易记录列表
    ///
    /// # 返回
    /// * 保存的记录数
    pub async fn save_backtest_details(
        &self,
        back_test_id: i64,
        strategy_type: StrategyType,
        inst_id: &str,
        time: &str,
        trade_records: Vec<TradeRecord>,
    ) -> Result<u64> {
        if trade_records.is_empty() {
            return Ok(0);
        }

        let details: Vec<BacktestDetail> = trade_records
            .into_iter()
            .map(|trade_record| {
                BacktestDetail::new(
                    back_test_id,
                    trade_record.option_type,
                    strategy_type.as_str().to_owned(),
                    inst_id.to_string(),
                    time.to_string(),
                    trade_record.open_position_time,
                    trade_record.signal_open_position_time,
                    trade_record.signal_status,
                    trade_record
                        .close_position_time
                        .unwrap_or_else(|| "".to_string()),
                    trade_record.open_price.to_string(),
                    trade_record.close_price.map(|p| p.to_string()),
                    trade_record.profit_loss.to_string(),
                    trade_record.quantity.to_string(),
                    trade_record.full_close.to_string(),
                    trade_record.close_type,
                    trade_record.win_num,
                    trade_record.loss_num,
                    trade_record.signal_value.unwrap_or_default(),
                    trade_record.signal_result.unwrap_or_default(),
                )
            })
            .collect();

        let count = self.repository.insert_details(&details).await?;
        info!(
            "回测详情保存成功: back_test_id={}, count={}",
            back_test_id, count
        );
        Ok(count)
    }

    /// 计算并更新绩效指标
    ///
    /// # 参数
    /// * `back_test_id` - 回测日志 ID
    /// * `initial_fund` - 期初资金
    /// * `final_fund` - 期末资金
    /// * `trade_records` - 交易记录列表
    /// * `start_time` - 回测开始时间 (毫秒时间戳)
    /// * `end_time` - 回测结束时间 (毫秒时间戳)
    ///
    /// # 返回
    /// * 更新的行数
    pub async fn update_performance_metrics(
        &self,
        back_test_id: i64,
        initial_fund: f64,
        final_fund: f64,
        trade_records: &[TradeRecord],
        start_time: i64,
        end_time: i64,
    ) -> Result<u64> {
        // 计算绩效指标
        let metrics = calculate_performance_metrics(
            initial_fund,
            final_fund,
            trade_records,
            start_time,
            end_time,
        );

        // 转换为领域模型
        let domain_metrics = BacktestPerformanceMetrics {
            sharpe_ratio: metrics.sharpe_ratio,
            annual_return: metrics.annual_return,
            total_return: metrics.total_return,
            max_drawdown: metrics.max_drawdown,
            volatility: metrics.volatility,
        };

        // 更新数据库
        let affected = self
            .repository
            .update_performance_metrics(back_test_id, &domain_metrics)
            .await?;

        info!(
            "绩效指标更新成功: back_test_id={}, sharpe={:.4}, annual_return={:.2}%, max_drawdown={:.2}%, volatility={:.2}%",
            back_test_id,
            metrics.sharpe_ratio,
            metrics.annual_return * 100.0,
            metrics.max_drawdown * 100.0,
            metrics.volatility * 100.0
        );

        Ok(affected)
    }
}

