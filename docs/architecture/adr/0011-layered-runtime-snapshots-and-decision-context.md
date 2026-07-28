# ADR-0011：分层运行快照与完整决策上下文

- 状态：已接受
- 首次接受：2026-07-23
- 决策者：Rust Quant Core
- 细化：[ADR-0002](0002-versioned-strategy-manifest-and-contracts.md)、[ADR-0005](0005-control-plane-and-data-plane.md)、[ADR-0009](0009-research-domain-and-tiered-simulation.md)

## 背景

“回测与实盘使用同一配置”不能通过共享 JSON 或把所有字段塞入 `StrategyRuntimeSnapshot` 实现：

- Strategy 配置是策略发布级；
- Portfolio/Risk 配置可能是用户、账户、combo 或系统组合级；
- Execution planning 还受交易所、instrument capability 和执行协议约束；
- MarketSnapshot、AccountSnapshot 与交易规格是本次决策证据，不是静态配置；
- fee、slippage、funding、latency 和 candle path 只属于模拟模型；
- Web 拥有用户配置存储和商业授权，Core 各 Domain 拥有配置语义与最终决策。

如果不区分作用域，Strategy 会错误接管用户风险，运行入口会重新解析 JSON，Research 也无法证明与某次 live 决策使用了同一完整上下文。

## 决策

### 1. “完整运行配置”不是一个万能对象

系统使用四类不可变语义对象；live 的交接记录与 Research 的场景记录不能互相伪装：

```text
第一层：Domain Policy Snapshots
  StrategyRuntimeSnapshot
  PortfolioPolicySnapshot
  RiskPolicySnapshot
  ExecutionPlanningPolicySnapshot

第二层：可复用的纯语义绑定
  DecisionContextCoreV1

第三层：subject-specific binding
  ExecutionDecisionContextSnapshot   # Web canonical ExecutionRequest
  ResearchDecisionContextSnapshot    # ResearchScenarioRef

第四层：Research 运行声明
  ResearchRunSpec
  DatasetManifest
  EvaluationManifest
  ResearchExecutionArtifactRef
```

重放一次业务决策还必须记录动态 Evidence：

```text
MarketSnapshotRef
AccountSnapshotRef
InstrumentRulesSnapshotRef
Clock / observed_at
```

动态 Evidence 不是配置，不进入 Policy Snapshot；它与 `DecisionContextCoreV1`、相应 subject binding 一起构成完整可重放输入。原始固定公共行情配置称 `MarketDataAccessCredential`，只在 Market/Gateway 的公共 read-only Adapter 配置内存中存在；其非敏感 `MarketDataAccessCredentialRef` 或 `market_data_source_profile_id` 可用于公共配额、市场采集 provenance Evidence 或必要 Market Contract，但既不是**决策** Evidence，也不是 subject。

### 2. Domain Policy Snapshot 的唯一 Owner

| 对象 | Owner | 作用域与内容 |
| --- | --- | --- |
| `StrategyRuntimeSnapshot` | Strategy | Definition/Artifact/Release generation、entry/exit 参数、evaluator state schema、输入要求和策略能力 |
| `PortfolioPolicySnapshot` | Portfolio | allocation、排序、净额、容量、冲突和组合限制 |
| `RiskPolicySnapshot` | Risk | 账户风险比例、最大损失、敞口/回撤、leverage/margin 限制、final-stop 约束和保护要求 |
| `ExecutionPlanningPolicySnapshot` | Execution | 订单类型、TIF、拆单、价格保护、部分成交、`ProtectionPlanningValue` 生成规则和最大未保护窗口；仅 Paper simulated OMS/live 将规划值落实为 `ProtectionPlan` Aggregate |

`StrategyRuntimeSnapshot` 不再拥有或内嵌 Portfolio、Risk、Execution policy。它可以声明兼容的 policy schema/capability 范围，但不能固定某个用户的 risk profile。

每个 owner：

- 负责 schema、默认值展开、单位、校验和规范序列化；
- 在 Published 后保持不可变；
- 通过新 identity/version 发布变化，不原地覆盖；
- 只暴露稳定 snapshot reference、hash 和公共决策 API；
- 不允许其他 Domain 重新解释其原始 JSON。

### 3. Web 用户配置不是 Core RiskPolicySnapshot

Web 的 `risk_profile_ref + risk_profile_version` 是用户配置来源和授权引用，不是可以直接执行的风险政策。

目标流程：

```text
Web user risk profile/version
  -> ExecutionRequest
  -> Core 通过 quant-web-client/版本化 owner contract 按精确 ref/version 读取
  -> 校验单位、范围、账户/产品兼容和默认值
  -> 发布/解析为不可变 RiskPolicySnapshot
  -> 返回 snapshot id/version/hash
```

- Web 不计算最终止损、数量或 RiskDecision；
- Core 不直连 Web 数据库，也不共享 Web ORM/Row；
- Core 不通过 email、slug 或“最新版本”猜测风险配置；
- 无法解析、版本缺失、已撤销或不兼容时 Blocked，不回退默认风险；
- 相同 Web profile version 必须幂等解析到相同 canonical Risk snapshot hash；
- 用户变更创建新版本，只影响随后创建的新 Decision Context；已形成的历史 Context 不变。

### 4. DecisionSubjectRef、DecisionContextCoreV1 与两种 binding

业务决策先绑定一个不会泄漏秘密、也不会把 Research 冒充用户执行的 subject：

```text
DecisionSubjectRef
  = ExecutionRequestRef { execution_request_id }       # 仅 Web canonical request
  | ResearchScenarioRef { research_run_id, scenario_id, simulated_account_id }
```

`ExecutionRequestRef` 只能由 Web 创建的 canonical `ExecutionRequest` 提供；Research 不得制造 synthetic `execution_request_id`、combo、credential、风险配置或用户账户字段来复用 live 类型。Research 如需回放已发生的 live 决策，只能把原 request 记为可选的只读 Evidence link，当前 subject 仍是 `ResearchScenarioRef`，不获得执行授权。

`PortfolioEvaluationBatchRef` 不是第三种 `DecisionSubjectRef`，也不创建新的执行授权。它只在账户级 Portfolio 评估时引用一组已经各自形成 `ExecutionRequestRef`/live Context 的 source request；单请求 batch 仍保持该 request 为唯一 subject。batch hash 与 source mapping 进入 Portfolio/Risk/Execution evidence，用于重放净额与结果归因，不能替代任一 source request 的 claim、credential 或商业资格。

`DecisionContextCoreV1` 是 Execution 对外暴露的纯、稳定 API 值：它固定所有 owner 已发布的政策语义，但不表示一笔 Web 执行任务。至少包含：

```text
schema_version
instrument + timeframe
StrategyRuntimeSnapshotRef(id/version/hash)
PortfolioPolicySnapshotRef(id/version/hash)
RiskPolicySnapshotRef(id/version/hash)
ExecutionPlanningPolicySnapshotRef(id/version/hash)
required Contract versions
strategy release identity/version
context_hash
```

`context_hash` 只哈希上述规范化、语义稳定字段；不含 `DecisionSubjectRef`、数据库 ID、`DecisionTime`、WallClock、绝对 expiry、claim/lease、kill-switch observation 或任何 credential reference。这样 Research 可以比较与 live 相同的政策语义，而不伪造 live 请求身份。

两种持久化 binding 分别为：

```text
ExecutionDecisionContextSnapshot (Execution owner)
  context_id + DecisionContextCoreV1
  subject = ExecutionRequestRef
  subject_binding_hash
  Web request intake/idempotency identity + claim receipt/fence reference
  observed activation_pointer identity/generation
  observed kill_switch_catalog_generation + matched scope generations
  prepared_at_wall_clock + valid_until_wall_clock

ResearchDecisionContextSnapshot (Research owner)
  research_context_id + DecisionContextCoreV1
  subject = ResearchScenarioRef
  subject_binding_hash
  ResearchRunSpecRef + scenario inputs
```

`subject_binding_hash` 规范哈希 `context_hash + DecisionSubjectRef` 与该 binding 所需的稳定来源引用；它用于请求/场景唯一性，不能替代 `context_hash` 做跨 subject 的政策 parity。`ExecutionDecisionContextSnapshot` 不保存原始凭证、`GatewayCredentialCapability` 或 `MarketDataAccessCredentialRef`。

创建规则：

1. 只引用 Published、未撤销且 schema/capability 兼容的快照；
2. 所有默认值必须在各 owner 发布快照时展开，Core/binding 不补业务默认值；
3. Execution 只能在通过 Web `ClaimExecutionRequestV1` 获得有效 claim 后，在自己的 owner transaction 中持久化 live binding；Research 只在自己的 Run transaction 中持久化 Research binding；
4. `PortfolioEvaluationBatch` 只可引用仍能验证为有效的 source claim 与各自 live Context。它固定 source request/claim receipt/context 的规范排序、account、decision window、动态 evidence 与 `batch_hash`；PortfolioTarget、RiskDecision 与计划结果必须保留对 batch 和逐 source outcome 的可审计引用；
5. RiskDecision、OrderIntent、aggregate `ExecutionPlan`（含 child OrderPlan）、ProtectionPlan、attempt 与审计事件都引用相同 live `context_id + context_hash + subject_binding_hash`，以及适用的 batch/source mapping；Research Evidence 引用相同的 Research binding identity；
6. Policy 引用撤销、最新 Kill Switch 命中、当前 capability 不满足、claim/permit/运行时有效期失效时对**新风险增加** fail-closed；Dispatcher 可以拒绝提交，但不能静默替换 Core 内的政策或重算另一版本计划。已形成的 `SafetyObligation` 仍只能沿原 request/context 执行 Query、Reconciliation、Cancel、Protect、Reduce、Close 等受限安全动作，绝不由此获得新开仓权限。

跨 Domain 不需要全局事务：各 Policy Snapshot 已不可变且先 Published，Execution 或 Research 只在各自 owner transaction 中原子发布对这些稳定引用的 binding。引用校验失败时 binding 不可见。

### 5. 动态 Evidence 与配置分离

以下对象在每次决策时变化，必须作为输入证据记录：

- confirmed MarketSnapshot、sequence、evidence cutoff；
- AccountSnapshot、余额/持仓/保证金与 freshness；
- InstrumentRulesSnapshot、精度、最小数量和交易能力；
- `DecisionTime`、Clock identity 与 Market event/source identity；
- credential/instrument capability verification reference（仅验证结果与 revision，绝不包含 credential material/capability）。

时间、TTL 与 hash 的职责固定：

| 项 | 来源 | 可用于 | 是否进入 hash |
| --- | --- | --- | --- |
| `DecisionTime` | 注入 Clock + 已确认 Market 事件时间 | Strategy/Portfolio/Risk/Execution planning、评估状态、Research replay | 进入 `decision_evidence_hash`，不进入 `context_hash` |
| `WallClock` | 真实运行时墙钟 | Web claim、lease、permit、stale、调度、`prepared_at_wall_clock` | 不进入 `context_hash` 或 parity hash |
| 相对 TTL 规则 | 已发布 Policy Snapshot | 最大信号年龄、计划时效、保护窗口的业务规则 | 经 Snapshot hash 间接进入 `context_hash` |
| 绝对 `valid_until_wall_clock` | `prepared_at_wall_clock +` 已发布 TTL 规则 | 当前 live binding 的运行时 liveness/final gate | 不进入 `context_hash` 或 `subject_binding_hash` |

`decision_evidence_hash` 规范哈希 Market/Account/Instrument evidence 与 `DecisionTime`；它与 `context_hash`、`subject_binding_hash` 分开保存。WallClock 记录可以用于审计和恢复时效判断，但不能让相同业务输入因为机器时间不同而产生 parity 差异。

`RiskDecision` identity 至少覆盖：

```text
context_hash
+ subject_binding_hash
+ PortfolioEvaluationBatchHash + source request mapping
+ PortfolioTargetHash
+ decision_evidence_hash
+ risk evaluation generation
```

同一 Context 在不同 AccountSnapshot/DecisionTime 下产生不同 RiskDecision 是正确行为，不属于 parity 漂移。`MarketDataAccessCredentialRef` 不是动态 Evidence，永远不得进入 Context、subject binding 或任何上述 hash。

### 6. ResearchRunSpec

Research 拥有不可变 `ResearchRunSpec`：

```text
research_run_id
DatasetManifestRef
EvaluationManifestRef
StrategyRuntimeSnapshotRef
PortfolioPolicySnapshotRef
RiskPolicySnapshotRef
ExecutionPlanningPolicySnapshotRef
SimulationProfileRef
simulated account initial state
ResearchExecutionArtifactRef
ClockSpec
RngSpec
SchedulerSpec
canonical spec hash
```

`DatasetManifestRef` 必须满足 ADR-0009 的 point-in-time 合同：固定 Market stream/universe 的首次可见时间与 revision、历史成员有效期、上市/退市、纳入算法、缺口/修订政策、当时有效 InstrumentRules 以及需要的 funding/index/mark 工件。`EvaluationManifestRef` 必须在读取目标 OOS 结果前固定 folds、purge/embargo、参数空间、优化器、trial 预算、选择规则、holdout 重用和集中度门禁。

`ResearchExecutionArtifactRef` 至少固定 Git revision、被评估 Strategy candidate artifact、quant-lab/quant-backtest/analytics 工件 digest、Release Unit Manifest、Cargo.lock、toolchain、target/profile/features 与依赖图 hash。`RngSpec` 固定 PRNG algorithm/version、master seed 和 substream 分区；`SchedulerSpec` 固定同时间事件 tie-breaker、partition ordering、并行 reduction 顺序和版本。只写一个裸 `Seed` 或“当前 binary”不能生成可重放 RunSpec。

Research 对每个模拟账户/场景以同一个 `DecisionContextCoreV1` builder 和 Domain API 构造 `ResearchDecisionContextSnapshot { ResearchScenarioRef }`，不创建或要求 Web `ExecutionRequest`、`strategy_signal_id`、combo、用户 credential、风险 profile 或真实执行账户字段。验证某个 live 配置时，RunSpec 必须引用与目标 live Context 相同的四个 Policy Snapshot hash 与可公开的动态 Evidence；若需要关联历史 live 请求，只能记录一个非授权的 Evidence link。一般策略研究必须明确声明 reference Portfolio/Risk/Execution policy，不能把结果冒充所有用户配置；Research 只生成并持久化 `ExecutionPlanningValue`、其 child `OrderPlan` 与 `ProtectionPlanningValue` 作为 Evidence，并将保护规划落实为 Research-owned `SimulationProtectionState`，不创建 live `OrderIntent`、`ExecutionPlan` 或 `ProtectionPlan` OMS Aggregate。只有 PaperEvent 的 simulated OMS 或 live Execution 可以从相同 planning value 初始化这些 Aggregate。

`SimulationProfile` 不进入 `ExecutionDecisionContextSnapshot`，也不得作为 Strategy、Portfolio、Risk 或 Execution planning API 的隐藏输入。其不变量限定为**相同单步输入**：当 Policy、Context Core、Market/Account/Instrument Evidence、EvaluationState before 与 DecisionTime 完全相同时，改变 SimulationProfile 不得改变该单步业务输出。

完整多事件模拟中，SimulationProfile 改变 Fill、费用、funding、latency 或 candle path 后会改变 SimulationLedger，并可能从下一时点开始改变 AccountSnapshot、RiskDecision 与 planning output。这是 `scenario divergence`，不是业务 parity 漂移。报告必须在首次 Ledger/Evidence hash 分歧处停止 exact parity，记录 event identity、差异原因和两侧状态 hash；只有动态 Evidence 仍相同的前缀继续要求逐层 exact parity。

### 6.1 Evidence 完整性、评价与 Promotion Receipt

Research 的状态必须分开：

```text
BacktestRun.status
EvidenceManifest.artifact_status
EvaluationGateResult.status
```

`EvidenceManifest.artifact_status = Completed` 只表示对象引用完整且原子可见；它可以对应失败、亏损、过拟合或样本不足的结果。只有按 RunSpec 引用的预先冻结 `EvaluationManifest` 计算出的 `EvaluationGateResult.status = eligible`，才可进入 Strategy promote 候选。

Strategy promote 必须创建不可变 `PromotionReceipt`，至少固定：

- Completed Evidence 与 eligible EvaluationGateResult identity/hash；
- 被 ResearchExecutionArtifactRef 评估的 candidate artifact/source tree hash；
- 目标 released Definition/Artifact/RuntimeSnapshot identity 与构建 digest；
- candidate 与 released 构建使用同一业务 symbol、同一快照/输入的逐层 parity Evidence；
- 批准人、批准时间和允许的 deployment channel。

重新构建后的 released artifact 没有上述等价链时不得激活；PromotionReceipt 只引用 Research 事实，不修改或接管 Research 表。

### 7. Parity 的精确定义

只有以下条件全部相同时才能称为 exact business parity：

- 四个 Domain Policy Snapshot 的 id/version/hash；
- `DecisionContextCoreV1` schema/`context_hash`；
- Market、Account、Instrument evidence 与 `decision_evidence_hash`；
- EvaluationState before；
- Clock/DecisionTime；
- 相同业务 symbol/API。

如果要证明“同一 live 请求”的端到端 parity，额外要求相同 `ExecutionRequestRef` 与 `subject_binding_hash`；ResearchScenario 与 live request 之间只能证明相同政策/业务决策 parity，不能因为 subject 不同而伪造用户执行身份。

逐层比较：

```text
StrategyEvaluationState after
StrategySignal / ExitIntent
PortfolioTarget
RiskDecision
ExecutionPlanningValue / child OrderPlan
ProtectionPlanningValue
decision trace
```

如果 Policy Snapshot、`context_hash`、`decision_evidence_hash` 或 DecisionTime 不同，只能称为 scenario comparison，不得标记 parity failure/success。完整模拟在首次 SimulationLedger/动态 Evidence 分歧之后同样进入 scenario divergence。WallClock、claim/lease 与绝对 TTL 记录不参与业务 parity 比较。live/Paper fixture 另验证相同 planning value 无损初始化的 `OrderIntent`、`ExecutionPlan`、`ProtectionPlan` 与其状态迁移；这不是 Research 创建 OMS Aggregate 的理由。

`RecoveryHarness` 不是第三种 Context 或 Aggregate 创建者。它只是 CI-only ephemeral integration-test artifact，在 disposable adapters 上驱动 live Execution intake/recovery 路径；不进入 deployable Release Unit、生产镜像或部署图，也不读取生产环境、Secret、数据库或真实交易所。

Fill、费用和 PnL 只有在相同 SimulationProfile 下要求确定性重放，不与真实交易所结果做 exact parity。

### 8. Canonical Serialization

所有 Snapshot、DecisionContextCore、binding 与 RunSpec 必须定义：

- 显式 `schema_version`；
- 字段单位与 Decimal scale；
- Map/Set 规范排序；
- 默认值展开规则；
- `None`、空集合和零值的规范表达；
- 时间戳精度与时区；
- `context_hash` 不包含 subject、数据库自增 ID、WallClock、绝对 TTL、claim/lease、日志时间或进程随机值；
- `subject_binding_hash` 只在相应 live/research binding 内规范包含 `context_hash + DecisionSubjectRef` 与稳定来源引用；
- `decision_evidence_hash` 只规范包含动态 evidence 与 DecisionTime；
- SHA-256 或批准的内容 hash；
- golden serialization snapshot 与 N/N-1 解析测试。

ResearchRunSpec 的 canonical hash 还必须覆盖 Dataset/Evaluation/ExecutionArtifact/RNG/Scheduler ref；并行执行不得以未固定的线程完成顺序、HashMap 遍历顺序或平台随机源影响事件顺序。bitwise replay 仅对相同执行工件、target 与数值后端成立；跨平台 Analytics 使用 EvaluationManifest 预先声明的数值容差。

价格、数量、费用、保证金和订单参数不得使用未量化的 `f64` 参与 Context/Plan hash。

### 9. 更新、撤销与回滚

- Policy 变化发布新 Snapshot；
- StrategyRelease 的 lifecycle 归 Strategy；Control 的 Active/ActivationPointer 是独立可变控制状态，每个 activation scope 维护独立 `activation_generation`，Snapshot 本身不可变；
- rollback 只能让 Control 指向历史已发布 Snapshot，或由 Strategy 发布新 Snapshot/由 Execution 创建新的 live binding，均不覆盖历史对象；
- Control 的 KillSwitchSnapshot 使用独立的 `kill_switch_catalog_generation + scope_generation`，Dispatcher 在最终门禁重新校验命中 scope；它不能与 release/activation generation 混用；
- 已形成的 Order/Attempt 保留原 Context identity、subject binding 与计划；后续恢复不得读取“当前最新配置”改写原计划；
- Risk 降低命令可以使用新的 RiskAction generation，但必须显式引用原 Context 和新动作政策，不能伪装为原决策重放。

## 结果

### 正面影响

- Strategy 不再拥有用户账户风险；
- Web 存储、Core policy 语义和 Execution 绑定职责清晰；
- backtest/live 可以证明使用相同完整业务上下文；
- 动态账户/行情变化不会被误判为配置漂移；
- rollback、恢复和审计可以重放当时实际使用的全部版本。

### 代价

- 新增 Decision Context 与 owner snapshot 发布流程；
- Web RiskProfile 到 Core RiskPolicySnapshot 需要版本化 Contract；
- 每次执行需要持久化更多引用和 hash；
- legacy JSON 默认值和环境变量必须显式迁移。

## 被否决的方案

### StrategyRuntimeSnapshot 包含所有用户风险配置

混淆策略发布级和账户/combo 级作用域，并把 Risk owner 转移给 Strategy。

### 每次执行重新解析 Web JSON

无法保证默认值、单位、版本、hash 和历史重放一致。

### 只记录“当前配置版本”

恢复时版本可能已变化，无法证明原订单使用的实际内容。

### 把 MarketSnapshot 和 AccountSnapshot 放入配置

它们是动态事实输入，不是不可变政策；混入配置会导致无意义的版本爆炸。

## 验证

- 相同 Web risk profile version 幂等得到相同 RiskPolicySnapshot hash；
- StrategyRuntimeSnapshot 不含 account、user、credential 或 risk profile 字段；
- 不同用户可以在同一 StrategyRuntimeSnapshot 下绑定不同 RiskPolicySnapshot；
- Context Core 只引用 Published 且兼容的 Policy Snapshot；live binding 只能来自有效 Web claim，Research binding 只能来自 ResearchScenario；
- 多个已 claim live binding 只能以规范 source ordering 形成同账户 `PortfolioEvaluationBatch`；batch 不改变任一 request 的 subject、claim 或 credential 语义；
- 任一语义引用 hash、默认值或单位变化都会改变 `context_hash`；subject 变化只改变 `subject_binding_hash`，动态 Evidence/DecisionTime 变化只改变 `decision_evidence_hash`；
- WallClock、绝对 TTL、claim/lease 与日志时间不会改变上述业务语义 hash；
- live binding 的 Policy 撤销、claim/permit 失效、命中 kill switch 或当前能力不足后禁止新开仓；
- 退订、会员/claim 到期或 credential 撤销不会把已形成的安全收敛责任伪造成新 request：前者保留受限 `SafetyObligation`，后者无可用安全 capability 时留下 `SafetyBlocked`/人工处置证据；
- RiskDecision、live OrderIntent、ExecutionPlan（含 child snapshot）、ProtectionPlan、Attempt 和恢复证据均可追溯到同一 `ExecutionDecisionContextSnapshot`；Research Evidence 可追溯到 `ResearchDecisionContextSnapshot`，并只保存 `ExecutionPlanningValue`/child `OrderPlan`/`ProtectionPlanningValue`，不伪造 Web 请求或 OMS Aggregate；
- exact parity fixture 同时证明 Policy hash、Context hash、动态 Evidence/DecisionTime 和逐层输出一致；
- 相同单步输入下，SimulationProfile 变化不改变 Signal、Target、RiskDecision、`ExecutionPlanningValue`、child `OrderPlan` 或 `ProtectionPlanningValue`；由相同 planning value 初始化的 live Aggregate 语义 hash 也必须保持不变；
- 完整运行在首次 Fill/Ledger/Evidence 分歧后标记 scenario divergence，不错误要求后续 RiskDecision/Planning 保持一致；
- DatasetManifest 能重建 point-in-time universe、数据 revision、InstrumentRules 与可见时间边界，EvaluationManifest 能证明 OOS/walk-forward 规则先于结果冻结；
- 相同 RunSpec 必须解析到相同 ResearchExecutionArtifact、PRNG substream 和 scheduler ordering；
- Completed Evidence 不自动成为 eligible；Strategy Release 必须存在可追溯 candidate -> released 的 PromotionReceipt；
- RecoveryHarness 在生产 Release Unit/镜像/部署图中不可达，且不能读取生产环境、Secret、数据库或真实交易所。
