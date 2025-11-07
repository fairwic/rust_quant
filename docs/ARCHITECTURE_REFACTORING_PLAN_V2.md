# 架构重构计划 v2.0 - 基于DDD原则的系统化恢复

## 📋 问题诊断

### 1. 循环依赖问题 ❌

**当前状态**:
```
strategies ──X──> orchestration
     ↑                 ↓
     └─────────────────┘
```

**根本原因**: 违反DDD分层原则
- strategies（业务层）不应该依赖 orchestration（编排层）
- orchestration 应该调用 strategies，而非反向

### 2. 职责不清问题 ❌

#### executor_common.rs
**问题**: 混合了多层职责
- ✅ 策略辅助逻辑（应该保留）
- ❌ 订单执行逻辑（属于execution包）
- ❌ 状态管理逻辑（属于orchestration包）

#### vegas_executor / nwe_executor
**问题**: 依赖 `StrategyExecutionStateManager`（属于orchestration）
- 这些executor应该是纯粹的策略数据处理器
- 状态管理应该由orchestration层负责

### 3. 模块位置问题 ❌

#### NweIndicatorCombine
**当前位置**: `strategies/implementations/nwe_strategy/indicator_combine.rs`
**应该位置**: `indicators/trend/nwe_indicator.rs` 或独立模块
**原因**: 指标组合是计算逻辑，不是策略决策逻辑

### 4. 孤儿规则问题 ❌

#### comprehensive_strategy.rs
**问题**: 为外部类型 `CandlesEntity` 实现外部 trait `High`, `Low`, `Close`
**违反**: Rust 孤儿规则
**解决方案**: 
- 选项A: 创建本地wrapper类型
- 选项B: 使用适配器模式
- 选项C: 在CandlesEntity上实现本地trait

---

## 🎯 重构方案

### Phase 1: 打破循环依赖 ⭐

#### 1.1 重构 executor 模块

**目标**: 移除对 orchestration 的依赖

**操作**:
```rust
// 旧设计 (错误)
impl VegasStrategyExecutor {
    async fn execute(&self, state_manager: &StrategyExecutionStateManager) {
        // 使用 orchestration 的状态管理器
    }
}

// 新设计 (正确) - 返回结果，由orchestration处理状态
impl VegasStrategyExecutor {
    async fn execute(&self, config: &StrategyConfig) -> Result<StrategyExecutionResult> {
        // 纯粹的策略执行，返回结果
        // orchestration 层负责处理结果和状态
    }
}
```

**原则**:
- Executor 只负责策略逻辑和信号生成
- 状态管理由 orchestration 负责
- 订单执行由 execution 负责

#### 1.2 重构 executor_common

**拆分策略**:
```
executor_common.rs (当前)
    ↓
    ├─ strategy_helpers.rs   (保留在strategies)
    │   - convert_candles_to_items
    │   - validate_candles
    │   - should_execute_strategy
    │
    ├─ order_helpers.rs      (移到execution包)
    │   - execute_order
    │
    └─ state_helpers.rs      (移到orchestration包)
        - update_candle_queue
        - get_latest_candle
```

### Phase 2: 模块职责重组 ⭐

#### 2.1 移动 NweIndicatorCombine

**从**: `strategies/implementations/nwe_strategy/indicator_combine.rs`
**到**: `indicators/trend/nwe/indicator_combine.rs`

**理由**:
- NweIndicatorCombine 是指标计算组合，不是策略决策
- indicators 包应该包含所有技术指标计算
- strategies 包应该只包含信号生成和决策逻辑

**依赖调整**:
```rust
// strategies/implementations/nwe_strategy/mod.rs
use rust_quant_indicators::trend::nwe::NweIndicatorCombine;
```

#### 2.2 创建 indicators/trend/nwe 模块

**目录结构**:
```
indicators/src/trend/nwe/
├── mod.rs
├── indicator_combine.rs   # 从 strategies 移过来
├── nwe_indicator.rs       # 已存在
└── config.rs              # 配置结构
```

### Phase 3: 解决孤儿规则 ⭐

#### 3.1 为 CandlesEntity 创建适配器

**新文件**: `strategies/src/adapters/candle_adapter.rs`

```rust
use rust_quant_market::models::CandlesEntity;
use ta::{High, Low, Close};

/// CandlesEntity的适配器，用于实现ta库的trait
pub struct CandleAdapter<'a>(&'a CandlesEntity);

impl<'a> High for CandleAdapter<'a> {
    fn high(&self) -> f64 {
        self.0.h.parse().unwrap_or(0.0)
    }
}

impl<'a> Low for CandleAdapter<'a> {
    fn low(&self) -> f64 {
        self.0.l.parse().unwrap_or(0.0)
    }
}

impl<'a> Close for CandleAdapter<'a> {
    fn close(&self) -> f64 {
        self.0.c.parse().unwrap_or(0.0)
    }
}

// 便捷函数
pub fn adapt(candle: &CandlesEntity) -> CandleAdapter {
    CandleAdapter(candle)
}
```

**使用**:
```rust
use crate::adapters::candle_adapter;

// 旧方式 (违反孤儿规则)
let high = candle.high();

// 新方式 (正确)
let high = candle_adapter::adapt(&candle).high();
```

### Phase 4: framework 模块清理 ⭐

#### 4.1 移除不属于 strategies 的逻辑

**保留** (策略核心):
- `strategy_trait.rs` - 策略接口定义
- `strategy_registry.rs` - 策略注册表
- `strategy_common.rs` - 策略通用函数
- `config/` - 策略配置

**移除/移动**:
- `scheduler_service` → 移到 orchestration
- `strategy_data_service` → 移到 orchestration  
- `big_data/*` → 移到 orchestration 或独立包
- `strategy_system_error` → 可能冗余，检查后删除

#### 4.2 重构 strategy_manager

**当前问题**: 
- 包含调度逻辑（应该在orchestration）
- 包含数据服务逻辑（应该在infrastructure）
- 类型不匹配（risk_config: String vs Value）

**解决方案**:
```rust
// strategy_manager.rs - 简化为纯粹的策略管理
pub struct StrategyManager {
    registry: StrategyRegistry,
    configs: DashMap<String, StrategyConfig>,
}

impl StrategyManager {
    // 只保留策略管理相关的方法
    pub fn register_strategy(&self, executor: Arc<dyn StrategyExecutor>) { }
    pub fn get_strategy(&self, name: &str) -> Option<Arc<dyn StrategyExecutor>> { }
    pub fn list_strategies(&self) -> Vec<String> { }
}
```

---

## 📊 重构执行计划

### 阶段划分

#### 🔹 Phase 1: 打破循环依赖 (2-3小时)
- [ ] 1.1 创建 strategies/src/adapters/ 模块
- [ ] 1.2 创建 CandleAdapter 解决孤儿规则
- [ ] 1.3 修改 comprehensive_strategy 使用适配器
- [ ] 1.4 创建 strategy_helpers.rs (从executor_common拆分)
- [ ] 1.5 移除 executor 对 orchestration 的引用

#### 🔹 Phase 2: 模块重组 (2-3小时)
- [ ] 2.1 创建 indicators/src/trend/nwe/ 目录
- [ ] 2.2 移动 NweIndicatorCombine 到 indicators
- [ ] 2.3 移动相关配置和类型
- [ ] 2.4 更新 strategies 中的导入

#### 🔹 Phase 3: 恢复功能 (2-3小时)
- [ ] 3.1 恢复 vegas_executor (不依赖orchestration)
- [ ] 3.2 恢复 nwe_executor (不依赖orchestration)
- [ ] 3.3 恢复 comprehensive_strategy (使用适配器)
- [ ] 3.4 恢复 mult_combine_strategy
- [ ] 3.5 恢复 top_contract_strategy

#### 🔹 Phase 4: 清理和优化 (1-2小时)
- [ ] 4.1 清理 framework 模块
- [ ] 4.2 修复 strategy_manager 类型问题
- [ ] 4.3 移除冗余代码
- [ ] 4.4 更新导入和导出

#### 🔹 Phase 5: 验证 (1小时)
- [ ] 5.1 编译所有包
- [ ] 5.2 运行测试
- [ ] 5.3 验证依赖关系正确性
- [ ] 5.4 更新文档

---

## ✅ 成功标准

### 架构质量
- ✅ 无循环依赖
- ✅ 严格遵守分层原则
- ✅ 职责单一清晰
- ✅ 符合DDD原则

### 编译质量
- ✅ 所有包编译通过 (0 errors)
- ✅ 只有允许的警告（deprecated等）
- ✅ clippy 通过

### 功能完整性
- ✅ 所有策略可用
- ✅ 所有执行器可用
- ✅ 框架功能完整

---

## 🚀 开始执行

**预计总时间**: 8-12小时
**并发策略**: 可并行处理Phase 1和Phase 2的部分工作

**立即开始**: Phase 1 - 打破循环依赖


