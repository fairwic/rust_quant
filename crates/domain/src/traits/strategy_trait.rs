//! 策略领域接口

use async_trait::async_trait;
use anyhow::Result;

use crate::entities::Candle;
use crate::value_objects::SignalResult;

/// 策略接口 - 定义策略必须实现的行为
#[async_trait]
pub trait Strategy: Send + Sync {
    /// 策略名称
    fn name(&self) -> &str;
    
    /// 分析K线并生成交易信号
    async fn analyze(&self, candles: &[Candle]) -> Result<SignalResult>;
    
    /// 初始化策略 (可选)
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// 清理资源 (可选)
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// 验证策略配置是否有效
    fn validate_config(&self) -> Result<()> {
        Ok(())
    }
}

/// 回测能力接口
#[async_trait]
pub trait Backtestable: Strategy {
    /// 执行回测
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


