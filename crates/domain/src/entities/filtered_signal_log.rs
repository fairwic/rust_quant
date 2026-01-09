use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// 过滤信号日志实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilteredSignalLog {
    /// 回测ID
    pub backtest_id: i64,
    /// 交易对
    pub inst_id: String,
    /// 周期
    pub period: String,
    /// 信号时间
    pub signal_time: NaiveDateTime,
    /// 信号方向 (LONG, SHORT)
    pub direction: String,
    /// 过滤原因JSON
    pub filter_reasons: String,
    /// 信号价格
    pub signal_price: f64,
    /// 指标快照JSON
    pub indicator_snapshot: Option<String>,
    /// 理论盈利
    pub theoretical_profit: Option<f64>,
    /// 理论亏损
    pub theoretical_loss: Option<f64>,
    /// 最终模拟PnL
    pub final_pnl: Option<f64>,
    /// 交易结果
    pub trade_result: Option<String>,
}

impl FilteredSignalLog {
    /// 创建新的过滤信号日志
    pub fn new(
        backtest_id: i64,
        inst_id: String,
        period: String,
        signal_time: NaiveDateTime,
        direction: String,
        filter_reasons: String,
        signal_price: f64,
    ) -> Self {
        Self {
            backtest_id,
            inst_id,
            period,
            signal_time,
            direction,
            filter_reasons,
            signal_price,
            indicator_snapshot: None,
            theoretical_profit: None,
            theoretical_loss: None,
            final_pnl: None,
            trade_result: None,
        }
    }
}
