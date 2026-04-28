# 交易所API配置系统使用指南

## 概述

交易所API配置系统允许一个API Key关联多个策略，并在策略触发时自动从Redis缓存中获取关联的API配置执行交易所下单操作。

## 数据库表结构

当前 `rust_quant` 运行库统一使用 Postgres `quant_core`。下面 DDL 仅用于说明本地
`exchange_api_config` 兼容表形态；用户真实 API Key 的业务归属仍在
`rust_quan_web.quant_web.user_api_credentials`，执行 worker 通过 Web 内部接口解析。

### 1. exchange_api_config（交易所API配置表）

存储交易所API凭证信息：

```sql
CREATE TABLE IF NOT EXISTS exchange_api_config (
  id BIGSERIAL PRIMARY KEY,
  exchange_name VARCHAR(50) NOT NULL,
  api_key VARCHAR(200) NOT NULL,
  api_secret VARCHAR(200) NOT NULL,
  passphrase VARCHAR(200),
  is_sandbox BOOLEAN NOT NULL DEFAULT FALSE,
  is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  description VARCHAR(500),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ,
  is_deleted SMALLINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_exchange_api_config_exchange_name
  ON exchange_api_config (exchange_name, is_enabled, is_deleted);

COMMENT ON TABLE exchange_api_config IS '交易所 API 配置兼容表，当前仅用于 rust_quant 本地兼容和审计';
COMMENT ON COLUMN exchange_api_config.id IS '主键 ID';
COMMENT ON COLUMN exchange_api_config.exchange_name IS '交易所名称，如 binance、okx、hyperliquid';
COMMENT ON COLUMN exchange_api_config.api_key IS '交易所 API Key';
COMMENT ON COLUMN exchange_api_config.api_secret IS '交易所 API Secret';
COMMENT ON COLUMN exchange_api_config.passphrase IS 'OKX 等交易所需要的 passphrase';
COMMENT ON COLUMN exchange_api_config.is_sandbox IS '是否使用沙箱环境';
COMMENT ON COLUMN exchange_api_config.is_enabled IS '是否启用';
COMMENT ON COLUMN exchange_api_config.description IS '配置描述';
COMMENT ON COLUMN exchange_api_config.created_at IS '创建时间';
COMMENT ON COLUMN exchange_api_config.updated_at IS '更新时间';
COMMENT ON COLUMN exchange_api_config.is_deleted IS '软删除标记，0 表示未删除';
```

### 2. strategy_api_config（策略与API配置关联表）

实现策略与API配置的多对多关系：

```sql
CREATE TABLE IF NOT EXISTS strategy_api_config (
  id BIGSERIAL PRIMARY KEY,
  strategy_config_id BIGINT NOT NULL,
  api_config_id BIGINT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ,
  is_deleted SMALLINT NOT NULL DEFAULT 0,
  CONSTRAINT strategy_api_unique UNIQUE (strategy_config_id, api_config_id, is_deleted)
);

CREATE INDEX IF NOT EXISTS idx_strategy_api_config_strategy
  ON strategy_api_config (strategy_config_id, is_enabled, is_deleted);
CREATE INDEX IF NOT EXISTS idx_strategy_api_config_api
  ON strategy_api_config (api_config_id, is_enabled, is_deleted);

COMMENT ON TABLE strategy_api_config IS '策略配置与交易所 API 配置关联兼容表';
COMMENT ON COLUMN strategy_api_config.id IS '主键 ID';
COMMENT ON COLUMN strategy_api_config.strategy_config_id IS '策略配置 ID';
COMMENT ON COLUMN strategy_api_config.api_config_id IS 'API 配置 ID';
COMMENT ON COLUMN strategy_api_config.priority IS '优先级，数字越小优先级越高';
COMMENT ON COLUMN strategy_api_config.is_enabled IS '是否启用';
COMMENT ON COLUMN strategy_api_config.created_at IS '创建时间';
COMMENT ON COLUMN strategy_api_config.updated_at IS '更新时间';
COMMENT ON COLUMN strategy_api_config.is_deleted IS '软删除标记，0 表示未删除';
```

## 架构设计

### 分层结构

```
orchestration (调度层)
    ↓
services (业务协调层)
    ├── ExchangeApiService (API配置管理)
    └── ExecutionWorker / crypto_exc_all facade (统一下单执行)
    ↓
domain (领域层)
    ├── ExchangeApiConfig (实体)
    ├── StrategyApiConfig (关联实体)
    └── Repository Traits (接口定义)
    ↓
infrastructure (基础设施层)
    ├── SqlxExchangeApiConfigRepository (数据库实现)
    └── SqlxStrategyApiConfigRepository (关联表实现)
```

### 数据流

1. **策略执行触发** → `StrategyExecutionService::execute_strategy()`
2. **获取API配置** → `ExchangeApiService::get_first_api_config()` 
   - 优先从Redis缓存获取（1小时过期）
   - 缓存未命中则从数据库查询并缓存
3. **获取用户凭证** → execution worker 调用 `rust_quan_web` 内部接口解析用户 API Key
4. **执行下单** → 通过 `crypto_exc_all` facade 按交易所执行，dry-run/live 结果回写 Web

## 使用示例

### 1. 创建API配置

```rust
use rust_quant_domain::entities::ExchangeApiConfig;
use rust_quant_infrastructure::repositories::SqlxExchangeApiConfigRepository;
use rust_quant_core::database::get_db_pool;

let api_config = ExchangeApiConfig::new(
    0,  // id (新建时为0)
    "okx".to_string(),
    "your_api_key".to_string(),
    "your_api_secret".to_string(),
    Some("your_passphrase".to_string()),
    false,  // is_sandbox
    true,   // is_enabled
    Some("主账户API".to_string()),
);

let repo = SqlxExchangeApiConfigRepository::new(get_db_pool());
let api_id = repo.save(&api_config).await?;
```

### 2. 关联策略与API配置

```rust
use rust_quant_services::exchange::create_exchange_api_service;

let api_service = create_exchange_api_service();

// 将策略配置ID=1关联到API配置ID=1，优先级为0（最高）
api_service.associate_strategy_with_api(1, 1, 0).await?;
```

### 3. 策略执行时自动获取API配置

策略执行服务会自动从Redis缓存或数据库获取关联的API配置：

```rust
// legacy 本地路径：在 StrategyExecutionService::execute_order_internal() 中
// 自动获取API配置并执行下单。新闭环优先走 Web execution task + worker。
let api_config = api_service.get_first_api_config(strategy_config_id).await?;
```

## Redis缓存机制

### 缓存键格式

```
strategy_api_config:{strategy_config_id}
```

### 缓存策略

- **过期时间**: 1小时（3600秒）
- **更新时机**: 
  - 创建/更新关联时自动清除缓存
  - 下次查询时重新加载并缓存

### 手动清除缓存

```rust
api_service.clear_cache(strategy_config_id).await?;
```

## 优先级机制

- 数字越小，优先级越高
- 策略执行时选择优先级最高的可用API配置
- 如果最高优先级的API配置不可用，自动降级到下一个优先级

## 安全注意事项

1. **API密钥加密**: 建议在生产环境中对API密钥进行加密存储
2. **权限控制**: API Key应设置最小必要权限（仅交易权限）
3. **沙箱测试**: 开发环境使用 `is_sandbox=true` 的配置
4. **访问日志**: 记录所有API配置的使用日志

## 扩展性

### 支持更多交易所

当前执行路径已经按 `crypto_exc_all` facade 逐步扩展到 Binance、OKX 等交易所。新增交易所时优先：

1. 在 `crypto_exc_all` 增加或完善对应交易所 adapter
2. 在 `rust_quant` execution worker 中增加凭证映射和 dry-run/live guard
3. 在 `rust_quan_web` 的用户 API Key 配置中补齐该交易所必需字段

### 多API配置负载均衡

可以在 `ExchangeApiService::get_first_api_config()` 中实现：
- 轮询策略
- 基于历史成功率的智能选择
- 基于当前负载的选择

## 故障处理

### API配置不可用

- 如果策略关联的API配置全部不可用，下单会失败并记录错误日志
- 建议设置监控告警

### Redis缓存失效

- 缓存失效不影响功能，会自动降级到数据库查询
- 性能会有轻微下降，但功能正常

## 相关文件

- 数据库表结构: `quant_core` Postgres 兼容表或服务迁移 SQL
- Domain实体: `crates/domain/src/entities/exchange_api_config.rs`
- Repository实现: `crates/infrastructure/src/repositories/exchange_api_config_repository.rs`
- Service层: `crates/services/src/exchange/exchange_api_service.rs`
- OKX订单服务: `crates/services/src/exchange/okx_order_service.rs`
- 策略执行集成: `crates/services/src/strategy/strategy_execution_service.rs`
