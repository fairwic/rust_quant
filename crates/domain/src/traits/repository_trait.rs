//! 仓储接口 - 定义数据访问的抽象

use async_trait::async_trait;
use anyhow::Result;

use crate::entities::{Candle, Order, StrategyConfig};
use crate::enums::Timeframe;

/// K线仓储接口
#[async_trait]
pub trait CandleRepository: Send + Sync {
    /// 查询K线数据
    async fn find_candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_time: i64,
        end_time: i64,
        limit: Option<usize>,
    ) -> Result<Vec<Candle>>;
    
    /// 获取最新K线
    async fn get_latest_candle(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<Candle>>;
    
    /// 保存K线 (批量)
    async fn save_candles(&self, candles: Vec<Candle>) -> Result<usize>;
}

/// 订单仓储接口
#[async_trait]
pub trait OrderRepository: Send + Sync {
    /// 根据ID查询订单
    async fn find_by_id(&self, id: &str) -> Result<Option<Order>>;
    
    /// 查询用户的所有订单
    async fn find_by_symbol(&self, symbol: &str) -> Result<Vec<Order>>;
    
    /// 保存订单
    async fn save(&self, order: &Order) -> Result<()>;
    
    /// 更新订单
    async fn update(&self, order: &Order) -> Result<()>;
}

/// 策略配置仓储接口
#[async_trait]
pub trait StrategyConfigRepository: Send + Sync {
    /// 根据ID查询配置
    async fn find_by_id(&self, id: i64) -> Result<Option<StrategyConfig>>;
    
    /// 查询交易对和周期的配置
    async fn find_by_symbol_and_timeframe(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Vec<StrategyConfig>>;
    
    /// 保存配置
    async fn save(&self, config: &StrategyConfig) -> Result<i64>;
    
    /// 更新配置
    async fn update(&self, config: &StrategyConfig) -> Result<()>;
    
    /// 删除配置
    async fn delete(&self, id: i64) -> Result<()>;
}


