# 策略可扩展性架构设计

## 📊 当前问题分析

### 新增策略需要修改的地方（7处）

| # | 文件 | 位置 | 操作 |
|---|------|------|------|
| 1 | `strategy/arc/indicator_values/` | 新建文件 | 创建 `arc_xxx_indicator_values.rs` |
| 2 | `indicator_values/mod.rs` | 导出模块 | 添加 `pub mod arc_xxx_indicator_values;` |
| 3 | `strategy_runner.rs` | import | 添加导入语句 |
| 4 | `strategy_runner.rs` | detect_strategy_type | 添加 match 分支 |
| 5 | `strategy_runner.rs` | run_ready_to_order | 添加 match 分支和 run_xxx_strategy |
| 6 | `strategy_data_service.rs` | import | 添加导入语句 |
| 7 | `strategy_data_service.rs` | initialize_strategy_data | 添加 match 分支和 initialize_xxx_data |

**问题总结**：
- ❌ 代码重复度高（70%以上相似）
- ❌ 新增策略需要修改多个文件
- ❌ 容易遗漏某个地方
- ❌ 难以维护和扩展

---

## 🎨 优化方案：Trait + Registry 模式

### 核心思想

**单一职责 + 开闭原则 + 依赖注入**

1. **定义统一接口** - 使用 Trait 定义策略行为
2. **策略注册中心** - 使用 Registry 管理所有策略
3. **工厂模式** - 动态创建策略实例
4. **类型擦除** - 使用 trait object 避免泛型传播

### 架构对比

#### 当前架构 ❌
```
if strategy_type == "Vegas" => run_vegas_strategy()
if strategy_type == "Nwe"   => run_nwe_strategy()
if strategy_type == "XXX"   => run_xxx_strategy()  // 需要修改多处
```

#### 优化后架构 ✅
```
let strategy = StrategyRegistry::get(strategy_type)?;
strategy.run()?;  // 自动调用对应策略，无需修改代码
```

---

## 🏗️ 详细设计

### 1. 定义策略 Trait

```rust
// src/trading/strategy/strategy_trait.rs

use async_trait::async_trait;
use anyhow::Result;
use std::collections::VecDeque;
use crate::trading::model::entity::candles::entity::CandlesEntity;
use crate::trading::strategy::order::strategy_config::StrategyConfig;
use crate::trading::strategy::strategy_common::SignalResult;
use crate::CandleItem;

/// 策略执行接口 - 所有策略必须实现
#[async_trait]
pub trait StrategyExecutor: Send + Sync {
    /// 策略名称（唯一标识）
    fn name(&self) -> &'static str;
    
    /// 策略类型（用于日志）
    fn strategy_type(&self) -> StrategyType;
    
    /// 初始化策略数据
    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandlesEntity>,
    ) -> Result<()>;
    
    /// 执行策略（生成信号）
    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<()>;
    
    /// 检测是否为该策略类型
    fn can_handle(&self, strategy_config: &str) -> bool;
}
```

### 2. 创建策略注册中心

```rust
// src/trading/strategy/strategy_registry.rs

use std::collections::HashMap;
use std::sync::Arc;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use anyhow::{anyhow, Result};
use super::strategy_trait::StrategyExecutor;

/// 策略注册中心 - 管理所有策略实现
pub struct StrategyRegistry {
    strategies: RwLock<HashMap<String, Arc<dyn StrategyExecutor>>>,
}

impl StrategyRegistry {
    pub fn new() -> Self {
        Self {
            strategies: RwLock::new(HashMap::new()),
        }
    }
    
    /// 注册策略（启动时调用一次）
    pub fn register(&self, strategy: Arc<dyn StrategyExecutor>) {
        let name = strategy.name();
        self.strategies.write().insert(name.to_string(), strategy);
        tracing::info!("策略已注册: {}", name);
    }
    
    /// 根据配置自动检测策略类型
    pub fn detect_strategy(&self, strategy_config: &str) -> Result<Arc<dyn StrategyExecutor>> {
        let strategies = self.strategies.read();
        for strategy in strategies.values() {
            if strategy.can_handle(strategy_config) {
                return Ok(strategy.clone());
            }
        }
        Err(anyhow!("未找到匹配的策略类型"))
    }
    
    /// 根据名称获取策略
    pub fn get(&self, name: &str) -> Result<Arc<dyn StrategyExecutor>> {
        self.strategies
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("策略未注册: {}", name))
    }
    
    /// 列出所有已注册策略
    pub fn list_strategies(&self) -> Vec<String> {
        self.strategies.read().keys().cloned().collect()
    }
}

/// 全局策略注册中心
pub static STRATEGY_REGISTRY: Lazy<StrategyRegistry> = Lazy::new(|| {
    let registry = StrategyRegistry::new();
    
    // 自动注册所有策略
    registry.register(Arc::new(VegasStrategyExecutor::new()));
    registry.register(Arc::new(NweStrategyExecutor::new()));
    // 未来新策略只需在此添加一行！
    
    registry
});

/// 获取全局注册中心
pub fn get_strategy_registry() -> &'static StrategyRegistry {
    &STRATEGY_REGISTRY
}
```

### 3. Vegas 策略实现示例

```rust
// src/trading/strategy/vegas_executor.rs

use async_trait::async_trait;
use anyhow::Result;
use super::strategy_trait::StrategyExecutor;
use crate::trading::strategy::arc::indicator_values::arc_vegas_indicator_values;
use crate::trading::indicator::vegas_indicator::VegasStrategy;

pub struct VegasStrategyExecutor;

impl VegasStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for VegasStrategyExecutor {
    fn name(&self) -> &'static str {
        "Vegas"
    }
    
    fn strategy_type(&self) -> StrategyType {
        StrategyType::Vegas
    }
    
    fn can_handle(&self, strategy_config: &str) -> bool {
        serde_json::from_str::<VegasStrategy>(strategy_config).is_ok()
    }
    
    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandlesEntity>,
    ) -> Result<()> {
        // 原 initialize_vegas_data 逻辑
        // ...
    }
    
    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<()> {
        // 原 run_vegas_strategy 逻辑
        // ...
    }
}
```

### 4. Nwe 策略实现示例

```rust
// src/trading/strategy/nwe_executor.rs

use async_trait::async_trait;
use anyhow::Result;
use super::strategy_trait::StrategyExecutor;
use crate::trading::strategy::nwe_strategy::NweStrategyConfig;

pub struct NweStrategyExecutor;

impl NweStrategyExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl StrategyExecutor for NweStrategyExecutor {
    fn name(&self) -> &'static str {
        "Nwe"
    }
    
    fn strategy_type(&self) -> StrategyType {
        StrategyType::Nwe
    }
    
    fn can_handle(&self, strategy_config: &str) -> bool {
        serde_json::from_str::<NweStrategyConfig>(strategy_config).is_ok()
    }
    
    async fn initialize_data(
        &self,
        strategy_config: &StrategyConfig,
        inst_id: &str,
        period: &str,
        candles: Vec<CandlesEntity>,
    ) -> Result<()> {
        // 原 initialize_nwe_data 逻辑
        // ...
    }
    
    async fn execute(
        &self,
        inst_id: &str,
        period: &str,
        strategy_config: &StrategyConfig,
        snap: Option<CandlesEntity>,
    ) -> Result<()> {
        // 原 run_nwe_strategy 逻辑
        // ...
    }
}
```

### 5. 简化的执行器

```rust
// src/trading/task/strategy_runner.rs (重构后)

/// 运行准备好的订单函数 - 使用策略注册中心（重构版）
pub async fn run_ready_to_order_with_manager(
    inst_id: &str,
    period: &str,
    strategy: &StrategyConfig,
    snap: Option<CandlesEntity>,
) -> Result<()> {
    // 1. 从注册中心获取策略（自动检测类型）
    let strategy_executor = get_strategy_registry()
        .detect_strategy(&strategy.strategy_config)?;
    
    // 2. 执行策略（无需 match）
    strategy_executor
        .execute(inst_id, period, strategy, snap)
        .await
}
```

### 6. 简化的数据初始化

```rust
// src/trading/services/strategy_data_service.rs (重构后)

pub async fn initialize_strategy_data(
    strategy: &StrategyConfig,
    inst_id: &str,
    time: &str,
) -> Result<StrategyDataSnapshot, StrategyDataError> {
    // 参数验证
    Self::validate_strategy_params(strategy, inst_id, time)?;
    
    // 获取K线数据
    let candles = /* ... */;
    
    // 1. 从注册中心获取策略
    let strategy_executor = get_strategy_registry()
        .detect_strategy(&strategy.strategy_config)
        .map_err(|e| StrategyDataError::ValidationError {
            field: format!("策略类型识别失败: {}", e),
        })?;
    
    // 2. 初始化数据（无需 match）
    strategy_executor
        .initialize_data(strategy, inst_id, time, candles)
        .await
        .map_err(|e| StrategyDataError::DataInitializationFailed {
            reason: format!("策略数据初始化失败: {}", e),
        })?;
    
    // 3. 返回快照
    Ok(StrategyDataSnapshot { /* ... */ })
}
```

---

## 🚀 未来新增策略流程

### 只需 3 步！

#### Step 1: 创建策略执行器（1个文件）

```rust
// src/trading/strategy/my_new_strategy_executor.rs

pub struct MyNewStrategyExecutor;

#[async_trait]
impl StrategyExecutor for MyNewStrategyExecutor {
    fn name(&self) -> &'static str { "MyNew" }
    fn strategy_type(&self) -> StrategyType { StrategyType::MyNew }
    fn can_handle(&self, config: &str) -> bool { /* 检测逻辑 */ }
    async fn initialize_data(&self, ...) -> Result<()> { /* 初始化 */ }
    async fn execute(&self, ...) -> Result<()> { /* 执行 */ }
}
```

#### Step 2: 注册策略（1行代码）

```rust
// src/trading/strategy/strategy_registry.rs

pub static STRATEGY_REGISTRY: Lazy<StrategyRegistry> = Lazy::new(|| {
    let registry = StrategyRegistry::new();
    registry.register(Arc::new(VegasStrategyExecutor::new()));
    registry.register(Arc::new(NweStrategyExecutor::new()));
    registry.register(Arc::new(MyNewStrategyExecutor::new()));  // ✅ 只需这一行！
    registry
});
```

#### Step 3: 完成！

**无需修改其他任何文件！** ✨

---

## 📊 架构对比总结

### 修改工作量对比

| 操作 | 当前架构 | 优化后架构 |
|------|---------|-----------|
| 新增策略文件 | 1个 | 1个 |
| 修改现有文件 | 6个 | 0个 |
| 添加代码行数 | 300+ | 50+ |
| 注册代码 | 无 | 1行 |
| **总工作量** | **高** | **极低** ⭐ |

### 优势对比

| 特性 | 当前架构 | 优化后架构 |
|------|---------|-----------|
| 扩展性 | ❌ 差 | ✅ 优秀 |
| 可维护性 | ⚠️ 中等 | ✅ 优秀 |
| 代码复用 | ❌ 低 | ✅ 高 |
| 错误风险 | ⚠️ 高 | ✅ 低 |
| 测试友好 | ⚠️ 一般 | ✅ 优秀 |
| 插件化 | ❌ 不支持 | ✅ 支持 |

---

## 🎯 进阶优化

### 1. 配置驱动的策略加载

```rust
// config/strategies.toml
[[strategies]]
name = "Vegas"
enabled = true
dll_path = "libvegas_strategy.so"  # 支持动态库

[[strategies]]
name = "Nwe"
enabled = true
dll_path = "libnwe_strategy.so"

[[strategies]]
name = "MyNew"
enabled = true
dll_path = "libmynew_strategy.so"
```

### 2. 策略热重载

```rust
impl StrategyRegistry {
    /// 热重载策略（无需重启）
    pub fn reload_strategy(&self, name: &str) -> Result<()> {
        // 卸载旧策略
        self.strategies.write().remove(name);
        
        // 加载新策略
        let new_strategy = load_strategy_from_config(name)?;
        self.register(new_strategy);
        
        Ok(())
    }
}
```

### 3. 策略版本管理

```rust
pub trait StrategyExecutor {
    fn version(&self) -> &'static str;
    fn compatible_versions(&self) -> Vec<&'static str>;
}
```

---

## 📝 实施计划

### Phase 1: 基础重构（1-2天）
- [ ] 创建 `strategy_trait.rs`
- [ ] 创建 `strategy_registry.rs`
- [ ] 重构 `VegasStrategyExecutor`
- [ ] 重构 `NweStrategyExecutor`
- [ ] 更新 `strategy_runner.rs`
- [ ] 更新 `strategy_data_service.rs`

### Phase 2: 测试验证（1天）
- [ ] 单元测试
- [ ] 集成测试
- [ ] 回归测试（Vegas/Nwe）

### Phase 3: 文档和示例（0.5天）
- [ ] 更新开发文档
- [ ] 创建新策略模板
- [ ] 提供示例代码

---

## 🎓 示例：添加 MACD 策略

### 当前方式（需要修改 6 个文件）

```diff
+ src/trading/strategy/arc/indicator_values/arc_macd_indicator_values.rs (300行)
+ src/trading/strategy/arc/indicator_values/mod.rs (1行)
+ src/trading/task/strategy_runner.rs (150行)
+ src/trading/services/strategy_data_service.rs (80行)
```

### 优化后方式（只需 1 个文件 + 1 行注册）

```rust
// src/trading/strategy/macd_executor.rs (50行)
pub struct MacdStrategyExecutor;

#[async_trait]
impl StrategyExecutor for MacdStrategyExecutor {
    // 实现接口...
}

// src/trading/strategy/strategy_registry.rs (1行)
registry.register(Arc::new(MacdStrategyExecutor::new()));
```

**工作量减少 85%！** 🎉

---

## 💡 建议

### 当前情况评估

**优先级**: 🟡 中等

**建议**: 
- ✅ **短期（1-2个策略）**: 保持当前架构即可
- ⭐ **中期（3-5个策略）**: 建议重构为 Trait 架构
- 🚀 **长期（5+个策略）**: 必须重构，否则维护成本爆炸

### 渐进式重构

不需要一次性重构，可以：
1. 先实现 Trait 和 Registry 框架
2. 保留现有 Vegas/Nwe 代码
3. 新策略使用新框架
4. 逐步迁移旧策略

---

## 🔗 参考资料

- **设计模式**: 策略模式 + 工厂模式 + 注册模式
- **Rust 最佳实践**: Trait Object + Dynamic Dispatch
- **相似项目**: 
  - Actix-web 的中间件系统
  - Tower 的 Service trait
  - Rust Plugin 系统

---

**文档版本**: v1.0  
**作者**: AI Assistant  
**日期**: 2025-10-28

