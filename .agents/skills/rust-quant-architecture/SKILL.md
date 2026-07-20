---
name: rust-quant-architecture
description: 约束 rust_quant 交易系统的领域归属、Rust 模块拆分、业务逻辑位置、数据库 CRUD、Ports/Adapters、Research 与回测、Vegas 迁移、生产运行和 AI 架构防腐。用于设计、评审、实现或重构 rust_quant 后端模块、策略/组合/风险/执行链路、ResearchBar/PaperEvent/RecoveryHarness、数据访问、运行入口和架构文档；也用于判断新代码应该放在哪里以及检查变更是否违反目标架构。
---

# Rust Quant 架构规范

## 目标

把架构文档转换成每次改动都能执行的放置、依赖和验证流程。优先维护业务 owner、时序正确性、交易安全和研究证据可信度，不为迁就 legacy 反向污染目标架构。

## 先定位仓库

1. 将 `/Users/mac2/onions/crypto_quant` 视为 umbrella workspace。
2. 将 `rust_quant/` 视为 Core owning repo；只在该目录执行 Git、Cargo、测试和提交。
3. 先检查当前分支、工作树和目标文件，保留用户已有改动。
4. 涉及 Web、Admin、News 或交易所 SDK 时，先确认 owner；跨仓库只使用 owner service API 或稳定 contract，不新增跨库读写。

## 按任务加载权威文档

先阅读[架构索引](../../../docs/architecture/README.md)，再按任务读取下列文件。不要只凭本 Skill 的摘要替代原文。

| 任务 | 必读文档 |
| --- | --- |
| 新模块、领域拆分、通用架构评审 | [目标架构](../../../docs/architecture/target-architecture.md)、[依赖规则](../../../docs/architecture/dependency-rules.md) |
| 业务逻辑、CRUD、SQL、事务、Consumer | [业务代码与数据访问](../../../docs/architecture/business-code-and-data-access.md)、[ADR-0007](../../../docs/architecture/adr/0007-owner-scoped-persistence-and-transaction-boundaries.md) |
| Research、Vegas、回测或模拟交易 | [ADR-0009](../../../docs/architecture/adr/0009-research-domain-and-tiered-simulation.md)、[Vegas 迁移实战](../../../docs/architecture/vegas-backtest-migration.md)、[通用量化逻辑归属](../../../docs/architecture/common-logic-placement.md) |
| Worker、订单、保护单、对账与恢复 | [生产运行与恢复](../../../docs/architecture/production-runtime.md)、[ADR-0004](../../../docs/architecture/adr/0004-portfolio-and-trading-domain-boundaries.md)、[ADR-0006](../../../docs/architecture/adr/0006-at-least-once-idempotency-and-recovery.md) |
| 新增代码、迁移 legacy、架构 Review | [AI 架构护栏](../../../docs/architecture/ai-coding-guardrails.md)、[迁移计划](../../../docs/architecture/migration-plan.md) |

ADR-0008 只保留为决策历史。遇到 Research 或 backtest 设计冲突时，以 ADR-0009 为准。

## 执行流程

### 1. 明确任务边界

区分当前请求是分析、诊断、设计、代码修改、迁移还是运行态验证。分析和诊断请求保持只读；只有用户要求修改时才写文件。实盘 mutation 必须取得明确授权。

先声明假设、歧义和成功标准。存在多个合理 owner 或会改变产品语义时，停止并说明选择影响。

### 2. 提交代码放置声明

新增或移动代码前输出：

```text
变更：
Owner：Market / Strategy / Portfolio / Account / Risk / Execution / Reconciliation / Research
切片：Command / Query / Event Consumer / Pure Policy / Simulation Kernel
入口：
Use Case：
Model / Policy：
Ports：
Adapters：
事务原子性：
跨进程 Contract：无 / 名称与版本
运行入口：
恢复 Owner：
验证：unit / integration / contract / parity / recovery
```

不能唯一填写时，不要先创建 `common`、`service`、`repository` 或万能 DTO。

### 3. 从真实调用链反向验证

使用当前代码、测试、数据库 contract 和运行入口复核文档假设。标记 legacy 边界，不把历史目录当成目标 owner。

对 Vegas 或回测至少追踪：

```text
入口
  -> Experiment / BacktestRun
  -> DatasetManifest
  -> StrategyRuntimeSnapshot
  -> Strategy Evaluator + scoped state
  -> decision-time barrier
  -> PortfolioTarget
  -> RiskDecision
  -> OrderPlan
  -> SimulationLedger / event simulation
  -> Analytics
  -> Completed ResearchEvidence
```

### 4. 实施最小垂直切片

优先完成一个可验证的 owner slice，不横向搬完整目录：

1. 定义内部 Input/Output 和业务 identity；
2. 在 Model/Policy 中实现不变量或纯决策；
3. 在 Use Case 中编排业务动作；
4. 以业务语言定义 Port；
5. 在 Adapter 中实现 SQL、HTTP、Redis、对象存储或交易所协议；
6. 在 App 中完成配置、依赖注入和循环；
7. 增加对应层级测试与迁移/删除条件。

不要顺手清理相邻 legacy，不建立无真实调用方的兼容层或扩展点。

### 5. 按风险验证

- Pure Model/Policy：单元测试、边界测试、确定性测试；
- CRUD/事务：Postgres 集成测试、幂等、锁/版本、索引和注释检查；
- Contract：producer/consumer 快照与版本兼容测试；
- Vegas/Research：逐层 parity、严格时序、成本、Seed/Manifest 重放和 symbol 重排不变性；
- Execution：部分成交、撤单竞态、Unknown、保护数量、lease、outbox 和恢复测试；
- 运行入口：配置、compose、启动/关闭和 deploy contract；
- 多步骤闭环：同步相关架构/迁移文档、`task_plan.md` 与 `AGENT_PROGRESS.md`。

没有新鲜测试或运行态证据时，不声称完成、生产可用或可晋级。

## 硬边界

### Domain 与 Quant

- 业务规则只属于明确 Domain；Domain 不依赖 Wire Contract 或具体 Adapter。
- `quant/math`、`quant/indicators`、`quant/backtest`、`quant/analytics` 只包含 owner 无关的纯机制。
- `quant/backtest` 只提供确定性时钟、事件调度、Replay、撮合和费用/滑点/资金费模型；不得依赖 Domain、数据库、环境变量或真实交易所。
- Research 是终端离线 Domain，可以通过稳定公开 API 编排 Market、Strategy、Portfolio、Risk、Execution 和 Quant；生产 Domain 不得依赖 Research。
- App 只做组合根、配置和运行循环，不承载交易规则。

### Strategy 与 Research

- Strategy 拥有 StrategyDefinition、StrategyArtifact、StrategyRelease 和 StrategyRuntimeSnapshot。
- Research 拥有 Experiment、BacktestRun、RunCheckpoint、DatasetManifest、SimulationProfile、SimulationLedger 和 ResearchEvidence。
- Strategy Release 只能引用 Completed ResearchEvidence，不复制、覆盖或接管研究证据。
- Evaluation state 使用：

```text
EvaluationScopeId + StrategyRuntimeSnapshotId + MarketStreamPartition
```

并行 run 或 deployment generation 不得共享可变 evaluator state。

### 三种模拟精度

- ResearchBar：验证 Strategy、Portfolio、Risk、OrderPlan 和成本后绩效；不声称覆盖 lease、outbox、Unknown 或恢复。
- PaperEvent：模拟 Ack、PartialFill、Reject、Cancel、Protection 和延迟，并复用 Execution 纯状态迁移。
- RecoveryHarness：使用 disposable storage 和 fault injection 验证 lease、outbox、Unknown、重放、保护与 Reconciliation；不作为策略收益证据。
- 多币种先收集同一 `decision_time` 的全部候选，再统一执行排序、净额、容量和风险；symbol 输入重排不得改变结果。
- SimulationLedger 不得写入生产 AccountProjection、Order 或 Fill 事实表。

### 数据访问

- Handler、Scheduler、Consumer 不直接执行 SQL 或调用交易所 SDK。
- Use Case 定义业务原子性，Port 使用业务动作命名，Postgres Adapter 实现事务、Row、SQL、锁和错误映射。
- 禁止 `Repository<T>`、`BaseService`、`update_by_id`、无条件 upsert 和 runtime DDL。
- 同 owner 的状态、幂等/Inbox、Outbox 和审计事实在单一数据库事务中提交。
- 跨 owner 使用本地事务 + Outbox/Inbox + 幂等 + 补偿/Reconciliation，不使用跨域大事务。
- ResearchEvidence 先按内容哈希上传不可变对象，再由 Research owner 数据库事务发布 manifest、引用、指标、幂等和 Completed；只保证原子可见，不虚构跨存储全局原子事务。
- 新表和新列必须有数据库原生注释；每条 SQL 都检查索引、基数、锁和扫描成本。

### 交易安全

- 区分 read-only preflight、dry-run、paper/sim 和 live mutation。
- 未经明确授权，不下单、撤单、平仓或修改交易所账户。
- 实盘订单必须先验证凭证、权限、symbol filters、数量精度、风险、worker lease 和保护单计划；没有止损不允许下单。
- 研究收益、胜率、Sharpe、回撤和 PnL 必须带数据范围、版本、费用、滑点、资金费和时序证据。

## Review 输出格式

架构评审按以下顺序输出：

1. 结论：可接受 / 需调整 / 阻塞；
2. Owner 与目标代码位置；
3. 当前实现证据和 legacy 差异；
4. 违反的依赖、数据、时序或运行边界；
5. 最小修订方案；
6. 必须补充的测试、迁移和运行证据。

把“目录看起来整齐”与“真实业务闭环可验证”分开判断。最终必须能回答：谁拥有事实、谁作决策、谁持久化、谁恢复，以及 backtest/paper/live 在哪一层保持 parity。
