# Vegas 与现有回测主链迁移实战

- 状态：迁移设计，尚未实施
- 日期：2026-07-23
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 核心决策：[ADR-0009：Research Domain、纯 Backtest Kernel 与分级模拟](adr/0009-research-domain-and-tiered-simulation.md)
- 运行配置：[ADR-0011：分层运行快照与完整决策上下文](adr/0011-layered-runtime-snapshots-and-decision-context.md)
- 工件隔离：[ADR-0010：基于依赖图的构建影响与生产工件隔离](adr/0010-build-impact-and-artifact-isolation.md)
- 迁移总计划：[Rust Quant 架构迁移计划](migration-plan.md)

## 1. 结论先行

现有 Vegas 回测可以迁入目标架构，但不能按一条线性“Strategy -> Portfolio -> Risk -> 完整 OMS -> AccountProjection”直接搬运。最终设计采用以下边界：

1. `domains/research` 拥有 Experiment、BacktestRun、DatasetManifest、EvaluationManifest、Checkpoint、SimulationProfile、trial ledger 和 ResearchEvidence；
2. `quant/backtest` 只保留确定性时钟、事件调度、Replay、撮合、费用、滑点和资金费，不直接调用业务 Domain；
3. Research use case 通过稳定 API 编排 Strategy、Portfolio、Risk 和必要的 Execution 纯能力；
4. StrategyEvaluationState 是 evaluator 内部状态，并带 BacktestRun/部署作用域；
5. 多币种同一决策时间先收集所有 Signal，再统一排序、净额和资金分配；
6. 模拟账户使用 Research `SimulationLedger`，不冒充生产 AccountProjection；
7. `ResearchBar` 只产生 `ExecutionPlanningValue + child OrderPlan + ProtectionPlanningValue`，并把保护规划落为 Research-owned `SimulationProtectionState`；只有 `PaperEvent` simulated OMS 或 live Execution 可以创建 `OrderIntent`、`ExecutionPlan` 和 `ProtectionPlan` Aggregate；
8. `ResearchBar`、`PaperEvent`、CI-only `RecoveryHarness` 分别验证研究经济行为、订单事件和故障恢复；
9. ResearchEvidence 使用内容寻址对象加 Completed Manifest 实现原子可见发布；`Completed` 只表示工件完整，不表示评价 eligible 或已晋级；
10. backtest、paper、shadow 和 live 使用同一强类型 Policy Snapshot 与同一业务 symbol；Research 通过 `ResearchDecisionContextSnapshot` 绑定场景，不伪造 live `ExecutionDecisionContextSnapshot`；
11. `DatasetManifest`、`EvaluationManifest`、`ResearchExecutionArtifactRef`、PRNG 和 Scheduler contract 必须在读取目标结果前冻结，Strategy 晋级必须有可追溯的 `PromotionReceipt`。

只创建新目录并复制 `BacktestRunner`、`BacktestContext` 和 `deal_signal`，仍会产生新的大编排模块。本文要求按 owner 和验证精度分片迁移。

## 2. 当前真实回测主链

```text
internal HTTP / bootstrap
  -> BacktestRunner
  -> BacktestExecutor
  -> CandleService + continuity validation
  -> VegasBacktestAdapter
  -> run_back_test
  -> SignalStage
  -> FilterStage
  -> PositionStage -> deal_signal
  -> BacktestService
  -> SqlxBacktestRepository / SqlxAuditRepository
```

| 当前职责 | 当前代码位置 | 观察 |
| --- | --- | --- |
| HTTP 请求映射 | `crates/rust-quant-cli/src/app/internal_server.rs` | 已有真实入口，但直接进入 legacy orchestration |
| 运行与模式选择 | `crates/orchestration/src/backtest/runner.rs` | Vegas/NWE、随机/指定、进度、环境变量和具体数据库装配混合 |
| 数据与批次 | `crates/orchestration/src/backtest/executor.rs` | 已有 confirmed candle、连续性校验、Semaphore 和参数批次 |
| Vegas 适配 | `crates/strategies/src/implementations/vegas_backtest.rs` | 正确复用 `get_trade_signal`，但仍把账户风险配置传入 Strategy |
| 回测循环 | `crates/strategies/src/framework/backtest/pipeline/` | 固定阶段顺序可保留，万能 Context 跨越多个 owner |
| 开平仓 | `crates/strategies/src/framework/backtest/signal.rs` 的 `deal_signal` | 资金、仓位、止损、反转、挂单和模拟成交耦合 |
| 结果保存 | `crates/services/src/strategy/backtest_service.rs` | 多表分步写入、读取环境变量、硬编码初始资金 |
| SQL | `crates/infrastructure/src/repositories/{backtest_repository,audit_repository}.rs` | SQL 已集中，但 runtime DDL 和多次独立写入仍是 legacy |

## 3. 迁移时保留什么

- 只使用 confirmed candle；
- 运行前校验历史 K 线连续性；
- Vegas 回测与 live 使用同一个 Strategy evaluator；
- 每个市场事件按确定顺序推进；
- 参数批次有明确并发上限；
- 保存成交、过滤信号、动态配置和决策证据；
- 支持指定配置、随机参数和断点进度；
- 失败不会被包装成整批成功；
- 保留 `initial_stop_price`、`initial_risk_amount`、`net_profit_r` 等风险证据。

这些是可复用资产，不能因为目录重构重新发明。

## 4. 当前必须拆开的债务

### 4.1 Strategy 与账户风险混合

`IndicatorStrategyBacktest::generate_signal` 和 Vegas `get_trade_signal` 接收 `BasicRiskStrategyConfig`，导致市场判断依赖账户政策。

目标规则：Strategy 只输出方向、置信度、证据、候选入场价和候选失效价；不读取账户余额、用户风险配置，不输出最终数量或最终 RiskDecision。

### 4.2 `position_leverage` 不是交易所杠杆

当前 `position_leverage=0.58/0.6` 实际乘以可用资金，表达资本占用比例。

```text
历史 position_leverage
  -> Portfolio allocation_ratio

真实 exchange leverage / margin mode
  -> Risk approval
  -> Execution realization
```

旧字段只允许在迁移 Adapter 映射，目标模型不得沿用歧义名称。

### 4.3 Vegas 状态身份和窗口漂移

当前代码存在：

- Vegas 默认 `min_k_line_num = 7000`；
- live executor 本地上限 `4000`；
- 实际缓存只保留 `300`；
- 缓存 key 只有 `inst_id + period + strategy_type`；
- 增量连续性检查只显式应用于 `VegasUniversal4h`。

目标状态身份：

```text
StrategyEvaluationStateKey {
    evaluation_scope_id,          // BacktestRunId 或 release/deployment generation
    runtime_snapshot_id,
    market_stream_partition,      // instrument + timeframe + source/version
}
```

并行实验不共享可变状态。EvaluationState 是 StrategyEvaluator 内部输入输出，不是 Signal 之后的新流水线 Stage。

### 4.4 万能 Context 与 `deal_signal`

当前 Context 同时保存行情、Signal、风险配置、持仓、shadow 和 audit；`deal_signal` 又处理开仓、平仓、挂单、风险、止盈止损和模拟成交。

目标按 typed output 拆分：

```text
StrategyEvaluation
PortfolioTarget
RiskDecision
ExecutionPlanningValue / child OrderPlan / ProtectionPlanningValue
SimulationProtectionState
SimulationFill
SimulationLedgerSnapshot
```

不再由一个可变结构允许所有 Stage 随意修改全部状态。

### 4.5 逐 symbol 分配会产生顺序偏差

全市场回测若按照 BTC、ETH、SOL 的遍历顺序逐个分配资金，排在前面的 symbol 会先占容量；只要换一下输入顺序，结果就可能改变。

目标必须有 decision-time barrier：同一时间的全部 MarketEvent 和 StrategySignal 收集完毕后，Portfolio 一次完成排序、净额、相关簇和容量选择。

### 4.6 回测不是完整生产 OMS

K 线参数回测无法证明网络 Unknown、lease、outbox、部分成交竞态和对账恢复。强行运行完整生产协议会显著降低参数搜索吞吐，并制造虚假安全感。

解决办法不是删掉这些验证，而是把它们分配给不同 SimulationProfile。

### 4.7 研究结果只有“原子可见”，没有跨存储原子

当前结果先写主表，再写明细、指标、过滤信号和审计，可能留下不完整但可查询的 Run。目标明确拆开 `BacktestRun.status`、`EvidenceManifest.artifact_status`、`EvaluationGateResult.status` 和 Strategy `PromotionReceipt.status`：Run 完成不等于 Evidence 工件完整，Evidence `Completed` 不等于评价 `eligible`，eligible 也不等于已晋级。

### 4.8 风险配置与止盈止损存在双语义

当前同一 `strategy_configs.risk_config` 会被解析为：

- `crates/domain/src/entities/strategy_config.rs` 的 `BasicRiskConfig`；
- `crates/strategies/src/framework/backtest/types.rs` 的 `BasicRiskStrategyConfig`。

后者还混入 `trade_fee_rate`、`position_leverage`、账户单笔风险比例和分批止盈比例。live 又通过 `apply_live_decision -> deal_signal` 调用回测实现，同时在执行 helper/payload 侧重复计算止盈止损。即使两个入口读取同一 JSON，也无法证明字段默认值、舍入、触发顺序和最终保护位一致。

目标必须拆成唯一 owner 的强类型快照：

```text
StrategyRuntimeSnapshot
  -> definition/artifact/release generation、策略参数、状态 schema 与能力

PortfolioPolicySnapshot
  -> allocation_ratio、目标仓位和净额

RiskPolicySnapshot
  -> 账户风险比例、最大损失、真实 leverage 边界、不可放宽的最终止损

ExecutionPlanningPolicySnapshot
  -> 订单/TIF/拆单/价格保护、部分成交和 ProtectionPlanningValue 生成政策

ExecutionDecisionContextSnapshot
  -> 仅 Web 已创建的用户 ExecutionRequest 对以上四个 Published Policy Snapshot 的稳定绑定

ResearchDecisionContextSnapshot
  -> ResearchScenarioRef 对同一 DecisionContextCore 与以上四个 Published Policy Snapshot 的稳定绑定
  -> 不含 Web ExecutionRequest、combo、credential_reference 或用户执行授权

动态 Evidence
  -> MarketSnapshotRef、AccountSnapshotRef、InstrumentRulesSnapshotRef、Clock

ResearchScenario
  -> 模拟账户、universe、初态与四个 Policy Snapshot 的研究 subject identity

ResearchRunSpec
  -> ResearchScenario + DatasetManifest + EvaluationManifest + SimulationProfile
  -> ResearchExecutionArtifactRef + ClockSpec + RngSpec + SchedulerSpec
  -> 不含 Web ExecutionRequest、ExecutionDecisionContextSnapshot 或用户 credential_reference

SimulationProfile
  -> fee、slippage、funding、latency、candle 内路径
```

运行入口不得再把同一 payload 同时反序列化成两个语义重叠的 Risk 类型。Strategy 快照不得包含用户 risk profile；Web profile 必须先由 Core Risk 解析成不可变 `RiskPolicySnapshot`。Strategy 候选位、Risk 最终决策和 Execution 保护实现必须分别留下 typed trace，便于逐层 parity。

### 4.9 Rust 代码形态债务

当前还存在以下“对象/函数边界不清”：

- `TradingState` 把 Strategy 状态、模拟资金、仓位、交易记录和风险推进放在同一个可变对象；
- `deal_signal` 是跨 Strategy、Portfolio、Risk、Execution、Research 的多 owner 大函数；
- `StopLossCalculator` 是零字段静态函数容器；
- `StrategyExecutor` 同时承担 evaluator、行情/缓存 I/O 和状态初始化；
- `StrategyRegistry` 是可变全局单例，无法由 RuntimeSnapshot 完整解释；
- legacy `OrderService`、`OrderCreationService` 等按技术名聚合多个业务动作。

迁移不能只把 `deal_signal` 移入 `impl TradingState` 或改名为 `TradingEngineService`。目标形态为：

- 有 identity/生命周期/不变量的 Order、Run、Release、SimulationLedger 使用 Aggregate；
- 无状态止损候选选择、指标和数值规则使用自由纯函数；
- 共享冻结配置的风险/组合/执行决策使用 Policy 对象；
- Vegas 使用 `Evaluator + StrategyEvaluationState`，同一纯 transition 服务 backtest/live；
- 读取、持久化、发事件使用具体 Use Case 对象和消费方 Port；
- Worker/quant-lab 只负责装配和循环。

## 5. 目标目录

```text
apps/quant-lab/src/
├── config.rs
├── entrypoints/{cli,internal_http}.rs
├── composition/research.rs
└── main.rs

crates/domains/
├── market/
├── strategy/src/
│   ├── model/
│   │   ├── strategy_definition.rs
│   │   ├── runtime_snapshot.rs
│   │   ├── strategy_signal.rs
│   │   ├── signal_evidence.rs
│   │   └── evaluation_state.rs
│   ├── strategies/vegas/
│   │   ├── config.rs
│   │   ├── feature_set.rs
│   │   ├── evaluator.rs
│   │   ├── candidate_levels.rs
│   │   ├── evidence.rs
│   │   └── rules/
│   │       ├── ema_structure.rs
│   │       ├── momentum.rs
│   │       ├── volume.rs
│   │       ├── fib.rs
│   │       ├── candle_pattern.rs
│   │       ├── long_entry.rs
│   │       └── short_entry.rs
│   ├── use_cases/commands/evaluate_market_snapshot.rs
│   ├── ports/evaluation_state_store.rs
│   └── api/evaluator.rs
├── portfolio/
├── account/
├── risk/
├── execution/
├── reconciliation/
└── research/src/
    ├── model/
    │   ├── experiment.rs
    │   ├── backtest_run.rs
    │   ├── dataset_manifest.rs
    │   ├── evaluation_manifest.rs
    │   ├── research_run_spec.rs
    │   ├── research_execution_artifact_ref.rs
    │   ├── research_decision_context.rs
    │   ├── simulation_profile.rs
    │   ├── simulation_ledger.rs
    │   ├── simulation_protection_state.rs
    │   ├── checkpoint.rs
    │   └── research_evidence.rs
    ├── policies/
    │   ├── parameter_space.rs
    │   ├── walk_forward.rs
    │   └── evaluation_gate.rs
    ├── use_cases/commands/
    │   ├── start_backtest_run.rs
    │   ├── execute_backtest_run.rs
    │   ├── checkpoint_backtest_run.rs
    │   ├── complete_backtest_run.rs
    │   ├── evaluate_backtest_run.rs
    │   └── publish_research_evidence.rs
    ├── ports/
    │   ├── experiment_store.rs
    │   ├── research_evidence_store.rs
    │   └── artifact_store.rs
    └── api/

crates/quant/
├── math/
├── indicators/
├── backtest/src/
│   ├── deterministic_clock.rs
│   ├── event_scheduler.rs
│   ├── replay.rs
│   ├── fill_model.rs
│   ├── fee_model.rs
│   ├── slippage_model.rs
│   └── funding_model.rs
└── analytics/

crates/adapters/
├── postgres/src/research/
│   ├── experiment_store.rs
│   └── research_evidence_store.rs
├── object-storage/src/research_artifact.rs
├── redis/src/strategy_evaluation_state.rs
└── simulated-exchange/              # 仅 PaperEvent/test 使用

tests/
├── parity/vegas/
├── research/
└── recovery/                       # CI-only ephemeral harness；不属于任何 deployable Release Unit
```

`quant/research` 不再存在。参数空间、walk-forward 和评估门禁有 Experiment 语义，归 Research；纯统计公式放 `quant/math` 或 `quant/analytics`。

## 6. 事实 Owner

| 事实 | Owner |
| --- | --- |
| 原始/标准化历史行情 | Market |
| 本次实验的数据选择、universe 与指纹 | Research DatasetManifest |
| Vegas 参数、规则、信号和内部状态迁移 | Strategy |
| 资本预算、排序、净额和目标仓位 | Portfolio |
| 事前审批、最终风险边界和持续风险 | Risk |
| `ExecutionPlanningValue`、确定性的 child `OrderPlan`、`ProtectionPlanningValue`，以及 live/Paper simulated OMS 的 OrderIntent、ExecutionPlan、ProtectionPlan 与纯订单状态迁移 | Execution |
| 真实余额、仓位、保证金和 PnL | Account |
| 模拟现金、仓位、费用、working orders、`SimulationProtectionState` 和权益 | Research SimulationLedger |
| Experiment、Run、Checkpoint、DatasetManifest、EvaluationManifest、trial ledger、ResearchEvidence 与 EvaluationGateResult | Research |
| StrategyDefinition、StrategyArtifact、Release、RuntimeSnapshot 与 PromotionReceipt | Strategy |

Research 只能编排，不复制其他 owner 的政策。

`DatasetManifest` 只能冻结并引用 Market 已存在的 point-in-time 历史 stream / snapshot。它至少固定 `market_data_source_profile_id`、exchange/product/quote、stream partition、event time、首次 `available_at`/ingested-at 边界、数据 revision、研究 `as_of` cutoff、缺口/重复/修订政策、当时有效的 InstrumentRules、funding/index/mark 工件，以及带 `effective_from/effective_to`、上市/退市和纳入/剔除算法版本的历史 universe membership。Research/quant-lab 对这些输入只读回放：不得直连 Market 表、写入或回填 K 线、持有 `MarketDataAccessCredential`，也不得用用户 `credential_reference` 替代公共数据来源。数据补采、修订或 universe 重建是 Market owner 的独立子 Manifest；它产生新的不可变版本，不覆盖已引用版本。

`EvaluationManifest` 必须在查看目标 OOS 结果前固定 hypothesis、训练/验证/OOS 窗口、walk-forward folds、purge/embargo、参数空间、优化器 algorithm/version/Seed/预算、候选选择规则、成本压力、最小事件/币种/市场状态覆盖、收益集中度和 holdout 重用计数。全部 trial（包括失败和淘汰项）写入不可变 trial ledger，不能只保存胜出参数。

`ExecutionPlanningValue` 是 Execution owner 的纯、规范可序列化 planning value，包含有序 child `OrderPlan` 与 `ProtectionPlanningValue`。live `ExecutionPlan` 才是持久 OMS Aggregate；它保存 planning hash、parent `OrderIntent` 与 child snapshot，并只由 live intake 或 PaperEvent simulated OMS 初始化。ResearchBar 只比较 planning value，并把保护规划落为 Research-owned `SimulationProtectionState`，不得创建、持久化或把 live Aggregate 当 Research 事实。

## 7. 三条不同流程

### 7.1 Experiment 控制流程

```text
quant-lab request
  -> Research::StartBacktestRun
  -> 冻结 RunId
  -> 冻结 ResearchRunSpec
       DatasetManifestRef + EvaluationManifestRef
       ResearchExecutionArtifactRef + ClockSpec + RngSpec + SchedulerSpec
  -> 为每个模拟账户构造 ResearchScenario
  -> 构造 ResearchDecisionContextSnapshot（不伪造 Web ExecutionRequest/ExecutionDecisionContextSnapshot）
  -> Running
  -> Execute / Checkpoint
  -> Complete 或 Failed
  -> Publish Completed Evidence artifact
  -> Evaluate -> eligible / rejected / inconclusive
  -> Strategy promote 时另建 PromotionReceipt
```

所有影响结果的输入必须进入不可变 Run Spec。`ResearchExecutionArtifactRef` 至少固定 Git revision、candidate Strategy artifact、quant-lab/backtest/analytics digest、Release Unit Manifest、Cargo.lock、toolchain、target/profile/features 和依赖图 hash；`RngSpec` 固定 PRNG algorithm/version、master seed 和稳定 substream 分区；`SchedulerSpec` 固定同时间事件 tie-breaker、partition ordering、并行 reduction ordering 及版本。环境变量只允许在 quant-lab 边界解析，不能在 Strategy、Research policy 或 Quant Kernel 内部临时读取。

### 7.2 ResearchBar 逐事件循环

```text
EventScheduler 取出最早 decision_time
  -> 读取该时点全部 MarketEvent
  -> SimulationLedger mark-to-market / funding
  -> 已生效 SimulationProtectionState 的 candle/tick 触发与模拟 fill
  -> SimulationLedger 应用该 fill，生成模拟 AccountSnapshot
  -> Continuous Risk
  -> RiskAction(Reduce / Close / KillSwitch) 经 Execution planning 生成 reduce-only plan
  -> FillModel / simulated exchange
  -> SimulationLedger 应用 RiskAction 的模拟成交
  -> 每个 StrategyEvaluator 更新自己的 EvaluationState
  -> 收集该时点全部 StrategySignal
  -> decision-time barrier
  -> Portfolio 一次完成排序、净额、容量与相关簇约束
  -> PreTrade RiskDecision
  -> ExecutionPlanningValue / child OrderPlan / ProtectionPlanningValue
  -> ProtectionPlanningValue 落为 SimulationProtectionState
  -> Strategy exit intent 与既有 RiskAction 净额后进入 Execution planning
  -> candle/tick FillModel
  -> SimulationLedger 应用模拟成交
  -> 回到 Continuous Risk；若有 RiskAction，走上方 reduce-only planning/fill/ledger 闭环直至稳定
  -> 记录 typed evidence event
  -> 下一 decision_time
```

同一 `decision_time` 的优先级固定为：先按 `SimulationProfile` 声明的路径结算已生效 `SimulationProtectionState`；再处理 `KillSwitch`、`Close`、`Reduce` 等 Continuous Risk 安全动作；Strategy exit 只能与同方向 reduce-only 动作合并，不能产生第二次平仓；只有不存在更严格 RiskAction 且账户仍允许增加风险时，才处理新的 Strategy entry。`RiskAction` 必须经 Execution planning 和 FillModel 回到 `SimulationLedger`，禁止直接改写模拟仓位或假造 Fill。

严格时序：

- Signal 只能读取当前已完成、生产可见的市场证据；
- 入场触发和入场价不能读取未来 K 线；
- 后续 K 线只用于已产生订单的撮合、止盈止损和持仓路径；
- candle 内价格路径必须由 SimulationProfile 明确；
- 多币组合使用统一资金、容量、相关簇和事件时钟；
- symbol 输入排序变化不得改变结果。

### 7.3 Evidence 发布流程

```text
typed event log + equity curve
  -> quant/analytics
  -> ResearchEvidence draft
  -> 内容哈希上传不可变大对象
  -> Research owner 单一 DB transaction
       EvidenceManifest
       metrics/index
       object references
       idempotency
       artifact_status = Completed
  -> Research 按冻结的 EvaluationManifest 计算 EvaluationGateResult
  -> Strategy 仅可引用 Completed + eligible 创建 PromotionReceipt
```

对象已写、数据库失败时不产生可见 Completed Evidence；孤立对象由 GC 清理。`Completed` 只证明证据对象和引用完整、原子可见，可以对应失败、亏损、过拟合或样本不足；它不能自动改写为 `eligible`。`PromotionReceipt` 必须固定 Completed Evidence、eligible gate、被评估的 candidate artifact、目标 released artifact/build digest、candidate 到 released 的同输入逐层 parity 证据、批准人/时间和允许的 deployment channel。

## 8. 三种 SimulationProfile

### 8.1 ResearchBar

适用：Vegas/NWE 参数搜索、walk-forward、多币组合、成本压力。

精确复用：

- Strategy evaluator/state transition；
- Portfolio policy；
- Risk policy；
- Execution planning 生成的 `ExecutionPlanningValue`、有序 child `OrderPlan` 与 `ProtectionPlanningValue`；
- Research-owned `SimulationProtectionState` 的确定性更新；
- 每次 Market/fill/funding 后的 `SimulationLedger -> simulated AccountSnapshot -> Continuous Risk -> RiskAction -> Execution planning -> FillModel -> SimulationLedger` 闭环。

近似部分：

- candle/tick fill；
- latency、slippage、fee、funding；
- 同 K 线内 stop/take-profit 路径。

不覆盖：lease、outbox、网络 Unknown、生产保护恢复、Reconciliation。

ResearchBar 的 Continuous Risk 只复用 Risk 与 Execution 的纯业务决策/计划，不运行生产 lease、Outbox、Gateway permit 或 Unknown 恢复；这些可靠性语义仍由 PaperEvent/RecoveryHarness 验证。

`SimulationProfile` 的业务不变量只适用于**相同单步输入**：当四个 Policy Snapshot、`ResearchDecisionContextSnapshot` 的 Context Core、Market/Account/Instrument Evidence、EvaluationState before 和 DecisionTime 相同时，改变 fee/slippage/funding/latency/candle path 不能改变该单步的 Signal、Target、RiskDecision、`ExecutionPlanningValue`、child `OrderPlan` 或 `ProtectionPlanningValue`。完整运行中，只要 Fill、fee、funding 或 Ledger 首次不同，下一时点输入已经不同；从首个模拟状态/Evidence hash 分歧起必须标记为 scenario divergence，不再错误要求后续业务输出相同。只有动态 Evidence 仍一致的前缀继续做 exact parity。

### 8.2 PaperEvent

适用：订单 Ack、PartialFill、Reject、Cancel、Protection 和延迟行为。

- 使用 Simulated Exchange Adapter；
- 由同一 planning value 在 simulated store 创建 `OrderIntent`、`ExecutionPlan`、`ProtectionPlan`，复用 Execution aggregate/state transition；
- 可以故障注入，但不调用真实交易所；
- 不写生产 Order/Fill/Account 表。

### 8.3 RecoveryHarness

适用：生产可靠性验证。

- 只作为 CI/test 编排产生的临时工件，不属于任何 deployable Release Unit、生产镜像或生产部署图；
- disposable Postgres；
- lease/inbox/outbox；
- 请求已发但响应 Unknown；
- 重复、乱序和崩溃重启；
- 部分成交保护、撤单竞态、Reconciliation；
- 只能使用 test credential、合成/隔离数据和 ephemeral 基础设施；不得读取生产环境变量、生产 secret、真实交易所或生产数据库；
- 不产出策略收益结论。

RecoveryHarness 不自建 OMS Aggregate；它只在 CI ephemeral adapters 上驱动 live Execution intake/recovery 路径并观察该路径创建的 live-shaped state。合法创建 `OrderIntent`、`ExecutionPlan`、`ProtectionPlan` 的路径仍只有 PaperEvent simulated OMS 或 live Execution。

## 9. Vegas 逻辑实际归属

| Vegas 内容 | 目标位置 |
| --- | --- |
| EMA、RSI、ATR、布林带数值 | `quant/indicators` |
| rolling、分位数和纯统计 | `quant/math` |
| EMA 结构、动量、Fib、K 线入场规则 | `domains/strategy/strategies/vegas/rules` |
| long/short 策略结论 | `vegas/evaluator.rs` |
| 候选入场价、候选失效价 | `vegas/candidate_levels.rs` |
| Signal 原因、权重和过滤证据 | `vegas/evidence.rs` |
| 参数与输入要求 | `vegas/config.rs` + StrategyDefinition |
| 指标 checkpoint | Strategy EvaluationState |
| `allocation_ratio=0.58` | Portfolio policy/snapshot |
| 用户风险、总敞口、最终止损 | Risk policy |
| ExecutionPlanningValue / child OrderPlan / ProtectionPlanningValue | Execution planning |
| live/Paper simulated OMS 的 OrderIntent / ExecutionPlan / ProtectionPlan | Execution |
| SimulationProtectionState | Research |
| 模拟现金、仓位和权益 | Research SimulationLedger |
| fee/slippage/funding 机制 | `quant/backtest` |
| 实验与证据 | Research |
| 真实交易所协议 | exchange-gateway / `crypto_exc_all` |

## 10. 现有文件迁移分配

| 当前文件/模块 | 目标 | 删除门 |
| --- | --- | --- |
| `internal_server.rs` 回测 Handler | `apps/quant-lab/entrypoints/internal_http.rs` | 调用方和 Contract 全部切换 |
| `backtest/runner.rs` | quant-lab 配置映射 + Research Run commands | 不再创建 Pool/Repository 或实现实验状态机 |
| `backtest/executor.rs` | Research execute use case + Market historical stream | 新旧批次 parity |
| `IndicatorStrategyBacktest` | Strategy Evaluator API | 删除账户 risk_config 参数 |
| `VegasBacktestAdapter` | 迁移期 Strategy API bridge | ResearchBar 成为唯一入口 |
| `SignalStage` | Strategy evaluator + scoped state | 逐 K 线 Signal parity |
| `FilterStage` | 按原因拆到 Strategy evidence、Portfolio 或 Risk | 每个 reason owner 明确 |
| `PositionStage` | Portfolio + Risk + ExecutionPlanningValue（含 child OrderPlan/ProtectionPlanningValue）+ ResearchBar fill | Target/Decision/Planning parity |
| `deal_signal` | 多 owner strangler | 调用方归零且差异已批准 |
| `BacktestContext` | Research RunState + owner typed outputs | 不再允许任意 Stage 修改全部状态 |
| `BacktestService` | Research complete/publish + quant analytics | 无环境变量、无硬编码初始资金 |
| `SqlxBacktestRepository` | Postgres Research owner module | runtime DDL 删除、事务测试通过 |
| `SqlxAuditRepository` | EvidenceManifest/EvidenceObjectRef writer | 不再暴露逐表万能写接口 |
| `StrategyProgressManager` | Research RunCheckpoint | 幂等断点恢复 parity |

### 10.1 当前关键类型的代码形态迁移

| 当前类型/函数 | 目标代码形态 | 目标位置 |
| --- | --- | --- |
| `TradingState` | 拆为 `StrategyEvaluationState`、Research `SimulationLedger` 和逐层 typed output | `domains/strategy/model` + `domains/research/model` |
| `deal_signal` | 删除；由 Research use case 串联多个 owner 的公开纯 API | `domains/research/use_cases/commands/execute_backtest_run` |
| `StopLossCalculator::select` | 自由纯函数；若需要冻结政策则由 Risk Policy 调用 | `domains/risk/policies/final_stop.rs` |
| `StrategyExecutor` | 纯 `StrategyEvaluator` API + EvaluationState Port + App/Use Case 编排 | `domains/strategy/{api,ports,use_cases}` |
| 全局可变 `StrategyRegistry` | App 装配并按 release generation 冻结的 `StrategyCatalog` | `domains/strategy/model` + `apps/*/composition` |
| `Order` | 私有关键字段、显式状态 transition、注入时间的 Aggregate | `domains/execution/model` |
| `OrderCreationService`/万能 `OrderService` | 按 `CreateOrderIntent`、`PrepareOrderSubmission`、`RequestCancellation` 等 Use Case 拆分 | `domains/execution/use_cases` |
| `ExecutionWorker::run_once` | 只处理 lease/message、Contract 映射、调用 Use Case 和 ack/checkpoint | `apps/execution-worker` |

删除门不仅是类型改名或调用方可编译，还必须证明原 owner 职责已由对应 Model/Policy/Use Case 接管，且 parity/recovery 测试覆盖。

## 11. 数据库增删改查

Research 定义业务 Port：

```rust
trait ResearchEvidenceStore {
    async fn publish_completed_evidence(
        &self,
        manifest: &CompletedEvidenceManifest,
    ) -> Result<ResearchEvidenceId, PublishEvidenceError>;
}
```

SQL 只位于：

```text
crates/adapters/postgres/src/research/
```

禁止 `insert_log`、`update_by_id`、`sqlx::Transaction` 或表名进入 Domain Port。

Research 数据库事务至少原子写入：

- Evidence identity、`artifact_status = Completed` 与所引用的 Run identity；不得借此覆盖 `BacktestRun.status`；
- Dataset/Evaluation/Strategy/Policy/SimulationProfile/ExecutionArtifact/RNG/Scheduler 引用与 hash；
- 指标与证据索引；
- 大对象内容哈希与引用；
- 幂等记录。

`EvaluationGateResult` 由独立 Research use case 根据冻结的 EvaluationManifest 写入；Strategy `PromotionReceipt` 由 Strategy owner 写入，不得塞进 Evidence 发布事务。schema 只通过 `migrations/` 维护，不允许 Repository runtime DDL。Web/Admin 查询 ResearchEvidence 必须经 Core Research Query API，不直查私有表。

## 12. 六类真实修改如何定位

### 修改 Vegas EMA 入场条件

修改 Strategy Vegas rule、单元测试和新 Definition version。Research 重新运行并产生新 Evidence；不改 Backtest Kernel、Portfolio、Risk 或 SQL。

### 资金比例从 58% 改为 30%

修改 Portfolio policy/version。StrategySignal 必须字节一致，从 PortfolioTarget 开始出现差异。

### 修改策略止盈或退出规则

修改 Strategy exit policy/RuntimeSnapshot version。StrategySignal/ExitIntent、`ExecutionPlanningValue`（及其 child `OrderPlan`）和 `ProtectionPlanningValue` 从 Strategy 层开始变化；Risk 仍用同一风险政策验证和约束，不在 `BasicRiskStrategyConfig` 中另设一套 backtest 止盈。ResearchBar 只更新对应 `SimulationProtectionState`；Paper/live 才从 planning value 创建 `ProtectionPlan` Aggregate。

### 修改最大亏损或最终止损

修改唯一的 `RiskPolicySnapshot` 与 Risk policy/version。Vegas 候选失效价和 Strategy exit intent 保持输入证据；Research、paper、shadow、live 调用同一 final-stop symbol，`RiskDecision`、approved quantity/stop、`ExecutionPlanningValue`（及其 child `OrderPlan`）和 `ProtectionPlanningValue` 从 Risk 层开始一致变化。不得修改 `BacktestRiskConfig` 或 `LiveStopLossService`，因为目标架构中不存在这两套实现。

### 修改手续费或滑点

修改 `quant/backtest` 模型或 SimulationProfile version。对**相同单步输入**，Signal、Target、RiskDecision、`ExecutionPlanningValue`、child `OrderPlan` 与 `ProtectionPlanningValue` 必须不变，模拟 Fill 与净指标可以变化。首次 Fill/Ledger/Evidence hash 分歧之后属于 scenario divergence，不能再要求后续输入和输出保持相同。

### 修改订单 Unknown/恢复

修改 Execution/Reconciliation 和 RecoveryHarness 测试，不跑大规模 Vegas 参数搜索证明恢复正确。

## 13. Vegas Parent Program 与单 Owner 子 Manifest

Vegas 不是一个可由单一 Slice 完成的迁移。编码前必须先在 Migration Registry 建立 parent Program：

```text
MP-vegas-research-parity-v1
```

Parent Program 只协调基线、依赖、跨 owner 验收和最终关闭条件；它不拥有业务事实、不直接改代码，也不能代替子 Manifest 的 owner/rollback/delete gate。禁止再建立“拆 `deal_signal`”这类同时由 Strategy、Portfolio、Risk、Execution、Research 和 Quant 共同拥有的 Slice。

| Child Manifest | 唯一 Owner | 迁移内容 | 主要依赖与验收 |
| --- | --- | --- | --- |
| `V0-market-point-in-time-dataset` | Market | point-in-time stream、revision、历史 universe、InstrumentRules、funding/index/mark 与内容 hash | 可按 `as_of` 重建数据和成员关系；补采只产生新版本 |
| `V1-research-run-governance` | Research | Experiment、BacktestRun、Dataset/Evaluation Manifest、trial ledger、RunSpec、ResearchDecisionContextSnapshot、Checkpoint | RunSpec 在读取结果前冻结；Research 不制造 Web request |
| `V2-strategy-vegas-evaluator` | Strategy | Vegas evaluator、规则、candidate levels、evidence、RuntimeSnapshot | 固定输入下逐 K 线 Signal/evidence parity |
| `V3-strategy-evaluation-state` | Strategy | EvaluationScopeId、state schema、预热/缺口/重启规则与 store Port | 并行 Run 隔离；冷启动、增量和恢复输出一致 |
| `V4-portfolio-policy` | Portfolio | allocation_ratio、排序、净额、相关簇、容量和 decision-time barrier | symbol 重排不改变 Target |
| `V5-risk-policy` | Risk | RiskPolicySnapshot、PreTrade/Continuous Risk、唯一 final-stop 约束 | Approve/Resize/Reject、RiskAction 和原因逐层 parity |
| `V6-execution-planning` | Execution | ExecutionPlanningPolicySnapshot、ExecutionPlanningValue、child OrderPlan、ProtectionPlanningValue | 纯 planning API 无 live store/lease/outbox 依赖 |
| `V7-quant-backtest-kernel` | Quant 技术 Owner | Clock、Scheduler、Replay、Fill/Fee/Slippage/Funding、数值与规范序列化 | Domain-free；同 artifact/target/RunSpec 可 bitwise replay |
| `V8-research-simulation-evidence` | Research | SimulationLedger、SimulationProtectionState、ResearchBar、Evidence 原子可见发布、EvaluationGateResult | 只保存 planning value；Completed 与 eligible 分离 |
| `V9-execution-paper-simulated-oms` | Execution | PaperEvent simulated store、OrderIntent/ExecutionPlan/ProtectionPlan 初始化和事件状态迁移 | 不写生产 Execution/Account 事实 |
| `V10-execution-recovery-harness` | Execution | lease、inbox/outbox、Unknown、重复/乱序、部分成交和保护恢复测试 | CI-only ephemeral；不进入 deployable Release Unit |
| `V11-reconciliation-recovery-harness` | Reconciliation | 对账差异、崩溃恢复、外部事实收敛和人工处置证据 | CI-only ephemeral；不访问生产 secret/环境 |
| `V12-strategy-promotion-receipt` | Strategy | candidate -> released artifact 等价链、PromotionReceipt、promote/rollback gate | 必须引用 Completed Evidence + eligible gate + released build parity |

每个子 Manifest 必须单独记录 `owner`、`scope`、`baseline`、`target contract`、`evidence`、`rollback` 和 `delete gate`。如果 Vegas 模式在策略语义、状态 schema、运行入口或交付物上互斥，必须把对应行继续拆成 `Vx-<mode>` 独立 Manifest；不得把多个 mode 藏在同一 Manifest 的可选步骤中，也不得通过一个“共同 Owner”掩盖多 owner 修改。

### 13.1 基线冻结责任

Parent Program 定义共同 fixture 目录和比较协议，各 owner 只冻结自己拥有的基线：

- Market 固定 BTC、ETH、其他币种的多个 point-in-time 窗口、历史 universe 与数据 revision；
- Research 固定 Run identity、Dataset/Evaluation Manifest、SimulationProfile、初始模拟资金和 Evidence schema；
- Strategy 固定逐 K 线 Signal、EvaluationState before/after、candidate level 与 evidence；
- Portfolio 固定排序、容量、净额与 Target；
- Risk 固定候选失效价、最终 stop、RiskDecision/RiskAction 与原因；
- Execution 固定 `ExecutionPlanningValue`、child `OrderPlan`、`ProtectionPlanningValue` 与 trace；
- Quant 固定 Artifact/RNG/Scheduler/数值 contract，并记录 7000/4000/300 窗口差异，不在基线阶段顺手改变行为。

共同验收是同一 legacy revision 和冻结输入可重复产生相同前缀证据；任何行为差异都落到首个 owner 层，而不是用最终 PnL 掩盖。

### 13.2 依赖顺序与切换门

推荐依赖顺序是：

```text
V0 -> V1 -> V2 -> V3
V3 -> V4 -> V5 -> V6
V3 -> V7
V6 + V7 -> V8 -> V9 -> V10 -> V11
V8 + V11 -> V12 -> promote gate
```

Research 只有在 `V0` point-in-time 数据合同、`V1` RunSpec 与 `V7` artifact/RNG/scheduler contract 可用后，才可发布新的可重放 Evidence。`V8` 的 Completed Evidence 不自动触发 `V12`；只有 frozen EvaluationManifest 得到 `eligible` 且 candidate/released 工件等价链完整，Strategy 才可签发 PromotionReceipt。

HTTP/quant-lab 切换、旧表停写、旧 Runner/Executor/万能 Context/`deal_signal` 删除，也必须由受影响 owner 各自建立 cutover/delete Manifest。Parent Program 只在所有调用方归零、回滚窗口结束、子 Manifest evidence 完整且生产授权门禁另行通过后关闭；不得用一次跨 owner“大切换”替代这些门。

## 14. 实战验收矩阵

| 维度 | 必须证明 | 不接受 |
| --- | --- | --- |
| Owner | Run/Evidence 归 Research，Definition/Release 归 Strategy | 两边都能写同一事实 |
| Quant | 无业务 Domain/DB/env 依赖 | “只依赖公开 API 所以没问题” |
| Strategy | 同输入逐 K 线 Signal/evidence 一致 | 最终收益接近 |
| State | Run 隔离、冷启动、增量、缺口一致 | 只跑一次完整历史 |
| Portfolio | 同时点统一分配、symbol 重排不变 | 按遍历顺序抢资金 |
| Risk | Approve/Resize/Reject、原因和边界一致 | 只比较是否成交 |
| 配置 | 同一四个 Policy Snapshot + `ResearchDecisionContextSnapshot` identity/version/hash；动态 Evidence 另行固定 | Research 伪造 `ExecutionDecisionContextSnapshot` 或同一 JSON 解析成两套结构 |
| 数据 | DatasetManifest 可按 `as_of` 重建 revision、历史 universe、上市/退市、InstrumentRules 与 funding/index/mark | 用当前币种列表回放历史或原地覆盖数据版本 |
| 评价 | EvaluationManifest 在 OOS 前冻结，全部 trial 和 walk-forward/purge/embargo 可审计 | 看完结果再改窗口/门槛或只保存胜出参数 |
| 止盈止损 | Strategy exit intent、候选失效价、最终 Risk stop、`ProtectionPlanningValue` 和模拟触发顺序逐层一致 | backtest/live 各有 Calculator/Service |
| Research planning | `ExecutionPlanningValue + child OrderPlan + ProtectionPlanningValue` 精确 parity；只落 `SimulationProtectionState` | Research 创建或持久化 live OMS Aggregate |
| Paper/live OMS | 相同 planning hash 无损初始化 OrderIntent/ExecutionPlan/ProtectionPlan 与 child snapshot | 用 Research child plan 冒充持久 OMS 事实 |
| Fill/PnL | 相同 artifact/target/RunSpec 可重放；首次动态状态分歧后标记 scenario divergence | 对不同 Ledger 输入仍强求全程输出一致或宣称等同真实交易所 |
| 确定性 | ResearchExecutionArtifactRef、PRNG substream、scheduler/reduction ordering 和数值 contract 完整 | 只记录裸 Seed 或“当前 binary” |
| ResearchBar | 不声称覆盖 OMS 恢复 | 用参数回测证明 Unknown 安全 |
| PaperEvent | 订单事件和保护状态迁移可验证 | 只生成最终成交 |
| Recovery | lease/outbox/Unknown/reconciliation 有 CI-only ephemeral 故障测试 | 进入生产镜像、读取生产 secret 或用收益报告代替 |
| Evidence | Completed 原子可见、对象引用完整，且与 EvaluationGateResult 状态分离 | 多表部分成功可查询或 Completed 自动等于 eligible |
| Promotion | PromotionReceipt 固定 Completed + eligible + candidate/released artifact 等价证据 | 重新构建后无等价链直接激活 Release |
| 生产安全 | 无真实 Exchange mutation、无生产事实写入 | 依赖人工避免误操作 |
| Legacy | 调用方、配置和表写入归零 | 只改文件名 |
| Program | 每个 Manifest 只有一个 owner；互斥 mode 继续拆分 | 多 owner Slice 或可选 mode 混装 |
| 代码形态 | Aggregate、纯函数、Policy、Use Case 各有明确存在理由 | 把大函数移入 `impl` 或新增零字段 Service |
| 构建边界 | Research-only 代码不进入生产 App 依赖图/镜像且无生产部署资格；共享 Domain 变化触发生产 parity | 为避免 CI 构建而复制 live 规则到回测 crate |

## 15. 最终判断

修订后的架构符合 Vegas 的实际研发工作流：Research 管实验，Quant 管纯模拟机制，Strategy/Portfolio/Risk/Execution 管各自业务规则，Paper 与 Recovery 对不同可靠性问题负责。

这比一条“完整但模糊”的线性流水线更复杂一点，却能明确回答：哪里改策略、哪里改资金、哪里改风险、哪里改撮合、哪里验证订单恢复、哪里写研究 SQL，以及哪种测试可以证明哪类结论。

当前仍是目标设计，尚未迁移代码。第一步是建立 `MP-vegas-research-parity-v1`，登记 `V0`～`V12` 的单 owner 子 Manifest，并由各 owner 分别冻结自身基线；在 Program 和目标合同未冻结前，不应一次性重写现有回测。
