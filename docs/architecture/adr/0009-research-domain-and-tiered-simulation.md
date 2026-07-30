# ADR-0009：Research Domain、纯 Backtest Kernel 与分级模拟

- 状态：已接受
- 首次接受：2026-07-20
- 最近修订：2026-07-28
- 取代：[ADR-0008](0008-backtest-reuses-domain-apis.md)
- 决策者：Rust Quant Core

## 背景

现有 Vegas 回测拥有真实业务生命周期：实验请求、参数空间、数据范围、断点进度、运行状态、成交与过滤证据、绩效、失败和结果持久化。它不是一组无 owner 的纯工具。

ADR-0008 曾让 `quant/backtest` 直接调用 Strategy、Portfolio、Risk、Execution 和 Account 的公开 API。该设计虽然避免复制业务规则，却会让 Quant crate 同时承担跨域编排、模拟状态和研究证据，容易成为新的 Orchestration God Crate。

同时，K 线参数回测、Paper 交易所模拟和生产恢复演练不是同一种精度：参数回测要求快速、确定性和大批量；Paper 模拟需要订单 Ack、PartialFill、Reject、延迟和保护事件；恢复演练需要 lease、outbox、Unknown、崩溃、重放和 Reconciliation。用一个 pipeline 同时承担三者，会同时损害性能和可信度。

## 决策

### 1. 建立 Research Domain

在 `crates/domains/research` 建立独立 owner，拥有：

- `Experiment`、`BacktestRun`、`RunCheckpoint`；
- `DatasetManifest` 与 historical universe/data fingerprint；
- `EvaluationManifest`、trial ledger、OOF/OOS 与 walk-forward 评价结果；
- `SimulationProfile`；
- `ResearchEvidence`、`EvidenceManifest` 与发布状态机；
- start、execute、checkpoint、complete、fail、publish 等用例；
- Experiment、Evidence、Evidence Object Storage 等业务 Port。

Market 仍拥有历史行情事实。Research 只拥有“本次实验选择了哪些 point-in-time 数据及其指纹”，不复制、回填或修改原始行情 owner。`DatasetManifest` 必须引用 Market 已冻结、内容寻址的批量历史 stream / snapshot，而不是在回测热循环中逐 K 线远程调用 Market；至少固定：

- `market_data_source_profile_id`、exchange/product/quote、stream partition、序号与内容 hash；
- 每条事实的 event time、首次 `available_at`/ingested-at 边界、数据 revision 与研究 `as_of` cutoff；
- confirmed/missing/duplicate/outlier/correction 的处理政策及其版本；
- point-in-time universe 成员的 `effective_from/effective_to`、上市/退市状态、纳入/剔除算法与算法 hash，禁止用当前币种列表回放历史；
- 当时有效的 `InstrumentRulesSnapshot`、tick/step/min quantity、交易能力，以及策略需要的 funding/index/mark 等历史 stream；
- 时区、bar close 语义、完整性报告和所有引用对象的 hash。

Research 只能通过 Market 的只读历史 API/contract 解析并消费这些不可变工件，不能直连 Market 表、触发 K 线 backfill 或持有 `MarketDataAccessCredential`。行情、instrument 生命周期、规则或补充 stream 的补采与修订由 Market owner 产生新的不可变事实版本；Research 基于这些事实重建新的 universe selection 与 `DatasetManifest`，两类旧版本都不得原地覆盖。

Strategy 仍拥有 `StrategyDefinition`、可执行技术 Artifact、Release 与 RuntimeSnapshot。ResearchEvidence 归 Research；Evidence 的 `Completed` 只表示本次运行及其引用工件完整、原子可见，不表示评价通过或可晋级。Research 依据运行前冻结的 `EvaluationManifest` 另行发布 `EvaluationGateResult { eligible | rejected | inconclusive }`。Strategy promote/rollback 不接管实验表；promote 必须创建 Strategy-owned `PromotionReceipt`，同时引用 Completed Evidence、eligible gate result、被研究的 candidate artifact 和即将发布的 released artifact 等价证据。

`EvaluationManifest` 必须在查看目标 OOS 结果前固定 hypothesis、训练/验证/OOS 窗口、walk-forward folds、purge/embargo、参数空间、优化器算法/version/Seed/预算、候选选择规则、费用与滑点压力、最小有效事件/币种/市场状态覆盖、收益集中度与 holdout 重用计数。所有 trial（包括失败和被淘汰项）进入不可变 trial ledger；不得只保存胜出的参数。

### 2. 收窄 Quant

```text
quant/math        纯数学
quant/indicators  技术指标
quant/backtest    DeterministicClock、EventScheduler、Replay、撮合、费用、滑点、资金费
quant/analytics   对权益、成交和事件序列进行纯指标计算
```

Quant 不依赖任何业务 Domain、Adapter、数据库、环境变量或真实交易所。`quant/research` 不再作为目标目录；参数空间、walk-forward、证据门禁等有实验语义的逻辑归 Research model/policies，纯统计部分下沉 `quant/math` 或 `quant/analytics`。

### 3. Research 是终端离线编排 Domain

```text
quant-lab App
  -> Research Use Case
       -> Market API / historical stream
       -> Strategy API
       -> Portfolio API
       -> Risk API
       -> Execution planning/state-transition API
       -> quant/backtest kernel
       -> quant/analytics
       -> Research Ports
```

生产 Market、Strategy、Portfolio、Account、Risk、Execution、Reconciliation 不依赖 Research。Research 只能使用它们的稳定公开 API，不能访问私有 module、Repository Port、数据库 Row 或生产 Adapter。Research 的 subject identity 是 `ResearchScenario`（模拟账户、universe、初态及政策引用），由 `ResearchRunSpec` 冻结，并通过 Research-owned `ResearchDecisionContextSnapshot { ResearchScenarioRef }` 绑定同一 `DecisionContextCore`；它不是 Web `ExecutionRequest` 或 live `ExecutionDecisionContextSnapshot`，不含用户 `credential_reference`，也不能进入 live mutation 路径。

### 4. 三种 SimulationProfile

#### ResearchBar

- 用于现有 Vegas/NWE 参数回测和多币种组合研究；
- 精确复用 Strategy evaluator、Portfolio policy、Risk policy 和 Execution planning，只生成/比较纯 `ExecutionPlanningValue`、其确定性的 child `OrderPlan` 与 `ProtectionPlanningValue`；ResearchBar 不创建或持久化 live `OrderIntent`、`ExecutionPlan`、`ProtectionPlan` OMS Aggregate；
- 使用声明清楚的 candle/tick fill、fee、slippage、funding 模型；
- 不运行生产 lease、outbox、网络 Unknown 或 Reconciliation；
- 目标是业务决策 parity 与研究吞吐，不宣称 OMS 恢复 parity。

每个 MarketEvent、模拟 Fill、funding 或权益变化后，ResearchBar 都必须从 `SimulationLedger` 生成模拟 `AccountSnapshot`，调用同一 `Risk::EvaluateContinuous`，并按稳定 `risk_action_decision_id = subject_binding_hash + trigger_event/evidence_hash + risk_policy_snapshot_hash + action_generation` 将 `RiskAction` 交回 Execution planning：

```text
SimulationLedger / simulated AccountSnapshot
  -> Continuous Risk
  -> RiskAction(Reduce / Close / KillSwitch / Continue)
  -> Execution planning（只生成可证明 reduce-only 的 ExecutionPlanningValue/child OrderPlan/ProtectionPlanningValue）
  -> FillModel / simulated exchange
  -> SimulationLedger
```

`RiskAction` 不得直接改写 Ledger 或伪造成交。Research 的 `KillSwitch` 只能映射为 Research-owned `SimulationNewRiskBlock`/Evidence，绝不发送 Control typed request；它阻断模拟新增风险但不阻断可证明 reduce-only 的保护、减仓或平仓。ResearchBar 将 `ProtectionPlanningValue` 落为 Research-owned `SimulationProtectionState`，它只表达模拟触发状态，不是 Execution `ProtectionPlan` Aggregate。相同 `decision_time` 内的优先级固定为：已生效 `SimulationProtectionState` 的触发按 `SimulationProfile` 声明的 candle/tick 路径先结算；随后 `KillSwitch`/`Close`/`Reduce` 等安全动作优先于 Strategy 的 exit intent 和任何新开/加仓；Strategy exit 只能与同方向 reduce-only 动作净额合并，不能生成第二次平仓；只在没有更严格 RiskAction 且账户仍允许增加风险时，才处理新的 Strategy entry。该顺序及输入/输出必须记录为 typed evidence。

#### PaperEvent

- 使用 Simulated Exchange 产生 Ack、PartialFill、Fill、Reject、Cancel 和 Protection Event；
- 复用 Execution 的纯 state-transition API；如需模拟 OMS，只在 simulated store 验证由同一 `ExecutionPlanningValue` 初始化的 live aggregate 状态迁移；
- 可注入延迟、流动性与部分成交；
- 不调用真实交易所，也不写生产 Order/Fill/Account 表。

#### RecoveryHarness

- 专门验证 lease、inbox/outbox、Unknown、重复、乱序、崩溃、保护缺失和对账恢复；
- 可以使用 disposable Postgres 与 fault-injection Adapter；
- 不参与大规模参数搜索，也不作为策略收益证据。
- 是 CI-only ephemeral integration-test artifact，不属于任何可部署 Release Unit，不进入 `core-runtime`、`core-maintenance` 或 `quant-lab` 镜像；
- 只能获得测试生成的凭证/数据与临时基础设施，禁止生产 environment、生产 Secret、真实交易所 Adapter 和生产部署资格。

### 5. SimulationLedger 不是 AccountProjection

Account 只拥有真实交易所账户投影。Research 使用 `SimulationLedger` 保存模拟现金、仓位、费用、资金费、working orders 和权益，并生成 Portfolio/Risk 可消费的模拟 `AccountSnapshot` read model。

模拟事实必须携带 `BacktestRunId`/`SimulationRunId`，不得进入生产 Account/Order/Fill identity 或事实表。

### 6. 同时点决策屏障

多币种回测必须先收集同一 decision time 的全部市场事件与 StrategySignal，再统一执行 Portfolio 排序、净额、容量和 Risk。禁止按 symbol 遍历顺序逐个占用资金，否则结果会随输入排序变化。

### 7. Evaluation State 作用域

```text
StrategyEvaluationStateKey
  = EvaluationScopeId
  + StrategyRuntimeSnapshotId
  + MarketStreamPartition
```

- backtest 的 `EvaluationScopeId` 是 `BacktestRunId`；
- live 的作用域是 release/deployment generation；
- 并行实验不得共享可变 evaluator state；
- live `StrategyEvaluationState` 只经 Redis Adapter 保存，不以 Postgres 作为同源或回退事实库；backtest 仅使用以 `EvaluationScopeId` 隔离的内存 Adapter；
- StrategyEvaluationState 是 evaluator 内部状态，不是交易 pipeline 的独立下游业务阶段。

### 8. Evidence 原子可见发布

Postgres 与对象存储之间不宣称全局原子。发布顺序为：

1. 以内容哈希幂等写入不可变大对象；
2. 单一 Research owner 数据库事务写入 EvidenceManifest、指标、引用、幂等记录与 `artifact_status = Completed`；
3. 只有 Completed manifest 对查询可见，但它仍可对应失败、亏损、过拟合或样本不足的实验；
4. Research 按运行前冻结的 EvaluationManifest 发布独立 `EvaluationGateResult`，只有 `eligible` 才可被 Strategy promotion 引用；
5. Strategy 创建 `PromotionReceipt`，固定 candidate Evidence/gate、candidate artifact、released artifact、构建身份与跨构建 parity 证据；Receipt 不得反写 Research 事实；
6. 数据库事务失败产生的孤立对象由 GC 清理。

运行状态、工件完整性、评价结论与策略晋级是四个不同状态机：

```text
BacktestRun.status
EvidenceManifest.artifact_status
EvaluationGateResult.status
StrategyRelease / PromotionReceipt.status
```

不得用一个 `Completed` 同时表达“跑完”“证据完整”“达标”和“已晋级”。

## 依赖方向

```text
Production Domains -> quant/math + quant/indicators

Research Domain
  -> Production Domain stable APIs
  -> quant/backtest + quant/analytics + quant/math

quant/* -> no business Domain

quant-lab App
  -> Research use cases + adapters + platform
```

该方向无循环：Research 是只被 quant-lab 调用的终端离线 Domain，生产交易路径不依赖它。

## Parity 边界

| 对象 | ResearchBar | PaperEvent | RecoveryHarness |
| --- | --- | --- | --- |
| StrategySignal | 必须精确 parity | 必须精确 parity | 非重点 |
| PortfolioTarget | 相同快照下精确 parity | 精确 parity | 非重点 |
| RiskDecision | 相同政策下精确 parity | 精确 parity | 故障门禁 parity |
| ExecutionPlanningValue / child OrderPlan | 精确 parity | 精确 parity | 非 OMS 恢复重点 |
| live OrderIntent / ExecutionPlan / ProtectionPlan aggregate | 不创建 | simulated store 中验证初始化/状态迁移 | 精确 identity/state |
| Continuous Risk / RiskAction | 模拟账户快照下精确 policy/planning parity | 精确 parity | 故障门禁与恢复 parity |
| Fill/PnL | 由显式撮合模型决定 | 由事件模拟决定 | 非收益证据 |
| lease/outbox/Unknown/recovery | 不模拟 | 部分模拟 | 完整验证 |

RecoveryHarness 不定义或直接创建第四套 OMS Aggregate；它只在 CI ephemeral adapters 上驱动 live Execution intake/recovery 代码路径，并观察该路径创建的 live-shaped Aggregate 与恢复证据。Aggregate 的合法创建者仍只有 PaperEvent simulated OMS 或 live Execution。

最终 PnL 接近不能替代 Signal、Target、RiskDecision、`ExecutionPlanningValue` 及其 child OrderPlan 的逐层 parity。live intake/PaperEvent 另须证明相同 planning value 无损初始化 `OrderIntent + ExecutionPlan + ProtectionPlan` aggregate。

`ExecutionPlanningValue` 是纯、规范可序列化的 planning value，含有序 child `OrderPlan` 与 `ProtectionPlanningValue`；`OrderPlan` 不独立替代 aggregate、身份或恢复语义。live `ExecutionPlan` 才是 Execution owner 的持久 OMS aggregate：它保存 planning value/hash、parent OrderIntent 与 child snapshot，并由 live intake 在同一事务内初始化。ResearchBar 只比较/持久化 planning value 到 Research Evidence；PaperEvent 可在 simulated store 验证 live aggregate 的初始化与状态迁移，但两者都不写生产 Execution 事实。

### 单一业务实现与配置快照

本 ADR 中“复用”具有以下强制含义：

1. backtest、paper、shadow、canary、live 调用相同的 Strategy evaluator/exit policy、Portfolio policy、Risk policy/final-stop constraint 和产生 `ExecutionPlanningValue` 的 Execution planning Rust API，不保留按运行模式复制的实现；live `ExecutionPlan` aggregate 的持久化/恢复只在 live 或 PaperEvent simulated store 验证；
2. 四个业务 owner 分别发布强类型 `StrategyRuntimeSnapshot`、`PortfolioPolicySnapshot`、`RiskPolicySnapshot` 和 `ExecutionPlanningPolicySnapshot`；Research 用不可变 `ResearchRunSpec` 引用它们，并为每个 `ResearchScenarioRef` 创建 `ResearchDecisionContextSnapshot`；生产 Execution 仅为 Web 已创建的请求使用 `ExecutionDecisionContextSnapshot` 绑定实际引用，Research 不创建或伪造后者；
3. 同一 JSON 分别反序列化成 `BacktestRiskConfig` 与 `LiveRiskConfig` 不构成配置复用；语义相同的字段只能有一个 owner 类型，运行模式差异只能在 Adapter 或 `SimulationProfile`；
4. Strategy 的候选失效价、退出意图/候选止盈计划是 Signal evidence，Risk 选择不可放宽的最终止损、风险边界和批准数量，Execution 合并 Strategy exit intent 与 RiskDecision 产生 `ExecutionPlanningValue`（及其 child `OrderPlan`）/`ProtectionPlanningValue`；仅 live intake 或 PaperEvent simulated OMS 可从同 hash 的 planning value 持久化 `OrderIntent`/`ExecutionPlan`/`ProtectionPlan` Aggregate；Research 不得在 `deal_signal`、`BacktestRisk` 或模拟 Service 中重新完成这些决策；
5. fee、slippage、funding、latency 和 candle 内路径属于 `SimulationProfile`，不得混入 Risk policy；allocation 归 Portfolio，账户风险比例与 leverage/margin 限制归 Risk，具体交易所 leverage/margin mode 由 Execution 实现；
6. exact parity fixture 固定四个 Policy Snapshot hash、Market/Account/Instrument evidence、EvaluationState before 和 Clock。用户执行路径额外固定 `ExecutionDecisionContextSnapshot` hash；Research 路径固定 `ResearchDecisionContextSnapshot`/`ResearchRunSpec` hash，二者不能互相替代。Research fixture 逐层比较规范序列化的 State after、Signal、Target、RiskDecision、`ExecutionPlanningValue`、其 child OrderPlan、`ProtectionPlanningValue` 与 decision trace；live/Paper fixture 额外比较由同值初始化的 `OrderIntent`、`ExecutionPlan`、`ProtectionPlan` 与状态迁移；identity 不同只能做 scenario comparison。

SimulationProfile 的不变量只适用于**相同单步业务输入**：给定相同 Policy Snapshot、Research/Execution Context Core、Market/Account/Instrument Evidence、EvaluationState before 与 DecisionTime，SimulationProfile 不得作为 Strategy、Portfolio、Risk 或 Execution planning API 的隐藏输入，因此该单步的 Signal、Target、RiskDecision、`ExecutionPlanningValue`、child OrderPlan 与 `ProtectionPlanningValue` 必须一致。

完整多事件运行中，撮合、fee、slippage、funding 或 candle path 一旦产生不同 Fill/费用/SimulationLedger，下一时点的 AccountSnapshot 或持仓证据就已不同；从该**首次模拟状态分歧**起，后续输出属于明确的 scenario divergence，不再要求业务输出相同，也不得标为 parity 成功或失败。报告必须记录首次状态分歧的 event identity、两个 Ledger/Evidence hash 与因果来源。只有在动态 Evidence 仍相同的前缀内，才继续要求逐层 exact parity。

`ResearchRunSpec` 除 DatasetManifest、四个 Policy Snapshot、SimulationProfile、模拟账户初态和 Clock 外，还必须固定：

- `ResearchExecutionArtifactRef`：Git revision、Strategy candidate artifact、quant-lab/quant-backtest/analytics 构建工件 digest、Release Unit Manifest/Cargo.lock/toolchain/target/profile/features hash；
- `RngSpec`：PRNG algorithm/version、master seed、按 run/scenario/instrument 分区的 substream 规则；
- `SchedulerSpec`：同时间事件 tie-breaker、partition ordering、并行 reduction 顺序和版本；
- 数值/序列化 schema 与允许的平台数值容差。

“字节一致重放”只对相同 ResearchExecutionArtifact、target、数值后端和 determinism spec 成立；跨平台 Analytics 必须使用预先声明的数值容差，不能伪装成 bitwise parity。

完整字段、canonical hash 和 Web risk profile 解析边界见 [ADR-0011](0011-layered-runtime-snapshots-and-decision-context.md)。

## 结果

### 正面影响

- Experiment、Checkpoint 和 Evidence 有唯一 owner；
- Quant 保持纯净、快速且不会膨胀成跨域总调度；
- 快速参数研究不会被生产恢复协议拖慢；
- Paper 与 Recovery 对各自精度负责，不再夸大 K 线回测能力；
- 多币种组合不会因 symbol 遍历顺序产生资金偏差；
- Evidence 发布具有可实现的原子可见语义。

### 代价

- 新增一个 Research Domain crate；
- Research 用例依赖多个稳定 Domain API，需要严格保持编排薄、业务判断留在 owner；
- 需要维护三个 SimulationProfile 及其能力声明；
- 现有 BacktestRunner、BacktestContext、deal_signal 与保存服务必须分片迁移。

## 被否决的方案

### `quant/backtest` 直接编排所有 Domain

会形成无 owner 的跨域中心，已由本 ADR 取代。

### Strategy 拥有所有 ResearchEvidence

Strategy 负责可执行定义与发布，Experiment/Run/Checkpoint/Evidence 有独立生命周期，应归 Research。

### 所有回测运行完整生产 OMS

参数搜索成本过高，且 candle 数据无法证明网络、lease 和恢复语义。

### ResearchBar 自建 Strategy/Risk/Order 规则

会产生与 paper/live 漂移的第二套业务系统，禁止采用。

## 验证

- Cargo/arch-check 阻止 `quant/*` 依赖业务 Domain；
- 生产 Domain 不依赖 Research；
- Research 只访问稳定 Domain API；
- 多币种 fixture 改变 symbol 输入顺序后结果字节一致；
- 相同 Dataset/Evaluation Manifest、ResearchExecutionArtifactRef、PRNG substream、SchedulerSpec、target 与 RunSpec 的运行可重放；跨平台差异只使用预先声明的数值容差；
- ResearchBar、PaperEvent、RecoveryHarness 分别满足本 ADR 的 parity 表；
- parity 报告证明每个精确层调用相同业务 symbol、相同版本化快照并产生相同规范 `ExecutionPlanningValue` 输出；不存在按运行模式复制的 Risk、止盈止损或 child OrderPlan。live/Paper simulated OMS 另证明该值无损初始化 `OrderIntent`、`ExecutionPlan`、`ProtectionPlan` Aggregate；
- DatasetManifest 能重建 point-in-time universe、数据 revision、InstrumentRules 与首次可见时间，EvaluationManifest 能证明 OOS/walk-forward/purge/embargo 在结果前冻结；
- 相同单步输入下 SimulationProfile 不改变业务 planning 输出；首次 Fill/Ledger/Evidence 分歧后明确记录 scenario divergence；
- Completed Evidence 的对象引用全部存在且状态不自动变为 eligible；Strategy promote 必须有引用 Completed Evidence、eligible gate 和 candidate/released artifact 等价链的 PromotionReceipt；
- RecoveryHarness 只作为 CI-only ephemeral artifact 运行，不进入任何 deployable Release Unit，也不读取生产环境、Secret 或真实交易所；
- Backtest 不产生任何生产 Order/Fill/Account 事实。
