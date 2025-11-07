//! # Rust Quant Services
//! 
//! 应用服务层 - 协调领域对象和基础设施，实现复杂业务流程
//! 
//! ## 职责
//! 
//! 1. **业务流程协调**: 组合多个领域对象完成业务目标
//! 2. **事务管理**: 定义事务边界
//! 3. **数据转换**: 在DTO和领域模型之间转换
//! 4. **业务验证**: 跨聚合根的业务规则验证
//! 
//! ## 架构位置
//! 
//! ```
//! orchestration (调度) → services (业务协调) → domain + infrastructure
//! ```
//! 
//! ## 与其他层的区别
//! 
//! - **vs domain**: domain包含领域逻辑，services协调领域对象
//! - **vs infrastructure**: infrastructure访问数据，services组织业务流程
//! - **vs orchestration**: orchestration做调度，services实现业务逻辑
//! 
//! ## 设计原则
//! 
//! 1. **无状态**: Service应该是无状态的，所有状态在domain
//! 2. **协调者**: 不实现业务规则，只协调domain对象
//! 3. **事务边界**: 在这里使用事务
//! 4. **依赖注入**: 通过构造函数注入Repository
//! 
//! ## 使用示例
//! 
//! ```rust,ignore
//! use rust_quant_services::strategy::StrategyConfigService;
//! 
//! // 创建服务
//! let service = StrategyConfigService::new().await;
//! 
//! // 加载配置
//! let configs = service.load_configs("BTC-USDT", "1H", Some("vegas")).await?;
//! 
//! // 启动策略
//! service.start_strategy(config_id).await?;
//! ```

pub mod strategy;
pub mod trading;
pub mod market;
pub mod risk;

// 重新导出常用服务
pub use strategy::StrategyConfigService;

