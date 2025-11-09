# TODO优先级分析（基于src/对比）

**分析时间**: 2025-11-08  
**策略**: 优先实现src/中已有的功能

---

## 分析方法

对比 `crates/` 中的TODO与 `src/` 中已有实现：
- ✅ **高优先级**: src/中已有实现，可参考或迁移
- 🟡 **中优先级**: src/中部分相关
- ⏳ **低优先级**: src/中没有，是新功能

---

## crates/ vs src/ 对比分析

### 1. 策略相关 TODO

#### crates/services - StrategyExecutionService

**TODO**:
- 实现策略信号返回
- 集成RiskManagementService
- 集成TradingService

**src/对应**:
```
src/trading/task/strategy_runner.rs - ✅ 存在
src/trading/strategy/ - ✅ 完整实现
  - strategy_manager.rs
  - strategy_registry.rs
  - vegas_executor.rs
  - nwe_executor.rs
  - comprehensive_strategy.rs
```

**优先级**: ⭐⭐⭐ **高** - src/中有完整实现

#### crates/strategies - executor依赖

**TODO**:
- 恢复vegas_executor
- 恢复nwe_executor
- 解耦orchestration依赖

**src/对应**:
```
src/trading/strategy/vegas_executor.rs - ✅ 存在
src/trading/strategy/nwe_executor.rs - ✅ 存在
src/trading/strategy/executor_common.rs - ✅ 存在
```

**优先级**: ⭐⭐⭐ **高** - 可直接参考src/实现

### 2. 风控相关 TODO

#### crates/services - RiskManagementService详细规则

**TODO**:
- 持仓限制检查
- 账户风险检查
- 交易频率检查

**src/对应**:
```
src/trading/analysis/position_analysis.rs - ✅ 存在
src/job/risk_positon_job.rs - ✅ 存在
src/job/risk_order_job.rs - ✅ 存在
src/job/risk_banlance_job.rs - ✅ 存在
```

**优先级**: ⭐⭐⭐ **高** - src/中有风控逻辑

### 3. 订单相关 TODO

#### crates/services - OrderCreationService

**TODO**:
- OrderRepository保存
- ExecutionService集成
- 平仓逻辑

**src/对应**:
```
src/trading/services/order_service/ - ✅ 存在
  - order_create_service.rs
  - order_query_service.rs
  - swap_order_service.rs
src/trading/model/order/ - ✅ 存在
```

**优先级**: ⭐⭐⭐ **高** - src/中有订单服务

### 4. 市场数据相关 TODO

#### crates/services - MarketDataService

**TODO**:
- Ticker服务
- 市场深度服务

**src/对应**:
```
src/trading/model/market/ - ✅ 存在
  - tickers.rs
  - tickers_volume.rs
src/trading/services/candle_service/ - ✅ 存在
```

**优先级**: ⭐⭐ **中** - src/中有部分实现

### 5. 数据同步 TODO

#### crates/orchestration - workflow模块

**TODO**:
- 恢复candles_job
- 恢复tickets_job
- 恢复trades_job
- 恢复account_job

**src/对应**:
```
src/trading/task/ - ✅ 完整存在
  - candles_job.rs
  - tickets_job.rs
  - trades_job.rs
  - account_job.rs
  - asset_job.rs
  - big_data_job.rs
```

**优先级**: ⭐⭐⭐ **高** - src/中有完整实现

### 6. 调度器相关 TODO

#### crates/orchestration - scheduler_service

**TODO**:
- 获取实际任务数量

**src/对应**:
```
src/trading/services/scheduler_service.rs - ✅ 存在
src/job/task_scheduler.rs - ✅ 存在
```

**优先级**: ⭐⭐ **中** - src/中有实现

### 7. 指标相关 TODO

#### crates/indicators - equal_high_low等

**TODO**:
- equal_high_low_indicator重构
- IsBigKLineIndicator实现

**src/对应**:
```
src/trading/indicator/equal_high_low_indicator.rs - ✅ 存在
src/trading/indicator/is_big_kline.rs - ✅ 存在
```

**优先级**: ⭐⭐⭐ **高** - src/中有实现，可迁移

### 8. AI相关 TODO

#### crates/ai-analysis

**TODO**:
- GPT-4集成
- 情绪分析
- 事件检测

**src/对应**:
```
src/ - ❌ 不存在
```

**优先级**: ⏳ **低** - 新功能，src/中没有

---

## 优先级总结

### ⭐⭐⭐ 高优先级（src/中已有实现）

| TODO | crates/位置 | src/对应 | 可行性 |
|---|---|---|---|
| 策略executor恢复 | strategies/ | strategy/vegas_executor.rs | ✅ 可迁移 |
| 风控规则实现 | services/risk/ | job/risk_*.rs | ✅ 可参考 |
| 订单服务完善 | services/trading/ | services/order_service/ | ✅ 可迁移 |
| 数据同步任务 | orchestration/workflow/ | task/*.rs | ✅ 可迁移 |
| equal_high_low | indicators/ | indicator/equal_high_low.rs | ✅ 可迁移 |

### ⭐⭐ 中优先级（src/中部分相关）

| TODO | crates/位置 | src/对应 | 说明 |
|---|---|---|---|
| Market服务 | services/market/ | model/market/ | 部分相关 |
| 调度器TODO | orchestration/ | services/scheduler_service.rs | 部分相关 |

### ⏳ 低优先级（src/中不存在）

| TODO | crates/位置 | src/对应 | 说明 |
|---|---|---|---|
| AI功能 | ai-analysis/ | ❌ 无 | 新功能 |
| 测试TODO | 各包tests/ | ❌ 无 | 测试补充 |

---

## 推荐执行顺序

### 第1批：数据同步任务（高价值，高优先级）

**原因**: src/中有完整实现，可直接迁移

1. **candles_job** - K线数据同步
   - src: `src/trading/task/candles_job.rs`
   - target: `crates/orchestration/src/workflow/candles_job.rs`
   - 工作量: 1-2小时

2. **tickets_job** - Ticker数据同步
   - src: `src/trading/task/tickets_job.rs`
   - target: `crates/orchestration/src/workflow/tickets_job.rs`
   - 工作量: 1-2小时

3. **account_job** - 账户数据同步
   - src: `src/trading/task/account_job.rs`
   - target: `crates/orchestration/src/workflow/account_job.rs`
   - 工作量: 1小时

### 第2批：策略executor（高价值，高优先级）

**原因**: 核心功能，src/中有实现

4. **vegas_executor恢复**
   - src: `src/trading/strategy/vegas_executor.rs`
   - target: `crates/strategies/src/implementations/vegas_executor.rs`
   - 工作量: 2-3小时（需要适配新架构）

5. **nwe_executor恢复**
   - src: `src/trading/strategy/nwe_executor.rs`
   - target: `crates/strategies/src/implementations/nwe_executor.rs`
   - 工作量: 2-3小时

### 第3批：风控规则（高价值，高优先级）

**原因**: 核心功能，src/中有逻辑

6. **风控规则实现**
   - src: `src/job/risk_*.rs`, `src/trading/analysis/position_analysis.rs`
   - target: `crates/services/src/risk/risk_management_service.rs`
   - 工作量: 3-4小时

### 第4批：订单服务（中等价值，中优先级）

7. **OrderRepository实现**
   - src: `src/trading/services/order_service/`
   - target: `crates/services/src/trading/`
   - 工作量: 2-3小时

### 第5批：指标迁移（中等价值，中优先级）

8. **equal_high_low迁移**
   - src: `src/trading/indicator/equal_high_low_indicator.rs`
   - target: `crates/indicators/src/pattern/equal_high_low_indicator.rs`
   - 工作量: 1-2小时

---

## 执行建议

### 立即开始（优先级最高）

**第1步: candles_job迁移** (1-2小时)
- 价值: 核心数据同步
- 难度: 低（直接迁移）
- 依赖: 无

开始？

---

**文档生成时间**: 2025-11-08

