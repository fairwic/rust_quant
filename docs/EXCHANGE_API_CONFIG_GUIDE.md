# 交易所API配置系统使用指南

## 概述

交易所API配置系统允许一个API Key关联多个策略，并在策略触发时自动从Redis缓存中获取关联的API配置执行交易所下单操作。

## 数据库表结构

### 1. exchange_api_config（交易所API配置表）

存储交易所API凭证信息：

```sql
CREATE TABLE `exchange_api_config` (
  `id` int NOT NULL AUTO_INCREMENT,
  `exchange_name` varchar(50) NOT NULL COMMENT '交易所名称（okx）',
  `api_key` varchar(200) NOT NULL COMMENT 'API Key',
  `api_secret` varchar(200) NOT NULL COMMENT 'API Secret',
  `passphrase` varchar(200) DEFAULT NULL COMMENT 'Passphrase（OKX需要）',
  `is_sandbox` tinyint(1) NOT NULL DEFAULT 0 COMMENT '是否沙箱环境',
  `is_enabled` tinyint(1) NOT NULL DEFAULT 1 COMMENT '是否启用',
  `description` varchar(500) DEFAULT NULL COMMENT '描述',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  `is_deleted` smallint NOT NULL DEFAULT 0,
  PRIMARY KEY (`id`),
  KEY `exchange_name` (`exchange_name`, `is_enabled`, `is_deleted`)
);
```

### 2. strategy_api_config（策略与API配置关联表）

实现策略与API配置的多对多关系：

```sql
CREATE TABLE `strategy_api_config` (
  `id` int NOT NULL AUTO_INCREMENT,
  `strategy_config_id` int NOT NULL COMMENT '策略配置ID',
  `api_config_id` int NOT NULL COMMENT 'API配置ID',
  `priority` int NOT NULL DEFAULT 0 COMMENT '优先级（数字越小优先级越高）',
  `is_enabled` tinyint(1) NOT NULL DEFAULT 1 COMMENT '是否启用',
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `updated_at` datetime DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  `is_deleted` smallint NOT NULL DEFAULT 0,
  PRIMARY KEY (`id`),
  UNIQUE KEY `strategy_api_unique` (`strategy_config_id`, `api_config_id`, `is_deleted`),
  KEY `strategy_config_id` (`strategy_config_id`, `is_enabled`, `is_deleted`),
  KEY `api_config_id` (`api_config_id`, `is_enabled`, `is_deleted`)
);
```

## 架构设计

### 分层结构

```
orchestration (调度层)
    ↓
services (业务协调层)
    ├── ExchangeApiService (API配置管理)
    └── OkxOrderService (OKX订单执行)
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
3. **获取账户信息** → `OkxOrderService::get_positions()` + `get_max_available_size()`
4. **执行下单** → `OkxOrderService::execute_order_from_signal()`

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
// 在 StrategyExecutionService::execute_order_internal() 中
// 自动获取API配置并执行下单
let api_config = api_service.get_first_api_config(strategy_config_id).await?;
let okx_service = OkxOrderService;
okx_service.execute_order_from_signal(&api_config, inst_id, &signal, size, price).await?;
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

当前仅支持OKX，扩展其他交易所需要：

1. 在 `ExchangeApiConfig` 中添加交易所特定字段
2. 实现对应的订单执行服务（如 `BinanceOrderService`）
3. 在 `OkxOrderService` 中添加交易所类型判断

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

- 数据库表结构: `create_table_exchange_api.sql`
- Domain实体: `crates/domain/src/entities/exchange_api_config.rs`
- Repository实现: `crates/infrastructure/src/repositories/exchange_api_config_repository.rs`
- Service层: `crates/services/src/exchange/exchange_api_service.rs`
- OKX订单服务: `crates/services/src/exchange/okx_order_service.rs`
- 策略执行集成: `crates/services/src/strategy/strategy_execution_service.rs`

