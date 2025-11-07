# 未迁移内容分析报告

## src/目录未迁移模块清单

### 应用启动模块 (src/app/)
```
src/app/
├── bootstrap.rs        应用启动流程，包含run_modes()等
└── mod.rs
```
**迁移状态**: 部分逻辑在crates/rust-quant-cli，需要完整迁移
**目标位置**: crates/rust-quant-cli/

### 配置模块 (src/app_config/)
```
src/app_config/
├── db.rs              数据库配置
├── email.rs           邮件配置  
├── env.rs             环境变量
├── log.rs             日志配置
├── redis_config.rs    Redis配置
└── shutdown_manager.rs 优雅关闭
```
**迁移状态**: 功能已在crates/core/，但src/中仍被引用
**目标位置**: 完全使用crates/core/

### 任务模块 (src/job/)
```
src/job/
├── announcements_job.rs      公告任务
├── risk_banlance_job.rs      风险余额任务
├── risk_order_job.rs         风险订单任务
├── risk_positon_job.rs       风险持仓任务
├── task_classification.rs    任务分类
└── task_scheduler.rs         任务调度
```
**迁移状态**: 未迁移
**目标位置**: crates/orchestration/workflow/

### WebSocket模块 (src/socket/)
```
src/socket/
├── websocket_service.rs      WebSocket服务
└── mod.rs
```
**迁移状态**: 未迁移
**目标位置**: crates/market/streams/

### 大量trading子模块 (src/trading/)
```
src/trading/
├── task/               17个任务文件（未迁移到orchestration）
├── services/           9个服务文件（未迁移到services包）
├── strategy/           15个策略文件（已部分迁移到strategies）
├── indicator/          大量indicator（已迁移到indicators）
├── model/              大量模型（需迁移到domain/infrastructure）
└── 其他子模块
```
**迁移状态**: 部分已迁移，大量代码仍在src/

## main.rs业务逻辑分析

### 核心业务流程

#### 1. app_init()
**功能**: 
- 环境变量加载
- 日志初始化
- 数据库连接池初始化
- Redis连接池初始化

**迁移状态**:
```
旧位置: src/lib.rs::app_init()
新位置: crates/rust-quant-cli/src/lib.rs::app_init()
迁移完成度: 100% ✅

对应新架构:
- 日志: rust_quant_core::logger::setup_logging()
- 数据库: rust_quant_core::database::init_db_pool()
- Redis: rust_quant_core::cache::init_redis_pool()
```

#### 2. 调度器初始化
**功能**: 创建全局JobScheduler

**迁移状态**:
```
旧位置: src/lib.rs::init_scheduler()
新位置: crates/rust-quant-cli::init_scheduler()
迁移完成度: 100% ✅
```

#### 3. run_modes() - 5种运行模式

##### 模式1: 数据同步 (IS_RUN_SYNC_DATA_JOB)
**功能**:
- init_all_ticker() - 初始化ticker
- run_sync_data_job() - 同步K线数据

**迁移状态**:
```
旧位置: src/trading/task/tickets_job.rs
        src/trading/task/basic.rs
        
新位置检查:
- crates/orchestration/workflow/tickets_job.rs ✅ 存在
- crates/orchestration/workflow/candles_job.rs ✅ 存在

迁移完成度: 90%
未完成: src/中的代码仍被bootstrap.rs引用
```

##### 模式2: Vegas回测 (IS_BACK_TEST)
**功能**: 
- task::basic::back_test() - 执行Vegas回测

**迁移状态**:
```
旧位置: src/trading/task/basic.rs::back_test()
        src/trading/task/backtest_executor.rs

新位置检查:
- crates/strategies/backtesting/ ✅ 框架存在
- crates/orchestration/workflow/backtest_executor.rs ✅ 存在

迁移完成度: 70%
问题: backtesting具体实现不完整
```

##### 模式3: NWE回测 (IS_BACK_TEST_NWE)
**功能**:
- task::basic::back_test_with_config() - NWE回测

**迁移状态**:
```
旧位置: src/trading/task/basic.rs::back_test_with_config()

新位置检查:
- crates/strategies/backtesting/ ✅ 框架存在
- NWE策略在 crates/strategies/implementations/nwe_executor.rs

迁移完成度: 70%
```

##### 模式4: WebSocket实时数据 (IS_OPEN_SOCKET)
**功能**:
- socket::websocket_service::run_socket()

**迁移状态**:
```
旧位置: src/socket/websocket_service.rs

新位置检查:
- crates/market/streams/websocket_service.rs ✅ 存在

迁移完成度: 50%
问题: src/socket/仍被引用，未完全迁移
```

##### 模式5: 实盘策略 (IS_RUN_REAL_STRATEGY)
**功能**:
- RiskBalanceWithLevelJob - 风险控制
- strategy_manager.start_strategy() - 启动策略

**迁移状态**:
```
旧位置: src/job/risk_banlance_job.rs
        src/trading/strategy/strategy_manager.rs

新位置检查:
- crates/orchestration/workflow/risk_banlance_job.rs ✅ 存在
- crates/strategies/framework/strategy_manager.rs ✅ 存在

迁移完成度: 80%
问题: job文件在orchestration，但src/job/仍被引用
```

#### 4. 心跳和监控
**功能**: 定期输出运行状态

**迁移状态**:
```
旧位置: src/app/bootstrap.rs (内联代码)
新位置: 需要提取到orchestration/monitoring/
迁移完成度: 0%
```

#### 5. 优雅关闭
**功能**:
- 信号处理
- 停止策略
- 关闭调度器
- 清理资源

**迁移状态**:
```
旧位置: src/lib.rs::graceful_shutdown_with_config()
新位置检查:
- crates/core/config/shutdown_manager.rs ✅ ShutdownManager存在
- crates/rust-quant-cli/ 需要集成

迁移完成度: 80%
```

## 业务逻辑迁移验证

### 已成功迁移的业务逻辑 ✅

1. **K线数据持久化** ✅
   - 旧: src/trading/services/candle_service/
   - 新: crates/market/repositories/candle_service.rs
   - 状态: ORM已迁移 (rbatis→sqlx)

2. **策略框架** ✅
   - 旧: src/trading/strategy/strategy_manager.rs
   - 新: crates/strategies/framework/strategy_manager.rs
   - 状态: 已迁移

3. **技术指标计算** ✅
   - 旧: src/trading/indicator/
   - 新: crates/indicators/
   - 状态: 9个核心模块已迁移

4. **订单模型** ✅
   - 旧: src/trading/model/order/
   - 新: crates/risk/order/
   - 状态: ORM已迁移 (rbatis→sqlx)

5. **策略配置管理** ✅
   - 旧: src/trading/model/strategy/strategy_config.rs
   - 新: crates/infrastructure/repositories/strategy_config_repository.rs
   - 状态: ORM已迁移，功能完整

### 部分迁移的业务逻辑 🟡

1. **回测引擎** 🟡
   - 旧: src/trading/task/backtest_executor.rs (完整实现)
   - 新: crates/strategies/backtesting/engine.rs (仅框架)
   - 问题: 具体回测逻辑未迁移
   - 缺失: run_vegas_test(), run_nwe_test()等实现

2. **WebSocket服务** 🟡
   - 旧: src/socket/websocket_service.rs (完整)
   - 新: crates/market/streams/websocket_service.rs (已存在)
   - 问题: bootstrap.rs仍引用src/socket/

3. **任务调度** 🟡
   - 旧: src/job/*.rs (5个job文件)
   - 新: crates/orchestration/workflow/ (已有相同文件)
   - 问题: src/job/仍被引用

### 未迁移的业务逻辑 ❌

1. **应用启动流程** ❌
   - src/app/bootstrap.rs::run()
   - src/app/bootstrap.rs::run_modes()
   - 核心编排逻辑，未迁移

2. **风险任务** ❌
   - src/job/risk_banlance_job.rs
   - src/job/risk_order_job.rs
   - src/job/risk_positon_job.rs
   - 虽然orchestration有同名文件，但src/仍被使用

3. **数据同步任务** ❌
   - src/trading/task/data_sync.rs
   - src/trading/task/candles_job.rs
   - src/trading/task/tickets_job.rs

4. **策略相关服务** ❌
   - src/trading/services/strategy_data_service.rs
   - src/trading/services/strategy_metrics.rs
   - src/trading/services/strategy_system_error.rs

5. **缓存服务** ❌
   - src/trading/cache/latest_candle_cache.rs
   - src/trading/strategy/arc/

## 未迁移TODO清单

### P0 - 阻塞性问题 (必须迁移)

#### TODO-1: 迁移app/bootstrap.rs
**当前**: src/app/bootstrap.rs (267行)
**目标**: crates/rust-quant-cli/src/app.rs
**内容**:
- run() 主流程
- run_modes() 模式编排
- setup_shutdown_signals() 信号处理
**工作量**: 2-3小时

#### TODO-2: 更新main.rs引用
**当前**: src/main.rs引用src/lib.rs
**目标**: 引用crates/rust-quant-cli
**工作量**: 30分钟

#### TODO-3: 迁移job模块到orchestration
**当前**: src/job/*.rs (5个文件)
**目标**: 确认orchestration/workflow/中的文件可用，删除src/job
**工作量**: 1-2小时验证和切换

#### TODO-4: 迁移socket到market
**当前**: src/socket/websocket_service.rs
**目标**: 使用crates/market/streams/websocket_service.rs
**工作量**: 1小时

### P1 - 重要但非阻塞

#### TODO-5: 迁移trading/task到orchestration
**当前**: src/trading/task/*.rs (17个文件)
**目标**: 验证orchestration/workflow/，删除src/trading/task
**工作量**: 2-3小时

#### TODO-6: 迁移trading/services
**当前**: src/trading/services/*.rs (9个服务)
**目标**: crates/services/(新包)或各业务包
**工作量**: 3-4小时

#### TODO-7: 迁移trading/cache
**当前**: src/trading/cache/
**目标**: crates/infrastructure/cache/
**工作量**: 1小时

#### TODO-8: 删除src/app_config
**当前**: src/app_config/ (功能已在core)
**目标**: 删除，全部使用rust_quant_core
**工作量**: 1小时验证后删除

### P2 - 清理工作

#### TODO-9: 删除src/trading/indicator
**状态**: 已迁移到crates/indicators/
**工作**: 验证后删除

#### TODO-10: 删除src/trading/strategy
**状态**: 已迁移到crates/strategies/
**工作**: 验证后删除

#### TODO-11: 删除src/trading/model
**状态**: 已迁移到domain/infrastructure
**工作**: 验证后删除

## 业务逻辑完整性验证

### 关键业务流程对比

#### 流程1: 应用启动
**旧实现** (src/app/bootstrap.rs::run):
```rust
1. init_scheduler() - 初始化调度器
2. validate_system_time() - 校验时间
3. run_modes() - 运行模式编排
4. 心跳任务
5. 信号处理
6. 优雅关闭
```

**新实现** (crates/rust-quant-cli):
```rust
1. init_scheduler() ✅ 已实现
2. validate_system_time() ❌ 未迁移
3. run_modes() ❌ 未迁移
4. 心跳 ❌ 未迁移
5. 信号处理 ❌ 未迁移
6. 优雅关闭 ✅ ShutdownManager已在core
```

**完整度**: 40%

#### 流程2: 数据同步
**旧实现** (src/trading/task/):
```rust
- run_sync_data_job() - 同步K线
- init_all_ticker() - 初始化ticker
- sync_top_contract() - 同步大数据
```

**新实现** (crates/orchestration/workflow/):
```rust
- candles_job.rs ✅ 存在
- tickets_job.rs ✅ 存在
- big_data_job.rs ✅ 存在
```

**完整度**: 90% (文件已迁移，需验证功能一致性)

#### 流程3: 回测执行
**旧实现** (src/trading/task/backtest_executor.rs):
```rust
- run_vegas_test() - Vegas回测
- run_nwe_test() - NWE回测
- run_back_test_strategy() - 通用回测
- 大量回测逻辑（~500行）
```

**新实现** (crates/strategies/backtesting/):
```rust
- engine.rs - 回测引擎框架（~80行）
- metrics.rs - 性能指标（~90行）
```

**完整度**: 30% (框架存在，具体实现缺失)
**缺失**: 完整的回测逻辑实现

#### 流程4: 实盘策略执行
**旧实现** (src/app/bootstrap.rs::run_modes):
```rust
1. RiskBalanceWithLevelJob::run() - 风险控制初始化
2. strategy_manager.start_strategy() - 启动策略
```

**新实现**:
```rust
1. crates/orchestration/workflow/risk_banlance_job.rs ✅
2. crates/strategies/framework/strategy_manager.rs ✅
```

**完整度**: 85% (核心逻辑已迁移)

#### 流程5: WebSocket实时数据
**旧实现** (src/socket/websocket_service.rs):
```rust
- run_socket() - 启动WebSocket
- 处理实时K线数据
```

**新实现** (crates/market/streams/websocket_service.rs):
```rust
- 已存在WebSocketService
```

**完整度**: 80% (功能已迁移，bootstrap仍引用旧路径)

## 问题总结

### 关键问题

1. **src/app/bootstrap.rs未迁移**
   - 这是应用的核心启动逻辑
   - run_modes()是5种模式的编排入口
   - 影响: main.rs无法使用新架构

2. **双重实现并存**
   - src/trading/task/和crates/orchestration/workflow/都有相同文件
   - src/job/和crates/orchestration/workflow/都有相同文件
   - 导致: 不确定使用哪个版本

3. **src/模块仍被bootstrap引用**
   - bootstrap.rs大量引用src/下的模块
   - 需要: 更新为crates/下的新路径

### 迁移优先级

**立即处理** (P0):
- TODO-1: 迁移bootstrap.rs
- TODO-2: 更新main.rs
- TODO-3: 统一job模块
- TODO-4: 统一socket模块

**短期处理** (P1):
- TODO-5: 统一task模块
- TODO-6: 迁移services
- TODO-7: 迁移cache

**清理工作** (P2):
- TODO-8至TODO-11: 删除已迁移的src/模块

## 建议方案

### 方案A: 完整迁移bootstrap和main (4-6h)

**步骤**:
1. 迁移bootstrap.rs到rust-quant-cli
2. 更新所有模块引用路径
3. 验证5种模式全部工作
4. 删除src/下已迁移模块

**结果**: 100%使用新架构

### 方案B: 保持双轨制

**说明**: 
- src/main.rs继续使用旧代码
- crates/rust-quant-cli/作为新架构入口
- 逐步迁移

**问题**: 维护两套代码

## 结论

**src/目录迁移完成度**: 40%

**核心未迁移内容**:
1. src/app/bootstrap.rs - 应用启动流程（关键）
2. src/job/ - 5个任务文件
3. src/socket/ - WebSocket服务
4. src/trading/task/ - 17个任务文件
5. src/trading/services/ - 9个服务文件

**业务逻辑一致性**: 
- 已迁移部分: 架构更清晰，但具体实现有缺失
- 未迁移部分: 仍在src/中工作

**建议**: 
执行方案A，完整迁移bootstrap.rs，统一使用新架构。

