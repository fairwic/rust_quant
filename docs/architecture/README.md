# Rust Quant 架构文档索引

本目录是 Rust Quant 长期参考架构、生产运行规范、依赖规则、迁移计划和架构决策记录（ADR）的唯一正式入口。迁移过渡期由 legacy 源仓库 `rust_quant` 承载规范基线，Core 目标实现统一进入 `rust_quant_alpha`；两者的仓库职责以 [ADR-0014](adr/0014-greenfield-target-repository-migration.md) 为准。

目标架构采用“模块化单体 + 五类物理目录 + Ports/Adapters + 控制面/数据面 + 可恢复交易闭环”。离线研究由独立 Research Domain 负责，`quant/backtest` 只提供确定性回放内核，并按 ResearchBar、PaperEvent、RecoveryHarness 三种保真度分别验证策略表现、订单事件和故障恢复。长期目标和现有实现迁移已经分开：目标文档不接受历史目录污染，兼容窗口只记录在迁移计划中。

发生冲突时，以已接受 ADR 为决策依据，以目标架构为总纲，以依赖规则和数据访问规范为具体落地规则。当前实现与目标不一致时，必须标记为 legacy，不得反向修改目标文档来迁就现状。

订单、撤单和保护单外部 mutation 的持久化与提交顺序，以 [ADR-0006](adr/0006-at-least-once-idempotency-and-recovery.md) 为唯一权威；owner-scoped 数据库事务的实现边界以 [ADR-0007](adr/0007-owner-scoped-persistence-and-transaction-boundaries.md) 为准。其他文档只解释各自视角，不得另立一套顺序。

Research 与 live 的同名术语不得混用：ResearchBar 以 `ResearchDecisionContextSnapshot -> ExecutionPlanningValue(child OrderPlan + ProtectionPlanningValue) -> SimulationLedger` 为边界，不创建生产 OMS 事实；PaperEvent 只能在隔离 simulated store 验证 aggregate 初始化/迁移；只有 live Execution 可以把 aggregate 与 `SubmitPending`、Outbox、permit 持久化为生产事实。权威边界见 [ADR-0009](adr/0009-research-domain-and-tiered-simulation.md)与 [ADR-0011](adr/0011-layered-runtime-snapshots-and-decision-context.md)。

跨 Owner 迁移的依赖是否满足不能由 Markdown 勾选或 Registry 人工布尔值决定，必须由当前 revision 的 Manifest、Evidence 与 `migration-check` 生成的不可变 `verdict.json` 计算。文档中列出的 Contract、Gate 或命令在实现和新鲜 Evidence 出现前都只是未来实施要求。

## 正式文档

| 文档 | 状态 | 说明 |
| --- | --- | --- |
| [长期目标架构](target-architecture.md) | 已接受 | 五类目录、业务 owner、Research Domain、跨仓库边界和完整交易闭环 |
| [生产运行与恢复](production-runtime.md) | 已接受 | 启动、行情、订单状态机、成交反馈、对账、恢复和关闭 |
| [依赖与代码归属规则](dependency-rules.md) | 已接受 | 允许依赖、禁止依赖、新代码放置和 CI 门禁 |
| [业务代码与数据访问放置规范](business-code-and-data-access.md) | 已接受 | Rust 对象/函数/Policy/Use Case 选择、CRUD、事务、SQL、Command/Query/Consumer 模板 |
| [量化通用逻辑归属](common-logic-placement.md) | 已接受 | 通用类型、数学、指标、纯回放内核、分析与 Research 归属边界 |
| [AI 编码与架构防腐护栏](ai-coding-guardrails.md) | 已接受 | 修改前声明、Golden Template、CI ratchet 与 Review 检查表 |
| [AI 架构迁移执行协议](ai-migration-execution-protocol.md) | 已接受 | Migration Manifest、基线锁定、迁移模式、Evidence、Verdict 与停止条件 |
| [Migration Program 注册表](migrations/programs/README.md) | 已接受 | 跨仓库父 Program、Owner 子 Manifest、Contract 与依赖图的机器可读索引 |
| [架构迁移计划](migration-plan.md) | 计划中 | 现有实现迁入目标架构的阶段、验证和删除条件 |
| [Vegas 与现有回测主链迁移实战](vegas-backtest-migration.md) | 迁移设计 | 以真实 Vegas/回测代码验证 Research 编排、三层模拟、逐文件分配与 parity 切换门 |
| [开源交易系统架构参考](reference-systems.md) | 参考 | NautilusTrader、LEAN、Barter、Hummingbot 与 Tesser 的取舍 |

## 架构决策记录

| ADR | 状态 | 决策 |
| --- | --- | --- |
| [ADR-0001](adr/0001-modular-monolith-and-business-modules.md) | 已接受 | 采用模块化单体与 domains/quant/contracts/adapters/platform 五类目录 |
| [ADR-0002](adr/0002-versioned-strategy-manifest-and-contracts.md) | 已接受 | 分离 Strategy 技术制品、ResearchEvidence、Release、RuntimeSnapshot 与 Wire Contract |
| [ADR-0003](adr/0003-explicit-runtime-composition-roots.md) | 已接受 | 将 API、Worker、工具拆成明确的运行入口和组合根 |
| [ADR-0004](adr/0004-portfolio-and-trading-domain-boundaries.md) | 已接受 | 分离 Strategy、Portfolio、Account、Risk 与 Execution |
| [ADR-0005](adr/0005-control-plane-and-data-plane.md) | 已接受 | 分离控制面与交易数据面 |
| [ADR-0006](adr/0006-at-least-once-idempotency-and-recovery.md) | 已接受 | 采用至少一次交付、幂等订单、保护闭环和显式恢复 |
| [ADR-0007](adr/0007-owner-scoped-persistence-and-transaction-boundaries.md) | 已接受 | 采用 owner-scoped Postgres Adapter、业务 Port 与事务边界 |
| [ADR-0008](adr/0008-backtest-reuses-domain-apis.md) | 已被取代 | 历史方案：让 Quant Simulation Tooling 编排 Domain API |
| [ADR-0009](adr/0009-research-domain-and-tiered-simulation.md) | 已接受 | Research 拥有实验与证据，Quant 退化为纯内核，并采用分级模拟与原子可见发布 |
| [ADR-0010](adr/0010-build-impact-and-artifact-isolation.md) | 已接受 | 用 Release Unit、Cargo 依赖图和工件白名单隔离生产、维护与研究发布 |
| [ADR-0011](adr/0011-layered-runtime-snapshots-and-decision-context.md) | 已接受 | 用领域政策快照、执行决策上下文和 ResearchRunSpec 定义完整运行配置 |
| [ADR-0012](adr/0012-multi-tenant-private-stream-management.md) | 已接受 | 用账户私有流、分片/lease 与配额协同定义 ExchangeSession 的容量和故障边界 |
| [ADR-0013](adr/0013-user-execution-request-and-public-market-data-credentials.md) | 已接受 | Web 是唯一执行请求 creator；平台固定 API Key 仅用于 Market 公共只读数据 |
| [ADR-0014](adr/0014-greenfield-target-repository-migration.md) | 已接受 | `rust_quant` 保留 legacy/治理基线，Core 目标实现和当前迁移工件统一进入 `rust_quant_alpha` |

## 阅读顺序

1. 先阅读[长期目标架构](target-architecture.md)与 [ADR-0013](adr/0013-user-execution-request-and-public-market-data-credentials.md)，确认业务 owner、唯一的用户执行请求路径，以及平台公共数据凭证边界。
2. 涉及 Worker、订单、成交或故障处理时，阅读[生产运行与恢复](production-runtime.md)。
3. 新增 crate、App 或跨域依赖前，先按 [ADR-0010](adr/0010-build-impact-and-artifact-isolation.md)判断 Release Unit，再检查[依赖与代码归属规则](dependency-rules.md)。
4. 新增业务逻辑、决定使用对象还是函数、编写 CRUD/事务/SQL/Consumer 前，检查[业务代码与数据访问放置规范](business-code-and-data-access.md)。
5. 使用 AI 修改架构相关代码前，执行[AI 编码与架构防腐护栏](ai-coding-guardrails.md)中的放置声明。
6. AI 迁移 legacy、crate、事实源或运行入口时，先按 [ADR-0014](adr/0014-greenfield-target-repository-migration.md)确认源仓库与目标仓库，再按 [Migration Program 注册表](migrations/programs/README.md)登记跨仓库依赖，按 [AI 架构迁移执行协议](ai-migration-execution-protocol.md)创建单 Owner Manifest，最后按[架构迁移计划](migration-plan.md)执行。
7. 修改 Vegas、回测或实盘配置时，先按 [ADR-0011](adr/0011-layered-runtime-snapshots-and-decision-context.md)固定完整决策上下文，再按 [ADR-0009](adr/0009-research-domain-and-tiered-simulation.md)确认 Research、Quant 和三种模拟边界，最后按 [Vegas 与现有回测主链迁移实战](vegas-backtest-migration.md)执行逐层 parity。
8. 如果需求与已接受 ADR 冲突，先新增替代 ADR，不得直接绕过现有决策。

## 适用边界

这套架构面向秒、分钟、小时级的多策略、多账户、多交易所生产量化。以下场景需要在真实需求出现后增加专门 ADR，不应提前把复杂度带入当前系统：

- 毫秒以下高频交易运行时；
- 用户动态上传并执行策略；
- 多地域主动—主动实盘执行；
- 大规模分布式训练或回测集群。
