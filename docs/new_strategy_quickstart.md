# 新策略快速参考卡片 🚀

## ✨ 只需 3 步添加新策略！

---

### 📝 Step 1: 创建执行器

**复制模板**: `src/trading/strategy/nwe_executor.rs`  
**重命名**: `src/trading/strategy/{your_strategy}_executor.rs`

**修改点** (仅 5 处)：

```rust
// 1️⃣ 结构名称
pub struct YourStrategyExecutor;  // 👈 改这里

// 2️⃣ name() 方法
fn name(&self) -> &'static str {
    "YourStrategy"  // 👈 改这里
}

// 3️⃣ strategy_type() 方法
fn strategy_type(&self) -> StrategyType {
    StrategyType::YourStrategy  // 👈 改这里
}

// 4️⃣ can_handle() 方法
fn can_handle(&self, strategy_config: &str) -> bool {
    serde_json::from_str::<YourStrategyConfig>(strategy_config).is_ok()  // 👈 改这里
}

// 5️⃣ 导入和策略逻辑
use crate::trading::strategy::your_strategy::{
    YourStrategy, YourStrategyConfig, YourSignalValues,  // 👈 改这里
};
// ... 其余逻辑保持模板结构
```

---

### 🔌 Step 2: 注册策略（1 行）

**文件**: `src/trading/strategy/strategy_registry.rs`

找到 `initialize_registry()` 函数，添加：

```rust
fn initialize_registry() -> StrategyRegistry {
    use super::vegas_executor::VegasStrategyExecutor;
    use super::nwe_executor::NweStrategyExecutor;
    use super::your_strategy_executor::YourStrategyExecutor;  // 👈 1. 导入
    
    let registry = StrategyRegistry::new();
    
    registry.register(Arc::new(VegasStrategyExecutor::new()));
    registry.register(Arc::new(NweStrategyExecutor::new()));
    registry.register(Arc::new(YourStrategyExecutor::new()));  // 👈 2. 注册
    
    registry
}
```

---

### 📦 Step 3: 导出模块（1 行）

**文件**: `src/trading/strategy/mod.rs`

```rust
// 🆕 策略可扩展性框架
pub mod strategy_trait;
pub mod strategy_registry;
pub mod vegas_executor;
pub mod nwe_executor;
pub mod your_strategy_executor;  // 👈 添加这一行
```

---

## ✅ 完成！

运行 `cargo build` 编译即可！

---

## 🔧 前置准备清单

添加执行器前，确保已完成：

- [ ] 策略配置结构: `YourStrategyConfig`
- [ ] 策略实现: `YourStrategy`
- [ ] 指标组合: `YourIndicatorCombine`
- [ ] 指标组合的 `next()` 方法
- [ ] 指标缓存管理器: `arc_your_indicator_values.rs`
- [ ] 在 `StrategyType` 枚举添加变体
- [ ] 在 `StrategyType::from_str()` 添加映射
- [ ] 在 `StrategyType::as_str()` 添加映射

---

## 📖 完整文档

详细说明请查看: `docs/how_to_add_new_strategy.md`

---

## 🆚 对比

| 操作 | 旧架构 | 新架构 |
|------|--------|--------|
| 修改文件数 | 6+ | 3 |
| 代码行数 | 300+ | 50+ |
| 修改核心文件 | ✅ 需要 | ❌ 不需要 |
| 容易出错 | ⚠️ 高 | ✅ 低 |
| 学习曲线 | 陡峭 | 平缓 |

**工作量减少 85%！** 🎉

---

**版本**: v1.0  
**最后更新**: 2025-10-28

