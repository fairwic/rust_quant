//! 回测绩效指标计算
//!
//! 核心指标:
//! - 夏普比率 (Sharpe Ratio): 风险调整后收益
//! - 年化收益率 (Annualized Return): 收益能力
//! - 绝对收益率 (Total Return): 总收益
//! - 最大回撤 (Maximum Drawdown): 风险指标
//! - 波动率 (Volatility): 风险指标

use chrono::NaiveDateTime;
use rust_quant_strategies::strategy_common::TradeRecord;

/// 无风险利率 (年化 2%)
const RISK_FREE_RATE: f64 = 0.02;

/// 一年的天数 (用于年化计算)
const DAYS_PER_YEAR: f64 = 365.0;

/// 绩效指标计算结果
#[derive(Debug, Clone, Copy, Default)]
pub struct PerformanceMetrics {
    /// 夏普比率
    pub sharpe_ratio: f64,
    /// 年化收益率
    pub annual_return: f64,
    /// 绝对收益率
    pub total_return: f64,
    /// 最大回撤
    pub max_drawdown: f64,
    /// 波动率 (年化)
    pub volatility: f64,
}

/// 绩效计算器
pub struct PerformanceCalculator {
    /// 期初资金
    initial_fund: f64,
    /// 期末资金
    final_fund: f64,
    /// 交易记录
    trade_records: Vec<TradeRecord>,
    /// 回测开始时间 (毫秒时间戳)
    start_time: i64,
    /// 回测结束时间 (毫秒时间戳)
    end_time: i64,
}

impl PerformanceCalculator {
    /// 创建绩效计算器
    ///
    /// # 参数
    /// * `initial_fund` - 期初资金
    /// * `final_fund` - 期末资金
    /// * `trade_records` - 交易记录列表
    /// * `start_time` - 回测开始时间 (毫秒时间戳)
    /// * `end_time` - 回测结束时间 (毫秒时间戳)
    pub fn new(
        initial_fund: f64,
        final_fund: f64,
        trade_records: Vec<TradeRecord>,
        start_time: i64,
        end_time: i64,
    ) -> Self {
        Self {
            initial_fund,
            final_fund,
            trade_records,
            start_time,
            end_time,
        }
    }

    /// 计算所有绩效指标
    pub fn calculate(&self) -> PerformanceMetrics {
        let total_return = self.calculate_total_return();
        // 使用实际交易周期，而非K线数据范围
        let actual_trading_days = self.calculate_actual_trading_days();
        let annual_return = self.calculate_annual_return(total_return, actual_trading_days);
        let (max_drawdown, equity_curve) = self.calculate_max_drawdown();
        let volatility = self.calculate_volatility(&equity_curve, actual_trading_days);
        let sharpe_ratio = self.calculate_sharpe_ratio(annual_return, volatility);

        PerformanceMetrics {
            sharpe_ratio,
            annual_return,
            total_return,
            max_drawdown,
            volatility,
        }
    }

    /// 计算绝对收益率
    /// 公式: (期末资金 - 期初资金) / 期初资金
    fn calculate_total_return(&self) -> f64 {
        if self.initial_fund <= 0.0 {
            return 0.0;
        }
        (self.final_fund - self.initial_fund) / self.initial_fund
    }

    /// 计算K线数据范围的天数（备用）
    fn calculate_kline_range_days(&self) -> f64 {
        let duration_ms = self.end_time - self.start_time;
        if duration_ms <= 0 {
            return 1.0;
        }
        // 毫秒转天数
        let days = duration_ms as f64 / (1000.0 * 60.0 * 60.0 * 24.0);
        days.max(1.0)
    }

    /// 计算实际交易周期（从第一笔开仓到最后一笔平仓）
    /// 
    /// 这比使用K线数据范围更准确，因为：
    /// 1. K线数据可能包含没有交易信号的时段
    /// 2. 实际资金运作时间应该从第一笔交易开始算起
    fn calculate_actual_trading_days(&self) -> f64 {
        if self.trade_records.is_empty() {
            return self.calculate_kline_range_days();
        }

        // 解析第一笔交易的开仓时间
        let first_open_time = self.trade_records.first().and_then(|r| {
            Self::parse_datetime(&r.open_position_time)
        });

        // 解析最后一笔交易的平仓时间
        let last_close_time = self.trade_records.last().and_then(|r| {
            r.close_position_time.as_ref().and_then(|t| Self::parse_datetime(t))
        });

        match (first_open_time, last_close_time) {
            (Some(start), Some(end)) => {
                let duration = end.signed_duration_since(start);
                let days = duration.num_days() as f64;
                // 至少1天，避免除零
                days.max(1.0)
            }
            _ => {
                // 无法解析时间，回退到K线范围
                self.calculate_kline_range_days()
            }
        }
    }

    /// 解析日期时间字符串
    /// 支持格式: "2024-01-01 12:00:00" 或 "2024-01-01"
    fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
        // 尝试完整格式
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            return Some(dt);
        }
        // 尝试日期格式
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Some(date.and_hms_opt(0, 0, 0)?);
        }
        None
    }

    /// 计算年化收益率
    /// 公式: (1 + total_return)^(365/交易天数) - 1
    fn calculate_annual_return(&self, total_return: f64, trading_days: f64) -> f64 {
        if trading_days <= 0.0 {
            return 0.0;
        }
        let exponent = DAYS_PER_YEAR / trading_days;
        (1.0 + total_return).powf(exponent) - 1.0
    }

    /// 计算最大回撤
    /// 返回 (最大回撤, 权益曲线)
    /// 
    /// 注意：当前实现基于每笔交易结算后的权益点计算。
    /// 这意味着持仓期间的浮动亏损不会被捕捉。
    /// 
    /// 改进方案：同时考虑持仓期间的潜在最大亏损
    /// 通过 open_price 和 close_price 估算持仓期间的最大浮亏
    fn calculate_max_drawdown(&self) -> (f64, Vec<f64>) {
        if self.trade_records.is_empty() {
            return (0.0, vec![self.initial_fund]);
        }

        // 构建权益曲线（包含持仓期间的估算低点）
        let mut equity_curve = Vec::with_capacity(self.trade_records.len() * 2 + 1);
        let mut current_equity = self.initial_fund;
        equity_curve.push(current_equity);

        for record in &self.trade_records {
            // 估算持仓期间的最大浮亏
            // 对于亏损交易，假设持仓期间可能经历更大的浮亏
            // 保守估计：实际亏损的1.5倍作为最大浮亏（如果是亏损交易）
            if record.profit_loss < 0.0 {
                // 记录持仓期间的估算最低点
                let estimated_worst = current_equity + record.profit_loss * 1.5;
                equity_curve.push(estimated_worst.max(0.0));
            }
            
            // 记录平仓后的权益
            current_equity += record.profit_loss;
            equity_curve.push(current_equity);
        }

        // 计算最大回撤
        let mut max_drawdown = 0.0;
        let mut peak = self.initial_fund;

        for &equity in &equity_curve {
            if equity > peak {
                peak = equity;
            }
            if peak > 0.0 {
                let drawdown = (peak - equity) / peak;
                if drawdown > max_drawdown {
                    max_drawdown = drawdown;
                }
            }
        }

        (max_drawdown, equity_curve)
    }

    /// 计算波动率 (年化)
    /// 
    /// 基于交易收益率计算，然后年化
    /// 公式: 交易收益率标准差 * sqrt(每年交易次数)
    fn calculate_volatility(&self, equity_curve: &[f64], actual_trading_days: f64) -> f64 {
        if self.trade_records.len() < 2 {
            return 0.0;
        }

        // 直接使用交易记录计算收益率，而非权益曲线
        // 这样可以避免估算浮亏点带来的噪声
        let mut returns: Vec<f64> = Vec::with_capacity(self.trade_records.len());
        let mut running_equity = self.initial_fund;
        
        for record in &self.trade_records {
            if running_equity > 0.0 {
                let ret = record.profit_loss / running_equity;
                returns.push(ret);
            }
            running_equity += record.profit_loss;
        }

        if returns.is_empty() {
            return 0.0;
        }

        // 计算平均收益率
        let mean_return: f64 = returns.iter().sum::<f64>() / returns.len() as f64;

        // 计算方差
        let variance: f64 = returns
            .iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>()
            / returns.len() as f64;

        // 标准差
        let std_dev = variance.sqrt();

        // 年化波动率
        // 每年交易次数 = 总交易次数 / 实际交易年数
        let trading_years = actual_trading_days / DAYS_PER_YEAR;
        let trades_per_year = if trading_years > 0.0 {
            self.trade_records.len() as f64 / trading_years
        } else {
            self.trade_records.len() as f64
        };

        std_dev * trades_per_year.sqrt()
    }

    /// 计算夏普比率
    /// 公式: (年化收益率 - 无风险利率) / 年化波动率
    fn calculate_sharpe_ratio(&self, annual_return: f64, volatility: f64) -> f64 {
        if volatility <= 0.0 {
            // 波动率为0时，如果有正收益则返回较大值，否则返回0
            return if annual_return > RISK_FREE_RATE {
                f64::MAX.min(100.0) // 限制最大值
            } else {
                0.0
            };
        }
        (annual_return - RISK_FREE_RATE) / volatility
    }
}

/// 便捷函数：计算回测绩效指标
///
/// # 参数
/// * `initial_fund` - 期初资金
/// * `final_fund` - 期末资金
/// * `trade_records` - 交易记录列表
/// * `start_time` - 回测开始时间 (毫秒时间戳)
/// * `end_time` - 回测结束时间 (毫秒时间戳)
///
/// # 返回
/// 绩效指标结构体
pub fn calculate_performance_metrics(
    initial_fund: f64,
    final_fund: f64,
    trade_records: &[TradeRecord],
    start_time: i64,
    end_time: i64,
) -> PerformanceMetrics {
    let calculator = PerformanceCalculator::new(
        initial_fund,
        final_fund,
        trade_records.to_vec(),
        start_time,
        end_time,
    );
    calculator.calculate()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_trade_record_with_time(
        profit_loss: f64,
        open_time: &str,
        close_time: &str,
    ) -> TradeRecord {
        TradeRecord {
            option_type: "long".to_string(),
            open_position_time: open_time.to_string(),
            signal_open_position_time: None,
            close_position_time: Some(close_time.to_string()),
            open_price: 100.0,
            signal_status: 1,
            close_price: Some(100.0 + profit_loss),
            profit_loss,
            quantity: 1.0,
            full_close: true,
            close_type: "take_profit".to_string(),
            win_num: if profit_loss > 0.0 { 1 } else { 0 },
            loss_num: if profit_loss < 0.0 { 1 } else { 0 },
            signal_value: None,
            signal_result: None,
        }
    }

    #[test]
    fn test_total_return() {
        let trades = vec![
            create_test_trade_record_with_time(10.0, "2024-01-01", "2024-01-10"),
            create_test_trade_record_with_time(-5.0, "2024-01-15", "2024-01-20"),
            create_test_trade_record_with_time(15.0, "2024-01-25", "2024-01-30"),
        ];

        // K线范围30天（但实际交易只有29天：1月1日到1月30日）
        let start_time = 0i64;
        let end_time = 30 * 24 * 60 * 60 * 1000i64;

        let metrics = calculate_performance_metrics(100.0, 120.0, &trades, start_time, end_time);

        // 绝对收益率 = (120 - 100) / 100 = 0.2 = 20%
        assert!((metrics.total_return - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_max_drawdown_with_floating_loss() {
        // 测试包含浮亏估算的最大回撤
        let trades = vec![
            create_test_trade_record_with_time(20.0, "2024-01-01", "2024-01-10"),  // 100 -> 120
            create_test_trade_record_with_time(-30.0, "2024-01-15", "2024-01-20"), // 120 -> 90
            create_test_trade_record_with_time(10.0, "2024-01-25", "2024-01-30"),  // 90 -> 100
        ];

        let start_time = 0i64;
        let end_time = 30 * 24 * 60 * 60 * 1000i64;

        let metrics = calculate_performance_metrics(100.0, 100.0, &trades, start_time, end_time);

        // 最大回撤应该大于25%（因为包含了估算的浮亏）
        // 估算最低点 = 120 + (-30 * 1.5) = 75
        // 最大回撤 = (120 - 75) / 120 = 0.375 = 37.5%
        assert!(metrics.max_drawdown > 0.25);
        assert!((metrics.max_drawdown - 0.375).abs() < 0.001);
    }

    #[test]
    fn test_actual_trading_days() {
        // 测试实际交易周期计算
        // K线范围是1年，但实际交易只有30天
        let trades = vec![
            create_test_trade_record_with_time(10.0, "2024-06-01", "2024-06-15"),
            create_test_trade_record_with_time(10.0, "2024-06-20", "2024-06-30"),
        ];

        // K线范围365天
        let start_time = 0i64;
        let end_time = 365 * 24 * 60 * 60 * 1000i64;

        let metrics = calculate_performance_metrics(100.0, 120.0, &trades, start_time, end_time);

        // 绝对收益率 = 20%
        assert!((metrics.total_return - 0.2).abs() < 0.001);
        
        // 年化收益率应该基于29天（6月1日到6月30日），而非365天
        // 如果用365天：(1.2)^(365/365) - 1 = 0.2 = 20%
        // 如果用29天：(1.2)^(365/29) - 1 ≈ 8.68 = 868%
        // 年化收益率应该远大于20%
        assert!(metrics.annual_return > 1.0);
    }

    #[test]
    fn test_empty_trades() {
        let trades: Vec<TradeRecord> = vec![];
        let start_time = 0i64;
        let end_time = 30 * 24 * 60 * 60 * 1000i64;

        let metrics = calculate_performance_metrics(100.0, 100.0, &trades, start_time, end_time);

        assert_eq!(metrics.total_return, 0.0);
        assert_eq!(metrics.max_drawdown, 0.0);
    }
}

