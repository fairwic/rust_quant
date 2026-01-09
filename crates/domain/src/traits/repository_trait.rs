//! 仓储接口 - 定义数据访问的抽象

use anyhow::Result;
use async_trait::async_trait;

use crate::entities::{
    BacktestDetail, BacktestLog, BacktestPerformanceMetrics, BacktestWinRateStats, Candle,
    ExchangeApiConfig, Order, Position, StrategyApiConfig, StrategyConfig, SwapOrder,
};
use crate::enums::Timeframe;
use crate::PositionStatus;

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
    async fn get_latest_candle(&self, symbol: &str, timeframe: Timeframe)
        -> Result<Option<Candle>>;

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

    /// 查询所有启用的配置
    async fn find_all_enabled(&self) -> Result<Vec<StrategyConfig>>;

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

/// 持仓仓储接口
#[async_trait]
pub trait PositionRepository: Send + Sync {
    /// 根据ID查询持仓
    async fn find_by_id(&self, id: &str) -> Result<Option<Position>>;

    /// 查询交易对的所有持仓
    async fn find_by_symbol(&self, symbol: &str) -> Result<Vec<Position>>;

    /// 查询所有未平仓持仓
    async fn find_open_positions(&self) -> Result<Vec<Position>>;

    /// 查询特定状态的持仓
    async fn find_by_status(&self, status: PositionStatus) -> Result<Vec<Position>>;

    /// 保存持仓
    async fn save(&self, position: &Position) -> Result<()>;

    /// 更新持仓
    async fn update(&self, position: &Position) -> Result<()>;

    /// 删除持仓
    async fn delete(&self, id: &str) -> Result<()>;
}

/// 回测日志仓储接口
#[async_trait]
pub trait BacktestLogRepository: Send + Sync {
    /// 写入回测日志，返回自增ID
    async fn insert_log(&self, log: &BacktestLog) -> Result<i64>;

    /// 批量写入回测详情
    async fn insert_details(&self, details: &[BacktestDetail]) -> Result<u64>;

    /// 更新回测胜率统计
    async fn update_win_rate_stats(
        &self,
        backtest_id: i64,
        stats: &BacktestWinRateStats,
    ) -> Result<u64>;

    async fn update_performance_metrics(
        &self,
        backtest_id: i64,
        metrics: &BacktestPerformanceMetrics,
    ) -> Result<u64>;

    /// 批量写入过滤信号记录
    async fn insert_filtered_signals(
        &self,
        signals: &[crate::entities::FilteredSignalLog],
    ) -> Result<u64>;
}

/// 交易所API配置仓储接口
#[async_trait]
pub trait ExchangeApiConfigRepository: Send + Sync {
    /// 根据ID查询API配置
    async fn find_by_id(&self, id: i32) -> Result<Option<ExchangeApiConfig>>;

    /// 查询所有启用的API配置
    async fn find_all_enabled(&self) -> Result<Vec<ExchangeApiConfig>>;

    /// 根据交易所名称查询启用的API配置
    async fn find_by_exchange(&self, exchange_name: &str) -> Result<Vec<ExchangeApiConfig>>;

    /// 保存API配置
    async fn save(&self, config: &ExchangeApiConfig) -> Result<i32>;

    /// 更新API配置
    async fn update(&self, config: &ExchangeApiConfig) -> Result<()>;

    /// 删除API配置
    async fn delete(&self, id: i32) -> Result<()>;
}

/// 策略与API配置关联仓储接口
#[async_trait]
pub trait StrategyApiConfigRepository: Send + Sync {
    /// 根据策略配置ID查询关联的API配置（按优先级排序）
    async fn find_by_strategy_config_id(
        &self,
        strategy_config_id: i32,
    ) -> Result<Vec<ExchangeApiConfig>>;

    /// 创建策略与API配置的关联
    async fn create_association(
        &self,
        strategy_config_id: i32,
        api_config_id: i32,
        priority: i32,
    ) -> Result<i32>;

    /// 删除关联
    async fn delete_association(&self, id: i32) -> Result<()>;

    /// 更新关联优先级
    async fn update_priority(&self, id: i32, priority: i32, is_enabled: bool) -> Result<()>;
}

/// 合约订单仓储接口
#[async_trait]
pub trait SwapOrderRepository: Send + Sync {
    /// 根据ID查询订单
    async fn find_by_id(&self, id: i32) -> Result<Option<SwapOrder>>;

    /// 根据内部订单ID查询
    async fn find_by_in_order_id(&self, in_order_id: &str) -> Result<Option<SwapOrder>>;

    /// 根据外部订单ID查询
    async fn find_by_out_order_id(&self, out_order_id: &str) -> Result<Option<SwapOrder>>;

    /// 查询指定交易对的订单
    async fn find_by_inst_id(&self, inst_id: &str, limit: Option<i32>) -> Result<Vec<SwapOrder>>;

    /// 查询待处理订单（用于幂等性检查）
    async fn find_pending_order(
        &self,
        inst_id: &str,
        period: &str,
        side: &str,
        pos_side: &str,
    ) -> Result<Vec<SwapOrder>>;

    /// 保存订单
    async fn save(&self, order: &SwapOrder) -> Result<i32>;

    /// 更新订单
    async fn update(&self, order: &SwapOrder) -> Result<()>;

    /// 根据策略ID和时间范围查询订单
    async fn find_by_strategy_and_time(
        &self,
        strategy_id: i32,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<SwapOrder>>;
}
