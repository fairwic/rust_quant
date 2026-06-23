use crate::backtest::{
    BackTestAnalysis, BackTestAnalysisModel, BackTestDetailModel, BackTestLogModel,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use rust_quant_market::models::CandlesEntity;
use std::sync::Arc;
use tokio::task;
use tracing::{error, info};
#[derive(Debug, Clone)]
pub struct PositionAnalysis {
    /// backtest ID。
    pub back_test_id: i32,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 时间周期，用于记录交易或执行状态。
    pub time_period: String,
    /// 类型标识。
    pub option_type: String,
    /// 开仓时间。
    pub open_time: DateTime<Utc>,
    /// 价格数值。
    pub open_price: f64,
    /// barsafter，用于记录交易或执行状态。
    pub bars_after: i32,
    /// 价格after，用于记录交易或执行状态。
    pub price_after: f64,
    /// 价格changepercent。
    pub price_change_percent: f64,
    /// isprofitable。
    pub is_profitable: bool,
}
impl PositionAnalysis {
    /// 封装当前函数，减少交易执行调用方重复实现相同细节。
    /// 返回 Result 以便错误透明上抛、统一降级处理，便于后续重试和观测。
    pub async fn analyze_positions(back_test_id: i32, candles: &[CandlesEntity]) -> Result<()> {
        info!(
            "Starting position analysis for back_test_id: {}",
            back_test_id
        );
        // 创建模型实例
        let detail_model = BackTestDetailModel;
        let analysis_model = BackTestAnalysisModel;
        // 查询需要分析的持仓记录
        let positions = detail_model
            .find_by_back_test_id(back_test_id as i64)
            .await
            .context("Failed to fetch positions for analysis")?;
        info!("Found {} positions to analyze", positions.len());
        if positions.is_empty() {
            return Ok(());
        }
        // 定义要分析的K线数量
        let bars_to_analyze = vec![1, 2, 3, 4, 5, 10, 20, 30];
        // 将K线数据转换为Arc以便在任务间共享
        let candles = Arc::new(candles.to_vec());
        // 创建分析任务
        let mut tasks = Vec::new();
        let chunk_size = (positions.len() / 100).max(1); // 将位置分成最多100个块
        for positions_chunk in positions.chunks(chunk_size) {
            let positions_chunk = positions_chunk.to_vec();
            let candles = Arc::clone(&candles);
            let bars_to_analyze = bars_to_analyze.clone();
            // 为每个块创建一个任务
            tasks.push(task::spawn(async move {
                let mut analyses = Vec::new();
                for position in positions_chunk {
                    match position.open_price.parse::<f64>() {
                        Ok(open_price) => {
                            // 将 NaiveDateTime 转换为字符串用于查找
                            let open_time_str = position
                                .open_position_time
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string();
                            // 查找开仓时间对应的K线索引
                            if let Some(open_index) = find_candle_index(&candles, &open_time_str) {
                                // 分析不同K线数量后的价格变化
                                for bars in &bars_to_analyze {
                                    if open_index + *bars as usize >= candles.len() {
                                        continue;
                                    }
                                    if let Ok(future_price) =
                                        candles[open_index + *bars as usize].c.parse::<f64>()
                                    {
                                        let price_change = calculate_price_change(
                                            &position.option_type,
                                            open_price,
                                            future_price,
                                        );
                                        // 创建分析记录
                                        analyses.push(BackTestAnalysis {
                                            id: None,
                                            back_test_id,
                                            inst_id: position.inst_id.clone(),
                                            time: position.time.clone(),
                                            option_type: position.option_type.clone(),
                                            open_position_time: Some(open_time_str.clone()),
                                            open_price: open_price.to_string(),
                                            bars_after: *bars,
                                            price_after: future_price.to_string(),
                                            price_change_percent: price_change.to_string(),
                                            is_profitable: if price_change > 0.0 { 1 } else { 0 },
                                            created_at: None,
                                        });
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "Failed to parse open price '{}': {}",
                                position.open_price, e
                            );
                        }
                    }
                }
                analyses
            }));
        }
        // 等待所有任务完成并收集结果
        let results = join_all(tasks).await;
        // 合并所有分析结果
        let mut all_analyses = Vec::new();
        for result in results {
            match result {
                Ok(analyses) => all_analyses.extend(analyses),
                Err(e) => error!("Task failed: {}", e),
            }
        }
        // 批量插入所有分析结果
        if !all_analyses.is_empty() {
            let affected_rows = analysis_model
                .batch_insert(all_analyses)
                .await
                .context("Failed to insert analysis records")?;
            info!(
                "Successfully inserted {} analysis records for back_test_id: {}",
                affected_rows, back_test_id
            );
            // 计算统计数据并更新 back_test_log
            info!("开始计算 back_test_id {} 的K线后胜率统计", back_test_id);
            let stats = analysis_model
                .calculate_position_stats(back_test_id)
                .await
                .context("Failed to calculate position statistics")?;
            // 打印分析结果
            info!(
                "统计结果 - 3K后胜率: {:.2}%, 5K后胜率: {:.2}%, 10K后胜率: {:.2}%",
                stats.three_bar_after_win_rate * 100.0,
                stats.five_bar_after_win_rate * 100.0,
                stats.ten_bar_after_win_rate * 100.0
            );
            // 更新 back_test_log 表
            let log_model = BackTestLogModel;
            let updated = log_model
                .update_position_stats(back_test_id as i64, stats)
                .await
                .context("Failed to update back_test_log with position statistics")?;
            info!(
                "成功更新 back_test_id: {} 的统计数据到 back_test_log 表 (影响行数: {})",
                back_test_id, updated
            );
        } else {
            info!(
                "No analysis records to insert for back_test_id: {}",
                back_test_id
            );
        }
        Ok(())
    }
}
// 查找K线索引的辅助函数
/// 加载 交易执行与风控 运行所需数据，并把缺失或异常交给调用方处理。
fn find_candle_index(candles: &[CandlesEntity], position_time: &str) -> Option<usize> {
    candles.iter().position(|c| {
        let candle_time =
            rust_quant_common::utils::time::mill_time_to_datetime_shanghai(c.ts).unwrap();
        let formatted_position_time = position_time
            .split('+')
            .next()
            .unwrap_or("")
            .replace("T", " ");
        candle_time == formatted_position_time
    })
}
// 计算价格变化的辅助函数
/// 计算 交易执行与风控 指标，保持公式和边界处理集中可审计。
fn calculate_price_change(option_type: &str, open_price: f64, future_price: f64) -> f64 {
    match option_type {
        "long" => (future_price - open_price) / open_price * 100.0,
        "short" => (open_price - future_price) / open_price * 100.0,
        _ => 0.0,
    }
}
