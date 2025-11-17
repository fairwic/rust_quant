# 架构层职责规范

## 📋 快速决策树

```
Q1: 这是业务逻辑还是基础设施？
├─ 基础设施 → infrastructure 或 core
└─ 业务逻辑 → 继续 Q2

Q2: 是纯粹的领域概念吗？
├─ 是 → domain
└─ 否 → 继续 Q3

Q3: 是哪种业务逻辑？
├─ 市场数据采集/存储 → market
├─ 交易策略/执行/风控 → trading (未来合并 strategies/risk/execution)
├─ 任务编排/调度 → orchestration
└─ 业务流程协调 → services

Q4: 是纯计算工具吗？
├─ 是 → indicators (未来移到 infrastructure)
└─ 否 → 继续 Q5

Q5: 是数据访问吗？
├─ 是 → infrastructure
└─ 否 → 继续 Q6

Q6: 是配置/日志/数据库吗？
├─ 是 → core
└─ 否 → common
```

---

## Layer 1: Application Layer（应用层）

### 职责
- 程序入口和启动逻辑
- 任务调度和编排
- 工作流管理
- 事件驱动协调

### 包含的包
- `rust-quant-cli`
- `orchestration`

### ✅ 允许做的事情
- 调用领域服务（`market`, `trading`）
- 任务调度和定时任务
- 工作流编排
- 事件发布和订阅

### ❌ 禁止做的事情
- 直接操作数据库
- 实现业务逻辑
- 直接调用基础设施层
- 包含领域模型定义

### 代码示例

```rust
// ✅ 正确：调用领域服务
use rust_quant_market::CandleService;
use rust_quant_trading::StrategyService;

pub async fn run_strategy() {
    let candle_service = CandleService::new();
    let strategy_service = StrategyService::new();
    // ...
}

// ❌ 错误：直接操作数据库
use sqlx::PgPool;
pub async fn get_candles() {
    sqlx::query("SELECT * FROM candles").fetch_all(&pool).await?;
}
```

---

## Layer 2: Domain Layer（领域层）

### 2.1 `domain` - 核心领域模型

#### 职责
- 定义业务实体（Entities）
- 定义值对象（Value Objects）
- 定义领域接口（Traits）
- 定义业务枚举（Enums）

#### ✅ 允许做的事情
- 定义业务模型
- 业务规则验证
- 领域事件定义

#### ❌ 禁止做的事情
- 依赖任何外部框架（sqlx, redis, tokio）
- 数据库操作
- 网络请求
- 文件IO（除非是业务需要）

#### 代码示例

```rust
// ✅ 正确：纯业务模型
pub struct Order {
    pub id: String,
    pub symbol: Symbol,
    pub side: OrderSide,
    pub price: Price,
    pub volume: Volume,
}

impl Order {
    pub fn new(...) -> Result<Self, OrderError> {
        // 业务验证
        if price.value() <= 0.0 {
            return Err(OrderError::InvalidPrice);
        }
        Ok(Self { ... })
    }
}

// ❌ 错误：依赖外部框架
use sqlx::FromRow;
#[derive(FromRow)]
pub struct Order { ... }
```

---

### 2.2 `market` - 市场数据领域

#### 职责
- 市场数据采集（WebSocket、REST API）
- 数据存储和查询
- 数据流管理
- K线数据、行情数据管理

#### ✅ 允许做的事情
- 实现数据采集逻辑
- 实现数据查询服务
- WebSocket流管理
- 数据缓存策略

#### ❌ 禁止做的事情
- 交易策略逻辑
- 订单执行逻辑
- 风控逻辑

#### 依赖规则
- ✅ 依赖 `domain`（使用领域模型）
- ✅ 依赖 `infrastructure`（通过接口）
- ✅ 依赖 `core`（配置、日志）
- ❌ 不依赖 `trading`

---

### 2.3 `trading` - 交易领域（未来合并）

#### 职责
- 交易策略实现
- 订单执行逻辑
- 风险管理逻辑
- 回测引擎

#### ✅ 允许做的事情
- 策略信号生成
- 订单创建和提交
- 风控检查
- 回测计算

#### ❌ 禁止做的事情
- 直接操作数据库（通过Repository）
- 直接调用交易所API（通过Exchange接口）

#### 依赖规则
- ✅ 依赖 `domain`（使用领域模型）
- ✅ 依赖 `market`（获取市场数据）
- ✅ 依赖 `infrastructure`（通过接口）
- ✅ 依赖 `indicators`（计算技术指标）
- ✅ 依赖 `exchanges`（通过接口）

---

## Layer 3: Infrastructure Layer（基础设施层）

### 3.1 `infrastructure` - 数据访问

#### 职责
- Repository实现（实现domain中定义的接口）
- 缓存实现
- 消息传递实现

#### ✅ 允许做的事情
- 实现 `domain::traits::XxxRepository`
- 数据库操作（sqlx）
- Redis缓存操作
- 数据转换（Entity ↔ Domain）

#### ❌ 禁止做的事情
- 定义业务逻辑
- 定义领域模型

#### 代码示例

```rust
// ✅ 正确：实现domain接口
use rust_quant_domain::traits::CandleRepository;

pub struct SqlxCandleRepository {
    pool: PgPool,
}

#[async_trait]
impl CandleRepository for SqlxCandleRepository {
    async fn find_candles(&self, ...) -> Result<Vec<Candle>> {
        // 数据库查询
        let entities = sqlx::query_as::<_, CandleEntity>(...)
            .fetch_all(&self.pool)
            .await?;
        // 转换为领域模型
        entities.into_iter().map(|e| e.to_domain()).collect()
    }
}
```

---

### 3.2 `indicators` - 技术指标库

#### 职责
- 纯计算函数（EMA, MACD, RSI等）
- 无业务逻辑
- 无状态

#### ✅ 允许做的事情
- 数学计算
- 技术指标计算
- 数据转换

#### ❌ 禁止做的事情
- 数据库操作
- 业务逻辑判断
- 策略决策

#### 代码示例

```rust
// ✅ 正确：纯函数
pub fn calculate_ema(prices: &[f64], period: usize) -> Vec<f64> {
    // 纯计算逻辑
}

// ❌ 错误：包含业务逻辑
pub fn should_buy(prices: &[f64]) -> bool {
    // 这是策略逻辑，不属于indicators
}
```

---

### 3.3 `exchanges` - 交易所适配器

#### 职责
- 交易所API封装
- 统一接口抽象
- 错误处理

#### ✅ 允许做的事情
- 封装交易所API
- 实现 `domain::traits::ExchangeAccount`
- 错误转换

#### ❌ 禁止做的事情
- 业务逻辑
- 策略决策

---

## Layer 4: Core Layer（核心基础设施）

### `core` - 核心基础设施

#### 职责
- 配置管理
- 数据库连接池
- Redis客户端
- 日志系统
- 时间工具

#### ✅ 允许做的事情
- 配置读取
- 连接池管理
- 日志记录
- 工具函数

#### ❌ 禁止做的事情
- 业务逻辑
- 领域模型定义

---

### `common` - 通用工具

#### 职责
- 工具函数
- 通用类型
- 常量定义
- 错误定义

#### ✅ 允许做的事情
- 通用工具函数
- 类型定义
- 常量定义

#### ❌ 禁止做的事情
- 业务逻辑
- 领域特定代码

---

## Layer 5: Analysis Layer（分析层）

### `analytics` - 性能分析

#### 职责
- 性能分析
- 报表生成
- 数据统计

#### 依赖规则
- ✅ 依赖 `infrastructure`（读取数据）
- ✅ 依赖 `domain`（使用领域模型）

---

### `ai-analysis` - AI分析

#### 职责
- 情绪分析
- 事件检测
- 市场影响预测

#### 依赖规则
- ✅ 依赖 `market`（获取市场数据）
- ✅ 依赖 `domain`（使用领域模型）

---

## 🔍 常见问题

### Q1: 策略代码应该放在哪里？

**当前**：`strategies` 包  
**未来**：`trading/strategies`  
**原则**：策略是交易领域的核心，应该和订单执行、风控在一起

---

### Q2: 技术指标应该放在哪里？

**当前**：`indicators` 包（独立包）  
**未来**：`infrastructure/indicators`  
**原则**：技术指标是纯计算工具，属于基础设施

---

### Q3: 市场数据服务应该放在哪里？

**当前**：`market` 包  
**未来**：保持 `market` 包  
**原则**：市场数据是独立领域，与交易领域分离

---

### Q4: 业务流程协调应该放在哪里？

**当前**：`services` 包  
**未来**：保持 `services` 包，但职责明确  
**原则**：只做业务流程协调，不包含业务逻辑

---

## 📝 检查清单

新增代码前检查：

- [ ] 代码放在正确的层？
- [ ] 依赖方向正确？
- [ ] 命名符合规范？
- [ ] 职责单一？
- [ ] 没有违反禁止事项？


