# 📋 分阶段迁移执行计划

> 🎯 **策略**: 稳妥推进，逐包修复和验证  
> ⏰ **预计总时长**: 15-20 小时  
> 📅 **开始时间**: 2025-11-06 23:20

---

## 📊 总体规划

### 5 个阶段，每个阶段独立验证

```
阶段 1: strategies 包      (5-6h)  🔴 最复杂
阶段 2: risk 包            (2-3h)  🟡 中等
阶段 3: execution 包       (2-3h)  🟡 中等
阶段 4: orchestration 包   (3-4h)  🟡 中等
阶段 5: cli 包 + 整体验证  (2-3h)  🟢 简单
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
总计:                      15-19h
```

---

## 🎯 阶段 1: 修复 strategies 包（最关键）

> ⏰ **预计时长**: 5-6 小时  
> 🔴 **难度**: 高  
> 🎯 **目标**: strategies 包编译通过

### Step 1.1: 添加缺失依赖（15 分钟）

**任务**: 在 `Cargo.toml` 中添加所有缺失的依赖

**执行**:
```toml
# crates/strategies/Cargo.toml [dependencies]
ta.workspace = true
uuid.workspace = true
futures.workspace = true
futures-util.workspace = true
ndarray.workspace = true
redis.workspace = true
clap.workspace = true
log.workspace = true
```

**验证**:
```bash
cd /Users/mac2/onions/rust_quant
# 验证依赖是否正确添加
cargo tree --package rust-quant-strategies --depth 1
```

---

### Step 1.2: 迁移缺失的核心模块（2 小时）

#### A. 迁移 strategy_common.rs ⭐⭐⭐⭐⭐

**文件**: `src/trading/strategy/strategy_common.rs`

**迁移到**: `crates/strategies/src/framework/strategy_common.rs`

**包含的重要类型**:
```rust
pub struct SignalResult { ... }
pub struct BasicRiskStrategyConfig { ... }
pub trait BackTestAbleStrategyTrait { ... }
pub fn run_test(...) { ... }
pub fn run_back_test_result(...) { ... }
pub fn parse_candle_to_data_item(...) { ... }
```

**验证**:
```bash
# 检查文件是否可编译
cargo check --package rust-quant-strategies 2>&1 | grep "strategy_common"
```

---

#### B. 迁移 order/strategy_config.rs ⭐⭐⭐⭐⭐

**文件**: `src/trading/strategy/order/*.rs`

**迁移到**: `crates/strategies/src/framework/config/`

**包含**:
- `StrategyConfig` - 策略配置核心类型
- `job_scheduler.rs` - 任务调度配置

**验证**:
```bash
cargo check --package rust-quant-strategies 2>&1 | grep "StrategyConfig"
```

---

#### C. 迁移 arc/indicator_values/ ⭐⭐⭐⭐

**文件**: `src/trading/strategy/arc/indicator_values/*.rs`

**迁移到**: `crates/strategies/src/cache/`

**包含**:
- `arc_vegas_indicator_values.rs` - Vegas 指标缓存
- `arc_nwe_indicator_values.rs` - NWE 指标缓存
- `ema_indicator_values.rs` - EMA 缓存

**验证**:
```bash
cargo check --package rust-quant-strategies 2>&1 | grep "arc_vegas\|arc_nwe"
```

---

#### D. 迁移 redis_operations.rs ⭐⭐⭐

**文件**: `src/trading/strategy/redis_operations.rs`

**迁移到**: `crates/strategies/src/implementations/redis_operations.rs`

**验证**:
```bash
cargo check --package rust-quant-strategies 2>&1 | grep "redis_operations"
```

---

### Step 1.3: 更新 indicators 包导出（30 分钟）

**任务**: 让 indicators 的子模块可以被外部访问

**修改文件**: `crates/indicators/src/lib.rs`

**修改前**:
```rust
pub mod trend;
pub mod momentum;
pub mod volatility;
pub mod volume;
pub mod pattern;
```

**修改后**:
```rust
pub mod trend;
pub mod momentum;
pub mod volatility;
pub mod volume;
pub mod pattern;

// 重新导出所有类型
pub use trend::*;
pub use momentum::*;
pub use volatility::*;
pub use volume::*;
pub use pattern::*;
```

**验证**:
```bash
# 检查导出是否正确
cargo doc --package rust-quant-indicators --no-deps
```

---

### Step 1.4: 批量修复循环依赖（1 小时）

**任务**: 将所有 `rust_quant_strategies::` 改为 `crate::`

**执行脚本**:
```bash
#!/bin/bash
# fix_strategies_circular_deps.sh

cd /Users/mac2/onions/rust_quant

find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/rust_quant_strategies::/crate::/g' \
    {} +

echo "✅ 循环依赖已修复"
cargo check --package rust-quant-strategies 2>&1 | head -30
```

**验证**:
```bash
# 检查是否还有循环依赖错误
cargo check --package rust-quant-strategies 2>&1 | grep "rust_quant_strategies::"
```

---

### Step 1.5: 修复所有导入路径（1 小时）

**任务**: 修复剩余的导入路径错误

**批量替换**:
```bash
find crates/strategies/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/crate::CandleItem/rust_quant_common::CandleItem/g' \
    -e 's/time_util::/rust_quant_common::utils::time::/g' \
    -e 's/use log::error/use tracing::error/g' \
    -e 's/use log::info/use tracing::info/g' \
    -e 's/use log::warn/use tracing::warn/g' \
    {} +
```

---

### Step 1.6: 修复模块内部引用（1 小时）

**任务**: 修复 framework 和 implementations 之间的引用

**示例**:
```rust
// ❌ 错误
use super::strategy_trait::{...};  // framework 目录下找不到

// ✅ 修复
use crate::framework::strategy_trait::{...};
```

---

### Step 1.7: 验证和测试（30 分钟）

**验证步骤**:
```bash
# 1. 编译检查
cargo check --package rust-quant-strategies

# 2. 运行测试
cargo test --package rust-quant-strategies

# 3. 生成文档
cargo doc --package rust-quant-strategies --no-deps

# 4. Clippy 检查
cargo clippy --package rust-quant-strategies
```

**验收标准**:
- ✅ 无编译错误
- ✅ 核心策略（Vegas, NWE）可用
- ✅ 基本测试通过

---

## 🎯 阶段 2: 修复 risk 包

> ⏰ **预计时长**: 2-3 小时  
> 🟡 **难度**: 中  
> 🎯 **目标**: risk 包编译通过

### Step 2.1: ORM 迁移 swap_order.rs（1 小时）

**任务**: 将 swap_order.rs 从 rbatis 迁移到 sqlx

**执行步骤**:
1. 移除 `extern crate rbatis;`
2. 添加 `use sqlx::FromRow;`
3. 添加 `#[derive(FromRow)]`
4. 将 `crud!` macro 改为手写方法
5. 将 `select_by_in_order_id` 改为 sqlx 查询

**参考**: `MARKET_PACKAGE_TEST_REPORT.md` 中的 ORM 迁移示例

---

### Step 2.2: ORM 迁移 swap_orders_detail.rs（1 小时）

**任务**: 类似 swap_order.rs 的迁移

**需要实现的方法**:
- `insert()` - 插入订单详情
- `update_by_map()` - 更新订单
- `get_new_update_order_id()` - 查询最新订单

---

### Step 2.3: 修复导入路径（30 分钟）

**批量替换**:
```bash
find crates/risk/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/time_util::/rust_quant_common::utils::time::/g' \
    -e 's/rust_quant_core::error::app_error::AppError/rust_quant_core::error::AppError/g' \
    {} +
```

---

### Step 2.4: 添加缺失依赖（10 分钟）

**Cargo.toml 添加**:
```toml
futures.workspace = true
```

---

### Step 2.5: 验证和测试（30 分钟）

```bash
cargo check --package rust-quant-risk
cargo test --package rust-quant-risk
cargo clippy --package rust-quant-risk
```

---

## 🎯 阶段 3: 修复 execution 包

> ⏰ **预计时长**: 2-3 小时  
> 🟡 **难度**: 中  
> 🎯 **目标**: execution 包编译通过

### Step 3.1: 检查 ORM 使用情况（30 分钟）

**任务**: 确定哪些文件使用了 rbatis

```bash
grep -r "rbatis\|RBatis" crates/execution/src/
```

---

### Step 3.2: ORM 迁移（1-1.5 小时）

**根据 Step 3.1 的结果**:
- 如果使用 rbatis，按 market 包的模式迁移
- 如果不使用，只需修复导入路径

---

### Step 3.3: 修复导入路径（30 分钟）

```bash
find crates/execution/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/crate::trading::/rust_quant_/g' \
    {} +
```

---

### Step 3.4: 验证和测试（30 分钟）

```bash
cargo check --package rust-quant-execution
cargo test --package rust-quant-execution
```

---

## 🎯 阶段 4: 修复 orchestration 包

> ⏰ **预计时长**: 3-4 小时  
> 🟡 **难度**: 中  
> 🎯 **目标**: orchestration 包编译通过

### Step 4.1: 检查 ORM 使用情况（30 分钟）

**任务**: 确定哪些 job 文件使用了 rbatis

```bash
grep -r "rbatis\|RBatis" crates/orchestration/src/
```

---

### Step 4.2: ORM 迁移（1-2 小时）

**根据检查结果迁移**

---

### Step 4.3: 批量修复导入路径（1 小时）

```bash
find crates/orchestration/src -name "*.rs" -type f -exec sed -i '' \
    -e 's/crate::trading::/rust_quant_/g' \
    -e 's/crate::job::/crate::workflow::/g' \
    {} +
```

---

### Step 4.4: 验证和测试（1 小时）

```bash
cargo check --package rust-quant-orchestration
cargo test --package rust-quant-orchestration
```

---

## 🎯 阶段 5: cli 包 + 整体验证

> ⏰ **预计时长**: 2-3 小时  
> 🟢 **难度**: 低  
> 🎯 **目标**: 整个 workspace 编译通过

### Step 5.1: 修复 cli 包（1 小时）

**任务**: 更新 cli 包的导入和配置

---

### Step 5.2: 整体编译验证（30 分钟）

```bash
cargo check --workspace
cargo build --workspace
```

---

### Step 5.3: 运行所有测试（1 小时）

```bash
cargo test --workspace
```

---

### Step 5.4: 生成文档（30 分钟）

```bash
cargo doc --workspace --no-deps --open
```

---

## 📋 详细执行清单

### 阶段 1: strategies 包

- [ ] Step 1.1: 添加依赖 (ta, uuid, futures, etc.)
- [ ] Step 1.2: 迁移 strategy_common.rs
- [ ] Step 1.3: 迁移 order/strategy_config.rs
- [ ] Step 1.4: 迁移 arc/indicator_values/
- [ ] Step 1.5: 迁移 redis_operations.rs
- [ ] Step 1.6: 更新 indicators 包导出
- [ ] Step 1.7: 批量修复循环依赖
- [ ] Step 1.8: 修复 CandleItem 导入
- [ ] Step 1.9: 修复 time_util 导入
- [ ] Step 1.10: 修复 log → tracing
- [ ] Step 1.11: 修复模块内部引用
- [ ] Step 1.12: 验证编译
- [ ] Step 1.13: 运行测试
- [ ] Step 1.14: 提交代码

### 阶段 2: risk 包

- [ ] Step 2.1: ORM 迁移 swap_order.rs
- [ ] Step 2.2: ORM 迁移 swap_orders_detail.rs
- [ ] Step 2.3: 修复导入路径
- [ ] Step 2.4: 添加 futures 依赖
- [ ] Step 2.5: 验证编译
- [ ] Step 2.6: 运行测试
- [ ] Step 2.7: 提交代码

### 阶段 3: execution 包

- [ ] Step 3.1: 检查 ORM 使用
- [ ] Step 3.2: ORM 迁移（如需要）
- [ ] Step 3.3: 修复导入路径
- [ ] Step 3.4: 验证编译
- [ ] Step 3.5: 运行测试
- [ ] Step 3.6: 提交代码

### 阶段 4: orchestration 包

- [ ] Step 4.1: 检查 ORM 使用
- [ ] Step 4.2: ORM 迁移（如需要）
- [ ] Step 4.3: 批量修复导入
- [ ] Step 4.4: 验证编译
- [ ] Step 4.5: 运行测试
- [ ] Step 4.6: 提交代码

### 阶段 5: cli 包 + 整体

- [ ] Step 5.1: 修复 cli 包
- [ ] Step 5.2: 整体编译验证
- [ ] Step 5.3: 运行所有测试
- [ ] Step 5.4: 生成文档
- [ ] Step 5.5: 性能测试
- [ ] Step 5.6: 最终提交

---

## 🔍 每个阶段的验收标准

### 阶段 1 验收
- ✅ rust-quant-strategies 编译通过
- ✅ 核心策略（Vegas, NWE）可用
- ✅ 基本测试通过
- ✅ 无循环依赖
- ✅ Git 提交已完成

### 阶段 2 验收
- ✅ rust-quant-risk 编译通过
- ✅ swap_order 模型可用
- ✅ swap_orders_detail 模型可用
- ✅ Git 提交已完成

### 阶段 3 验收
- ✅ rust-quant-execution 编译通过
- ✅ order_service 可用
- ✅ Git 提交已完成

### 阶段 4 验收
- ✅ rust-quant-orchestration 编译通过
- ✅ 核心 job 可用
- ✅ Git 提交已完成

### 阶段 5 验收
- ✅ rust-quant-cli 编译通过
- ✅ 整个 workspace 编译通过
- ✅ 所有基本测试通过
- ✅ 可以运行主程序
- ✅ 文档生成完整

---

## 📊 进度追踪

### 当前进度
```
阶段 0: 准备工作        ✅ 已完成
阶段 1: strategies 包   🔄 准备开始
阶段 2: risk 包         ⏳ 待开始
阶段 3: execution 包    ⏳ 待开始
阶段 4: orchestration 包 ⏳ 待开始
阶段 5: cli + 整体      ⏳ 待开始
```

---

## 🚀 立即开始 - 阶段 1

**准备好了吗？**

我将开始执行 **阶段 1: 修复 strategies 包**

**第一步**: 添加缺失的依赖

请确认是否开始！

---

*分阶段迁移计划 - 2025-11-06 23:20*  
*开始执行阶段 1...*

