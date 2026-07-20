# ADR-0009：Research Domain、纯 Backtest Kernel 与分级模拟

- 状态：已接受
- 首次接受：2026-07-20
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
- `SimulationProfile`；
- `ResearchEvidence`、`EvidenceManifest` 与发布状态机；
- start、execute、checkpoint、complete、fail、publish 等用例；
- Experiment、Evidence、Evidence Object Storage 等业务 Port。

Market 仍拥有历史行情事实。Research 只拥有“本次实验选择了哪些 point-in-time 数据及其指纹”，不复制原始行情 owner。

Strategy 仍拥有 `StrategyDefinition`、可执行技术 Artifact、Release 与 RuntimeSnapshot。ResearchEvidence 归 Research；Strategy promote/rollback 只引用已完成 Evidence identity，不接管实验表。

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

生产 Market、Strategy、Portfolio、Account、Risk、Execution、Reconciliation 不依赖 Research。Research 只能使用它们的稳定公开 API，不能访问私有 module、Repository Port、数据库 Row 或生产 Adapter。

### 4. 三种 SimulationProfile

#### ResearchBar

- 用于现有 Vegas/NWE 参数回测和多币种组合研究；
- 精确复用 Strategy evaluator、Portfolio policy、Risk policy 和 OrderPlan；
- 使用声明清楚的 candle/tick fill、fee、slippage、funding 模型；
- 不运行生产 lease、outbox、网络 Unknown 或 Reconciliation；
- 目标是业务决策 parity 与研究吞吐，不宣称 OMS 恢复 parity。

#### PaperEvent

- 使用 Simulated Exchange 产生 Ack、PartialFill、Fill、Reject、Cancel 和 Protection Event；
- 复用 Execution 的纯 Order aggregate/state transition；
- 可注入延迟、流动性与部分成交；
- 不调用真实交易所，也不写生产 Order/Fill/Account 表。

#### RecoveryHarness

- 专门验证 lease、inbox/outbox、Unknown、重复、乱序、崩溃、保护缺失和对账恢复；
- 可以使用 disposable Postgres 与 fault-injection Adapter；
- 不参与大规模参数搜索，也不作为策略收益证据。

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
- StrategyEvaluationState 是 evaluator 内部状态，不是交易 pipeline 的独立下游业务阶段。

### 8. Evidence 原子可见发布

Postgres 与对象存储之间不宣称全局原子。发布顺序为：

1. 以内容哈希幂等写入不可变大对象；
2. 单一 Research owner 数据库事务写入 EvidenceManifest、指标、引用、幂等记录与 Completed 状态；
3. 只有 Completed manifest 对查询和 promote 可见；
4. 数据库事务失败产生的孤立对象由 GC 清理。

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
| OrderIntent/OrderPlan | 精确 parity | 精确 parity | 精确 identity/state |
| Fill/PnL | 由显式撮合模型决定 | 由事件模拟决定 | 非收益证据 |
| lease/outbox/Unknown/recovery | 不模拟 | 部分模拟 | 完整验证 |

最终 PnL 接近不能替代 Signal、Target、RiskDecision 和 OrderPlan 的逐层 parity。

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
- 同一 RunId、Manifest、Seed 重放结果一致；
- ResearchBar、PaperEvent、RecoveryHarness 分别满足本 ADR 的 parity 表；
- Completed Evidence 的对象引用全部存在，未完成 run 不可用于 promote；
- Backtest 不产生任何生产 Order/Fill/Account 事实。
