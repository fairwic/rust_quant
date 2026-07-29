# Rust Quant 架构迁移计划

- 状态：实施中（`rust_quant_alpha` target-layout P0、`migration-check` P1、Market F1/F2、Strategy A1R 与 Architecture Governance P0.1 已提交并登记；CI/CD 按用户决定延后，尚无业务切片获得 current-revision Verdict，Market M2R 尚未开始）
- 首次制定：2026-07-18
- 最近修订：2026-07-29
- 目标架构：[Rust Quant 长期目标架构](target-architecture.md)
- 数据访问规则：[业务代码与数据访问放置规范](business-code-and-data-access.md)
- AI 执行协议：[AI 架构迁移执行协议](ai-migration-execution-protocol.md)
- 阶段 0/1 产物：[baseline-2026-07](migrations/baseline-2026-07/README.md)（依赖图、Owner Ledger、运行拓扑、legacy allowlist、`cargo xtask arch-check`）

## 1. 目的

本文只描述现有实现如何迁入目标架构。兼容层不得成为长期模型；每个兼容入口必须有真实调用方、测试、owner、删除条件和复查日期。

当前已确认的 legacy 基线包括扁平 `common/core/domain/infrastructure/services/orchestration` crate、单一 CLI 多运行角色，以及 Web `execution_tasks/exchange_order_results` 同时承载商业交接与交易结果的历史边界。迁移必须正面处理这些事实，不能只移动目录。

### 1.1 仓库迁移边界

- `rust_quant` 是 legacy 源仓库与过渡期架构规范基线；迁移前现有生产行为仍以它为准。
- `rust_quant_alpha` 是 Core 目标实现仓库。当前迁移的 Core Owner Manifest/Evidence/Verdict、目标业务代码、Migration SQL、Release Unit 和 App 都在该仓库创建。
- Core Manifest 的 legacy `source_paths` 必须钉住 `rust_quant@<commit>`；目标路径只允许 `rust_quant_alpha` 当前仓库相对路径。
- `rust_quan_web` 等其他 owner 仓库不迁入 Core 目标仓库，仍通过各自 Manifest 和版本化 Contract 协作。
- 最终仓库/生产入口切换是独立 Cutover，不因目录迁完、Cargo 通过或本地 parity 自动发生。

完整决策与 Registry 字段语义见 [ADR-0014](adr/0014-greenfield-target-repository-migration.md)。

## 2. 迁移原则

- 先冻结行为和数据证据，再移动代码；
- 以一个可运行的 vertical slice 为单位迁移，不按“先迁完所有 Model/Repository/Service”横向批量搬家；
- 目录变化与策略、风控、订单参数变化分开；
- 新旧实现先 shadow/parity 对比，再切单一事实源；
- 每个切片都明确 owner、表、Contract、运行入口、回滚和 legacy 删除门；
- 小型 legacy bugfix 可留原位，但不能新增依赖或扩大旧抽象；
- 不通过跨库读取、共享 ORM、万能 Service 或中央 Scheduler 缩短路径；
- 未获得显式实盘授权时，迁移验证只使用 contract、fixture、paper、dry-run、shadow 和 signed read-only。
- legacy 来源与目标实现必须分仓冻结；不得把 `rust_quant` 未提交工作树复制为目标基线，也不得继续在 legacy 仓库创建目标架构业务包。
- 自动执行只可以从 Web owner 创建的 canonical `ExecutionRequest` 进入；不存在 Core 自营/系统自营执行路径，也不得以技术迁移名义补建该路径。
- 固定服务 API Key 只服务于 Market 公共只读行情采集；它不能作为 Account、Risk、用户 credential 或 `ExecutionRequest` 的输入、回退或前置条件。
- 每次固定 Market Key 访问都必须记录非敏感 key ref、Market owner、只读 endpoint/method、观察时间、权限/响应 evidence ref + hash 与“无用户 credential fallback”；Research 只读消费已发布 DatasetManifest。

## 3. 每个迁移切片的固定清单

父迁移计划先登记依赖与 Contract；每个实施型 Owner 子 Manifest 再填写：

| 项目 | 必填内容 |
| --- | --- |
| 业务目标 | 用户/系统可验证的单一结果 |
| 父迁移计划 | `migration_program_id`、业务结果、Owner 子 Manifest、依赖图、共享 Contract/version 与兼容窗口；父计划不承载混合 Owner 的代码范围 |
| Owner 子 Manifest | 每个子 Manifest 唯一 Domain/跨仓库 owner；父迁移计划本身不拥有代码/表，且不得填写 `secondary_owners`，跨 Owner 工作拆为各自子 Manifest |
| 依赖 | `depends_on` 的子 Manifest/不可变输入；未满足的依赖不得以同一大切片绕过 |
| 当前入口 | 现有调用方、进程、API/消息 |
| 当前数据 | 表、字段、唯一键、状态机、数量级 |
| 目标路径 | App -> Use Case -> Model/Policy -> Port -> Adapter |
| Contract | producer/consumer、当前版本、目标版本、N/N-1 兼容窗口与各自的子 Manifest |
| 原子性 | 本 Owner 的 opening slot/业务唯一约束、状态、完整计划、审批引用、幂等、Outbox 必须一起写入的内容；消费者 Inbox 去重、claim/outcome 本地事务与外部 I/O 时点。禁止跨 Owner 大事务 |
| Shadow | 新旧实现如何对比且不产生双副作用 |
| 切换 | 读切换、写切换、feature flag/release generation |
| 回滚 | 回滚入口、数据兼容和允许时限 |
| 删除门 | 调用方、配置、表、监控和白名单的删除条件 |
| 验证 | unit、integration、contract、parity、recovery、deploy contract |

该清单必须保存为 `docs/architecture/migrations/<migration-id>-<slug>/manifest.toml`，使用 [`migrations/_template.toml`](migrations/_template.toml) 的机器可读 schema；聊天计划不能替代 Manifest。每个实施型子切片只能选择 `structure_only`、`behavior_change`、`cutover`、`legacy_delete` 中一个主模式，并固定已提交的 `architecture_baseline_git_sha`、规范/Release Unit/Contract snapshot hash、输入与输出工件 hash。没有填写完整清单、基线未冻结、依赖未满足或模式混合时，正常路径不开始迁移。

本计划已按用户决定延后受跟踪 CI，因此实施期采用 [AI 执行协议的“实施输入门”](ai-migration-execution-protocol.md#延后受跟踪-ci-时的实施输入门)：已提交且 Registry 为 `created` 的 predecessor，在 Manifest/Evidence/hash 与本地验证完整时，可以被 successor 钉住为源码实施输入；这不表示 `depends_on` 满足。successor 最高只能处于 `implementing`，不得部署 App/runtime、切换事实源、写生产数据库、触发跨 Owner 副作用或交易所 mutation；predecessor 漂移时 descendant Evidence 必须失效并重验。

## 4. 阶段 0：冻结当前基线与 Owner Ledger

- 记录 Cargo 内部依赖图和已知违规基线；
- 记录生产二进制、容器 command、配置、数据库和启动依赖；
- 为 Strategy Signal、Web Execution Task、Readiness、订单结果和 internal API 建立 Contract snapshot；
- 记录 `quant_core` 与 `quant_web` 当前表 owner、写入者、读取者和数据量；
- 为关键策略建立固定输入下的 evaluator、portfolio、risk 和 execution parity 基线；
- 为 Vegas 以 current-migration Owner 子 Manifest 固定 point-in-time DatasetManifest、EvaluationManifest、四个 Domain Policy Snapshot、`ResearchDecisionContextSnapshot`、SimulationProfile、ResearchExecutionArtifactRef、指标预热长度、动态 Evidence、RngSpec 与 SchedulerSpec；
- DatasetManifest 必须覆盖 Market stream revision/首次可见时间、历史 universe 成员有效期与上市/退市、纳入算法、数据缺口/修订政策、当时有效 InstrumentRules 和需要的 funding/index/mark 工件；
- EvaluationManifest 必须在读取目标 OOS 结果前固定 train/validation/OOS、walk-forward folds、purge/embargo、参数空间、优化器/Seed/预算、选择规则、holdout 重用和收益集中度门；
- 记录 Vegas 当前 backtest、paper/live 的实际窗口差异、状态缓存 identity 和信号字段差异；
- 记录 `TradingState`、`deal_signal`、`StrategyExecutor`、全局 Registry、零字段 Service/Calculator 的调用方、owner 混合和删除条件；
- 记录 `BasicRiskConfig`、`BasicRiskStrategyConfig` 及 live/helper 止盈止损计算的字段来源、默认值、舍入与触发顺序；
- 记录当前 Cargo 传递依赖、总 CLI binary 清单、Docker 实际复制内容和 CI build matrix，区分 Research-only、共享 Domain 和生产 App 变更；
- 记录订单状态、lease、重试、保护单和恢复的现有行为。

验证：同一基线可以在迁移前后重复执行，并能识别策略结论、订单参数、状态机和 Contract 漂移。

## 5. 阶段 1：先建立防腐骨架，不搬业务

- 建立 `apps` 与 `crates/{domains,quant,contracts,adapters,platform}` 目录约定；
- 建立最小 command/query/event-consumer 三个模板；
- 建立 `cargo xtask arch-check` 的只读报告；
- 在任何代码首次迁入 `apps/` 或 `crates/{domains,quant,contracts,adapters,platform}` 前，先补齐 target-layout P0 门禁：机器可读 package/path role map、未知 workspace package fail-closed、`apps/` 与全部目标源码根扫描、target Domain/Quant/Adapter 路径规则、以及 baseline/allowlist 变更必须关联独立 Manifest 的完整性校验；
- P0 之后、继续扩展已落地 Domain/Adapter 之前，执行独立 P0.1 模块硬化：按 [ADR-0015](adr/0015-capability-first-modules-and-api-spi-boundaries.md)增加 capability/API-SPI、façade、文件预算、Port 完整性和 Gateway capability 注入式门禁。Architecture Governance 只修改 `xtask`/role map/违规登记，不能同时重排 Market 或 Strategy 业务代码；
- 建立 Migration Manifest 模板、人工 Evidence 和 `cargo xtask migration-check --manifest <path>` 的 schema/基线/diff-scope 只读报告；
- 建立 `release-units/{core-runtime,core-maintenance,quant-lab}.toml`，先记录 root package、binary allowlist、forbidden package、生产部署资格和测试集；
- 在 Release Unit Manifest 中登记六个生产组合根的目标 identity；每个 App 只有在首个真实 Use Case + Adapter + runtime loop 一起到达时才创建独立 Cargo package，禁止阶段 1 先生成空 App。形成后仍可打入同一个 `core-runtime` 镜像；schema-tool 与 quant-lab 分别使用独立工件；
- 保存 legacy allowlist，CI 先禁止新增违规；
- 对新增零字段 Service/Manager/Calculator、Aggregate 公开可变不变量、Model/Policy 隐式时间/环境变量/全局业务缓存建立 ratchet；
- 建立由 Git diff、Cargo 反向传递依赖和 Release Unit Manifest 推导的 build-impact 报告：Research-only 改动不构建或部署 `core-runtime`，共享 Domain 改动不得漏掉生产构建和 parity，无法判断时 fail closed；
- 增加生产镜像内容检查：实际 binary 与 `core-runtime` allowlist 完全一致，禁止 Research/Backtest/Paper/candidate/schema-tool；
- 只有 build-impact 基线证明候选策略与已发布策略确有独立编译/发布生命周期时，才在 Strategy owner 内建立 api/released/candidates 三个 catalog 级 package；`signal-worker` 不依赖 candidates，`quant-lab` 可以依赖 released + candidates；
- 第一个真实 persistence slice 到达时才建立 Postgres Adapter crate 及所需 owner/capability module，不预建空 owner 目录；
- R1 首个真实 Research Use Case 到达时才建立 Research Domain，并在同一切片包含 Experiment/Run/Evidence identity、实际调用方、必要 Port/Adapter 与状态机；不以只有 Trait/Fake 的“最小骨架”冒充完成；
- RecoveryHarness 只建立 CI-only ephemeral integration-test artifact；不加入任何可部署 Release Unit，不获得生产 environment、Secret 或真实交易所 Adapter；
- Migration 采用单一有序目录和 owner 文件名。

验证：不迁移任何业务行为，现有构建仍可运行；legacy ratchet 只拦已覆盖旧目录中的新增违规，不因历史债务让全仓长期红灯。它不等于 target-layout 已受保护：首个 `structure_only` 目录搬迁前，P0 门禁必须对目标 package/app 注入违规能失败、未知 package 不可静默跳过，并有当前已提交 revision 的 CI/本地证据。

## 6. 阶段 2：先完成 Market → Strategy → Research 上游地基，再进入执行 Golden Slice

Market Velocity 是首个 Golden **计划**，不是第一个应落地的下游执行模块。实际业务实现必须先形成可独立验证的上游事实链：

```text
M1 canonical MarketBar / instrument / timeframe / finality
  -> M2 public read-only K 线采集与规范化
  -> A1R Strategy Fake-only Port 纯结构移除
  -> G2 Architecture Governance P0.1 门禁
  -> M2R Market public-Kline structure-only 模块/API-SPI 拆分
  -> M3A Market owner storage + historical query
  -> M4A point-in-time DatasetSnapshot
  -> S1 StrategyDefinition / StrategyRuntimeSnapshot / Evaluator / Signal
  -> R1 deterministic backtest + strategy parity
  -> A Strategy/Web handoff
  -> C0 Execution intake
  -> B0/B deterministic dry-run
  -> C1 live 前置与恢复
```

平台固定 API Key 只允许在 M2 的 Market 公共只读 Adapter 配置中出现；M1、M3A、M4A、Strategy、Research、Risk 和 Execution 都只能看到非敏感 source profile/ref。Research/Backtest 必须通过 Market historical API/DatasetSnapshot 读取同一 canonical MarketBar，禁止定义第二套 K 线事实或直连采集凭证。

M2 已暴露出两个会在 M3A 继续放大的结构风险：canonical bar 与 OKX public-market Gateway 单文件过大，以及 Domain 根门面同时公开业务 API 与 Adapter Port。因此 M3A 前增加三个互相独立的 `structure_only` 子 Manifest：

1. `MIG-MVE-A1R-REMOVE-FAKE-ONLY-PORT-V1` 的唯一 Owner 是 Strategy；先以调用点闭包证明当前 handoff Port/Use Case 只有测试 Fake、没有生产 Adapter/运行入口，再从生产编译面移除，保留 blocked A1 的 Contract/边界发现记录供未来 successor 使用；
2. `MIG-20260729-MODULE-BOUNDARY-GUARDRAILS-P0-1` 的唯一 Owner 是 Architecture Governance，只实现 ADR-0015 的静态门禁和注入测试；
3. `MIG-MKT-F2R-CAPABILITY-API-SPI-V1` 的唯一 Owner 是 Market，在不改变请求、Decimal、时间、finality、错误与输出语义的前提下，把 Market 和 public-market/OKX 按 capability 拆分并建立 `api`/`spi` 双门面。

当前 Strategy signal-handoff 只有测试 Fake、没有生产 Adapter，不能被 G2 误判为完成能力，也不能用通用 allowlist 永久豁免。A1R、G2 与 M2R 不能合成跨 Owner 的“P0.1 大 Manifest”。

Registry 已登记的 `MIG-MKT-F3-MARKET-BAR-STORAGE-V1` 与 `MIG-MKT-F4-DATASET-SNAPSHOT-V1` 依赖边不能原地改写，因此保留为不可实施的早期登记；实际后续使用新 successor `MIG-MKT-F3A-MARKET-BAR-STORAGE-V1 -> MIG-MKT-F4A-DATASET-SNAPSHOT-V1`，其中 F3A 显式依赖 M2R。不得通过修改旧 child 的 `depends_on` 绕过 registration revision。

下文 `A → C0 → B0 → B → C1` 只描述**上游地基完成之后**的执行 Golden Slice 内部依赖，不表示可以从 A 开始迁移整个系统。可以提前冻结防腐 Contract 草案，但在 M1～M4A、S1、R1 获得当前 revision 的 Verdict 前，不得创建 A1 的数据库、Outbox、Dispatcher、Web consumer 或 runtime wiring。

2026-07-29 已登记的 `MIG-MVE-A1-STRATEGY-SIGNAL-HANDOFF-V1` 错误地把 `depends_on` 冻结为空。Registry 的 child identity 与依赖不可原地改写，因此该 child 只能保留为被阻塞的 Contract/边界发现记录，不能继续充当实施前置。未来恢复 handoff 实施时必须新登记 successor child，并显式依赖 M4A、S1 与 R1 的不可变 Verdict。

上游地基完成后，Market Velocity 执行 Golden Slice 仍不能成为同时迁移信号、商业授权、Portfolio、Risk、OMS、Paper 和恢复协议的巨型 Manifest。业务上仍是 A（交接）、B（决策）、C（恢复）三个切片；其中 C 必须先完成 `C0` 的最小 Execution intake 结构子 Manifest，随后 B0 固定 test-only Evidence Provider，B 才能使用稳定 typed Port 和不可变输入，最后按 Owner DAG 完成 `C1` 的账户绑定、市场证据、Account admission、交易所能力、风险估值、安全监测、审批与 Execution 恢复。`RecoveryHarness` 只是验证这些 live 路径的 CI-only ephemeral artifact，不是一个可部署的 C1 业务切片。每个 Owner 子 Manifest 都必须有独立 Manifest、Evidence 和 Verdict；前一依赖未验证时，后一切片只能在“实施输入门”下钉住已提交工件并编写目标源码，不能声称依赖满足、进入 `verified`、装配 runtime 或实施切换。

```text
A Strategy/Web handoff
  -> C0 Execution intake storage/model/typed Port（structure_only）
  -> B0 immutable test-only evidence provider
  -> B deterministic dry-run parity
  -> C1 Web binding + Market resolved evidence + Account admission + Exchange capability
  -> C1 Risk valuation + Safety monitoring
  -> C1 Risk approval + Execution recovery
  -> CI-only RecoveryHarness 验证
```

父计划 `MP-market-velocity-execution-v1` 必须按 [Program Registry](migrations/programs/registry.toml) 与下表登记 Owner 子 Manifest；父计划没有单一代码 Owner，表中每一行才是拥有唯一 Owner 的可实施子 Manifest。表中的条目不是一个可跨 Owner 提交的工作包，而是 `depends_on` 与 Contract 的编排约束。

| 业务切片 | Owner 子 Manifest | 此子 Manifest 唯一 Owner | `depends_on` | 本 Owner 本地事实/事务 | 跨 Owner Contract |
| --- | --- | --- | --- | --- | --- |
| A | `A1-strategy-signal-handoff` | Strategy | M4A、S1、R1；现有错误登记的无依赖 A1 仅保留为 blocked discovery record，实施须新建 successor child | `StrategySignalHandoffV1`、RuntimeSnapshot 的市场输入要求、Outbox、delivery identity | `CreateExecutionRequestFromSignalV1` command v1 → Web；`RequiredMarketEvidenceV1` → Market/Risk |
| A | `A2-web-execution-request` | Web | A1 | canonical `ExecutionRequestV1`、Inbox/去重、商业 blocker、claim 状态 | `ClaimExecutionRequestV1` / `RenewExecutionRequestClaimV1` / `ReleaseExecutionRequestClaimV1`：Execution → Web；对应 receipt：Web → Execution；`ReportExecutionRequestOutcomeV1` → Web；`ExecutionRequestOutcomeReceiptV1` → Execution |
| C0 | `C0-execution-intake` | Execution | A2 | `CoreExecutionIntakeV1`、claim receipt、幂等约束 | 发起 Claim/Renew/Release 并消费对应 Web receipt；发布 Outcome 并消费 `ExecutionRequestOutcomeReceiptV1`；`AcceptedExecutionRequestV1` → B0 |
| B0 | `B0-immutable-test-evidence-provider` | Execution（test-only Adapter） | C0 | 无业务事实；只组合带 hash 的 Market/Account/Instrument fixture 与 Snapshot | `ImmutableDecisionEvidenceBundleV1` → B1/B2/B3；禁止 runtime wiring、网络/DB 写入 |
| B | `B1-portfolio-dry-run` | Portfolio | B0 | 只读/纯 `PortfolioTarget` 证据；无跨 Owner 写入 | `PortfolioTargetV1` v1 → Risk |
| B | `B2-risk-dry-run` | Risk | B1 | 只读/纯 `RiskDecision` 证据；无审批持久化 | `RiskDecisionV1` v1 → Execution |
| B | `B3-execution-planning-dry-run` | Execution | B2 | 只读/纯 `ExecutionPlanningValue`、child `OrderPlan`、`ProtectionPlanningValue` evidence；无 OMS Aggregate/Outbox | `ExecutionPlanningValueV1`，仅供 Evidence |
| C1 | `C1-web-account-binding` | Web | A2 | binding version/generation、商业状态与 credential revision/revocation generation | `ExecutionAccountBindingV1` → Account/Risk/Execution |
| C1 | `C1-market-resolved-evidence` | Market | A1 | Bar finalization、逐项 readiness、一次决策的解析集合与 TTL | `BarFinalizationV1`、`MarketDecisionReadinessV1`、`ResolvedMarketEvidenceSetV1` → Risk/Execution |
| C1 | `C1-account-admission-facts` | Account | C1-web-account-binding | AccountProjection、session generation/write fence、recovery closure 与 admission 状态 | `AccountAdmissionEvidenceV1`、`AccountFactV1`、`AccountRecoveryClosedV1` → Risk/Execution |
| C1 | `C1-exchange-execution-capability` | Execution（exchange-gateway Adapter 提供能力证据） | C1-web-account-binding | 不产生账户/订单业务事实；固定 exchange/product capability profile | `ExchangeExecutionCapabilityProfileV1` → Risk/Execution |
| C1 | `C1-risk-valuation` | Risk | C1-market-resolved-evidence、C1-account-admission-facts、C1-exchange-execution-capability | 估值输入、外部仓位风险占用与不可变 valuation snapshot | `RiskValuationSnapshotV1` → Execution |
| C1 | `C1-safety-monitoring` | Execution | C1-account-admission-facts、C1-exchange-execution-capability | `SafetyObligation`、monitoring fence、Outbox/Ack 与闭合证据 | `SafetyMonitoringV1` → Account；`SafetyMonitoringAckV1` → Execution |
| C1 | `C1-risk-approval` | Risk | B3、C1-risk-valuation | 不可变 approval、Risk Outbox | `RiskApprovalV1` v1 → Execution |
| C1 | `C1-execution-recovery` | Execution | C1-risk-approval、C1-safety-monitoring | opening slot、Order/Attempt/Protection、Execution Outbox/Inbox 与 live 恢复状态机 | `OrderSubmissionRequestedV1` / `OrderCancelRequestedV1` / `ProtectionSubmissionRequestedV1` → Dispatcher；`ExchangeMutationOutcomeV1` → Execution |

所有表项只在各自 Owner 的数据库和本地事务中写事实；任何跨服务调用都在事务外，经版本化 command/claim receipt/Event 交接。Web 是 claim lease owner：`ClaimExecutionRequestV1`、`RenewExecutionRequestClaimV1`、`ReleaseExecutionRequestClaimV1` 均由 Execution -> Web，Web -> Execution receipt；`ReportExecutionRequestOutcomeV1` 由 Execution -> Web，Web -> Execution outcome receipt。字段、方向和禁止字段以 Program Registry 为准，不能用笼统 receipt 替代。父计划、共享 Contract 或“同一业务请求”都不能授权跨库大事务。

### 6.1 切片 A：Signal handoff 与 Web 商业授权交接

```text
MarketSnapshot
  -> Market input to Strategy-owned Market Velocity `StrategySignal`
  -> Strategy handoff（Strategy owner）
  -> CreateExecutionRequestFromSignalV1（quant-web-client）
  -> Web 匹配商业资格并创建 canonical ExecutionRequest
```

范围只包含信号与商业交接：Market Velocity `StrategySignal` 的唯一业务 owner 是 Strategy，Market 只提供 snapshot/input。A1 固定 `StrategySignal` identity/version/evidence cutoff，并由 Strategy owner 在本地事务内持久化 `StrategySignalHandoffV1` 与自己的 Outbox；A2 再由 Web owner 通过 `CreateExecutionRequestFromSignalV1`、订阅、会员、产品资格和 verified active credential 创建 canonical `ExecutionRequestV1`，并在自己的事务中维护 claim lease 与 Outcome 投影。Claim/Renew/Release command 固定为 Execution -> Web，receipt 固定反向；Strategy handoff payload 不得携带用户、credential、risk profile 或预组装 `ExecutionRequest[]`；Strategy/Core 不查询订阅，也不参与扇出。

本节只能在 M4A point-in-time DatasetSnapshot、S1 同源 Strategy evaluator 与 R1 deterministic backtest/parity 的前置 Verdict 闭合后实施。缺少任一前置时，只允许继续维护 Contract discovery，不允许新增 handoff 表、Outbox、Dispatcher 或 Web 投递。

News 若参与，只能先以 `NewsInsightV1 -> Strategy evaluator -> StrategySignal` 进入，不能绕过 Strategy 直接调用 Web 的执行请求入口。

验证：Contract producer/consumer 兼容、重复 signal command/claim/outcome 幂等、Web 对 blocker 的结构化返回、Strategy handoff Outbox retry、以及断言 Strategy/Core 侧没有 Web 订阅/凭证/风险配置读取。此切片不引入 Portfolio、Risk、Order、Execution OMS Outbox、交易所 Adapter 或任何 live mutation。

### 6.2 切片 C0：最小 Core Execution intake（`structure_only`）

`C0` 是 C 的前置 Owner 子 Manifest，不是提前接入 OMS。它只由 Execution owner 建立最小的 `CoreExecutionIntakeV1` 本地持久化模型、稳定的 typed intake Port 和 `ExecutionRequestV1` decoder；Execution 通过 Web `ClaimExecutionRequestV1` 取得有期限的 request claim，按 `RenewExecutionRequestClaimV1` 续租、按 `ReleaseExecutionRequestClaimV1` 释放，并以 `ReportExecutionRequestOutcomeV1` 写回结果。每个 Web request identity 只能得到一个本地 claim receipt；Web credential、用户资料和产品资格只保留为不含 Secret 的 opaque reference，Execution 不读取或写入 Web 事实。

`C0` 的本地事务只包含 intake identity、版本、claim receipt 状态和幂等约束；它不创建 Order、attempt、Protection、Execution OMS Outbox 或交易所调用。Web 仍在自己的事务内创建/更新 `ExecutionRequestV1` 及 claim lease，Core 只通过上述版本化 Contract 交互，禁止跨库大事务或 Core 直接写 `quant_web`。

验证：重复投递不产生第二个 Core intake record；未知/过期 contract 被 fail-closed；decoder 不接受 Secret、固定 Market API Key 或“系统自营”标记；所有真实执行字段仍不可达。`C0` 验证后，切片 B 只能通过该 typed Port 构造 Dry-run Context。

### 6.3 切片 B0：不可变 test-only Evidence Provider

`B0-immutable-test-evidence-provider` 是 Execution owner 的 test-only Adapter，不是新的业务事实 owner。它只读取 Market、Account、Instrument 各自已发布的不可变 fixture/DatasetManifest/Snapshot，并以 `ImmutableDecisionEvidenceBundleV1` 固定 `market/account/instrument` evidence ref + hash、Clock identity、Seed 和四个 Policy Snapshot。B0 不得调用网络、读取“当前”数据库、写入数据库、读取环境变量业务默认值、持有用户 credential/固定 Market Key 或进入任何 App/runtime 装配。

验证：缺任一输入 hash 即 fail-closed；相同 bundle 必须产生字节相同的输入视图；`core-runtime`、Paper、Live、scheduler、Market/Account/Risk worker 的依赖图中不得出现 B0。B0 是 B 的唯一动态 Evidence 入口，不能另造第二套 replay/fixture 读取器。

### 6.4 切片 B：确定性决策 Dry-run parity

```text
accepted CoreExecutionIntakeV1
  -> ImmutableDecisionEvidenceBundleV1
  -> ExecutionDecisionContextSnapshot
  -> PortfolioTarget + PreTradeSnapshot
  -> RiskDecision
  -> ExecutionPlanningValue + child OrderPlan + ProtectionPlanningValue
  -> Dry-run Evidence
```

该业务切片由 B1/B2/B3 三个 Owner 子 Manifest 依次验证 B0 同输入的逐层输出，不创建生产 OrderIntent、Order、Outbox、attempt 或外部调用。它冻结四个 Published Policy Snapshot、不可变 Market/Account/Instrument Evidence、EvaluationState before、Clock/Seed，并在首次差异层失败；不能用最终 PnL 抵消前序差异。`ExecutionPlanningValue`、child `OrderPlan` 与 `ProtectionPlanningValue` 是纯规划值，Research/Dry-run 可以保存；它们不得被命名或持久化为 live OMS `ExecutionPlan`/`ProtectionPlan`。PaperEvent 只有在 simulated OMS 中才可从相同 planning value 初始化模拟 Aggregate。若为保持现有 Contract 需要改变默认值、舍入、时间或错误语义，必须单独使用 `behavior_change` Manifest，不能伪装成 `structure_only`。

验证：明确的 `ExecutionRequest` 到 Context 映射、B0 bundle contract、unit/contract/parity fixture、Dry-run 只保存决定与 `ExecutionPlanningValue` 证据、以及生产数据库、真实凭证和 raw mutation SDK 在装配层物理不可达。

### 6.5 切片 C1：live Owner 前置、OMS 恢复与 CI-only RecoveryHarness

切片 B 的纯决策 parity 稳定只证明 planning 语义，不授权 live mutation。`C1-risk-approval` 与 `C1-execution-recovery` 开始前，Program 必须已经取得当前且 `pass` 的 `C1-web-account-binding`、`C1-market-resolved-evidence`、`C1-account-admission-facts`、`C1-exchange-execution-capability`、`C1-risk-valuation` 与 `C1-safety-monitoring` Verdict；任一缺失、blocked、过期或 Contract revision 不匹配时，只允许继续 read-only/dry-run，`production_mutation_allowed=false`。

Risk owner 按 evaluation identity 在自己的本地事务中持久化不可变审批与 `RiskApprovalV1` Outbox；Execution owner 通过 Inbox 消费后，再由 live Execution intake/recovery 代码路径在自己的本地事务中原子取得 opening slot，并将批准的 `ExecutionPlanningValue` 落实为 live-only 持久 `OrderIntent`、`ExecutionPlan`、`ProtectionPlan`、`SubmitPending`、完整计划、幂等和对应 mutation Outbox。Dispatcher 只接受 current event generation/aggregate version，Fenced Gateway 的 `ExchangeMutationOutcomeV1` 必须绑定 attempt/permit 后再回到 Execution Inbox。各 Owner 通过版本化 Contract 交接，不能形成跨 Owner 大事务。

**CI-only ephemeral RecoveryHarness** 接入 disposable Postgres、fault-injection Fenced Gateway 和生产形状的状态机，验证上述相同 live 代码路径；它只提供临时 Adapter 和故障编排，不定义或直接创建第四套 Aggregate。该 Harness 不进入任何 deployable Release Unit、生产镜像或生产部署图，也不能接触生产 secret、账户或网络。

验证至少覆盖两个 worker 竞争 opening slot、permit consume/revoke、Submit/Cancel/Protect generation rollover、attempt outcome 原子性、Unknown 禁止直接重投、DefinitivelyAbsent recovery 新建 generation Outbox、旧 delivery ack/no-op、durable blocker retry、Outbox 重放，以及 User Stream 与 signed snapshot/query 间的 Fill/Cancel gap。RecoveryHarness 不是 PaperEvent，也不是收益证明；真实生产数据库、真实凭证和 live Exchange Adapter 必须物理不可达。

A/B/C 三个业务切片（C 拆为 C0/C1）都不得把生产 mutation Event 路由给 Simulated/Dry-run Adapter。A、C0、B、C1 分别只对交接、intake、决策、live owner 前置与恢复协议负责；完成 C1 Evidence 只表示模板具备推广资格，不等于生产 cutover 已授权。

### 6.6 Vegas 是第二个验收切片，不替代首个 Golden 计划

Market Velocity 继续作为最小生产垂直切片；Vegas 用来证明目标架构能承载“有滚动状态、复杂规则、参数研究和回测/live parity”的真实策略。Vegas 验收切片按以下边界推进：

```text
Research::BacktestRun + DatasetManifest
  -> historical event stream
  -> Vegas Evaluator（内部 EvaluationState）
  -> 同时点 Signal barrier
  -> Portfolio allocation
  -> Pre-trade RiskDecision
  -> ExecutionPlanningValue + child OrderPlan + ProtectionPlanningValue
  -> SimulationProtectionState
  -> ResearchBar fill model + SimulationLedger
  -> Analytics
  -> ResearchEvidence 原子可见发布
```

本切片先覆盖 backtest、paper 和 read-only shadow，不改变当前 live 默认版本，也不触发真实下单。

Vegas 不是单一可实施 Manifest。开始任何目标代码迁移前，必须在 Program Registry 新建 `MP-vegas-research-parity-v1`，并把下列条目登记为各自唯一 Owner 的 child Manifest；Program 本身不拥有代码、表或跨 Owner 事务：

| Child Manifest | 唯一 Owner | `depends_on` | 单一职责 |
| --- | --- | --- | --- |
| `V0-market-point-in-time-dataset` | Market | 无 | 发布历史 stream/universe/revision/InstrumentRules 工件 |
| `V1-research-run-governance` | Research | V0 | Dataset/Evaluation Manifest、RunSpec、Artifact/RNG/Scheduler ref 与 trial ledger |
| `V2-strategy-vegas-evaluator` | Strategy | V1 | Vegas evaluator、规则与 Signal evidence parity |
| `V3-strategy-evaluation-state` | Strategy | V2 | scoped state、预热、缺口与 checkpoint 语义 |
| `V4-portfolio-policy` | Portfolio | V3 | 同时点 barrier、排序、净额、容量与 allocation |
| `V5-risk-policy` | Risk | V4 | PreTrade/Continuous Risk、final stop 与 RiskAction |
| `V6-execution-planning` | Execution | V5 | `ExecutionPlanningValue`、child OrderPlan、`ProtectionPlanningValue` |
| `V7-quant-backtest-kernel` | Quant（技术 owner，无业务事实） | V3 | Clock/Scheduler/Replay/Fill/Fee/Slippage/Funding 纯机制 |
| `V8-research-simulation-evidence` | Research | V6、V7 | SimulationLedger、SimulationProtectionState、ResearchDecisionContextSnapshot、Analytics、Evidence/Gate |
| `V9-execution-paper-simulated-oms` | Execution | V8 | Paper simulated OMS 初始化与状态迁移 |
| `V10-execution-recovery-harness` | Execution | V9 | CI-only lease/outbox/Unknown/保护恢复 |
| `V11-reconciliation-recovery-harness` | Reconciliation | V10 | CI-only 差异检测与恢复命令 |
| `V12-strategy-promotion-receipt` | Strategy | V8、V11 | candidate -> released 工件等价与晋级审计 |

每个 child 还必须按 `structure_only -> behavior_change -> cutover -> legacy_delete` 拆分互斥模式；上表只表示业务依赖，不授权一个 commit 同时修改多 Owner。Migration Manifest 控制代码/事实源迁移，不能替代每次研究运行的 ResearchRunSpec/EvaluationManifest。

必须完成：

- 把 EMA/RSI/ATR 等纯指标迁入 `quant/indicators`，Vegas 入场与过滤保留在 Strategy；
- 引入 `StrategyEvaluationStateKey = EvaluationScopeId + RuntimeSnapshotId + MarketStreamPartition`，消除并行 Run 和仅按 symbol/period/type 缓存的歧义；
- Strategy evaluator 不再接收账户级 `BasicRiskStrategyConfig`；
- 将历史 `position_leverage` 的资金比例语义迁为 Portfolio `allocation_ratio`，真实交易所 leverage 单独建模；
- 拆分并冻结 `StrategyRuntimeSnapshot`、`PortfolioPolicySnapshot`、`RiskPolicySnapshot`、`ExecutionPlanningPolicySnapshot`；Strategy 快照不再承载用户风险或其他 owner 政策，同一 payload 不再解析为两个语义重叠的 Risk 类型；
- 引入 `ResearchDecisionContextSnapshot`，由 Research Run 事务绑定 `ResearchScenarioRef` 与四个 Published Policy Snapshot；Research 不创建或伪造 Web `ExecutionRequest`/live `ExecutionDecisionContextSnapshot`；
- 引入 `ResearchRunSpec`，固定 point-in-time DatasetManifest、EvaluationManifest、四个 Policy Snapshot、SimulationProfile、模拟账户初态、ResearchExecutionArtifactRef、ClockSpec、RngSpec 与 SchedulerSpec；动态 Market/Account/Instrument Evidence 与配置分离；
- Strategy 保留候选失效价、退出意图/候选止盈政策，Risk 使用唯一 Policy 产生不可放宽的最终止损、风险边界和批准数量，Execution 合并 Strategy exit intent 与 RiskDecision 生成 `ExecutionPlanningValue`（含 child `OrderPlan` value）/`ProtectionPlanningValue`；ResearchBar 只将其落实为 `SimulationProtectionState`，不创建 live `OrderIntent`、`ExecutionPlan` 或 `ProtectionPlan`；
- `trade_fee_rate`、slippage、funding、latency 和 candle path 全部迁入 `SimulationProfile`，不再混入风险配置；
- `quant/backtest` 只迁移确定性时钟、事件调度、撮合、费用、滑点和资金费；Research use case 驱动同一 Strategy、Portfolio、Risk 和 `ExecutionPlanningValue`（含 child `OrderPlan` value）API；
- 多币种在同一 decision time 先收集全部 Signal，再统一排序、净额和分配；新增 symbol 重排不变性测试；
- 固定指标预热/最大历史窗口，解释并消除当前 backtest 与 live 的 7000/4000 等不一致；
- 保留 filtered signal、动态配置、RiskDecision、OrderDecision、trade detail 与指标证据；
- ResearchEvidence 由 Research owner 发布：先内容寻址上传大对象，再以单一数据库事务发布 Completed EvidenceManifest；Completed 只表示完整可见，EvaluationGateResult 单独表达 eligible/rejected/inconclusive；
- Strategy promote 必须生成 PromotionReceipt，固定 candidate Evidence/gate、ResearchExecutionArtifactRef、released artifact/build digest 与跨构建 parity；不能因重新编译后名称相同就宣称等价；
- 明确 `ResearchBar` 不覆盖 lease/outbox/Unknown/recovery；PaperEvent 与 RecoveryHarness 分别建立独立验收；
- RecoveryHarness 是 CI-only ephemeral artifact，不进入任何镜像或生产部署权限；
- 建立现有 pipeline 与新 pipeline 的逐事件 parity 报告，并对所有差异分类。
- parity 报告记录实际调用的业务 symbol 与各快照 identity/version/hash，并比较 EvaluationState before/after、Signal、Target、RiskDecision、`ExecutionPlanningValue`（含 child `OrderPlan` value）、`ProtectionPlanningValue` 和 decision trace；最终 PnL 接近不能通过。
- SimulationProfile 不得成为单步业务 Policy 的隐藏输入；但首次 Fill/费用/SimulationLedger hash 分歧后，后续动态 Evidence 已不同，必须标记 scenario divergence，不再错误要求后续 RiskDecision/Planning 完全相同。

完整逐文件分配和切换门见 [Vegas 与现有回测主链迁移实战](vegas-backtest-migration.md)。

## 7. 阶段 3：解决 Web/Core 执行事实所有权

### 7.1 Web 保留的事实

- 用户、会员、订单、`strategy x symbol` combo；
- API credential 配置与 verified/active 状态；
- 产品资格、执行授权和 `ExecutionRequest`；
- Core 交易事实的用户展示投影。

### 7.2 Core 迁入的事实

- OrderIntent、live-only `ExecutionPlan`、Order、Fill、Protection；
- client order identity、订单状态机和 Unknown；
- ReconciliationResult 与恢复任务。

### 7.3 迁移顺序

1. 冻结现有 `execution_tasks`、attempt 和 `exchange_order_results` Contract；
2. 在 A 中由 Web 引入 canonical `ExecutionRequestV1` 及其 Claim/Outcome Contract schema；在 C0 中由 Execution 引入最小 intake record/typed Port，并由 Execution -> Web 发起 `ClaimExecutionRequestV1`、`RenewExecutionRequestClaimV1`、`ReleaseExecutionRequestClaimV1`、`ReportExecutionRequestOutcomeV1`，由 Web -> Execution 返回各 receipt；两者只以这些版本化 Contract 交接，保留旧 payload 边界映射；
3. B0 先固定 immutable Evidence bundle，B 只对该 intake + bundle 做 deterministic Dry-run parity，并只产生 `ExecutionPlanningValue`；
4. C1 后 Core 建立独立 Order/Fill/Protection owner storage；
5. Core 通过 Web owner API 更新请求状态，不直写 Web 表；
6. Web 通过 Core API/Event 建立只读结果投影；
7. shadow 对比旧 `exchange_order_results` 与 Core 事实；
8. 切换 Web 展示读取；
9. 旧结果表降级为兼容投影，调用方归零后删除或冻结写入。

验证：同一个执行请求只能生成一个稳定 Core OrderIntent；Web 投影丢失可从 Core 重建；Core 不再把 Web 表当 OMS。

## 8. 阶段 4：按业务链路继续迁移

推荐顺序：

1. Market canonical model、normalization、symbol rules、finality、quality、storage 与 point-in-time DatasetSnapshot；
2. Market 公共只读 K 线同步；平台固定 API Key 只存在于公共 Market Adapter，禁止进入 Domain 或 Research；
3. Strategy Definition、Registry、RuntimeSnapshot、Evaluator、EvaluationState 与 Signal；
4. Research Domain + deterministic Backtest Kernel，先完成首个策略的严格时序、成本和逐层 parity；
5. Portfolio allocation、冲突和净额的 dry-run；
6. Pre-trade Risk 与冻结 snapshot 的 dry-run；
7. ExecutionPlanningValue、PaperEvent 与恢复测试地基；
8. Strategy → Web handoff、canonical ExecutionRequest 与 read-only intake；
9. Execution OMS、订单状态和交易所 Gateway；
10. FillEvent、AccountProjection、Continuous RiskAction 与 Protection saga；
11. Reconciliation、恢复命令、其他策略的 Backtest/live parity、Analytics 与 ResearchEvidence；Vegas 继续作为第二个策略验收切片。

每个步骤继续使用第 3 节清单，不能在同一切片中同时调整策略判断、资本分配、风险阈值和执行协议。

代码形态迁移同样按垂直切片完成：

- `TradingState` 拆为 Strategy EvaluationState、Research SimulationLedger 和 owner typed output，禁止重建新的万能 Context；
- `deal_signal` 由 Research use case 的跨域编排逐步绞杀，不能整体搬入 `impl` 或新 `TradingEngineService`；
- 无状态计算优先迁为纯函数；共享冻结配置的决策迁为 Policy；
- 有 identity/生命周期的 Order、Run、Release 使用 Aggregate，关键字段通过 transition 保护；
- Worker 只保留循环、Contract 映射、Use Case 调用和 ack/checkpoint；
- Trait 只保留真实 Port、稳定 API 或多实现边界，删除没有调用证据的工厂/基类式抽象。

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

`signal-worker` 只装配 Market -> StrategySignal。唯一执行路径必须等待 Web canonical `ExecutionRequestV1` 经 C0 的 `ClaimExecutionRequestV1`、`RenewExecutionRequestClaimV1`、`ReleaseExecutionRequestClaimV1` 租约和 `ReportExecutionRequestOutcomeV1` 结果 Contract 带回稳定账户、凭证和风险配置引用后，才由 `execution-worker` 装配账户级 Portfolio 与事前 Risk；不得创建 Core 自营/系统自营 `ExecutionRequest`，也不得把固定 Market 公共只读 API Key 当成这些引用或验证前置。持续 Risk 初期由 `account-worker` 装配。只有独立吞吐、故障隔离或安全证据出现时，另立 ADR 增加 `portfolio-worker` 或 `risk-worker`。

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
- 让 ResearchEvidence 只读引用 Experiment、DatasetManifest、样本、成本和 SimulationProfile 的不可变 identity/hash；
- 让 ResearchEvidence 引用 EvaluationManifest、trial ledger、ResearchExecutionArtifactRef、RngSpec、SchedulerSpec，并记录首次 scenario divergence；
- 把 lifecycle、promote、rollback 写入 `StrategyRelease`；
- promote 创建 `PromotionReceipt`，证明被研究 candidate 与 released artifact 的 source/build/parity 链；
- 发布不可变 `StrategyRuntimeSnapshot` 给数据面；
- Portfolio、Risk、Execution 分别发布自己的不可变 Policy Snapshot；
- Web `risk_profile_ref + version` 经 Core Risk 校验、默认值展开和 canonical hash 后幂等解析为 `RiskPolicySnapshot`，缺失或不兼容时不允许默认放行；
- Execution 发布不可变 `ExecutionDecisionContextSnapshot`；Research 发布不可变 `ResearchRunSpec`，并为每个场景创建 `ResearchDecisionContextSnapshot`；
- 为有状态 evaluator 建立由 EvaluationScopeId、RuntimeSnapshotId 与 MarketStreamPartition 组成的状态身份；
- Registry、Catalog、Signal builder 和 Worker 使用同一 strategy identity；
- legacy alias 只在边界 Adapter 保留。

验证：历史 Definition/Artifact/Evidence 字节身份不被覆盖；相同 RunSpec、`ResearchDecisionContextSnapshot`、ResearchExecutionArtifact、动态 Evidence、RNG/Scheduler 和 EvaluationState before 可逐层重放；Completed Evidence 不自动变为 eligible，Release 变化不会修改历史信号、订单或回测事实。

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
- backtest、paper、shadow、canary、live 使用同一 Strategy entry/exit、Portfolio、Risk/final-stop 和 Execution planning 实现；Research 使用 `ResearchDecisionContextSnapshot`，live 使用 `ExecutionDecisionContextSnapshot`；exact parity 同时证明四个 Policy Snapshot、Context Core、动态 Evidence、EvaluationState before 和 Clock identity 一致；
- Research-only crate 不进入生产 App 依赖图和 `core-runtime` 镜像；Research-only 变更无生产部署资格，共享 Domain 变更由 Cargo 依赖图触发受影响生产构建与 parity；
- 三个 Release Unit Manifest、实际镜像内容、Compose 角色和 deploy contract 一致；
- 尚未发布的候选策略只存在于 Strategy candidates catalog；晋级 released catalog 后才进入生产构建与 live RuntimeSnapshot，不为每个策略建立服务或 crate；
- 新增代码按 Entity/Value Object、纯函数、Policy、Use Case、Port、Adapter 唯一归类，不再新增零状态万能 Service/Calculator；
- Web ExecutionRequest 与 Core OMS 事实完全分离；
- 数据库 CRUD、事务、Outbox 和查询归属可以唯一定位；
- 成交反馈、持续风险、保护和 Reconciliation 闭环完整；
- 控制面不在交易热路径；
- 所有外部 mutation 都有幂等、Unknown、恢复和对账证据；
- legacy 入口、兼容字段和旧表写入全部有明确结束结论。
- 现有 Vegas 回测入口完成 parity 切换后，`BacktestRunner`、`BacktestExecutor`、万能 `BacktestContext` 与 `deal_signal` 的对应 legacy 职责均有删除证据。
- 每个架构迁移切片都有版本化 Manifest、当前 revision 的 Evidence 和 Migration Verdict；规范基线、实际 diff、迁移模式、授权与完成状态一致。
