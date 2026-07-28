# ADR-0003：采用明确的运行入口与组合根

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-23
- 决策者：Rust Quant Core

## 背景

当前主 CLI 同时承担 internal server、行情同步、策略运行、execution worker、readiness、维护任务和研究入口，并通过大量环境变量决定进程角色。单个入口因此依赖大量 crate，初始化、资源占用、部署合同和关闭行为难以独立验证。

生产已经通过同一镜像运行多个职责容器，可以在不立即增加镜像矩阵的前提下，让二进制职责与容器职责对齐。但“同一镜像”仅指六个生产角色共享 `core-runtime`，不允许 Research、Backtest、Paper 研究入口或维护工具继续混入生产工件。

第一版方案为每个业务模块都安排 Worker，但业务模块与运行进程并不一一对应。Portfolio 和事前 Risk 通常处于同一低延迟调用链；过早拆成 Worker 会增加消息、一致性和运维成本。

## 决策

建立以下默认 App 组合根：

- `control-api`；
- `market-worker`；
- `signal-worker`；
- `account-worker`；
- `execution-worker`；
- `reconciliation-worker`；
- `schema-tool`；
- `quant-lab`。

职责：

- `market-worker` 只装配公共 Market 数据读取；平台固定的 `MarketDataAccessCredential` 仅在其 App/`exchange-gateway` 公共数据 Adapter 边界使用，不能装配私有账户或 mutation capability；
- `signal-worker` 装配 Market -> StrategySignal 的确定性链路，不为未知账户计算最终仓位；
- `account-worker` 消费成交、余额、持仓和保证金事实，并可在初期装配持续 Risk；
- `execution-worker` 只消费 Web 已创建并经 owner Contract lease 的用户 `ExecutionRequest`，装配账户级 Portfolio、事前 Risk、OMS、外部 mutation 和保护单；不存在系统自营请求路径，Portfolio/Risk 的 owner 不因此转移给 Execution；
- `reconciliation-worker` 比较交易所与内部事实并发送 typed owner command；
- `control-api` 管理发布与运行快照，不提交真实订单；
- `quant-lab` 只负责装配 Research use case、历史数据和证据 Adapter，不直接实现回测交易规则。

不默认建立 `portfolio-worker` 或 `risk-worker`。只有独立轮询/流消费、吞吐、扩缩容、故障隔离、安全边界或部署生命周期证据出现时，才通过 ADR 增加。

每个 App 是独立 Cargo package，只装配本职责需要的 Use Case、Port 实现、Contract mapper、配置、日志、健康检查和关闭流程。Worker Use Case 提供 `run_once` 或单消息处理入口；循环、轮询、取消、timeout 和 shutdown 由 App/Platform 负责。

六个生产 App binary 进入 `core-runtime` Release Unit；`schema-tool` 进入 `core-maintenance`；`quant-lab` 与 Research/Backtest/Paper 研究入口进入 `quant-lab` Release Unit。工件内容、依赖图和 CI/CD 影响判断以 [ADR-0010](0010-build-impact-and-artifact-isolation.md) 为准。

环境变量只允许在 App 或 Platform configuration 边界读取并解析为强类型配置。业务 Domain 接收配置对象或不可变 Runtime Snapshot。

## 结果

### 正面影响

- 每个生产进程职责、依赖和资源占用可独立观察；
- 避免业务模块数量直接变成进程数量；
- 事前交易链路不需要为 Portfolio/Risk 增加网络跳数；
- Account 投影、Execution 与 Reconciliation 生命周期清晰；
- 同一镜像仍可打包多个明确二进制。

### 代价

- 增加少量 App crate 和装配代码；
- execution-worker/account-worker 装配多个 Domain，需要确保只调用公开 API；
- Platform 必须保持足够薄，避免形成新 `core`；
- compose、Dockerfile、部署脚本和 CI 必须随入口迁移同步更新。

## 被否决的方案

### 保留单一二进制与模式变量

无法隔离依赖、初始化和进程职责，新增角色会继续扩大 bootstrap。

### 每个 Domain 一个 Worker

把业务模块误当部署单元，提前增加消息和一致性成本。

### 拥有所有业务条件的中央 Scheduler

会把 God Crate 从 CLI 转移到 Scheduler。Scheduler 只能触发 typed command，不能判断策略、风险或执行规则。

### 每个策略独立 Worker

会导致部署单元爆炸。默认使用通用 signal-worker 与显式 Definition/Registry。

### 立即把六个生产角色拆成独立镜像

当前需要职责隔离，不需要增加六倍镜像发布矩阵。同一 `core-runtime` 镜像可以包含六个明确生产二进制，但 Research/Backtest 与维护工件必须物理分离。

## 兼容与迁移

- 第一阶段保留现有二进制名称、compose command 和环境变量映射；
- 新 App 先包装现有行为，不在入口拆分时改变策略或执行语义；
- 每迁移一个入口，同步 Dockerfile、compose、部署/回滚脚本和 deploy contract tests；
- 线上 revision、健康、日志和任务证据确认后，才删除旧模式分支。
