//! 策略领域接口
use crate::entities::Candle;
use crate::value_objects::SignalResult;
use anyhow::Result;
use async_trait::async_trait;
/// 策略接口 - 定义策略必须实现的行为
#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    async fn analyze(&self, candles: &[Candle]) -> Result<SignalResult>;
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
    fn validate_config(&self) -> Result<()> {
        Ok(())
    }
}
/// 回测能力接口
#[async_trait]
pub trait Backtestable: Strategy {
    /// 执行回测
    /// 封装当前函数，减少回测策略调用方重复实现相同细节。
    /// 采用 async 以便与数据库/网络 I/O 协调，减少阻塞并提升并发吞吐。
    async fn backtest(
        &self,
        historical_data: &[Candle],
        initial_balance: f64,
    ) -> Result<BacktestResult>;
}
/// 回测结果
#[derive(Debug, Clone)]
pub struct BacktestResult {
    /// 初始资金
    pub initial_balance: f64,
    /// 最终资金
    pub final_balance: f64,
    /// 总收益率
    pub total_return: f64,
    /// 交易次数
    pub total_trades: usize,
    /// 胜率
    pub win_rate: f64,
    /// 最大回撤
    pub max_drawdown: f64,
    /// 夏普比率
    pub sharpe_ratio: f64,
}
impl BacktestResult {
    pub fn profit(&self) -> f64 {
        self.final_balance - self.initial_balance
    }
    pub fn profit_percent(&self) -> f64 {
        (self.profit() / self.initial_balance) * 100.0
    }
}
