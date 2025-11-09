# src/ 到 crates/ 迁移优先级计划

**创建时间**: 2025-11-08  
**策略**: 优先迁移src/中已有的功能到新DDD架构

---

## 🎯 核心发现

### src/ 目录现状

**存在大量完整实现**:
- ✅ `src/trading/task/` - 完整的任务调度（10+个job）
- ✅ `src/trading/strategy/` - 完整的策略实现
- ✅ `src/trading/services/` - 完整的服务层
- ✅ `src/trading/indicator/` - 大量技术指标
- ✅ `src/job/` - 风控和任务调度

**技术栈**:
- ❌ 使用rbatis（旧ORM）
- ❌ 直接的数据库操作
- ❌ 未分层的架构

### crates/ 目录现状

**DDD架构完善**:
- ✅ 分层清晰
- ✅ 使用sqlx
- ✅ 依赖正确
- ⏳ 部分功能TODO

---

## 📊 迁移价值分析

### 高价值迁移（立即处理）

| 源文件 | 行数 | 目标位置 | 价值 |
|---|---|---|---|
| task/candles_job.rs | 311行 | orchestration/workflow/ | ⭐⭐⭐ 核心数据同步 |
| task/tickets_job.rs | 57行 | orchestration/workflow/ | ⭐⭐⭐ 市场数据 |
| strategy/vegas_executor.rs | ~300行 | strategies/implementations/ | ⭐⭐⭐ 核心策略 |
| strategy/nwe_executor.rs | ~300行 | strategies/implementations/ | ⭐⭐⭐ 核心策略 |
| job/risk_*.rs | ~200行 | services/risk/ | ⭐⭐⭐ 风控逻辑 |

### 中价值迁移（后续处理）

| 源文件 | 行数 | 目标位置 | 价值 |
|---|---|---|---|
| services/order_service/ | ~150行 | services/trading/ | ⭐⭐ 订单管理 |
| indicator/equal_high_low.rs | ~100行 | indicators/pattern/ | ⭐⭐ 指标补充 |

---

## 🔧 迁移策略

### 策略1: 直接迁移（简单任务）

**适用**: account_job (10行), tickets_job (57行)

**步骤**:
1. 复制src/代码到crates/
2. 替换rbatis为sqlx
3. 更新导入路径
4. 调整为新架构

**预估**: 每个30分钟-1小时

### 策略2: 重构迁移（复杂任务）

**适用**: candles_job (311行), strategy executors

**步骤**:
1. 分析原有逻辑
2. 创建新架构实现
3. 保留核心算法
4. 适配Repository接口
5. 添加单元测试

**预估**: 每个2-3小时

### 策略3: 分步迁移（大型模块）

**适用**: 风控规则, 订单服务

**步骤**:
1. 先迁移核心接口
2. 再迁移业务逻辑
3. 分多个PR完成
4. 逐步测试

**预估**: 4-8小时

---

## 📋 推荐执行顺序（按src/存在优先）

### 第1批：简单任务（2-3小时）✅ 立即开始

**1. account_job迁移** (10行 → ~50行)
- 源: `src/trading/task/account_job.rs`
- 目标: `crates/orchestration/src/workflow/account_job.rs`
- 难度: ⭐ 简单
- 依赖: 无
- **立即执行** ✅

**2. tickets_job迁移** (57行 → ~100行)
- 源: `src/trading/task/tickets_job.rs`
- 目标: `crates/orchestration/src/workflow/tickets_job.rs`
- 难度: ⭐ 简单
- 依赖: 无

### 第2批：中等任务（3-4小时）

**3. candles_job迁移** (311行 → ~200行)
- 源: `src/trading/task/candles_job.rs`
- 目标: `crates/orchestration/src/workflow/candles_job.rs`
- 难度: ⭐⭐ 中等
- 依赖: 需要CandleRepository（已有）

**4. risk_position_job迁移** (~100行)
- 源: `src/job/risk_positon_job.rs`
- 目标: `crates/services/src/risk/position_risk_service.rs`
- 难度: ⭐⭐ 中等
- 依赖: RiskManagementService（已有框架）

### 第3批：复杂任务（6-8小时）

**5. vegas_executor恢复**
- 源: `src/trading/strategy/vegas_executor.rs`
- 目标: `crates/strategies/src/implementations/vegas_executor.rs`
- 难度: ⭐⭐⭐ 复杂
- 依赖: 需要适配新Strategy接口

**6. nwe_executor恢复**
- 源: `src/trading/strategy/nwe_executor.rs`
- 目标: `crates/strategies/src/implementations/nwe_executor.rs`
- 难度: ⭐⭐⭐ 复杂
- 依赖: 需要适配新Strategy接口

---

## ⚠️ 迁移注意事项

### 1. ORM替换

```rust
// ❌ 旧代码（rbatis）
let model = CandlesModel::new().await;
model.insert(&entity).await?;

// ✅ 新代码（sqlx）
use rust_quant_infrastructure::repositories::SqlxCandleRepository;
let repo = SqlxCandleRepository::new(pool);
repo.save(&candle).await?;
```

### 2. 架构适配

```rust
// ❌ 旧代码（直接调用）
CandlesModel::get_list(...).await?;

// ✅ 新代码（通过Repository）
use rust_quant_domain::traits::CandleRepository;
repo.find_candles(...).await?;
```

### 3. 依赖注入

```rust
// ✅ 新架构
pub struct CandlesJob {
    candle_repo: Arc<dyn CandleRepository>,
    market_service: Arc<MarketDataService>,
}
```

---

## 🚀 开始执行

### 立即开始：account_job迁移

**预估时间**: 30分钟  
**价值**: 账户数据同步  
**难度**: ⭐ 简单

开始？

---

**文档生成时间**: 2025-11-08  
**准备状态**: ✅ 分析完成，准备迁移

