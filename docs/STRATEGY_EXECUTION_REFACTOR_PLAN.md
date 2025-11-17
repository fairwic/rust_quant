# 策略执行流程重构计划

## 当前实现状态

### 现有业务逻辑（rust_quant_old）

#### 完整流程

```
1. 策略执行器（vegas_executor.rs / nwe_executor.rs）
   ├── 获取K线数据
   ├── 计算指标
   ├── 生成信号：strategy.get_trade_signal() → SignalResult
   └── 调用 execute_order(signal)

2. execute_order (executor_common.rs)
   ├── 检查信号有效性
   ├── 记录信号日志（异步，不阻塞）
   ├── 解析风险配置
   └── 调用 SwapOrderService.ready_to_order()

3. SwapOrderService.ready_to_order (swap_order_service.rs)
   ├── 幂等性检查（避免重复下单）
   ├── 获取当前持仓
   ├── 计算可用资金
   ├── 计算下单数量
   ├── 风控检查
   └── 实际下单到交易所
```

---

### 新架构当前状态

#### 问题分析

```rust
// ❌ 当前问题：策略执行器内部直接调用了execute_order
// crates/strategies/src/implementations/vegas_executor.rs:180-188

async fn execute(...) -> Result<()> {  // 返回()，不返回SignalResult
    // 生成信号
    let signal_result = vegas_strategy.get_trade_signal(...);
    
    // 直接下单（违反分层原则）
    execute_order(&StrategyType::Vegas, ..., &signal_result, ...).await?;
    
    Ok(())  // 信号丢失，外部无法获取
}
```

**问题**：
1. 策略层（strategies）直接调用了下单逻辑（violates separation of concerns）
2. SignalResult没有返回给services层
3. services层无法做统一的风控和订单管理

---

## 重构方案

### 方案A：修改策略执行器返回类型 ⭐ 推荐

#### 修改策略trait

```rust
// crates/strategies/src/framework/strategy_trait.rs
#[async_trait]
pub trait StrategyExecutor: Send + Sync {
    // ❌ 当前
    async fn execute(...) -> Result<()>;
    
    // ✅ 修改为
    async fn execute(...) -> Result<SignalResult>;
}
```

#### 修改各个策略实现

```rust
// crates/strategies/src/implementations/vegas_executor.rs
async fn execute(...) -> Result<SignalResult> {
    // 1-8. 获取数据、计算指标（保持不变）
    ...
    
    // 9. 生成信号
    let signal_result = vegas_strategy.get_trade_signal(...);
    
    // 10. 返回信号（不再调用execute_order）
    Ok(signal_result)  // ✅ 返回信号给services层
}
```

#### Services层处理信号和下单

```rust
// crates/services/src/strategy/strategy_execution_service.rs
pub async fn execute_strategy(...) -> Result<SignalResult> {
    // 1. 执行策略，获取信号
    let signal = strategy_executor.execute(...).await?;
    
    // 2. 检查信号
    if !has_signal(&signal) {
        return Ok(signal);
    }
    
    // 3. 记录信号日志（异步）
    self.save_signal_log_async(..., &signal);
    
    // 4. 执行下单
    self.execute_order_internal(..., &signal).await?;
    
    Ok(signal)
}
```

**优点**：
- ✅ 清晰的职责分离
- ✅ services层统一处理信号和下单
- ✅ 易于测试

**缺点**：
- ❌ 需要修改所有策略实现（vegas, nwe, squeeze等）

---

### 方案B：通过Context传递信号（临时方案）

#### 扩展ExecutionContext

```rust
// 在StrategyExecutionContext中添加信号存储
pub trait StrategyExecutionContext {
    fn state_manager(&self) -> &dyn ExecutionStateManager;
    fn time_checker(&self) -> &dyn TimeChecker;
    fn signal_logger(&self) -> &dyn SignalLogger;
    
    // ⭐ 新增：存储最后一个信号
    fn set_last_signal(&self, signal: SignalResult);
    fn get_last_signal(&self) -> Option<SignalResult>;
}
```

#### 策略执行器设置信号

```rust
async fn execute(..., context: &dyn StrategyExecutionContext) -> Result<()> {
    let signal = vegas_strategy.get_trade_signal(...);
    
    // 存储信号到context
    context.set_last_signal(signal.clone());
    
    // 调用execute_order（保持兼容）
    execute_order(..., &signal, ...).await?;
    
    Ok(())
}
```

#### Services层获取信号

```rust
pub async fn execute_strategy(...) -> Result<SignalResult> {
    let context = create_execution_context();
    
    // 执行策略
    strategy_executor.execute(..., &context).await?;
    
    // 从context获取信号
    let signal = context.get_last_signal()
        .unwrap_or_else(|| SignalResult::empty());
    
    Ok(signal)
}
```

**优点**：
- ✅ 最小改动
- ✅ 向后兼容

**缺点**：
- ❌ 设计不够清晰
- ❌ 使用了可变的context（不够优雅）

---

### 方案C：当前临时方案（已实现）

#### 保持现状

```rust
// crates/services/src/strategy/strategy_execution_service.rs
pub async fn execute_strategy(...) -> Result<SignalResult> {
    // 1. 执行策略（内部包含下单）
    strategy_executor.execute(inst_id, period, config, snap).await?;
    
    // 2. 返回空信号（TODO标记）
    let signal = SignalResult::empty();
    Ok(signal)
}
```

**特点**：
- ✅ 快速实现，编译通过
- ✅ 保留原有业务逻辑（strategies层仍然下单）
- ⚠️  TODO标记清晰，等待后续重构

---

## 推荐实施路径

### Phase 1：当前状态（已完成）✅

```
orchestration → services → strategies (内部包含execute_order下单)
```

- 架构基本符合DDD
- 策略执行逻辑保留在strategies层
- TODO标记清晰

### Phase 2：解耦信号生成和下单（推荐）

#### 步骤1：修改策略trait返回SignalResult

```rust
// 修改trait定义
async fn execute(...) -> Result<SignalResult>;
```

#### 步骤2：修改所有策略实现

```rust
// Vegas策略
async fn execute(...) -> Result<SignalResult> {
    let signal = vegas_strategy.get_trade_signal(...);
    // 不再调用execute_order
    Ok(signal)  // 直接返回信号
}

// Nwe策略
async fn execute(...) -> Result<SignalResult> {
    let signal = nwe_strategy.get_trade_signal(...);
    Ok(signal)
}

// 其他策略类似...
```

#### 步骤3：Services层统一处理

```rust
pub async fn execute_strategy(...) -> Result<SignalResult> {
    // 1. 执行策略，获取信号
    let signal = strategy_executor.execute(...).await?;
    
    // 2. 记录信号
    self.save_signal_log_async(..., &signal);
    
    // 3. 下单
    self.execute_order(..., &signal).await?;
    
    Ok(signal)
}
```

---

## 详细实现参考

### 信号日志记录

**原始实现**：`rust_quant_old/src/trading/task/strategy_runner.rs:641-669`

```rust
pub fn save_signal_log(inst_id: &str, period: &str, signal_result: &SignalResult) {
    // 异步记录，不阻塞下单
    let signal_json = serde_json::to_string(&signal_result).unwrap_or_else(|e| {
        error!("序列化信号失败: {}", e);
        format!("{:?}", signal_result)
    });
    
    let signal_record = StrategyJobSignalLog {
        inst_id: inst_id.to_string(),
        time: period.to_string(),
        strategy_type: StrategyType::Vegas.as_str().to_owned(),
        strategy_result: signal_json,
    };
    
    tokio::spawn(async move {
        StrategyJobSignalLogModel::new()
            .await
            .add(signal_record)
            .await
            .map_err(|e| error!("写入信号日志失败: {}", e))
            .ok();
    });
}
```

**新架构需要**：
- 创建 `infrastructure/repositories/signal_log_repository.rs`
- 创建数据库表：`strategy_signal_log`

---

### 下单流程

**原始实现**：`rust_quant_old/src/trading/strategy/executor_common.rs:99-153`

```rust
pub async fn execute_order(
    strategy_type: &StrategyType,
    inst_id: &str,
    period: &str,
    signal_result: &SignalResult,
    strategy_config: &StrategyConfig,
) -> Result<()> {
    // 1. 检查信号
    if !signal_result.should_buy && !signal_result.should_sell {
        return Ok(());
    }
    
    // 2. 记录日志
    save_signal_log(inst_id, period, signal_result);
    
    // 3. 解析风险配置
    let risk_config: BasicRiskStrategyConfig = 
        serde_json::from_str(&strategy_config.risk_config)?;
    
    // 4. 下单
    SwapOrderService::new()
        .ready_to_order(strategy_type, inst_id, period, signal_result, &risk_config, config_id)
        .await?;
    
    Ok(())
}
```

**新架构需要**：
- 实现 `services/trading/order_service.rs`
- 实现幂等性检查
- 实现资金计算
- 集成OKX下单API

---

## 数据库表结构

### 信号日志表

```sql
CREATE TABLE strategy_signal_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    inst_id VARCHAR(50) NOT NULL,
    period VARCHAR(10) NOT NULL,
    strategy_type VARCHAR(50) NOT NULL,
    signal_result TEXT,  -- JSON格式
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_inst_period (inst_id, period),
    INDEX idx_created_at (created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
```

---

## 当前TODO清单

### Services层（strategy_execution_service.rs）

- [x] 基本框架完成
- [x] 调用策略执行器
- [x] TODO标记完整业务逻辑
- [ ] 实现 save_signal_log_async（需要SignalLogRepository）
- [ ] 实现 execute_order_internal（需要OrderService、RiskService）
- [ ] 完整的风控检查
- [ ] 完整的订单创建

### Strategies层

- [x] execute_order函数存在（临时在strategies层）
- [ ] 重构：将execute_order移至services层
- [ ] 重构：修改execute返回SignalResult
- [ ] 重构：strategies层只负责信号生成

### Infrastructure层

- [ ] 创建 SignalLogRepository
- [ ] 创建数据库表 strategy_signal_log

### Execution层

- [ ] 实现 OrderService
- [ ] 实现幂等性检查
- [ ] 实现资金计算
- [ ] 集成OKX下单API

---

## 总结

### 当前状态 ✅

- 架构框架完整
- 策略能够执行（内部包含下单）
- 编译通过
- TODO标记清晰

### 下一步重构

优先级顺序：
1. **Phase 1**（当前）：保持现状，strategies层内部下单
2. **Phase 2**：修改策略trait返回SignalResult
3. **Phase 3**：Services层统一处理信号和下单
4. **Phase 4**：完整实现信号日志、风控、订单服务

遵循**渐进式重构原则**：每个Phase都能编译通过并正常运行。

