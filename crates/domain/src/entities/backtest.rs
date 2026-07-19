//! 回测日志与详情领域实体
//!
//! 与 legacy `back_test_log`、`back_test_detail` 表保持字段兼容，
//! 但通过领域实体抽象提供更明确的含义。
use serde::{Deserialize, Serialize};
/// 回测日志聚合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestLog {
    /// 自增主键
    pub id: Option<i64>,
    /// 策略类型 (vegas / nwe ...)
    pub strategy_type: String,
    /// 交易对
    pub inst_id: String,
    /// 时间周期 (如 5m / 1H)
    pub timeframe: String,
    /// 胜率
    pub win_rate: String,
    /// 期末资金
    pub final_fund: String,
    /// 开仓次数
    pub open_positions_num: i32,
    /// 策略配置 JSON
    pub strategy_detail: Option<String>,
    /// 风险配置 JSON
    pub risk_config_detail: String,
    /// 总收益
    pub profit: String,
    /// 开仓后第 1 根胜率
    pub one_bar_after_win_rate: f32,
    /// 开仓后第 2 根胜率
    pub two_bar_after_win_rate: f32,
    /// 开仓后第 3 根胜率
    pub three_bar_after_win_rate: f32,
    /// 开仓后第 4 根胜率
    pub four_bar_after_win_rate: f32,
    /// 开仓后第 5 根胜率
    pub five_bar_after_win_rate: f32,
    /// 开仓后第 10 根胜率
    pub ten_bar_after_win_rate: f32,
    /// K 线开始时间
    pub kline_start_time: i64,
    /// K 线结束时间
    pub kline_end_time: i64,
    /// K 线数量
    pub kline_nums: i32,
    /// 夏普比率
    pub sharpe_ratio: Option<f64>,
    /// 年化收益率
    pub annual_return: Option<f64>,
    /// 绝对收益率
    pub total_return: Option<f64>,
    /// 最大回撤
    pub max_drawdown: Option<f64>,
    /// 波动率(年化)
    pub volatility: Option<f64>,
}
impl BacktestLog {
    #[allow(clippy::too_many_arguments)]
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(
        strategy_type: String,
        inst_id: String,
        timeframe: String,
        win_rate: String,
        final_fund: String,
        open_positions_num: i32,
        strategy_detail: Option<String>,
        risk_config_detail: String,
        profit: String,
        kline_start_time: i64,
        kline_end_time: i64,
        kline_nums: i32,
    ) -> Self {
        Self {
            id: None,
            strategy_type,
            inst_id,
            timeframe,
            win_rate,
            final_fund,
            open_positions_num,
            strategy_detail,
            risk_config_detail,
            profit,
            one_bar_after_win_rate: 0.0,
            two_bar_after_win_rate: 0.0,
            three_bar_after_win_rate: 0.0,
            four_bar_after_win_rate: 0.0,
            five_bar_after_win_rate: 0.0,
            ten_bar_after_win_rate: 0.0,
            kline_start_time,
            kline_end_time,
            kline_nums,
            sharpe_ratio: None,
            annual_return: None,
            total_return: None,
            max_drawdown: None,
            volatility: None,
        }
    }
}
/// 回测明细
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestDetail {
    /// 自增主键
    pub id: Option<i64>,
    /// backtest ID。
    pub back_test_id: i64,
    /// 类型标识。
    pub option_type: String,
    /// 类型标识。
    pub strategy_type: String,
    /// 交易所合约或现货交易对标识。
    pub inst_id: String,
    /// 周期。
    pub timeframe: String,
    /// 开仓时间。
    pub open_position_time: String,
    /// 开仓时间。
    pub signal_open_position_time: Option<String>,
    /// 状态值。
    pub signal_status: i32,
    /// 平仓时间。
    pub close_position_time: String,
    /// 价格数值。
    pub open_price: String,
    /// 离场价格。
    pub close_price: Option<String>,
    /// 收益亏损，用于交易策略计算。
    pub profit_loss: String,
    /// 数量。
    pub quantity: String,
    /// full收盘，用于交易策略计算。
    pub full_close: String,
    /// 类型标识。
    pub close_type: String,
    /// winnums，用于交易策略计算。
    pub win_nums: i64,
    /// 亏损nums，用于交易策略计算。
    pub loss_nums: i64,
    /// 信号值，用于交易策略计算。
    pub signal_value: String,
    /// 信号结果，用于交易策略计算。
    pub signal_result: String,
    /// 止损来源（如 "Engulfing", "KlineHammer" 等）
    pub stop_loss_source: Option<String>,
    /// 止损更新历史(JSON序列化的Vec<StopLossUpdate>)
    pub stop_loss_update_history: Option<String>,
    /// 入场时冻结的有效保护价。
    pub initial_stop_price: Option<f64>,
    /// 本条记录对应数量的初始价格风险金额。
    pub initial_risk_amount: Option<f64>,
    /// 扣除回测手续费后的出场收益 R 倍数。
    pub net_profit_r: Option<f64>,
}
impl BacktestDetail {
    #[allow(clippy::too_many_arguments)]
    /// 构建 回测与策略研究 所需实例，并集中初始化依赖和默认状态。
    pub fn new(
        back_test_id: i64,
        option_type: String,
        strategy_type: String,
        inst_id: String,
        timeframe: String,
        open_position_time: String,
        signal_open_position_time: Option<String>,
        signal_status: i32,
        close_position_time: String,
        open_price: String,
        close_price: Option<String>,
        profit_loss: String,
        quantity: String,
        full_close: String,
        close_type: String,
        win_nums: i64,
        loss_nums: i64,
        signal_value: String,
        signal_result: String,
        stop_loss_source: Option<String>,
        stop_loss_update_history: Option<String>,
        initial_stop_price: Option<f64>,
        initial_risk_amount: Option<f64>,
        net_profit_r: Option<f64>,
    ) -> Self {
        Self {
            id: None,
            back_test_id,
            option_type,
            strategy_type,
            inst_id,
            timeframe,
            open_position_time,
            signal_open_position_time,
            signal_status,
            close_position_time,
            open_price,
            close_price,
            profit_loss,
            quantity,
            full_close,
            close_type,
            win_nums,
            loss_nums,
            signal_value,
            signal_result,
            stop_loss_source,
            stop_loss_update_history,
            initial_stop_price,
            initial_risk_amount,
            net_profit_r,
        }
    }
}
/// 回测胜率统计
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BacktestWinRateStats {
    /// onebarafterwin 费率。
    pub one_bar_after_win_rate: f32,
    /// twobarafterwin 费率。
    pub two_bar_after_win_rate: f32,
    /// threebarafterwin 费率。
    pub three_bar_after_win_rate: f32,
    /// fourbarafterwin 费率。
    pub four_bar_after_win_rate: f32,
    /// fivebarafterwin 费率。
    pub five_bar_after_win_rate: f32,
    /// tenbarafterwin 费率。
    pub ten_bar_after_win_rate: f32,
}
/// 回测绩效指标
///
/// 包含夏普比率、年化收益率、最大回撤、波动率等核心风险收益指标
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BacktestPerformanceMetrics {
    /// 夏普比率 (Sharpe Ratio)
    /// 计算公式: (年化收益率 - 无风险利率) / 年化波动率
    /// 无风险利率默认使用 2%
    pub sharpe_ratio: f64,
    /// 年化收益率 (Annualized Return)
    /// 计算公式: (期末资金/期初资金)^(365/交易天数) - 1
    pub annual_return: f64,
    /// 绝对收益率 (Total Return)
    /// 计算公式: (期末资金 - 期初资金) / 期初资金
    pub total_return: f64,
    /// 最大回撤 (Maximum Drawdown)
    /// 计算公式: (峰值 - 谷值) / 峰值
    pub max_drawdown: f64,
    /// 波动率 (Annualized Volatility)
    /// 计算公式: 日收益率标准差 * sqrt(365)
    pub volatility: f64,
}
