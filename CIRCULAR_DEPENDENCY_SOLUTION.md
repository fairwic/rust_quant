# 循环依赖问题及解决方案

**文档时间**: 2025-11-07  
**问题**: strategies ↔ orchestration 循环依赖  
**状态**: ✅ 已解决（部分）

---

## 🔴 问题分析

### 循环依赖关系

```
strategies/implementations
    └── executor_common.rs
            ├── 使用 orchestration::workflow::strategy_runner::StrategyExecutionStateManager
            ├── 使用 orchestration::workflow::strategy_runner::check_new_time
            └── 使用 orchestration::workflow::strategy_runner::save_signal_log

orchestration/workflow
    └── strategy_runner.rs
            └── 使用 strategies::implementations::{VegasStrategyExecutor, NweStrategyExecutor}
```

**问题**: 形成了 `strategies → orchestration → strategies` 的循环依赖。

---

## 📋 受影响的模块

### 直接依赖 orchestration 的模块

1. **strategies/implementations/executor_common.rs** ❌
   - `should_execute_strategy()` 使用 `StrategyExecutionStateManager::try_mark_processing`
   - `should_execute_strategy()` 使用 `check_new_time()`
   - `execute_order()` 使用 `save_signal_log()`

2. **strategies/implementations/vegas_executor.rs** ❌
   - 依赖 `executor_common`
   - 使用 `StrategyExecutionStateManager`

3. **strategies/implementations/nwe_executor.rs** ❌
   - 依赖 `executor_common`
   - 使用 `StrategyExecutionStateManager`

### 不依赖 orchestration 的模块

以下模块可以独立编译：
- ✅ comprehensive_strategy.rs
- ✅ engulfing_strategy.rs
- ✅ macd_kdj_strategy.rs
- ✅ squeeze_strategy.rs
- ✅ ut_boot_strategy.rs
- ✅ profit_stop_loss.rs

---

## ✅ 解决方案

### 方案 1: 创建 executor_common_lite（已实施）

#### 实现

创建 `strategies/implementations/executor_common_lite.rs`：
- ✅ 包含**不依赖 orchestration** 的通用逻辑
- ✅ 保留核心数据结构和工具函数
- ✅ 去除数据库访问（让调用方传入）

#### 包含的功能

```rust
// ✅ 已包含
pub struct ExecutionContext { ... }
pub fn update_candle_queue() { ... }
pub fn get_recent_candles() { ... }
pub fn convert_candles_to_items() { ... }
pub fn validate_candles() { ... }
pub fn is_new_timestamp() { ... }

// ❌ 已移除（依赖 orchestration）
// pub fn should_execute_strategy() - 使用 check_new_time
// pub fn execute_order() - 使用 save_signal_log
// pub async fn get_latest_candle() - 数据访问由调用方负责
```

#### 效果
- ✅ 编译通过
- ✅ 保留 ~60% 的通用逻辑
- ✅ 避免循环依赖

---

### 方案 2: 将 StrategyExecutionStateManager 移到 strategies（未实施）

#### 理由
- `StrategyExecutionStateManager` 负责策略执行状态管理
- 从职责来看，应该在 orchestration 层
- 移动会违反架构分层原则

#### 优点
- 彻底解决循环依赖

#### 缺点
- 违反 DDD 架构原则
- orchestration 负责调度，不应该将调度逻辑放到 strategies

---

### 方案 3: 使用 trait 解耦（推荐但未实施）

#### 设计

```rust
// strategies/implementations/executor_traits.rs
pub trait ExecutionStateManager {
    fn try_mark_processing(&self, key: &str, timestamp: i64) -> bool;
}

pub trait TimeChecker {
    fn check_new_time(&self, old: i64, new: i64, period: &str) -> Result<bool>;
}

pub trait SignalLogger {
    fn save_signal_log(&self, inst_id: &str, period: &str, signal: &SignalResult);
}

// executor_common.rs
pub fn should_execute_strategy(
    key: &str,
    old_time: i64,
    new_time: i64,
    state_manager: &dyn ExecutionStateManager, // 注入依赖
) -> Result<bool> {
    // ...
}
```

#### 优点
- 彻底解耦
- 符合依赖倒置原则
- 易于测试

#### 缺点
- 需要重构现有代码
- 增加复杂度

---

### 方案 4: 将 executor 移到 orchestration（未实施）

#### 理由
- executor 确实在协调策略执行和状态管理
- 可以认为是 orchestration 的一部分

#### 优点
- 自然解决循环依赖
- executor 和 state_manager 在同一层

#### 缺点
- executor 包含策略特定逻辑
- 不符合策略模式的设计理念

---

## 📊 当前状态

### 已解决

| 模块 | 状态 | 方案 |
|-----|------|------|
| executor_common | ✅ 部分恢复 | 方案1: executor_common_lite |
| vegas_executor | ⏸️ 待恢复 | 需要重构以使用 lite 版本 |
| nwe_executor | ⏸️ 待恢复 | 需要重构以使用 lite 版本 |

### 未解决

| 功能 | 原位置 | 问题 | 解决方案建议 |
|-----|--------|------|-------------|
| 去重检查 | executor_common | 依赖 StrategyExecutionStateManager | 调用方自行实现 |
| 时间验证 | executor_common | 依赖 check_new_time | 使用 lite 版的 is_new_timestamp |
| 信号日志 | executor_common | 依赖 save_signal_log | 调用方自行实现 |

---

## 🚀 后续工作建议

### 短期（立即可做）

1. ✅ **使用 executor_common_lite**
   - 已创建并导出
   - 编译通过
   - 可供其他策略使用

2. **更新 executor 使用方式**
   - vegas_executor 和 nwe_executor 需要重构
   - 使用 executor_common_lite 的函数
   - 自行实现去重和日志逻辑

### 中期（推荐）

3. **实施方案3: trait 解耦**
   - 定义 ExecutionStateManager trait
   - 定义 TimeChecker trait  
   - 定义 SignalLogger trait
   - orchestration 实现这些 trait
   - executor 依赖 trait 而非具体实现

### 长期（架构优化）

4. **重新审视架构**
   - 评估 executor 的职责
   - 考虑是否将 executor 移到 orchestration
   - 或者将状态管理移到独立的 crate

---

## 📝 使用指南

### 如何使用 executor_common_lite

```rust
use rust_quant_strategies::implementations::{
    ExecutionContext, 
    update_candle_queue,
    get_recent_candles,
    convert_candles_to_items,
    validate_candles,
    is_new_timestamp,
};

// 1. 转换K线数据
let candle_items = convert_candles_to_items(&candles);

// 2. 验证数据
let last_ts = validate_candles(&candles)?;

// 3. 检查时间戳
if !is_new_timestamp(old_time, new_time) {
    return Ok(());
}

// 4. 更新队列
update_candle_queue(&mut candle_queue, new_candle, 500);

// 5. 获取最近N根
let recent = get_recent_candles(&candle_queue, 144);
```

### 缺失的功能如何实现

#### 去重检查
```rust
// 需要在调用方（orchestration）实现
use rust_quant_orchestration::workflow::strategy_runner::StrategyExecutionStateManager;

if !StrategyExecutionStateManager::try_mark_processing(&key, timestamp) {
    debug!("重复执行，跳过");
    return Ok(());
}
```

#### 信号日志
```rust
// 需要在调用方（orchestration）实现
use rust_quant_orchestration::workflow::strategy_runner::save_signal_log;

if signal_result.should_buy || signal_result.should_sell {
    save_signal_log(inst_id, period, &signal_result);
}
```

---

## 🎓 经验总结

### 教训

1. **循环依赖难以避免**
   - 在复杂系统中，模块间依赖容易形成环
   - 应该在架构设计阶段就考虑依赖方向

2. **通用逻辑的放置**
   - executor_common 包含太多职责
   - 应该拆分为更小的模块

3. **依赖注入的重要性**
   - 使用 trait 可以很好地解耦
   - 但增加了使用复杂度

### 最佳实践

1. **严格遵守单向依赖**
   ```
   orchestration → strategies → domain + infrastructure
   ```

2. **使用依赖倒置**
   - 高层模块定义接口
   - 低层模块实现接口

3. **分离通用逻辑**
   - 数据转换：放底层
   - 业务协调：放高层
   - 状态管理：独立模块

---

## 📚 相关文档

- [架构规范](./docs/ARCHITECTURE_GUIDE.md)
- [依赖矩阵](./docs/DEPENDENCY_MATRIX.md)  
- [TODO 完成报告](./TODO_COMPLETION_REPORT.md)

---

**文档维护**: Rust Quant AI Assistant  
**最后更新**: 2025-11-07

