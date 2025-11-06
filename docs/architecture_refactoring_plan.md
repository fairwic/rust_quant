# Rust Quant 项目架构重构方案

## 📌 重构目标
将当前的单体 `trading/` 模块（159个文件）重构为清晰的 DDD 分层架构，提升代码可维护性、可测试性和可扩展性。

---

## 🏗️ 架构分层设计

### 1. **Domain 层（领域层）**
**职责**：核心业务逻辑，不依赖任何外部框架

```
src/domain/
├── market/                    # 市场数据领域
│   ├── entities/
│   │   ├── candle.rs         # K线实体 [已优化]
│   │   ├── ticker.rs         # 行情实体
│   │   └── order_book.rs     # 订单簿实体
│   ├── value_objects/
│   │   ├── price.rs          # 价格值对象
│   │   ├── volume.rs         # 成交量值对象
│   │   └── timeframe.rs      # 时间周期值对象
│   ├── repositories/
│   │   └── candle_repository.rs  # K线仓储接口（trait）
│   └── services/
│       └── market_data_service.rs  # 市场数据领域服务
│
├── strategy/                  # 策略领域
│   ├── entities/
│   │   ├── strategy_config.rs    # 策略配置实体
│   │   └── signal.rs             # 信号实体
│   ├── value_objects/
│   │   ├── indicator_value.rs    # 指标值对象
│   │   └── position.rs           # 持仓值对象
│   ├── strategies/              # 具体策略实现
│   │   ├── mod.rs
│   │   ├── strategy_trait.rs    # 策略trait
│   │   ├── nwe_strategy/        # NWE策略
│   │   ├── vegas_strategy/      # Vegas策略
│   │   ├── squeeze_strategy/    # Squeeze策略
│   │   └── engulfing_strategy/  # 吞没策略
│   ├── indicators/              # 技术指标（领域逻辑）
│   │   ├── trend/
│   │   │   ├── ema.rs
│   │   │   ├── sma.rs
│   │   │   └── super_trend.rs
│   │   ├── momentum/
│   │   │   ├── rsi.rs
│   │   │   ├── macd.rs
│   │   │   └── kdj.rs
│   │   ├── volatility/
│   │   │   ├── atr.rs
│   │   │   └── bollinger.rs
│   │   └── volume/
│   │       └── volume_indicator.rs
│   └── repositories/
│       └── strategy_repository.rs  # 策略仓储接口
│
├── risk/                      # 风控领域
│   ├── entities/
│   │   ├── risk_limit.rs     # 风险限额实体
│   │   └── risk_event.rs     # 风险事件实体
│   ├── value_objects/
│   │   ├── stop_loss.rs      # 止损值对象
│   │   └── take_profit.rs    # 止盈值对象
│   ├── services/
│   │   ├── position_risk_service.rs  # 持仓风控服务
│   │   └── order_risk_service.rs     # 订单风控服务
│   └── policies/
│       ├── risk_policy.rs    # 风控策略trait
│       └── implementations/  # 具体风控策略
│
├── order/                     # 订单领域
│   ├── entities/
│   │   ├── order.rs          # 订单实体
│   │   └── trade.rs          # 成交实体
│   ├── value_objects/
│   │   ├── order_side.rs     # 订单方向
│   │   └── order_type.rs     # 订单类型
│   ├── services/
│   │   └── order_validator.rs  # 订单验证服务
│   └── repositories/
│       └── order_repository.rs  # 订单仓储接口
│
└── shared/                    # 跨领域共享
    ├── events/               # 领域事件
    │   ├── market_event.rs
    │   ├── strategy_event.rs
    │   └── order_event.rs
    └── specifications/       # 规约模式
        └── common_specs.rs
```

---

### 2. **Application 层（应用层）**
**职责**：用例编排，协调领域对象完成业务流程

```
src/application/
├── commands/                  # 命令处理（写操作）
│   ├── mod.rs
│   ├── strategy/
│   │   ├── create_strategy_command.rs
│   │   ├── update_strategy_command.rs
│   │   └── execute_strategy_command.rs
│   ├── order/
│   │   ├── place_order_command.rs
│   │   └── cancel_order_command.rs
│   └── handlers/
│       └── command_handler_trait.rs
│
├── queries/                   # 查询处理（读操作）
│   ├── mod.rs
│   ├── strategy/
│   │   ├── get_strategy_query.rs
│   │   └── list_strategies_query.rs
│   ├── market/
│   │   └── get_candles_query.rs
│   └── handlers/
│       └── query_handler_trait.rs
│
├── services/                  # 应用服务（编排领域服务）
│   ├── mod.rs
│   ├── strategy_orchestrator.rs   # 策略编排服务
│   ├── backtest_service.rs        # 回测服务
│   └── real_trading_service.rs    # 实盘交易服务
│
├── dto/                       # 数据传输对象
│   ├── strategy_dto.rs
│   ├── order_dto.rs
│   └── market_dto.rs
│
└── workflows/                 # 工作流（复杂业务流程）
    ├── trading_workflow.rs
    └── risk_check_workflow.rs
```

---

### 3. **Infrastructure 层（基础设施层）**
**职责**：技术实现细节，依赖外部框架和服务

```
src/infrastructure/
├── persistence/               # 数据持久化
│   ├── mod.rs
│   ├── database/
│   │   ├── connection_pool.rs
│   │   ├── mysql_connection.rs
│   │   └── migrations/       # 数据库迁移脚本
│   ├── repositories/         # 仓储实现（实现domain层trait）
│   │   ├── candle_repository_impl.rs
│   │   ├── strategy_repository_impl.rs
│   │   └── order_repository_impl.rs
│   └── entities/             # ORM实体（数据库映射）
│       ├── candle_entity.rs
│       └── order_entity.rs
│
├── messaging/                 # 消息通信
│   ├── websocket/
│   │   ├── okx_websocket.rs  # OKX WebSocket客户端
│   │   └── connection_manager.rs
│   └── message_bus/          # 内部消息总线
│       ├── event_bus.rs
│       └── handlers/
│
├── cache/                     # 缓存实现
│   ├── redis_cache.rs
│   ├── memory_cache.rs
│   └── cache_strategy.rs     # 缓存策略
│
├── config/                    # 配置管理
│   ├── mod.rs
│   ├── app_config.rs         # 应用配置
│   ├── database_config.rs    # 数据库配置
│   ├── redis_config.rs       # Redis配置
│   ├── log_config.rs         # 日志配置
│   └── environment.rs        # 环境变量管理
│
├── scheduler/                 # 任务调度
│   ├── mod.rs
│   ├── job_scheduler.rs      # 任务调度器
│   ├── jobs/                 # 具体任务
│   │   ├── sync_candles_job.rs
│   │   ├── strategy_runner_job.rs
│   │   ├── risk_check_job.rs
│   │   └── cleanup_job.rs
│   └── job_registry.rs       # 任务注册器
│
├── external_api/              # 外部API集成
│   ├── okx_client/           # OKX交易所API
│   │   ├── market_api.rs
│   │   ├── trading_api.rs
│   │   └── account_api.rs
│   └── notification/
│       └── email_service.rs  # 邮件服务
│
└── monitoring/                # 监控和可观测性
    ├── metrics.rs            # 指标收集
    ├── tracing.rs            # 链路追踪
    └── health_check.rs       # 健康检查
```

---

### 4. **Interfaces 层（接口层）**
**职责**：对外暴露的接口适配器

```
src/interfaces/
├── api/                       # REST API（可选）
│   ├── mod.rs
│   ├── routes/
│   │   ├── strategy_routes.rs
│   │   └── market_routes.rs
│   └── middleware/
│       └── auth_middleware.rs
│
└── cli/                       # 命令行接口
    ├── mod.rs
    └── commands/
        ├── run_backtest.rs
        └── start_trading.rs
```

---

### 5. **Shared 层（共享层）**
**职责**：跨层共享的工具和类型

```
src/shared/
├── types/                     # 公共类型定义
│   ├── mod.rs
│   ├── result.rs             # 统一Result类型
│   ├── id.rs                 # ID类型封装
│   └── decimal.rs            # 高精度数值类型
│
├── utils/                     # 工具函数
│   ├── mod.rs
│   ├── time_util.rs          # [迁移自根目录]
│   ├── math_util.rs
│   └── validation.rs
│
├── constants/                 # 全局常量
│   ├── mod.rs
│   ├── timeframes.rs
│   └── instrument_types.rs
│
└── errors/                    # 统一错误处理
    ├── mod.rs
    ├── app_error.rs          # [迁移并增强]
    ├── domain_error.rs       # 领域错误
    └── infrastructure_error.rs
```

---

## 🔄 迁移路径（渐进式重构）

### **阶段一：基础设施层重构（1-2周）**
✅ **优先级：高** - 为后续重构打基础

1. **创建 `infrastructure/` 目录结构**
   ```bash
   mkdir -p src/infrastructure/{persistence,messaging,cache,config,scheduler}
   ```

2. **迁移配置模块**
   - `app_config/` → `infrastructure/config/`
   - 重命名为更规范的结构

3. **迁移 WebSocket 服务**
   - `socket/` → `infrastructure/messaging/websocket/`

4. **整合任务调度**
   - `job/` + `trading/task/` → `infrastructure/scheduler/`

### **阶段二：领域层拆分（2-3周）**
✅ **优先级：高** - 核心业务逻辑解耦

1. **创建领域边界**
   ```bash
   mkdir -p src/domain/{market,strategy,risk,order,shared}
   ```

2. **迁移市场数据**
   - `trading/model/market/` → `domain/market/entities/`
   - `trading/domain_service/candle_domain_service.rs` → `domain/market/services/`

3. **迁移策略逻辑**
   - `trading/strategy/` → `domain/strategy/strategies/`
   - `trading/indicator/` → `domain/strategy/indicators/`
   - 重新组织为 trend/momentum/volatility/volume 子类别

4. **提取风控领域**
   - 从 `job/risk_*.rs` 提取核心逻辑 → `domain/risk/`

### **阶段三：应用层构建（1-2周）**
✅ **优先级：中** - 编排业务流程

1. **创建 CQRS 模式**
   ```bash
   mkdir -p src/application/{commands,queries,services}
   ```

2. **迁移业务编排**
   - `trading/services/` → `application/services/`
   - 拆分为 Commands 和 Queries

### **阶段四：共享层整理（1周）**
✅ **优先级：低** - 清理和优化

1. **迁移工具和类型**
   - `time_util.rs` → `shared/utils/time_util.rs`
   - `trading/utils/` → `shared/utils/`
   - `trading/types.rs` → `shared/types/`

---

## 📋 迁移检查清单

### **关键文件迁移映射**

| 当前位置 | 目标位置 | 说明 |
|---------|---------|------|
| `app_config/` | `infrastructure/config/` | 配置管理 |
| `socket/` | `infrastructure/messaging/websocket/` | WebSocket服务 |
| `job/` | `infrastructure/scheduler/jobs/` | 定时任务 |
| `trading/task/` | `infrastructure/scheduler/jobs/` | 任务执行器 |
| `trading/model/market/` | `domain/market/entities/` | 市场数据实体 |
| `trading/strategy/` | `domain/strategy/strategies/` | 策略实现 |
| `trading/indicator/` | `domain/strategy/indicators/` | 技术指标 |
| `trading/services/` | `application/services/` | 应用服务 |
| `trading/domain_service/` | `domain/*/services/` | 领域服务 |
| `time_util.rs` | `shared/utils/time_util.rs` | 时间工具 |
| `error/` | `shared/errors/` | 错误处理 |

---

## ⚠️ 风险评估与缓解

### **潜在风险**

1. **🔴 重构周期长** - 4-6周全量迁移
   - **缓解**：采用渐进式迁移，保证每个阶段可独立测试

2. **🟡 测试覆盖不足**
   - **缓解**：在重构前补充关键路径的集成测试

3. **🟡 循环依赖风险**
   - **缓解**：严格遵守依赖方向：Domain ← Application ← Infrastructure

### **回滚策略**

- 使用 Git Feature Branch 进行重构
- 每个阶段完成后合并主分支
- 保留旧代码的 `deprecated/` 目录作为参考

---

## 🎯 重构后预期收益

### **代码质量提升**
- ✅ 模块职责清晰，单一职责原则
- ✅ 依赖方向明确，避免循环依赖
- ✅ 领域逻辑与技术实现解耦

### **可维护性提升**
- ✅ 新增策略只需修改 `domain/strategy/strategies/`
- ✅ 切换数据库只需修改 `infrastructure/persistence/`
- ✅ 测试隔离度高，Mock 成本低

### **可扩展性提升**
- ✅ 支持多交易所（只需扩展 `infrastructure/external_api/`）
- ✅ 支持多种部署模式（单体/微服务）
- ✅ 支持插件化策略开发

---

## 📚 参考资料

- [领域驱动设计（DDD）](https://martinfowler.com/bliki/DomainDrivenDesign.html)
- [整洁架构（Clean Architecture）](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html)
- [CQRS 模式](https://martinfowler.com/bliki/CQRS.html)
- [Rust 项目结构最佳实践](https://doc.rust-lang.org/cargo/guide/project-layout.html)

---

## 🔧 下一步行动

1. **评审本方案**：团队确认重构目标和分层逻辑
2. **补充单元测试**：为核心模块添加测试覆盖
3. **创建迁移分支**：`git checkout -b refactor/ddd-architecture`
4. **开始阶段一**：基础设施层重构

---

**版本**: v1.0  
**日期**: 2025-11-06  
**作者**: AI Assistant

