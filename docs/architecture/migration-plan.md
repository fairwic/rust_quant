# Rust Quant 架构迁移计划

- 状态：计划中
- 首次制定：2026-07-18
- 最近修订：2026-07-21
- 目标架构：[Rust Quant 长期目标架构](target-architecture.md)
- 数据访问规则：[业务代码与数据访问放置规范](business-code-and-data-access.md)

## 1. 目的

本文只描述现有实现如何迁入目标架构。兼容层不得成为长期模型；每个兼容入口必须有真实调用方、测试、owner、删除条件和复查日期。

当前已确认的 legacy 基线包括扁平 `common/core/domain/infrastructure/services/orchestration` crate、单一 CLI 多运行角色，以及 Web `execution_tasks/exchange_order_results` 同时承载商业交接与交易结果的历史边界。迁移必须正面处理这些事实，不能只移动目录。

## 2. 迁移原则

- 先冻结行为和数据证据，再移动代码；
- 以一个可运行的 vertical slice 为单位迁移，不按“先迁完所有 Model/Repository/Service”横向批量搬家；
- 目录变化与策略、风控、订单参数变化分开；
- 新旧实现先 shadow/parity 对比，再切单一事实源；
- 每个切片都明确 owner、表、Contract、运行入口、回滚和 legacy 删除门；
- 小型 legacy bugfix 可留原位，但不能新增依赖或扩大旧抽象；
- 不通过跨库读取、共享 ORM、万能 Service 或中央 Scheduler 缩短路径；
- 未获得显式实盘授权时，迁移验证只使用 contract、fixture、paper、dry-run、shadow 和 signed read-only。

## 3. 每个迁移切片的固定清单

每个切片开始前填写：

| 项目 | 必填内容 |
| --- | --- |
| 业务目标 | 用户/系统可验证的单一结果 |
| Owner | 唯一 Domain 与跨仓库 owner |
| 当前入口 | 现有调用方、进程、API/消息 |
| 当前数据 | 表、字段、唯一键、状态机、数量级 |
| 目标路径 | App -> Use Case -> Model/Policy -> Port -> Adapter |
| Contract | 当前版本、目标版本、兼容窗口 |
| 原子性 | opening slot/业务唯一约束、状态、完整计划、审批引用、幂等、Outbox 必须一起写入的内容；claim/outcome 事务与外部 I/O 时点 |
| Shadow | 新旧实现如何对比且不产生双副作用 |
| 切换 | 读切换、写切换、feature flag/release generation |
| 回滚 | 回滚入口、数据兼容和允许时限 |
| 删除门 | 调用方、配置、表、监控和白名单的删除条件 |
| 验证 | unit、integration、contract、parity、recovery、deploy contract |

没有填写完整清单，不开始迁移。

## 4. 阶段 0：冻结当前基线与 Owner Ledger

- 记录 Cargo 内部依赖图和已知违规基线；
- 记录生产二进制、容器 command、配置、数据库和启动依赖；
- 为 Strategy Signal、Web Execution Task、Readiness、订单结果和 internal API 建立 Contract snapshot；
- 记录 `quant_core` 与 `quant_web` 当前表 owner、写入者、读取者和数据量；
- 为关键策略建立固定输入下的 evaluator、portfolio、risk 和 execution parity 基线；
- 为 Vegas 固定 DatasetManifest、Strategy Runtime Snapshot、SimulationProfile、指标预热长度、风险/资金配置、费用、滑点与随机 Seed；
- 记录 Vegas 当前 backtest、paper/live 的实际窗口差异、状态缓存 identity 和信号字段差异；
- 记录订单状态、lease、重试、保护单和恢复的现有行为。

验证：同一基线可以在迁移前后重复执行，并能识别策略结论、订单参数、状态机和 Contract 漂移。

## 5. 阶段 1：先建立防腐骨架，不搬业务

- 建立 `apps` 与 `crates/{domains,quant,contracts,adapters,platform}` 目录约定；
- 建立最小 command/query/event-consumer 三个模板；
- 建立 `cargo xtask arch-check` 的只读报告；
- 保存 legacy allowlist，CI 先禁止新增违规；
- 建立一个 Postgres Adapter crate 的 owner module 骨架；
- 建立 Research Domain 最小骨架，只含 Experiment/Run/Evidence identity、Port 与状态机，不先搬回测交易逻辑；
- Migration 采用单一有序目录和 owner 文件名。

验证：不迁移任何业务行为，现有构建仍可运行；门禁只拦新增违规，不因历史债务让全仓长期红灯。

## 6. 阶段 2：建立 Golden Vertical Slice

首个 Golden Slice 选择已有真实调用方、Contract 和运行证据的 Market Velocity，但只迁移一条最小链路：

```text
MarketSnapshot
  -> Market Velocity StrategySignal
  -> Web ExecutionRequest（商业资格交接）
  -> PortfolioTarget + PreTradeSnapshot
  -> Pre-trade RiskDecision
     ├─ Dry-run
     │    -> OrderIntent + ExecutionPlan + ProtectionPlan
     │    -> Dry-run Evidence
     ├─ PaperEvent
     │    -> OrderIntent + ExecutionPlan + ProtectionPlan
     │    -> Simulated Exchange -> SimulationLedger
     └─ RecoveryHarness
          -> Risk owner 按 risk_evaluation_id 持久化不可变 RiskDecision
          -> OrderIntent + ExecutionPlan + ProtectionPlan
          -> disposable Postgres 原子取得 AccountOpeningSlot
             + 提交 SubmitPending + Idempotency + Outbox
          -> Dispatcher + MutationPermit -> Fault-injection Fenced Exchange Mutation Gateway
          -> attempt / Unknown / replay / recovery evidence
```

本阶段不改变策略条件、风险阈值、订单参数，不触发 live mutation。

该切片必须完整落地：

- Strategy Definition、Signal v1 与固定 evidence cutoff；
- Portfolio 的最小默认 policy；
- Pre-trade RiskDecision；
- Web `execution_tasks` 到 `ExecutionRequest` 的明确映射；
- Core `OrderIntent`、`ExecutionPlan`、`ProtectionPlan` 的公开 model、稳定 identity 与纯计划/状态 API；生产形状持久化只在 RecoveryHarness disposable storage 中验证；
- Dry-run 只保存带运行身份的决策/计划证据，不创建生产 Order、Outbox 或外部调用；
- PaperEvent 通过 Simulated Exchange 产生订单事件并写 `SimulationLedger`，不写生产 Order/Fill/Account 表，也不复用生产提交 Outbox；
- RecoveryHarness 单独使用 disposable Postgres 验证权威持久化顺序：Risk owner 按 evaluation identity 提交不可变审批，Execution owner 再原子取得 opening slot，并写入审批唯一绑定、`SubmitPending`、完整计划、幂等和 `OrderSubmissionRequestedV1` Outbox；
- RecoveryHarness 覆盖两个 staging worker 竞争 opening slot、slot 等待 permit/attempt、Account watermark/保护闭合后释放、cancel/Recovery revoke 与 Gateway permit consume 竞态、stale/expired permit DefinitelyNotSent、Submit/Cancel/Protect mutation generation rollover、attempt outcome 原子性、Unknown outcome 禁止同 kind 直接重投、DefinitivelyAbsent recovery 按原 kind 新建 generation Outbox、旧 delivery ack/no-op、durable blocker retry 和 Outbox 重放；
- RecoveryHarness 注入 User Stream 与 signed snapshot/query 之间的 Fill/Cancel，证明按 sequence/watermark 合并补 gap，闭合前 NotReady/Dispatcher off；
- 生产数据库连接、真实凭证与 live Exchange Adapter 在该进程装配中物理不可达；
- Postgres owner module 与生产 Dispatcher 只通过 RecoveryHarness/fault-injection Adapter 验证，不混入 Dry-run/PaperEvent 收益链路；
- signal-worker / execution-worker 的 App 装配；
- command、query、event-consumer 三类最小示例；
- parity、contract、integration 和 recovery 测试。

本阶段不得把生产 `OrderSubmissionRequestedV1` 路由给 Simulated/Dry-run Adapter：Dry-run、PaperEvent、RecoveryHarness 各守自己的保真度边界。只有 RecoveryHarness 装配生产 Outbox/Dispatcher/MutationPermit/Fenced Gateway 协议，并且生产数据库、live adapter 与真实凭证物理不可达；安全性不能只依赖 `mode=paper` 或可误配的路由开关。数据库事务提交前不得确认上游或调用外部 Adapter。

验证：新旧路径 shadow 输出在冻结输入下语义一致；Dry-run/PaperEvent 不产生生产交易事实；RecoveryHarness 中任一原子写失败时 opening slot、`SubmitPending`、完整计划和 Outbox 均不可见，permit consume/revoke 只有一方成功，stale Dispatcher 不能触达 SDK，Unknown 恢复会原子创建新 Outbox 且未完成 attempt 不被盲目重发，startup gap 不丢事件；切回 legacy 不需要回滚破坏性 schema。

Golden Slice 通过后才允许以它为模板迁移其他业务。

### 6.1 Vegas 是第二个验收切片，不替代首个 Golden Slice

Market Velocity 继续作为最小生产垂直切片；Vegas 用来证明目标架构能承载“有滚动状态、复杂规则、参数研究和回测/live parity”的真实策略。Vegas 验收切片按以下边界推进：

```text
Research::BacktestRun + DatasetManifest
  -> historical event stream
  -> Vegas Evaluator（内部 EvaluationState）
  -> 同时点 Signal barrier
  -> Portfolio allocation
  -> Pre-trade RiskDecision
  -> OrderIntent / OrderPlan
  -> ResearchBar fill model + SimulationLedger
  -> Analytics
  -> ResearchEvidence 原子可见发布
```

本切片先覆盖 backtest、paper 和 read-only shadow，不改变当前 live 默认版本，也不触发真实下单。

必须完成：

- 把 EMA/RSI/ATR 等纯指标迁入 `quant/indicators`，Vegas 入场与过滤保留在 Strategy；
- 引入 `StrategyEvaluationStateKey = EvaluationScopeId + RuntimeSnapshotId + MarketStreamPartition`，消除并行 Run 和仅按 symbol/period/type 缓存的歧义；
- Strategy evaluator 不再接收账户级 `BasicRiskStrategyConfig`；
- 将历史 `position_leverage` 的资金比例语义迁为 Portfolio `allocation_ratio`，真实交易所 leverage 单独建模；
- `quant/backtest` 只迁移确定性时钟、事件调度、撮合、费用、滑点和资金费；Research use case 驱动同一 Strategy、Portfolio、Risk 和 OrderPlan API；
- 多币种在同一 decision time 先收集全部 Signal，再统一排序、净额和分配；新增 symbol 重排不变性测试；
- 固定指标预热/最大历史窗口，解释并消除当前 backtest 与 live 的 7000/4000 等不一致；
- 保留 filtered signal、动态配置、RiskDecision、OrderDecision、trade detail 与指标证据；
- ResearchEvidence 由 Research owner 发布：先内容寻址上传大对象，再以单一数据库事务发布 Completed EvidenceManifest；
- 明确 `ResearchBar` 不覆盖 lease/outbox/Unknown/recovery；PaperEvent 与 RecoveryHarness 分别建立独立验收；
- 建立现有 pipeline 与新 pipeline 的逐事件 parity 报告，并对所有差异分类。

完整逐文件分配和切换门见 [Vegas 与现有回测主链迁移实战](vegas-backtest-migration.md)。

## 7. 阶段 3：解决 Web/Core 执行事实所有权

### 7.1 Web 保留的事实

- 用户、会员、订单、`strategy x symbol` combo；
- API credential 配置与 verified/active 状态；
- 产品资格、执行授权和 `ExecutionRequest`；
- Core 交易事实的用户展示投影。

### 7.2 Core 迁入的事实

- OrderIntent、ExecutionPlan、Order、Fill、Protection；
- client order identity、订单状态机和 Unknown；
- ReconciliationResult 与恢复任务。

### 7.3 迁移顺序

1. 冻结现有 `execution_tasks`、attempt 和 `exchange_order_results` Contract；
2. 引入 `ExecutionRequestV1`，保留旧 payload 边界映射；
3. Core 建立独立 Order/Fill/Protection owner storage；
4. Core 通过 Web owner API 更新请求状态，不直写 Web 表；
5. Web 通过 Core API/Event 建立只读结果投影；
6. shadow 对比旧 `exchange_order_results` 与 Core 事实；
7. 切换 Web 展示读取；
8. 旧结果表降级为兼容投影，调用方归零后删除或冻结写入。

验证：同一个执行请求只能生成一个稳定 Core OrderIntent；Web 投影丢失可从 Core 重建；Core 不再把 Web 表当 OMS。

## 8. 阶段 4：按业务链路继续迁移

推荐顺序：

1. Market normalization、symbol rules、quality 与 snapshot；
2. Strategy Definition、Registry、Evaluator 与 Signal；
3. Portfolio allocation、冲突和净额；
4. Pre-trade Risk 与冻结 snapshot；
5. Execution OMS、订单状态和交易所 Gateway；
6. FillEvent 与 AccountProjection；
7. Continuous RiskAction；
8. Protection saga；
9. Reconciliation 与恢复命令；
10. Research Domain、其他策略的 Backtest/live parity、Analytics 与 ResearchEvidence；Vegas 已作为第二验收切片先固定模板。

每个步骤继续使用第 3 节清单，不能在同一切片中同时调整策略判断、资本分配、风险阈值和执行协议。

## 9. 阶段 5：运行入口收敛

按证据逐步建立：

- `control-api`；
- `market-worker`；
- `signal-worker`；
- `account-worker`；
- `execution-worker`；
- `reconciliation-worker`；
- `schema-tool`；
- `quant-lab`。

`signal-worker` 只装配 Market -> StrategySignal。用户路径必须等 Web `ExecutionRequest` 带回稳定账户、凭证和风险配置引用后，才由 `execution-worker` 装配账户级 Portfolio 与事前 Risk；系统自营路径使用 Core `ExecutionRequest` 进入同一用例。持续 Risk 初期由 `account-worker` 装配。只有独立吞吐、故障隔离或安全证据出现时，另立 ADR 增加 `portfolio-worker` 或 `risk-worker`。

`quant-lab` 只装配 Research use case、历史数据 Adapter、Experiment/Evidence Store 和对象存储，不直接依赖 Strategy 私有实现或在入口循环中写交易规则。

迁移期间可保留旧二进制名称和 compose command 映射，但每个新 App 只能初始化本职责需要的配置、连接和 Secret。

验证：每个 App 有独立强类型配置、release build、startup/readiness/liveness、取消和优雅关闭测试；Dockerfile、compose、部署/回滚脚本和 deploy contract 同步。

### 9.1 当前落地：六角色 Phase 1

当前实现先完成运行拓扑收敛，不把它误记成领域迁移完成：

- 新增六个固定 binary/组合根，并将生产 Compose、runtime image、发布、回滚和只读验收合同对齐为 `control-api / market-worker / signal-worker / account-worker / execution-worker / reconciliation-worker`；
- Market 长期进程只合并 symbol sync、radar、scanner 和最多 2 天的 bounded repair；paper、全市场观察、schema 和历史 backfill 保持 Job/profile；Market 不持有 Web execution secret，signal dispatch 固定关闭；
- Signal 共享 Vegas 4H 行情入口，但按策略类型配置独立 symbol scope，并在启动时冻结允许执行的 config ID；Market Velocity lane 使用 `strategy_key@preset` 精确加载，缺配置、slug 错配或不满足 live cutover contract 时拒绝启动；
- Execution/Account/Reconciliation 使用代码入口固定的互斥 lane，不再依赖同一二进制的环境变量模式切换；轮询前先完成构造和 audit readiness 检查；
- 首次切换需要显式 cutover token，并保存 legacy 服务镜像拓扑；单次与 scheduler live-handoff 均纳入清退/回滚范围，避免与新 `signal-worker` 重复消费。CI 在进程稳定检查后强制执行只读生产验收，但仍不把该检查冒充依赖级 readiness。
- 发布维护入口已去重：promote/rollback 保持薄包装，固定六角色由版本化清单提供，公共 SSH/Compose/安全检查只保留一份；迁移期 cutover/legacy restore 在生产验收与回滚窗口结束后必须删除。

尚未完成、不得被文档掩盖的迁移债务：

1. `account-worker` 仍是 confirmation bridge，成交后保护单同步 mutation 尚未迁回最终 Execution owner；
2. `reconciliation-worker` 仍是 report replay bridge，尚未拥有完整差异检测、恢复命令和人工升级闭环；
3. 除 `control-api` 外目前是 process liveness，不是依赖、checkpoint、lease 与数据新鲜度 readiness；
4. Market radar 内部 legacy detached task 尚未完全纳入统一 supervisor/cancellation；
5. 生产切换前必须核对两份 Market Velocity `strategy_configs`、六角色环境变量、数据库/Redis/Web internal API 连通性，并在非 mutation 环境完成 shadow/canary。

验证：本阶段只在本地合同和编译通过后算“可进入切换准备”，不能宣称生产已切换；线上完成还必须有六容器 revision、restart、lane 日志、checkpoint 和 Web/Core read-only 链路证据。

## 10. 阶段 6：策略版本对象拆分

- 从旧 Manifest 拆出不可变 `StrategyDefinition`；
- 把可执行代码/模型身份写入 StrategyArtifact；
- 把 Experiment、DatasetManifest、样本、成本、SimulationProfile 和评估写入 ResearchEvidence；
- 把 lifecycle、promote、rollback 写入 `StrategyRelease`；
- 发布不可变 `StrategyRuntimeSnapshot` 给数据面；
- 为有状态 evaluator 建立由 EvaluationScopeId、RuntimeSnapshotId 与 MarketStreamPartition 组成的状态身份；
- Registry、Catalog、Signal builder 和 Worker 使用同一 strategy identity；
- legacy alias 只在边界 Adapter 保留。

验证：历史 Definition/Artifact/Evidence 字节身份不被覆盖；相同 RunId、Manifest、Seed 可重放；Release 变化不会修改历史信号和回测事实。

## 11. 阶段 7：控制面与数据面解耦

- 将 Definition、Release pointer、配置快照和 kill switch 收敛到 control-api；
- Worker 只消费已发布 Runtime Snapshot，不在热路径同步调用管理 API；
- 为控制面不可用、配置过期和 kill switch 传播建立测试；
- 删除数据面中的临时管理查询、环境变量业务默认值和隐式 fallback。

验证：关闭控制面后，数据面按合同继续安全运行或 fail-closed，不产生无版本交易。

## 12. 阶段 8：保护与恢复故障演练

- 覆盖重复事件、消息乱序、行情缺口和账户流断线；
- 覆盖请求已发但响应未知；
- 覆盖订单各状态的进程崩溃；
- 覆盖部分成交、保护数量不足、保护请求超时；
- 覆盖撤单与成交竞态、平仓部分成交和保护单调整；
- 覆盖 outbox 重放、lease 过期和 checkpoint 恢复；
- 覆盖交易所与内部订单、成交、持仓和保护单对账。

验证：恢复不产生重复订单；无法证明安全的状态进入阻塞或人工处置；超过最大未保护窗口会停止新开仓并触发明确 RiskAction。

## 13. 阶段 9：删除 Legacy

只有同时满足以下条件才删除旧 `services`、`orchestration`、`infrastructure` 或 CLI 分支：

- 所有真实调用方已迁移；
- Contract、parity、integration 和 recovery 回归通过；
- 新旧 shadow 差异已解释且达到切换门槛；
- release build 和 deploy contract 通过；
- 生产 revision、运行入口、日志和数据库证据已核对；
- 回滚方案仍在约定窗口内可执行；
- 删除后没有孤立配置、任务、表、投影、监控或 allowlist。

## 14. 迁移完成标准

- 目标目录成为新增代码唯一入口；
- 架构门禁从 ratchet 收敛为全量规则，legacy allowlist 清零；
- Strategy 不直接生成最终订单，Portfolio/Account/Risk/Execution 边界可验证；
- Strategy evaluator 不读取环境变量、不接收账户风险配置，回测/live 使用同一评估状态迁移；
- Web ExecutionRequest 与 Core OMS 事实完全分离；
- 数据库 CRUD、事务、Outbox 和查询归属可以唯一定位；
- 成交反馈、持续风险、保护和 Reconciliation 闭环完整；
- 控制面不在交易热路径；
- 所有外部 mutation 都有幂等、Unknown、恢复和对账证据；
- legacy 入口、兼容字段和旧表写入全部有明确结束结论。
- 现有 Vegas 回测入口完成 parity 切换后，`BacktestRunner`、`BacktestExecutor`、万能 `BacktestContext` 与 `deal_signal` 的对应 legacy 职责均有删除证据。
