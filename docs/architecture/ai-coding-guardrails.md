# AI 编码与架构防腐护栏

- 状态：已接受
- 日期：2026-07-23
- 最近修订：2026-08-03
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 放置规则：[业务代码与数据访问放置规范](business-code-and-data-access.md)
- 详细目录：[目标目录与代码放置规则](target-directory-layout.md)
- 迁移协议：[AI 架构迁移执行协议](ai-migration-execution-protocol.md)
- 模块与可见面：[ADR-0015](adr/0015-capability-first-modules-and-api-spi-boundaries.md)

## 1. 目标

目录只能降低混乱概率，不能阻止 AI 把业务规则写进 Handler、把 SQL 写进 Use Case，或创建新的万能 Service。本规范将架构从“建议”变成可审查、可测试、可渐进自动执行的约束。

## 2. 规则权威顺序

发生冲突时按以下顺序处理：

1. 系统、用户和仓库安全规则；
2. 已接受 ADR；
3. `target-architecture.md`；
4. `dependency-rules.md` 与 `business-code-and-data-access.md`；
5. 子目录 `AGENTS.md` 的增量规则；
6. 模板和示例。

子目录规则只写相对上层的差异，并链接权威文档。禁止复制整份架构规范，否则多份文本会在后续修改中漂移。

## 3. 编码前强制输出

任何新增功能、重构或跨文件修改开始前，AI 必须先声明：

```text
业务目标：
唯一 Owner：
Capability / 子领域：
Domain Wave：
能力总账 target / status / reuse_policy：
已有 canonical 实现 / 唯一复用路径：
职责：Command / Query / Event Consumer / Pure Policy
代码形态：Entity / Value Object / Pure Function / Policy / Stateful Transition / Use Case / Port / Adapter
为何不能使用更简单的纯函数或 module：
要保持的不变量：
输入入口与 Contract：
Use Case：
Model / Policy：
Ports：
Port 完整性：生产 Use Case 调用方 / 生产 Adapter / 失败与恢复证据
Adapters：
公开面：`api` 消费者 / `spi` 实现者 / 私有 module
预计文件预算：生产行数 / 总行数 / façade / tests
热路径与有界状态：窗口 / allocation / collect-sort / 配置快照：
运行模式与替换的 Adapter：
Backtest/Paper/Live 共用的业务 symbol、四个 Policy Snapshot 与 Decision Context：
Context 类型：live `ExecutionDecisionContextSnapshot` / Research `ResearchDecisionContextSnapshot`（不得混用）：
稳定账户绑定与准入证据：ExecutionAccountBinding / AccountAdmissionEvidence / ExchangeAccountRef：
必需行情证据：RequiredMarketEvidence / BarFinalization / MarketDecisionReadiness / ResolvedMarketEvidenceSet：
允许因 SimulationProfile/Adapter 产生差异的层：
构建影响：Release Unit；root/受影响 Cargo package；是否有生产部署资格：
事务边界：
持久化顺序与外部副作用边界：
数据库往返预算：正常路径 batch 数 / bind 上限 / 锁内 await：
并发业务唯一约束（不得只写 worker lease）：
幂等身份：
Web `ClaimExecutionRequestReceiptV1` ref/hash / current fence / expiry：
Attempt / Unknown / 恢复证据：
SafetyObligation / SafetyMonitoring 闭合与监测交接：
失败与恢复 Owner：
验证：
```

如果任务属于 legacy、crate、Owner、事实源、运行入口或 Backtest/Live 双实现迁移，聊天声明只用于沟通，不能替代能力总账和当前 Domain Wave 的 L2 语义盘点。AI 必须先查询 `rust_quant_alpha/architecture/business-capability-catalog.toml`，按 [AI Domain Wave 迁移执行协议](ai-migration-execution-protocol.md)冻结 owner、target、legacy 处置、业务不变量、验证和删除门。

出现以下情况时必须停止并请求确认：

- 同一事实可能由两个 owner 写入；
- 需求要求跨库直写或 Admin/Web 绕过 Core；
- 需要改变 Order、Fill、Protection、Risk、Release 等状态机语义；
- 需要实盘 mutation；
- 现有 Contract 无法表达且可能影响其他仓库；
- 迁移同时改变目录和策略/风控/执行行为，无法分开验证。
- Strategy evaluator 需要账户余额、用户风险配置、最终下单数量或环境变量才能运行。
- 同一业务语义准备新增 `Backtest*` 与 `Live*` 两套配置、Policy、止盈止损或 `ExecutionPlanningValue`（含 child `OrderPlan`）；或准备让 Research 创建/比较 live-only `ExecutionPlan` aggregate；
- 准备创建零字段 `*Service`、`*Manager`、`*Calculator`，但无法说明其状态、依赖或策略版本；
- Aggregate 需要公开可变字段或任意 setter 才能被现有调用方使用。
- 计划创建 Core/系统/自营 `ExecutionRequest`，或把平台固定 Market 数据 API Key 当作用户账户、私有流或 mutation 凭证；该产品不存在自营执行路径，见 [ADR-0013](adr/0013-user-execution-request-and-public-market-data-credentials.md)。
- 跨仓库请求没有 owner repo 的 Contract source、版本化 Envelope，或 Web 到 Core 的执行请求没有 Claim/Renew/Release/Outcome 生命周期。
- 计划让 Execution 直接消费原始用户私有流/query，或让 AccountProjection 晚于 OMS；固定方向必须是 Account owner 更新投影并发布 `AccountFactV1` 给 Execution Inbox。
- `execution_account_ref` 无法解析为 Web owner 的稳定 `ExecutionAccountBindingV1`，或 lease/shard/fence 仍以可轮换 credential 作为账户身份。
- 多周期策略没有 `RequiredMarketEvidenceV1`/Bar finality，或准备以短周期新鲜替代必需长周期未闭合证据。
- 计划在仍有 ManagedExposure、开放订单、Unknown/attempt/permit 或未闭合 Account evidence 时删除 `SafetyObligation`/关闭监测会话。
- 计划先写 Port/Trait 和 Fake，无法指出同一 Wave 中的生产 Use Case 调用方、生产 Adapter 与恢复证据。
- 计划让其他 Domain/Research 导入 `spi`，让 Adapter 访问 Domain 私有 capability，或让 App 在已登记的运行装配入口以外使用 `spi`。
- 计划建立 Owner 级 `enums.rs`、`types.rs`、`common.rs`、`shared.rs`，或继续向已超过 ADR-0015 Error 预算的目标文件增加业务。
- 一个 Use Case 需要四个及以上有副作用 Port、两个独立恢复结果，且无法拆出清晰的后续 Command/Consumer/process manager。
- 计划为 `module-boundary-policy.toml` 已登记的 canonical 类型、指标、公式或 Engine 新增同义名称或第二实现。
- 计划在生产代码新增 `support`、`*_helpers`、`*_support` 等泛化容器，或用它们绕过文件预算。
- 计划在 provider/advisory lock 或数据库事务内执行外部 SDK/HTTP I/O，或让正常 snapshot 提交逐行 SQL `await`。

## 4. 默认迁移单位：Domain 业务闭环

AI 不按“先建所有 model，再建所有 repository，再建所有 service”的横向方式生成，也不再为每个微小切片维护独立 Manifest。一次只推进当前 Domain Wave 内可验证的业务闭环：

```text
入口映射
  -> Owner capability
  -> API Input/Output
  -> Use Case + Model/Policy
  -> SPI Port
  -> production Adapter
  -> 必要 Contract
  -> Tests
```

每个 capability 必须能说明输入、业务结果、数据库变化、外部副作用、失败状态和恢复方式；整个 Wave 必须形成真实入口到生产 Adapter、恢复和 parity 的闭环。只创建实际需要的 capability 目录，不先建空 `model/ports/use_cases` 骨架。Fake-only Port 不是完成能力。

## 5. 代码形态

不预建通用 command/query/consumer 目录或模板。只有真实 capability 出现重复且稳定的代码形态时，才提炼局部示例；模板不得创建业务万能基类。

- Command：状态变化、事务、幂等和 Outbox；
- Query：只读模型、索引、分页和陈旧度；
- Event Consumer：合同版本、Inbox、顺序、ack 和重放；
- Pure Policy：无 I/O、显式输入、确定性输出；
- Process Manager：只在跨多个本地事务且有独立恢复状态时使用。

## 6. 自动门禁

目标仓库已经提供两个活跃命令：

```text
cargo xtask arch-check
cargo xtask wave-check --wave W1
```

`arch-check` 校验能力总账唯一性、owner/kind/Wave/reuse_policy、目标路径、package role、依赖、Release Unit、API/SPI、Port 和文件预算。`wave-check` 还要求指定 Wave 的非 deferred capability 全部为 implemented，然后一次执行冻结的 Cargo package tests。它们不能替代行为、数据库、恢复、parity 或发布验证。

| 门禁类别 | 必须由谁执行 |
|---|---|
| 能力总账、目录、依赖方向、禁止 API、role map、未知 package、静态 Contract 与测试注册 | `cargo xtask arch-check` |
| Wave capability 完成状态与冻结 Cargo package tests | `cargo xtask wave-check --wave Wx` |
| owner 单事务、唯一约束、`ExecutionPlanningValue` 到 live aggregate 的无损初始化、RiskAction 去重 | Postgres/Adapter 集成测试 |
| 外部 owner binding、Envelope、N/N-1、`ActivationEligibilityV1`、claim fence、AccountFact/SafetyMonitoring | Contract/compatibility test |
| Strategy/Portfolio/Risk/Planning value parity、ResearchBar 安全顺序与模拟 KillSwitch | 确定性 parity/safety harness |
| `RequiredMarketEvidence`/Bar finality、Account admission/recovery、claim/permit/fence、Unknown 与恢复 | pre-trade/recovery 集成测试 |
| Release Unit、镜像 allowlist、cutover、真实 mutation 授权与 Evidence | release contract / deploy verification / 显式授权 |

以下目标要求必须由上表至少一种门禁覆盖：

1. 路径与 crate 分区；
2. Domain 对 SQLx、Redis、Reqwest、SDK、环境变量和 Contract 的禁止依赖；
3. Contract 对 Domain、SQLx 和 SDK 的禁止依赖；
4. App 以外的环境变量读取；
5. 跨 owner SQL 和无 owner migration；
7. Testkit 被生产代码依赖；
8. 未版本化跨进程 payload；
9. 新增/触碰文件行数上限；
10. 必需的 owner 声明、数据库注释与 Contract snapshot。
11. `quant/*` 依赖业务 Domain、Adapter、数据库或环境变量；
12. Strategy evaluator 接收账户风险配置，或产生最终仓位/订单数量；
13. 策略状态 key 缺少 RuntimeSnapshotId、MarketStreamId 或等价版本身份。
14. 生产 Domain 依赖 Research，或 Research 绕过其他 Domain 公开 API；
15. ResearchBar 声称覆盖 lease、outbox、Unknown、保护恢复或 Reconciliation；
16. 多币种回测按 symbol 遍历顺序逐个分配资金；
17. ResearchEvidence 由 Strategy 表直接拥有，或跨对象存储/Postgres 宣称全局原子。
18. 新增零字段 `*Service`、`*Manager`、`*Calculator` 仅提供 associated functions；
19. Aggregate 暴露可绕过状态机的不变量字段，或 Model/Policy 读取系统时间、全局随机源、环境变量和进程全局业务缓存；
20. backtest/paper/live 新增重复 Strategy、Portfolio、Risk、止盈止损、`ExecutionPlanningValue`（含 child `OrderPlan`）实现，或同一 payload 解析为语义重叠的配置；Research 创建、持久化或用 live-only `ExecutionPlan` aggregate 做 parity；
21. 生产 App 依赖 `strategy-candidates`、`domains/research`、`quant/backtest`、`quant/analytics` 或 `apps/quant-lab`；
22. CI 构建影响范围与 Cargo 反向传递依赖/Release Unit Manifest 不一致：Research-only 改动误触发生产镜像，或共享 Domain 改动漏掉生产构建和 parity；
23. `core-runtime` 镜像 binary 与 allowlist 不一致，或混入 Research/Backtest/Paper/candidate/schema-tool；
24. 生产 App 不是独立 Cargo package，或其依赖闭包包含 Research-only package；
25. `StrategyRuntimeSnapshot` 混入 account、user、credential、risk profile 或其他 owner 的政策内容；
26. Execution intake 没有原子持久化四个 Published Policy Snapshot 的 `ExecutionDecisionContextSnapshot`，或后续对象缺少 `context_id + context_hash + subject_binding_hash`；
27. 热路径读取“最新配置”、环境变量业务默认值或 Web 原始 JSON，或者 Snapshot/Context/RunSpec 缺少 canonical hash 与兼容测试。
28. 新能力未登记 capability id、owner、target、Wave、reuse_policy 和消费者，或同一 target 被重复占用；
29. 迁移实现改变业务输出、默认值、舍入、时序、事务或错误语义，却未在当前 Wave 语义矩阵标为 optimize 或 retire；
30. 同一实现批次混合多个事实 owner，或把 cutover、legacy delete 与业务迁移一起执行；
31. Wave Evidence 或测试不属于当前 revision，或只用 AI 文字结论代替确定性证据；
32. cutover、生产写入或真实交易 mutation 缺少独立显式授权。
33. Core 在信号生成/handoff 阶段查询 Web 订阅、接收候选用户/credential/risk profile 商业明细，或自行创建/批量提交 Web canonical `ExecutionRequest`；正确边界是 `StrategySignal -> CreateExecutionRequestFromSignalV1 -> Web owner`。Web 创建请求后，Execution 可以消费其稳定引用并按 owner Contract 校验；
34. News 把 AI 分析直接作为可执行信号或直接调用 Web 执行请求入口；正确链路只能是 `NewsInsightV1 { version, published_at, available_at } -> StrategyRuntimeSnapshot 声明可消费的 evaluator -> StrategySignal -> Web owner`，且 evaluator 只能在 `available_at <= DecisionTime` 时消费；
35. 将 OHLCV/Candle 事实放入 `common`、`platform/kernel` 或 `quant/*`，或让 DB Row/交易所 DTO 成为其长期模型；canonical MarketBar/Candle 必须归 Market；
36. target package/app 未被 role map 分类、未知 package 被静默跳过、`apps/` 未被静态扫描，或能力总账与真实目标路径漂移。
37. Core 自行创建/持久化/消费系统或自营 `ExecutionRequest`，或没有 Web 商业授权即进入用户 live Account、Portfolio、Risk、Execution 或 mutation；已持久化 `SafetyObligation` 的既有风险收敛除外，但它必须引用原 request/context/order identity，只可对 `ManagedExposure` 执行 Query/Reconciliation/Cancel/Protect/Reduce/Close，仍受 capability、permit/fence 与 reduce-only 证明约束。Research 必须使用 `ResearchScenario`/`ResearchRunSpec`，只能产生 Research-scoped 模拟输出，不能伪造用户请求或形成 live Account/OMS 事实。
38. 将平台 `MarketDataAccessCredential` 与 Web 用户 `credential_reference` 混用，或让前者进入 Account、私有流、Risk、Execution、MutationPermit、用户凭证表或跨仓库执行 Contract；公共只读数据与私有/mutation 配额必须隔离。
39. 跨仓库 Contract 没有 owner repo source、schema/version、N/N-1 兼容声明、统一 Envelope（event/correlation/causation/idempotency/aggregate/sequence/time/partition）或消费端绑定来源。
40. Core 以轮询/SQL 解释 Web `execution_tasks`，而不是通过 Web owner Claim/Renew/Release/Outcome Contract 持有可恢复 lease、幂等 receipt 与结果投影。
41. `degraded`/stale Account 或非 `ReadyForNewRisk` 的 `MarketDecisionReadiness` 允许新增风险，或把运行时 session/readiness 事实写进不可变 Policy Snapshot/Decision Context；该状态只允许对账、持续风控和可证明 reduce-only。
42. 多 owner 迁移没有按 Domain Wave 拆出唯一事实 owner、依赖关系、各自本地事务/Inbox/Outbox 及 Contract cutover 约束。
43. `ActivationPointer` 绕过 Strategy owner 的 `ActivationEligibilityV1`，或 channel×stage、release/evidence/eligibility generation、撤销语义没有 Contract test。
44. Research 绕过 Market historical API/Contract 直读 Market Storage、backfill、生产 Adapter 或 `MarketDataAccessCredential`。
45. 持续风险动作缺少 `risk_action_decision_id = subject_binding_hash + trigger_event/evidence_hash + risk_policy_snapshot_hash + action_generation`，导致重放重复减仓/平仓。
46. Claim/Renew/Release/Outcome 缺少 current `ClaimExecutionRequestReceiptV1` 的 `claim_fence`/expiry/CAS，或 live Context、batch/source mapping、Risk approval、planning/OMS、attempt、Gateway capability、final gate、permit 未绑定同一 receipt ref/hash，TTL 未被最早 claim expiry 截断。
47. 原始用户私有流/query 绕过 Account owner 进入 Execution，或 Account 未先更新投影就发布缺 source cursor/session generation/projection revision 的 `AccountFactV1`。
48. 稳定 `ExchangeAccountRef` 与 credential revision/revocation generation 混为一个 identity，credential rotation 创建第二个 session/lease/slot，或 `ExecutionAccountBindingV1` 和 `AccountAdmissionEvidenceV1` 的账户/产品/mode/binding generation 不匹配时仍新增风险。
49. Strategy 缺少 `RequiredMarketEvidenceV1`，或 Market 没有用 `BarFinalizationV1`/`MarketDecisionReadinessV1` 形成完整 `ResolvedMarketEvidenceSetV1`；Bar final/revision/source/continuity generation、迟到/源切换、aggregate hash/TTL 不能确定性聚合。
50. `SafetyObligation` 闭合谓词不完整；`SafetyMonitoringV1` 缺受管摘要、最低 session generation/watermark，Add/Update/Remove 没有 fence、Outbox/Inbox、`SafetyMonitoringAckV1`、全量重放与保守会话保留；或 credential 撤销后绕过 `SafetyBlocked`/产品授权的人工处置。
51. `ObservedExternalPosition` 被禁止自动处置，却未计入 RiskValuationSnapshot 的保证金/净敞口/风险预算。
52. `ResearchRunSpec` 未绑定实际 `ResearchExecutionArtifactRef`/EvaluationManifest，Completed Evidence 被误当作 promotion eligibility，或 candidate 重建为 released 后没有 `PromotionReceiptV1`。
53. RecoveryHarness 被加入可部署 Release Unit/生产镜像，获得生产 Secret/账户/存储，或使用非临时基础设施。
54. 同一业务概念在能力总账外创建第二份实现、共享 Registry 或目录表，或者以普通源码 hash 代替业务 parity。
55. 其他 Domain/Research 导入目标 Domain `spi`，Adapter 绕过 `spi` 访问私有 capability，或 App 在已登记的运行装配入口以外导入 `spi`。
56. `api` 重导出 Port/Adapter/Row/SDK，`spi` 暴露私有 Aggregate/Use Case，或 crate 根绕过双门面平铺公开类型。
57. Domain/Adapter 生产代码、任意 Rust 文件、façade 或测试文件超过 ADR-0015 预算，或通过宽生成文件豁免、`part1.rs`/`helpers.rs` 掩盖。
58. `lib.rs`、`mod.rs`、`api.rs`、`spi.rs` 承载业务分支、SQL、SDK 映射或大段测试。
59. 新增 Domain 级 `enums.rs`、`types.rs`、`common.rs`、`shared.rs`，把不同 capability/owner 的状态或不同 Wire/Row/SDK 表示混在一起。
60. 非测试 Port 没有生产 Use Case 调用方、同 Wave 生产 Adapter、失败/原子性/恢复证据，或只为 Fake/Mock 建立生产 Port。
61. 一个 Use Case 持有四个及以上有副作用 Port、两个可独立恢复的主要结果，或通过万能 `Services`/`EverythingPort` 隐藏依赖。
62. `exchange-gateway` 以 provider 大文件重新混合 public-market、private-account、fenced-mutation capability。
63. 仅因文件未超过预算而保留多个独立变化原因，导致修改一条规则必须同时理解无关状态、协议、持久化或运行生命周期。
64. 有序规则链继续共享可变布尔值和字符串原因，或拆分后改变规则顺序、首个阻塞和诊断输出；正确形式是短编排入口、业务命名私有规则和强类型结果，并由 parity 测试固定顺序语义。
65. 宽 Domain 对象、宽 SQL 或多规则 readiness 继续平铺维护：Domain 应按不变量拆值对象，Postgres Adapter 应以单一 Row/bind 映射维护列对应，readiness 应由私有 assessment 统一去重和归约。
66. App 监督循环混合连接、live loop、重试、时间策略和业务恢复决定，或把业务错误分类搬入 App；App 只拆运行职责并保留唯一入口，恢复与状态迁移仍归 Domain。
67. 旧配置版本越过 `raw -> canonical` 转换层渗入业务流程，或在没有真实调用方、存量数据、外部契约和迁移窗口时保留 legacy 类型、别名、旧 re-export。
68. `api`/`spi` 成为无 capability 分组的类型袋，迫使调用方阅读整个 Domain 才能找到入口；门面必须按业务能力导航且不保留无证据的平铺兼容出口。

## 7. 渐进 Ratchet

当前 Workspace 已有扁平 crate、跨层依赖和 legacy 数据路径，不能直接以最终规则扫描全仓并长期红灯。

实施顺序：

1. 生成只读违规基线；
2. CI 禁止新增违规；
3. 每完成一个 Domain Wave，删除对应 legacy 白名单；
4. 白名单必须包含 owner、原因、删除条件和最晚复查日期；
5. 违规总数只能下降，不能通过扩大 glob 或忽略目录恢复绿灯；
6. 最终删除 legacy allowlist。

`rust_quant_alpha` 已建立 package/path role、未知 package fail-closed、`apps/` 与目标源码根扫描、capability/API-SPI/Port/file-budget 静态规则，以及能力总账和 Domain Wave 门禁。受跟踪 CI 仍按迁移阶段延后；当前本地 PASS 不能外推为行为、恢复、数据库、SDK parity 或生产发布已验证。

小型 legacy bugfix 可以留在原位置，但不得新增跨层依赖、扩大 API 或把新能力继续堆入 legacy。新增业务能力默认进入目标架构。

## 8. Review 检查表

### 8.1 边界

- 是否只有一个事实 owner；
- 是否放入正确 Domain、capability 和 Wave；
- 是否出现跨 Domain 私有依赖、跨库 SQL 或共享 Row；
- 是否把 Market/Account/Execution 的 freshness/session/执行可用性证据误写成 Control 的 Readiness；Control 只能发布 Release/Kill Switch 并聚合只读诊断；
- Web 是否拥有稳定 `ExecutionAccountBindingV1`，Account 是否只发布 `AccountAdmissionEvidenceV1`/`AccountFactV1`，且 credential 轮换不改变 `ExchangeAccountRef`；
- 用户私有流/query 是否只由 Account owner 消费并先更新 AccountProjection；Execution 是否只经 Inbox 消费 owner fact；
- Core 的信号路径是否只把 `StrategySignal` 提交给 Web owner、没有读取候选商业明细或创建 Web 请求；Execution 是否只在 Web 已创建的请求中消费稳定授权引用；News 是否只提交可追溯、版本化的 `NewsInsightV1` 给 Strategy ingress，且仅由 `StrategyRuntimeSnapshot` 声明可消费的 evaluator 在 `available_at <= DecisionTime` 时消费；
- 是否错误保留 Core 自营执行路径；平台 Market 公共数据材料是否被严格限制为 Adapter 内的只读能力，且与用户 `credential_reference`、私有流、账户和 mutation capability 分离；
- 是否把技术失败误写成业务状态。

### 8.2 Rust 代码形态

- Entity/Aggregate 是否确有 identity、生命周期或不变量，而不是 DTO 加方法；
- Value Object 是否在构造时校验且保持不可变；
- 无状态确定性逻辑是否优先使用纯函数；
- Policy 对象是否只持有不可变、带版本的强类型快照；
- Stateful Evaluator 是否显式传入/返回完整作用域 State；
- Use Case 对象是否因为需要 Port 和流程编排而存在，而不是万能 Service；
- Trait 是否有真实生产边界或多实现证据，而不是只为 Fake/Mock 创建；
- 非测试 Port 是否有生产 Use Case 调用方、生产 Adapter、失败/原子性/恢复证据；
- 一个 Use Case 是否只有一个主要结果和恢复 Owner；四个及以上副作用 Port 是否已拆分或有明确理由；
- Domain 是否按 capability 导航，其他 Domain 是否只见 `api`、Adapter 是否只见 `spi`、App 是否仅在已登记的运行装配入口使用 `spi`；
- `api`/`spi`/crate root 是否泄漏 Port、私有 Model、Row、SDK 或万能 glob re-export；
- enum/error 是否与状态机/用例/Port 共置，Wire/Row/SDK 表示是否仍在各自边界；
- 生产代码、总文件、façade 和测试文件是否满足 ADR-0015 预算；
- 修改一条小规则是否被迫理解多个不相干变化原因，而不仅是文件是否超行数；
- 有序规则是否由短编排入口和强类型结果保持原顺序、首个阻塞与诊断 parity；
- 宽 Domain 对象是否按不变量拆值对象，宽 SQL 是否只有一个 Adapter Row/bind 映射及逐字段数据库回读证据；
- readiness/validation 的 blocker、degradation、去重和最终状态是否在一个私有 assessment 中归约；
- App 长循环是否按 supervisor/live loop/retry/timing 拆运行职责，同时把错误分类和恢复决定留在 Domain；
- 多版本兼容是否只停在 raw 输入转 canonical 模型处；legacy、别名和旧 re-export 是否具有真实消费者或迁移窗口证据；
- `api`/`spi` 是否按 capability 可导航，而不是平铺的类型袋；
- 能破坏不变量的字段是否私有，时间/随机/外部事实是否显式输入；
- 是否把 `deal_signal` 一类跨 owner 大函数仅移动进 `impl`，而没有拆分 owner。

### 8.3 数据库

- SQL 是否只在 Postgres Adapter；
- Port 是否使用业务语言；
- 事务是否覆盖状态、幂等和 outbox；
- Outbox 的业务语义/恢复是否仍由原 Owner Use Case 决定，Publisher/App/Adapter 是否只负责通用投递机制；
- Execution live 下单事务是否同时覆盖 AccountOpeningSlot、不可变审批引用/父 Intent 与 `ExecutionPlanningValue` hash 唯一绑定、OrderIntent、由该值无损初始化的完整 `ExecutionPlan`/`ProtectionPlan`、`SubmitPending`、幂等和提交 Outbox；
- 是否只有事务提交后才确认上游或发布提交任务，且交易所 I/O 只由 Fenced Gateway 在 Dispatcher 签发的 current MutationPermit 被原子消费后发起；
- 查询是否有索引、范围、分页和锁评估；
- 新表/列是否有数据库注释；
- 删除是否符合事实保留规则。

### 8.4 交易安全

- 是否由 Risk owner 先持久化不可变 RiskDecision，再由 Execution owner 原子持久化稳定订单身份、由 `ExecutionPlanningValue` 初始化的 live aggregate、`SubmitPending`、幂等和 Outbox；
- 是否区分 read-only、dry-run、paper、shadow、canary、live；
- 是否保留 lease、精度、余额、凭证、`MarketDecisionReadiness`/账户新鲜度和保护门禁；非 `ReadyForNewRisk` 时是否只允许可证明的 reduce-only；
- Strategy 声明的 `RequiredMarketEvidenceV1` 是否由 Market 以 `BarFinalizationV1`/`MarketDecisionReadinessV1` 完整解析为 `ResolvedMarketEvidenceSetV1`，并满足 final/revision/source/continuity/迟到/aggregate hash/TTL 规则；多周期集合是否禁止部分满足放行；
- `ObservedExternalPosition` 是否进入 RiskValuationSnapshot 的 unmanaged exposure/保证金占用，默认阻塞或压缩新风险；
- `ExchangeExecutionCapabilityProfileV1` 与 `RiskValuationSnapshotV1` 是否绑定目标 exchange/product/mode，Unsupported/Unknown 是否 fail-closed；
- 持续 RiskAction 是否以 `risk_action_decision_id = subject_binding_hash + trigger_event/evidence_hash + risk_policy_snapshot_hash + action_generation` 幂等，避免重放重复减仓/平仓；
- 未有独立 Risk Reservation ADR 时，是否以 `ExchangeAccountRef` 为键的持久 opening slot/唯一约束禁止同一物理账户并发独立开仓，而非只依赖 worker lease、`execution_account_ref` 或 credential revision；风险降低旁路是否可证明 reduce-only 并先冻结风险增加 claim；
- Fenced Gateway 是否在网络 I/O 边界原子消费 attempt/version/fence/generation/payload hash/expiry 均匹配的 current permit；revoked/stale/expired 是否 DefinitelyNotSent 且不触达 SDK；raw mutation SDK 是否对 Dispatcher/其他 App 物理不可达；
- 门禁失败是否按 Expired/Blocked/可恢复分类，后者有 durable next_eligible_at/唤醒条件；
- attempt claim、提交前取消/恢复 revoke 与 Gateway consume 是否竞争同一 version/permit；attempt ledger 是否区分 Submit/Cancel/Protect 并将 outcome、permit 终态、状态迁移、后续 Outbox 原子提交；
- Submit/Cancel/Protect 的 mutation event、attempt、permit 是否绑定 `mutation_event_id`/`mutation_generation`/`expected_aggregate_version`，旧 delivery 是否只能 ack/no-op；
- transient blocker、可重试 DefinitelyNotSent、lease/fence/gate 变化确认 delivery 后，是否原子 rollover 到新 generation 的 delayed Outbox/RetrySchedule；Scheduler 是否被禁止复用旧事件或直接 claim；
- Unknown outcome 是否禁止直接生成同 kind mutation Outbox；是否只有在持久 DefinitivelyAbsent/RecoveryAuthorized 且无可发送 permit 后，recovery transaction 才保持原 mutation/目标 identity、按 Submit/Cancel/Protect kind 新建对应 Outbox；不具备稳定 client identity 与缺席证明的 live 能力是否 Unsupported；
- Account 启动是否先取得单调 session generation、订阅/缓冲 User Stream，再合并 signed snapshot/query watermark 并补 gap；是否只有 `AccountRecoveryClosedV1` 有效后该账户 Dispatcher 才启用，Execution 未自行订阅；
- Web claim 是否以 current fence/expiry 进入 Context、capability、final gate 和 permit，Renew/Release/Outcome 是否由 Web CAS；旧 fence 与越过 claim TTL 的 capability 是否无法触达 SDK；
- SafetyObligation 是否满足完整闭合谓词，`SafetyMonitoringV1` remove 是否由 Account 以 `SafetyMonitoringAckV1` 对 current fence/session generation/watermark 确认；credential 撤销是否进入 `SafetyBlocked`，无法证明无义务时是否保守保留会话；
- 部分成交后保护数量是否正确；
- `Unknown`、撤单/成交竞态和重启是否可恢复；
- 没有有效止损计划时是否 fail-closed。

### 8.5 Contract

- Owner 与版本是否明确；
- Contract 是否在其 owner repo 定义并发布消费端绑定；Envelope 与业务 payload 是否分离；
- Domain 是否与 Wire DTO 解耦；
- 是否有旧 payload、未知字段和 snapshot 测试；
- event/correlation/causation/idempotency/aggregate/sequence 是否完整。
- Web `ExecutionRequest` 的 Core 消费是否经过 Claim、续租、释放、撤销与 Outcome 回写的 owner Contract，而不是跨库轮询。
- Claim receipt 是否返回单调 `claim_fence`/`claim_expires_at`；Renew/Release/Outcome 是否携带 current fence，并有迟到旧 fence Contract test。
- `ExecutionAccountBindingV1`、`RequiredMarketEvidenceV1`、`BarFinalizationV1`/`MarketDecisionReadinessV1`/`ResolvedMarketEvidenceSetV1`、`AccountFactV1`/`AccountRecoveryClosedV1`、`SafetyMonitoringV1`/`SafetyMonitoringAckV1`、`ExchangeExecutionCapabilityProfileV1`/`RiskValuationSnapshotV1` 是否各自有唯一 owner、版本和 N/N-1 窗口。

### 8.6 测试

- Model/Policy 单元测试是否固定业务不变量；
- Adapter 集成测试是否覆盖 SQL、约束和事务；
- Contract test 是否覆盖跨仓库兼容；
- Recovery test 是否覆盖重复、超时、崩溃、乱序和对账；
- Parity test 是否证明 backtest/paper/live 使用相同业务规则。

### 8.7 策略与回测防漂移

- Strategy evaluator 是否只消费 Market 证据、Strategy Runtime Snapshot、自身 Evaluation State，以及由该 Snapshot 声明可消费且 `available_at <= DecisionTime` 的版本化 `NewsInsightV1`；
- 候选止损/失效价是否仍是信号证据，而不是偷偷完成最终风险审批；
- 资金分配比例、真实 leverage、最大亏损与订单数量是否由不同 owner 明确建模；
- `StrategyRuntimeSnapshot` 是否只包含 Strategy owner 事实，并且不含账户、用户、凭证、risk profile 或其他 owner 的政策；
- Portfolio/Risk/Execution 是否各自发布强类型快照，Execution 是否用同一 `ExecutionDecisionContextSnapshot` 绑定四个 Published 引用；
- backtest/paper/live 是否调用同一 Strategy entry/exit、Portfolio、Risk/final-stop 和纯 `ExecutionPlanningValue`（含 child `OrderPlan`/`ProtectionPlanningValue`）业务 symbol，且没有 `LiveRisk`/`BacktestRisk` 双实现；Research 不得创建或比较 live-only `ExecutionPlan` aggregate，Paper 如模拟 aggregate 只能验证从同一 planning value 的初始化/状态迁移；
- fee/slippage/funding/candle path 是否只来自 SimulationProfile，未混入账户风险配置；
- Research use case 是否调用与 paper/live 相同的 Strategy、Portfolio、Risk 和必要 Execution 公开 API；
- Research 历史行情是否只经 Market historical API/Contract 获得，而没有直读 Market Storage、backfill、生产 Adapter 或 `MarketDataAccessCredential`；
- `quant/backtest` 是否仍保持无 Domain 依赖的纯模拟内核；
- 是否由 `ResearchRunSpec` 固定 DatasetManifest、EvaluationManifest、四个 Policy Snapshot、SimulationProfile、模拟账户初态、Clock/Seed、`ResearchExecutionArtifactRef` 和 canonical hash；
- DatasetManifest 是否保留 point-in-time universe 成员有效期、上市/退市、`available_at`/revision、缺口/修订政策和历史 InstrumentRules；EvaluationManifest 是否在看结果前固定 OOS/walk-forward、purge/embargo、参数搜索/Seed/预算、选择规则、holdout 重用和集中度审计；
- Completed Evidence 是否只表示运行/工件完整可见，promotion eligibility 是否独立；candidate 重新构建为 released 时是否有 `PromotionReceiptV1` 串联源码/工件/Evidence 与跨构建 parity；
- EvaluationStateKey 是否包含 EvaluationScopeId，确保并行 Run 不共享可变状态；
- 同一 decision time 的多币信号是否先收集后统一分配，symbol 重排是否不改变结果；
- parity 是否逐层比较 Signal、Target、RiskDecision、`ExecutionPlanningValue` 和 FillEvent，而不是只比较最终 PnL；live `OrderIntent`/`ExecutionPlan` 另以初始化/恢复集成测试验证；
- exact parity fixture 是否同时固定 Decision Context hash、动态 Market/Account/Instrument Evidence、EvaluationState before 与 Clock，并比较 State after、`ProtectionPlanningValue` 与 decision trace；identity 不同是否只标记为 scenario comparison；
- ResearchBar 是否先处理既有保护/working order，再写模拟 ledger、执行具有稳定 decision identity 的持续风控与 reduce-only planning，最后才在允许时评估新 entry；模拟 KillSwitch 是否只写 Research-scoped `SimulationNewRiskBlock`/Evidence，不请求 Control；
- live `StrategyEvaluationState` 是否仅由 Redis Adapter 保存，且并行 ResearchRun 是否以 `EvaluationScopeId` 隔离内存状态；
- ResearchBar、PaperEvent、RecoveryHarness 是否各自只声称覆盖其精度边界；
- RecoveryHarness 是否严格 CI-only、临时存储、无生产 Secret/账户/部署资格，并被 Execution/Account/Reconciliation 变更触发；
- 模拟成交是否只进入 SimulationLedger/ResearchEvidence，未污染生产订单/账户事实；
- Evidence 是否通过内容寻址对象 + Research owner Completed manifest 实现原子可见，而非虚构跨存储原子；
- 环境变量是否只在 `quant-lab` App 解析后映射成强类型 ExperimentSpec。

Vegas 的具体基线与迁移门见 [Vegas 与现有回测主链迁移实战](vegas-backtest-migration.md)。

### 8.8 唯一实现与性能证据

- 是否先查询目标仓库 `architecture/module-boundary-policy.toml`，并复用已登记的 Market、Quant、Strategy
  canonical 实现；Adapter DTO/Row 是否只表达 provider/storage 语义并在边界映射；
- 流式或逐 K 线代码是否使用有界窗口，避免每步复制完整窗口、头删 Vec、无界 collect/sort 和未消费诊断序列化；
- 配置是否由 App 一次解析并以不可变 typed snapshot 注入，Domain/Quant 热路径是否完全不读 env/“最新版本”；
- 外部读取是否在数据库事务和锁之外；正常提交是否按明确上限 batch，bind 数、statement 大小和锁时长是否受控；
- 策略优化是否保留 Feature/Decision/Backtest parity，数据库优化是否有真实 PostgreSQL 原子性/CAS/并发测试；
- 性能结论是否记录相同 release 构建、相同输入、查询次数或多次耗时；单次墙钟差异只能描述为方向性观察。

## 9. 文档与实现同步

以下变化必须同步文档或 ADR：

- 新 Domain、App、Contract major version；
- Owner 转移；
- 新的跨仓库写入链路；
- 状态机语义改变；
- 事务原子性改变；
- 新消息中间件或新一致性模型；
- `risk-worker`、`portfolio-worker` 等新运行角色；
- Postgres Adapter 拆 crate；
- 任何架构例外。
- SimulationProfile 能力边界、ResearchEvidence owner 或 Evidence 发布协议变化。
- AI 架构迁移协议、能力总账 schema、Domain Wave、完成状态或 wave-check 语义变化。
- 用户执行请求 owner、平台市场数据凭证范围、跨仓库 Contract source 或 public/private quota 语义变化。

执行具体迁移时，能力总账的 baseline 锁定 legacy 来源。AI 发现规范需要变化时必须停止当前实现，先单独更新或替代 ADR；禁止在实现后修改规范文档为代码辩护。capability 状态、Wave plan、Evidence 和 legacy ledger 必须随实际证据同步。

普通函数新增不要求更新架构文档；文档不应成为逐文件清单。

单纯把纯函数改成零字段对象、把对象改名为 Service、或把跨 owner 大函数移入 `impl` 不属于架构迁移，也不能作为删除 legacy allowlist 的证据。

## 10. 禁止用文档代替执行

以下表述只有获得新鲜证据后才能使用：

- “架构门禁已启用”；
- “所有依赖已符合目标”；
- “Web 不再拥有订单事实”；
- “恢复测试已覆盖全部状态”；
- “迁移完成”。

在对应代码、CI、Contract、测试和生产迁移完成前，必须明确写成“目标”“计划中”或“legacy 仍存在”。
