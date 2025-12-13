//! # Rust Quant Infrastructure
//!
//! 基础设施层 - 实现领域层定义的接口
//!
//! ## 职责
//!
//! 1. **数据访问**: 实现 Repository 接口，连接数据库
//! 2. **缓存管理**: Redis 缓存实现
//! 3. **消息传递**: 事件总线、Pub/Sub
//! 4. **外部服务**: 交易所API、邮件服务等
//!
//! ## 架构原则
//!
//! - 实现 `domain` 包中定义的 trait
//! - 可替换性: 不同环境可使用不同实现
//! - 可测试性: 提供 Mock 实现用于测试
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use rust_quant_infrastructure::repositories::SqlxCandleRepository;
//! use rust_quant_domain::traits::CandleRepository;
//!
//! // 创建仓储
//! let repo = SqlxCandleRepository::new(db_pool);
//!
//! // 使用领域接口
//! let candles = repo.find_candles("BTC-USDT", Timeframe::H1, start, end, None).await?;
//! ```

pub mod cache;
pub mod exchanges;
pub mod messaging;
pub mod repositories;

// 重新导出常用类型
pub use exchanges::*;
pub use repositories::{
    SignalLogEntity, SignalLogRepository, SqlxBacktestRepository,
    SqlxCandleRepository, SqlxStrategyConfigRepository,
    StrategyConfigEntity, StrategyConfigEntityModel,
};

// 导出通用缓存接口（泛型，不依赖业务类型）
pub use cache::{CacheProvider, InMemoryCache, RedisCache, TwoLevelCache};
