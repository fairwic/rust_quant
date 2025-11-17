# Services层依赖分析：是否应该移除infrastructure依赖？

## 当前问题

### 1. 违反依赖倒置原则（DIP）

**当前代码**：
```rust
// ❌ services层直接依赖具体实现
pub struct CandleService {
    repository: SqlxCandleRepository,  // 具体类型
}

pub struct StrategyConfigService {
    repository: StrategyConfigEntityModel,  // 具体类型
}
```

**问题**：
- 编译时耦合到 `infrastructure` 的具体实现
- 无法替换实现（如 MySQL → PostgreSQL）
- 难以测试（需要真实数据库）
- 违反 DDD 架构原则

---

## 应该移除infrastructure依赖

### 架构原则

```
┌─────────────────────────────────────────┐
│  Services 层（应用服务）                 │
│  - 依赖 domain::traits（接口）          │
│  - 不依赖 infrastructure（实现）        │
└──────────────┬──────────────────────────┘
               │ 依赖
┌──────────────▼──────────────────────────┐
│  Domain 层（领域模型）                   │
│  - 定义 traits（接口）                   │
│  - 零外部依赖                            │
└──────────────┬──────────────────────────┘
               │ 实现
┌──────────────▼──────────────────────────┐
│  Infrastructure 层（基础设施）            │
│  - 实现 domain::traits                   │
│  - 提供具体实现                          │
└──────────────────────────────────────────┘
```

### 正确做法

```rust
// ✅ services层依赖domain接口
use rust_quant_domain::traits::CandleRepository;

pub struct CandleService {
    repository: Box<dyn CandleRepository>,  // trait类型
}

impl CandleService {
    // 通过构造函数注入具体实现
    pub fn new(repository: Box<dyn CandleRepository>) -> Self {
        Self { repository }
    }
}
```

**在应用入口注入**：
```rust
// rust-quant-cli/src/app/bootstrap.rs
use rust_quant_infrastructure::SqlxCandleRepository;
use rust_quant_services::market::CandleService;

let sqlx_repo = SqlxCandleRepository::new();
let candle_service = CandleService::new(Box::new(sqlx_repo));
```

---

## 当前使用infrastructure的地方

### 1. 直接使用具体Repository类型

**文件**: `crates/services/src/market/mod.rs`
```rust
// ❌ 当前
pub struct CandleService {
    repository: SqlxCandleRepository,  // 具体类型
}
```

**应该改为**：
```rust
// ✅ 重构后
use rust_quant_domain::traits::CandleRepository;

pub struct CandleService {
    repository: Box<dyn CandleRepository>,  // trait类型
}
```

---

### 2. 直接使用ExchangeFactory

**文件**: `crates/services/src/market/asset_service.rs`
```rust
// ❌ 当前
use rust_quant_infrastructure::ExchangeFactory;

let exchange = ExchangeFactory::create_default_account()?;
```

**问题**：
- `ExchangeFactory` 是工厂模式，但仍在 `infrastructure` 层
- `services` 层不应该知道如何创建具体实现

**解决方案**：
- **方案A**：将 `ExchangeFactory` 移到 `domain` 层（但需要依赖配置）
- **方案B**：在应用入口创建并注入（推荐）

```rust
// ✅ 方案B：在应用入口注入
// rust-quant-cli/src/app/bootstrap.rs
use rust_quant_infrastructure::exchanges::OkxAccountAdapter;
use rust_quant_domain::traits::ExchangeAccount;
use rust_quant_services::market::AssetService;

let exchange: Box<dyn ExchangeAccount> = Box::new(OkxAccountAdapter::new()?);
let asset_service = AssetService::new(exchange);
```

---

### 3. 直接创建Repository实例

**文件**: `crates/services/src/strategy/strategy_execution_service.rs`
```rust
// ❌ 当前
use rust_quant_infrastructure::SignalLogRepository;

let repo = SignalLogRepository::new();
```

**应该改为**：
```rust
// ✅ 重构后
pub struct StrategyExecutionService {
    signal_log_repo: Box<dyn SignalLogRepository>,  // 需要定义trait
}

impl StrategyExecutionService {
    pub fn new(signal_log_repo: Box<dyn SignalLogRepository>) -> Self {
        Self { signal_log_repo }
    }
}
```

---

## 重构方案

### 步骤1：定义domain traits（如果缺失）

**文件**: `crates/domain/src/traits/signal_log_repository.rs`（新建）
```rust
#[async_trait]
pub trait SignalLogRepository: Send + Sync {
    async fn save_signal_log(
        &self,
        inst_id: &str,
        period: &str,
        strategy_type: &str,
        signal_json: &str,
    ) -> Result<u64>;
    
    async fn find_recent_signals(
        &self,
        inst_id: &str,
        period: &str,
        limit: usize,
    ) -> Result<Vec<SignalLogEntity>>;
}
```

### 步骤2：重构services层使用trait

**文件**: `crates/services/src/market/mod.rs`
```rust
// ✅ 依赖domain接口
use rust_quant_domain::traits::CandleRepository;

pub struct CandleService {
    repository: Box<dyn CandleRepository>,
}

impl CandleService {
    // 通过构造函数注入
    pub fn new(repository: Box<dyn CandleRepository>) -> Self {
        Self { repository }
    }
}
```

### 步骤3：在应用入口注入实现

**文件**: `crates/rust-quant-cli/src/app/bootstrap.rs`
```rust
use rust_quant_infrastructure::SqlxCandleRepository;
use rust_quant_services::market::CandleService;

// 创建具体实现
let sqlx_repo = SqlxCandleRepository::new();

// 注入到service
let candle_service = CandleService::new(Box::new(sqlx_repo));
```

---

## 移除infrastructure依赖的好处

### 1. 符合DDD架构原则 ✅

- Services层只依赖Domain接口
- Infrastructure层实现Domain接口
- 依赖方向正确：Services → Domain ← Infrastructure

### 2. 易于测试 ✅

```rust
// 可以注入mock实现
struct MockCandleRepository;

#[async_trait]
impl CandleRepository for MockCandleRepository {
    async fn find_candles(...) -> Result<Vec<Candle>> {
        Ok(vec![])  // 返回测试数据
    }
}

#[tokio::test]
async fn test_candle_service() {
    let mock_repo = Box::new(MockCandleRepository);
    let service = CandleService::new(mock_repo);
    // 测试service逻辑，无需真实数据库
}
```

### 3. 易于替换实现 ✅

```rust
// 从MySQL切换到PostgreSQL
// 只需在应用入口修改，services层无需改动

// 之前
let repo = SqlxCandleRepository::new();

// 之后（如果实现PostgreSQL版本）
let repo = PostgresCandleRepository::new();
let service = CandleService::new(Box::new(repo));
```

### 4. 编译时解耦 ✅

- Services包不依赖Infrastructure包
- 减少编译时间
- 减少循环依赖风险

---

## 当前保留infrastructure依赖的"好处"（不推荐）

### 1. 代码简单 ✅

```rust
// 当前：直接创建
let service = CandleService::new(SqlxCandleRepository::new());

// 重构后：需要注入
let repo = SqlxCandleRepository::new();
let service = CandleService::new(Box::new(repo));
```

**代价**：违反架构原则，难以测试和扩展

### 2. 减少样板代码 ✅

**当前**：services层可以直接创建实例

**代价**：编译时耦合，无法替换实现

---

## 推荐方案

### 完全移除infrastructure依赖

**修改**：
1. `Cargo.toml` 移除 `rust-quant-infrastructure.workspace = true`
2. 所有services使用 `Box<dyn Trait>` 或 `Arc<dyn Trait>`
3. 在应用入口（`rust-quant-cli`）注入具体实现

**好处**：
- ✅ 符合DDD架构
- ✅ 易于测试
- ✅ 易于扩展
- ✅ 编译时解耦

**代价**：
- ⚠️ 需要更多依赖注入代码
- ⚠️ 需要定义更多domain traits

---

## 实施建议

### 优先级1：核心服务

1. `CandleService` - 使用 `Box<dyn CandleRepository>`
2. `StrategyConfigService` - 使用 `Box<dyn StrategyConfigRepository>`
3. `StrategyExecutionService` - 使用 `Box<dyn SignalLogRepository>`

### 优先级2：交易所服务

1. `AssetService` - 注入 `Box<dyn ExchangeAccount>`
2. `AccountService` - 注入 `Box<dyn ExchangeAccount>`
3. `ContractsService` - 注入 `Box<dyn ExchangeContracts>`

### 优先级3：工厂模式

- 将 `ExchangeFactory` 移到应用入口层
- 或创建 `ExchangeFactory` trait在domain层

---

## 总结

**应该移除 `infrastructure` 依赖**，原因：

1. ✅ 符合DDD架构原则
2. ✅ 易于测试（可注入mock）
3. ✅ 易于扩展（可替换实现）
4. ✅ 编译时解耦

**当前保留依赖的"好处"**（不推荐）：
- 代码简单（但违反架构原则）
- 减少样板代码（但难以测试和扩展）

**推荐做法**：
- Services层依赖Domain traits
- 在应用入口注入Infrastructure实现




