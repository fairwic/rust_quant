# Service层调用模式：何时通过Domain，何时直接调用Infrastructure

## 核心原则

### 标准模式（推荐）：Service → Domain Trait → Infrastructure

```
┌─────────────┐
│  Service    │
└──────┬──────┘
       │ 依赖接口
┌──────▼──────────┐
│  Domain Trait   │  (定义接口)
└──────┬──────────┘
       │ 实现
┌──────▼──────────┐
│ Infrastructure  │  (具体实现)
└─────────────────┘
```

### 直接调用模式（特殊情况）：Service → Infrastructure

```
┌─────────────┐
│  Service    │
└──────┬──────┘
       │ 直接依赖
┌──────▼──────────┐
│ Infrastructure  │  (工具类/工厂/配置)
└─────────────────┘
```

---

## 场景1：必须通过Domain Trait（业务逻辑相关）

### ✅ 数据访问（Repository）

**原因**：
- 涉及业务逻辑
- 需要可替换实现（MySQL → PostgreSQL）
- 需要可测试（注入mock）

**示例**：

```rust
// ✅ 正确：通过Domain Trait
use rust_quant_domain::traits::CandleRepository;

pub struct CandleService {
    repository: Box<dyn CandleRepository>,  // trait类型
}

impl CandleService {
    pub fn new(repository: Box<dyn CandleRepository>) -> Self {
        Self { repository }
    }
    
    pub async fn get_candles(&self, ...) -> Result<Vec<Candle>> {
        // 调用trait方法
        self.repository.find_candles(...).await
    }
}

// 在应用入口注入
let sqlx_repo = SqlxCandleRepository::new();
let service = CandleService::new(Box::new(sqlx_repo));
```

**适用场景**：
- `CandleRepository` - K线数据访问
- `StrategyConfigRepository` - 策略配置访问
- `OrderRepository` - 订单数据访问
- `PositionRepository` - 持仓数据访问
- `SignalLogRepository` - 信号日志访问

---

### ✅ 外部服务调用（Exchange Adapter）

**原因**：
- 涉及业务逻辑（交易、账户查询）
- 需要支持多交易所
- 需要可测试（注入mock exchange）

**示例**：

```rust
// ✅ 正确：通过Domain Trait
use rust_quant_domain::traits::ExchangeAccount;

pub struct AssetService {
    exchange: Box<dyn ExchangeAccount>,  // trait类型
}

impl AssetService {
    pub fn new(exchange: Box<dyn ExchangeAccount>) -> Self {
        Self { exchange }
    }
    
    pub async fn fetch_balances(&self) -> Result<Vec<AssetBalance>> {
        // 调用trait方法
        self.exchange.fetch_asset_balances(None).await
    }
}

// 在应用入口注入
let okx_adapter = OkxAccountAdapter::new()?;
let service = AssetService::new(Box::new(okx_adapter));
```

**适用场景**：
- `ExchangeMarketData` - 市场数据（K线、Ticker）
- `ExchangeAccount` - 账户查询（余额、持仓）
- `ExchangeContracts` - 合约数据（持仓量、成交量）
- `ExchangePublicData` - 公共数据（公告）

---

### ✅ 业务规则验证（Domain Service）

**原因**：
- 涉及核心业务逻辑
- 需要可替换实现
- 需要可测试

**示例**：

```rust
// ✅ 正确：通过Domain Trait
use rust_quant_domain::traits::RiskValidator;

pub struct RiskManagementService {
    validator: Box<dyn RiskValidator>,
}

impl RiskManagementService {
    pub fn new(validator: Box<dyn RiskValidator>) -> Self {
        Self { validator }
    }
    
    pub async fn validate_order(&self, order: &Order) -> Result<()> {
        // 调用trait方法
        self.validator.validate_order_risk(order).await
    }
}
```

---

## 场景2：可以直接调用Infrastructure（非业务逻辑）

### ✅ 工厂类（Factory）

**原因**：
- 不涉及业务逻辑，只是创建实例
- 工厂本身是基础设施的一部分
- 可以接受编译时依赖

**示例**：

```rust
// ✅ 可以接受：直接调用Factory
use rust_quant_infrastructure::ExchangeFactory;

pub struct AccountService;

impl AccountService {
    pub async fn fetch_balance(&self) -> Result<serde_json::Value> {
        // 工厂创建实例（临时依赖）
        let exchange = ExchangeFactory::create_default_account()?;
        exchange.fetch_balance(None).await
    }
}
```

**注意**：
- 如果工厂逻辑复杂，建议移到应用入口层
- 或者创建 `ExchangeFactory` trait在domain层

**适用场景**：
- `ExchangeFactory` - 创建交易所适配器
- `RepositoryFactory` - 创建Repository实例（如果存在）

---

### ✅ 工具类（Utilities）

**原因**：
- 纯工具函数，无业务逻辑
- 不涉及状态管理
- 不涉及业务规则

**示例**：

```rust
// ✅ 可以接受：直接调用工具类
use rust_quant_infrastructure::utils::DataConverter;

pub struct DataService;

impl DataService {
    pub fn convert_format(&self, data: &str) -> String {
        // 工具函数，无业务逻辑
        DataConverter::to_json(data)
    }
}
```

**适用场景**：
- 数据格式转换工具
- 加密/解密工具
- 时间格式化工具
- 字符串处理工具

---

### ✅ 数据传输对象（DTO/Entity）

**原因**：
- 只是数据结构，无业务逻辑
- 用于序列化/反序列化
- 不涉及业务规则

**示例**：

```rust
// ✅ 可以接受：直接使用Entity
use rust_quant_infrastructure::repositories::StrategyConfigEntity;

pub struct StrategyConfigService {
    repository: Box<dyn StrategyConfigRepository>,
}

impl StrategyConfigService {
    // Entity只是数据结构，可以直接使用
    pub async fn load_entity(&self, id: i64) -> Result<StrategyConfigEntity> {
        // Entity用于数据转换
        let entity = self.repository.find_by_id(id).await?;
        // 转换为领域模型
        entity.to_domain()
    }
}
```

**适用场景**：
- `StrategyConfigEntity` - 数据库实体
- `CandleEntity` - K线实体
- `OrderEntity` - 订单实体
- 所有 `FromRow` 的数据库实体

**注意**：
- Entity应该只用于数据转换
- 业务逻辑应该使用Domain模型

---

### ✅ 配置类（Configuration）

**原因**：
- 系统配置，不涉及业务逻辑
- 通常是静态的或从环境变量读取
- 不涉及业务规则

**示例**：

```rust
// ✅ 可以接受：直接使用配置
use rust_quant_core::config::DatabaseConfig;

pub struct DatabaseService;

impl DatabaseService {
    pub fn get_connection_string(&self) -> String {
        let config = DatabaseConfig::load();
        config.connection_string()
    }
}
```

**适用场景**：
- 数据库配置
- Redis配置
- 日志配置
- 系统配置

---

### ✅ 缓存实现（如果只是工具类）

**原因**：
- 如果缓存只是工具类（如 `InMemoryCache`）
- 不涉及业务逻辑
- 可以接受编译时依赖

**示例**：

```rust
// ✅ 可以接受：直接使用简单缓存
use rust_quant_infrastructure::cache::InMemoryCache;

pub struct CacheService {
    cache: InMemoryCache<String, String>,
}

impl CacheService {
    pub fn new() -> Self {
        Self {
            cache: InMemoryCache::new(),
        }
    }
}
```

**注意**：
- 如果缓存涉及业务逻辑（如策略缓存），应该通过Domain Trait
- 如果只是简单的key-value缓存，可以直接使用

---

## 决策树

```
需要调用Infrastructure？
│
├─ 是否涉及业务逻辑？
│  │
│  ├─ 是 → ✅ 必须通过Domain Trait
│  │   ├─ Repository（数据访问）
│  │   ├─ Exchange Adapter（外部服务）
│  │   └─ Domain Service（业务规则）
│  │
│  └─ 否 → 继续判断
│      │
│      ├─ 是否是工厂类？
│      │  └─ 是 → ⚠️ 可以接受（但建议移到应用入口）
│      │
│      ├─ 是否是工具类？
│      │  └─ 是 → ✅ 可以直接调用
│      │
│      ├─ 是否是DTO/Entity？
│      │  └─ 是 → ✅ 可以直接使用（仅用于数据转换）
│      │
│      ├─ 是否是配置类？
│      │  └─ 是 → ✅ 可以直接使用
│      │
│      └─ 其他 → ❌ 应该通过Domain Trait
```

---

## 实际代码示例

### 示例1：CandleService（✅ 通过Domain Trait）

```rust
// ✅ 正确：通过Domain Trait
use rust_quant_domain::traits::CandleRepository;

pub struct CandleService {
    repository: Box<dyn CandleRepository>,  // trait类型
}

impl CandleService {
    pub fn new(repository: Box<dyn CandleRepository>) -> Self {
        Self { repository }
    }
    
    pub async fn get_candles(&self, ...) -> Result<Vec<Candle>> {
        self.repository.find_candles(...).await  // 调用trait方法
    }
}
```

**原因**：
- 涉及业务逻辑（K线数据查询）
- 需要可替换实现（MySQL → PostgreSQL）
- 需要可测试（注入mock）

---

### 示例2：AccountService（⚠️ 当前使用Factory，建议重构）

```rust
// ⚠️ 当前：直接使用Factory
use rust_quant_infrastructure::ExchangeFactory;

pub struct AccountService;

impl AccountService {
    pub async fn fetch_balance(&self) -> Result<serde_json::Value> {
        let exchange = ExchangeFactory::create_default_account()?;  // 直接调用
        exchange.fetch_balance(None).await
    }
}
```

**问题**：
- 违反依赖倒置原则
- 难以测试（无法注入mock）
- 难以替换实现

**建议重构**：

```rust
// ✅ 重构后：通过Domain Trait
use rust_quant_domain::traits::ExchangeAccount;

pub struct AccountService {
    exchange: Box<dyn ExchangeAccount>,  // trait类型
}

impl AccountService {
    pub fn new(exchange: Box<dyn ExchangeAccount>) -> Self {
        Self { exchange }
    }
    
    pub async fn fetch_balance(&self) -> Result<serde_json::Value> {
        self.exchange.fetch_balance(None).await  // 调用trait方法
    }
}

// 在应用入口注入
let okx_adapter = OkxAccountAdapter::new()?;
let service = AccountService::new(Box::new(okx_adapter));
```

---

### 示例3：使用Entity（✅ 可以直接使用）

```rust
// ✅ 可以接受：Entity只是数据结构
use rust_quant_infrastructure::repositories::StrategyConfigEntity;

pub struct StrategyConfigService {
    repository: Box<dyn StrategyConfigRepository>,
}

impl StrategyConfigService {
    pub async fn load_config(&self, id: i64) -> Result<StrategyConfig> {
        // Entity用于数据转换，可以直接使用
        let entity = self.repository.find_by_id(id).await?;
        entity.to_domain()  // 转换为领域模型
    }
}
```

**原因**：
- Entity只是数据结构，无业务逻辑
- 用于数据转换（Entity → Domain）
- 不涉及业务规则

---

### 示例4：使用工具类（✅ 可以直接调用）

```rust
// ✅ 可以接受：工具类无业务逻辑
use rust_quant_infrastructure::utils::TimeFormatter;

pub struct TimeService;

impl TimeService {
    pub fn format_timestamp(&self, ts: i64) -> String {
        // 工具函数，无业务逻辑
        TimeFormatter::format(ts)
    }
}
```

**原因**：
- 纯工具函数，无业务逻辑
- 不涉及状态管理
- 不涉及业务规则

---

## 总结

### 必须通过Domain Trait ✅

1. **Repository** - 数据访问
2. **Exchange Adapter** - 外部服务调用
3. **Domain Service** - 业务规则验证
4. **任何涉及业务逻辑的组件**

### 可以直接调用Infrastructure ✅

1. **Factory** - 创建实例（但建议移到应用入口）
2. **工具类** - 纯工具函数
3. **DTO/Entity** - 数据结构（仅用于数据转换）
4. **配置类** - 系统配置
5. **简单缓存** - 如果只是工具类

### 判断标准

- **涉及业务逻辑** → 必须通过Domain Trait
- **不涉及业务逻辑** → 可以直接调用
- **不确定** → 优先通过Domain Trait（更安全）

---

## 最佳实践

### 1. 默认使用Domain Trait

```rust
// ✅ 默认做法：通过Domain Trait
pub struct MyService {
    repository: Box<dyn MyRepository>,  // trait类型
}
```

### 2. 特殊情况才直接调用

```rust
// ✅ 特殊情况：工具类可以直接调用
use rust_quant_infrastructure::utils::MyUtil;

pub struct MyService;

impl MyService {
    pub fn do_something(&self) {
        MyUtil::helper_function();  // 工具函数
    }
}
```

### 3. 在应用入口注入实现

```rust
// ✅ 在应用入口（rust-quant-cli）注入
let sqlx_repo = SqlxCandleRepository::new();
let service = CandleService::new(Box::new(sqlx_repo));
```

---

## 当前代码检查清单

### ✅ 已正确使用Domain Trait

- `CandleService` - 使用 `Box<dyn CandleRepository>`
- `StrategyConfigService` - 使用 `Box<dyn StrategyConfigRepository>`

### ⚠️ 需要重构（直接调用Infrastructure）

- `AccountService` - 直接使用 `ExchangeFactory`
- `AssetService` - 直接使用 `ExchangeFactory`
- `ContractsService` - 直接使用 `ExchangeFactory`
- `PublicDataService` - 直接使用 `ExchangeFactory`

### ✅ 可以接受（工具类/Entity）

- 使用 `StrategyConfigEntity` - Entity只是数据结构
- 使用 `InMemoryCache` - 简单缓存工具

---

## 重构优先级

### 优先级1：核心业务逻辑

1. Repository - 必须通过Domain Trait ✅
2. Exchange Adapter - 必须通过Domain Trait ⚠️

### 优先级2：工具类

1. Factory - 可以接受，但建议移到应用入口
2. 工具类 - 可以直接调用 ✅
3. Entity - 可以直接使用 ✅




