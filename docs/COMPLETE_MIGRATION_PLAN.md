# 🎯 完整迁移执行计划

> 📅 **开始时间**: 2025-11-07  
> 🎯 **目标**: 零技术债务的完整迁移  
> ⏱️ **预计时长**: 13-18小时

---

## 📋 执行策略

### 核心原则
1. **系统化批量处理** - 不逐个文件修复，而是批量模式识别和替换
2. **优先级导向** - 先解决阻塞性问题，再优化细节
3. **增量验证** - 每个阶段完成后立即验证编译

---

## Phase 1: 快速修复 strategies 包 (2-3h)

### Step 1.1: 批量修复导入路径 ✅

**策略**: 使用 sed 批量替换常见错误模式

```bash
# 替换 crate::trading → 正确路径
find crates/strategies/src -name "*.rs" -exec sed -i '' \
    -e 's/use crate::trading::model::/use rust_quant_common::types::/g' \
    -e 's/use crate::trading::services::/use crate::framework::/g' \
    -e 's/use crate::arc::/use rust_quant_infrastructure::cache::/g' \
    {} +
```

### Step 1.2: 修复 indicators 导入 ✅

**问题映射**:
```
vegas_indicator → momentum::vegas 或 trend::vegas
kdj_simple_indicator → momentum::kdj
macd_simple_indicator → momentum::macd
rsi_rma_indicator → momentum::rsi
atr_stop_loos → volatility::atr
```

### Step 1.3: 迁移缺失模块 ✅

**需要迁移的文件**:
1. `strategy_metrics.rs` → `strategies/framework/metrics.rs` ✅
2. `strategy_system_error.rs` → `strategies/framework/error.rs`
3. 其他缺失的类型定义

### Step 1.4: 临时方案 ✅

对于复杂依赖：
- 使用 `#[cfg(feature = "todo")]` 标记暂时无法编译的代码
- 添加 TODO 注释说明后续处理计划

---

## Phase 2: Risk 包 ORM 迁移 (1.5-2h)

参考 market 包的成功经验：

### 文件清单
1. `swap_order.rs` - rbatis → sqlx ✅
2. `swap_orders_detail.rs` - rbatis → sqlx ✅

### 迁移模式
```rust
// Before (rbatis)
crud!(SwapOrderEntity {}, "swap_order");

// After (sqlx)
#[derive(FromRow)]
struct SwapOrderEntity { ... }

impl SwapOrderEntity {
    async fn insert(&self) -> Result<u64> {
        sqlx::query("INSERT INTO ...").execute(pool).await
    }
}
```

---

## Phase 3: Execution 包迁移 (1.5-2h)

### 检查项
1. 是否使用 rbatis？
2. 导入路径是否正确？
3. 依赖关系是否合理？

---

## Phase 4: Orchestration 包重构 (2-3h)

### 重点工作
1. 移除对 strategies 的直接依赖
2. 通过 domain 接口交互
3. ORM 迁移（如需要）

---

## Phase 5: 整体验证 (2-3h)

### 验证清单
- [ ] 所有包编译通过
- [ ] 核心测试通过
- [ ] 无循环依赖
- [ ] 文档完整

---

## 当前状态

**Phase 1**: 进行中 (30%)
**Overall**: 开始执行

---

*完整迁移计划 - 执行中*

