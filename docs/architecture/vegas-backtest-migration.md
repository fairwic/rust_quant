# Vegas 与现有回测主链迁移实战

- 状态：迁移设计，尚未实施
- 日期：2026-07-20
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 核心决策：[ADR-0009：Research Domain、纯 Backtest Kernel 与分级模拟](adr/0009-research-domain-and-tiered-simulation.md)
- 迁移总计划：[Rust Quant 架构迁移计划](migration-plan.md)

## 1. 结论先行

现有 Vegas 回测可以迁入目标架构，但不能按一条线性“Strategy -> Portfolio -> Risk -> 完整 OMS -> AccountProjection”直接搬运。最终设计采用以下边界：

1. `domains/research` 拥有 Experiment、BacktestRun、DatasetManifest、Checkpoint、SimulationProfile 和 ResearchEvidence；
2. `quant/backtest` 只保留确定性时钟、事件调度、Replay、撮合、费用、滑点和资金费，不直接调用业务 Domain；
3. Research use case 通过稳定 API 编排 Strategy、Portfolio、Risk 和必要的 Execution 纯能力；
4. StrategyEvaluationState 是 evaluator 内部状态，并带 BacktestRun/部署作用域；
5. 多币种同一决策时间先收集所有 Signal，再统一排序、净额和资金分配；
6. 模拟账户使用 Research `SimulationLedger`，不冒充生产 AccountProjection；
7. `ResearchBar`、`PaperEvent`、`RecoveryHarness` 分别验证研究经济行为、订单事件和故障恢复；
8. ResearchEvidence 使用内容寻址对象加 Completed Manifest 实现原子可见发布，不虚构跨存储原子事务。

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
OrderIntent / OrderPlan
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

当前结果先写主表，再写明细、指标、过滤信号和审计，可能留下不完整但可查询的 Run。目标以 Research `Running/Completed/Failed` 状态机和 Completed EvidenceManifest 控制可见性。

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
    │   ├── simulation_profile.rs
    │   ├── simulation_ledger.rs
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
└── recovery/
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
| OrderIntent、OrderPlan 和纯订单状态迁移 | Execution |
| 真实余额、仓位、保证金和 PnL | Account |
| 模拟现金、仓位、费用、working orders 和权益 | Research SimulationLedger |
| Experiment、Run、Checkpoint、ResearchEvidence | Research |
| StrategyDefinition、StrategyArtifact、Release、RuntimeSnapshot | Strategy |

Research 只能编排，不复制其他 owner 的政策。

## 7. 三条不同流程

### 7.1 Experiment 控制流程

```text
quant-lab request
  -> Research::StartBacktestRun
  -> 冻结 RunId
  -> 冻结 DatasetManifest
  -> 冻结 StrategyRuntimeSnapshot
  -> 冻结 Portfolio/Risk policy version
  -> 冻结 SimulationProfile + Seed
  -> Running
  -> Execute / Checkpoint
  -> Complete 或 Failed
  -> Publish Evidence
```

所有影响结果的输入必须进入不可变 Run Spec。环境变量只允许在 quant-lab 边界解析，不能在 Strategy、Research policy 或 Quant Kernel 内部临时读取。

### 7.2 ResearchBar 逐事件循环

```text
EventScheduler 取出最早 decision_time
  -> 读取该时点全部 MarketEvent
  -> SimulationLedger mark-to-market / funding
  -> 每个 StrategyEvaluator 更新自己的 EvaluationState
  -> 收集该时点全部 StrategySignal
  -> decision-time barrier
  -> Portfolio 一次完成排序、净额、容量与相关簇约束
  -> PreTrade RiskDecision
  -> OrderIntent / OrderPlan
  -> candle/tick FillModel
  -> SimulationLedger 应用模拟成交
  -> Continuous Risk
  -> 记录 typed evidence event
  -> 下一 decision_time
```

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
       status = Completed
  -> 对查询和 Strategy Release 可见
```

对象已写、数据库失败时不产生可见 Completed Evidence；孤立对象由 GC 清理。

## 8. 三种 SimulationProfile

### 8.1 ResearchBar

适用：Vegas/NWE 参数搜索、walk-forward、多币组合、成本压力。

精确复用：

- Strategy evaluator/state transition；
- Portfolio policy；
- Risk policy；
- OrderIntent/OrderPlan。

近似部分：

- candle/tick fill；
- latency、slippage、fee、funding；
- 同 K 线内 stop/take-profit 路径。

不覆盖：lease、outbox、网络 Unknown、生产保护恢复、Reconciliation。

### 8.2 PaperEvent

适用：订单 Ack、PartialFill、Reject、Cancel、Protection 和延迟行为。

- 使用 Simulated Exchange Adapter；
- 复用 Execution Order aggregate/state transition；
- 可以故障注入，但不调用真实交易所；
- 不写生产 Order/Fill/Account 表。

### 8.3 RecoveryHarness

适用：生产可靠性验证。

- disposable Postgres；
- lease/inbox/outbox；
- 请求已发但响应 Unknown；
- 重复、乱序和崩溃重启；
- 部分成交保护、撤单竞态、Reconciliation；
- 不产出策略收益结论。

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
| OrderIntent/OrderPlan | Execution |
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
| `PositionStage` | Portfolio + Risk + OrderPlan + ResearchBar fill | Target/Decision/Plan parity |
| `deal_signal` | 多 owner strangler | 调用方归零且差异已批准 |
| `BacktestContext` | Research RunState + owner typed outputs | 不再允许任意 Stage 修改全部状态 |
| `BacktestService` | Research complete/publish + quant analytics | 无环境变量、无硬编码初始资金 |
| `SqlxBacktestRepository` | Postgres Research owner module | runtime DDL 删除、事务测试通过 |
| `SqlxAuditRepository` | EvidenceManifest/EvidenceObjectRef writer | 不再暴露逐表万能写接口 |
| `StrategyProgressManager` | Research RunCheckpoint | 幂等断点恢复 parity |

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

- Run identity 与 Completed 状态；
- Dataset/Strategy/Policy/SimulationProfile 版本；
- 指标与证据索引；
- 大对象内容哈希与引用；
- 幂等记录。

schema 只通过 `migrations/` 维护，不允许 Repository runtime DDL。Web/Admin 查询 ResearchEvidence 必须经 Core Research Query API，不直查私有表。

## 12. 五类真实修改如何定位

### 修改 Vegas EMA 入场条件

修改 Strategy Vegas rule、单元测试和新 Definition version。Research 重新运行并产生新 Evidence；不改 Backtest Kernel、Portfolio、Risk 或 SQL。

### 资金比例从 58% 改为 30%

修改 Portfolio policy/version。StrategySignal 必须字节一致，从 PortfolioTarget 开始出现差异。

### 修改最大亏损或最终止损

修改 Risk policy/version。Vegas 候选失效价保持输入证据；RiskDecision、approved quantity/stop 和 OrderPlan 变化。

### 修改手续费或滑点

修改 `quant/backtest` 模型或 SimulationProfile version。Signal、Target、RiskDecision、OrderPlan 应不变，模拟 Fill 与净指标变化。

### 修改订单 Unknown/恢复

修改 Execution/Reconciliation 和 RecoveryHarness 测试，不跑大规模 Vegas 参数搜索证明恢复正确。

## 13. 推荐迁移切片

### Slice 0：冻结当前基线

- 固定 BTC、ETH、其他币种三层样本和多个窗口；
- 固定当前配置、初始资金、成本、Seed；
- 保存逐 K 线 Signal、交易、过滤和指标；
- 记录 7000/4000/300 窗口差异，不顺手改变策略行为。

验收：同一 legacy revision 可重复产生相同基线。

### Slice 1：建立 Research 最小 owner

- Experiment、Run、Checkpoint、Evidence identity；
- Postgres Research owner module；
- quant-lab 只装配 use case；
- 先包装 legacy engine，不改变交易结果。

验收：运行生命周期和持久化 owner 清晰，legacy 结果 parity。

### Slice 2：迁移 Vegas Evaluator

- 移动纯指标；
- 按规则族迁移 evaluator；
- 移除环境变量和账户 RiskConfig；
- legacy Adapter 调用新 API。

验收：逐 K 线 Signal/evidence parity。

### Slice 3：迁移 EvaluationState

- 引入 EvaluationScopeId；
- backtest 使用 Run-scoped in-memory store；
- live 使用 Redis/Postgres Adapter；
- 统一预热、窗口、缺口和恢复规则。

验收：冷启动、增量、重启、重复和缺口输出一致；并行 Run 隔离。

### Slice 4：拆 `deal_signal`

- 提取 Portfolio allocation；
- 提取 PreTrade Risk；
- 提取 OrderIntent/OrderPlan；
- 建立 SimulationLedger 和 ResearchBar fill；
- 增加 decision-time barrier。

验收：Signal、Target、Decision、Plan 逐层 parity；symbol 重排结果一致。

### Slice 5：切换纯 Backtest Kernel

- 迁移 Clock、Scheduler、Replay、Fill/Fee/Slippage/Funding；
- Quant crate 移除所有 Domain 依赖；
- 支持单币和统一多币事件流。

验收：相同 Run Spec 字节可重放，并发批次不改变单次结果。

### Slice 6：切换 Evidence 发布

- 内容寻址对象；
- Research owner 完成事务；
- runtime DDL、环境变量业务分支和硬编码初始资金删除；
- GC 和幂等测试。

验收：任何可见 Completed Evidence 必要引用完整，重复完成不生成第二份事实。

### Slice 7：补 PaperEvent 与 RecoveryHarness

- PaperEvent 覆盖订单事件与保护；
- RecoveryHarness 覆盖 lease/outbox/Unknown/reconciliation；
- 不把 Recovery 结果混入收益 Evidence。

验收：三种 profile 能力声明、测试和报告完全分开。

### Slice 8：删除 Legacy

- 当前 HTTP payload 切到 quant-lab；
- 调用方归零后删除 Runner、Executor、万能 Context 和 `deal_signal` 对应职责；
- 保留约定发布窗口的可回滚路径。

## 14. 实战验收矩阵

| 维度 | 必须证明 | 不接受 |
| --- | --- | --- |
| Owner | Run/Evidence 归 Research，Definition/Release 归 Strategy | 两边都能写同一事实 |
| Quant | 无业务 Domain/DB/env 依赖 | “只依赖公开 API 所以没问题” |
| Strategy | 同输入逐 K 线 Signal/evidence 一致 | 最终收益接近 |
| State | Run 隔离、冷启动、增量、缺口一致 | 只跑一次完整历史 |
| Portfolio | 同时点统一分配、symbol 重排不变 | 按遍历顺序抢资金 |
| Risk | Approve/Resize/Reject、原因和边界一致 | 只比较是否成交 |
| OrderPlan | side/quantity/protection plan 一致 | 只比较最终 PnL |
| Fill/PnL | 相同 SimulationProfile 可重放 | 宣称等同真实交易所 |
| ResearchBar | 不声称覆盖 OMS 恢复 | 用参数回测证明 Unknown 安全 |
| PaperEvent | 订单事件和保护状态迁移可验证 | 只生成最终成交 |
| Recovery | lease/outbox/Unknown/reconciliation 有故障测试 | 用收益报告代替 |
| Evidence | Completed 原子可见、对象引用完整 | 多表部分成功可查询 |
| 生产安全 | 无真实 Exchange mutation、无生产事实写入 | 依赖人工避免误操作 |
| Legacy | 调用方、配置和表写入归零 | 只改文件名 |

## 15. 最终判断

修订后的架构符合 Vegas 的实际研发工作流：Research 管实验，Quant 管纯模拟机制，Strategy/Portfolio/Risk/Execution 管各自业务规则，Paper 与 Recovery 对不同可靠性问题负责。

这比一条“完整但模糊”的线性流水线更复杂一点，却能明确回答：哪里改策略、哪里改资金、哪里改风险、哪里改撮合、哪里验证订单恢复、哪里写研究 SQL，以及哪种测试可以证明哪类结论。

当前仍是目标设计，尚未迁移代码。第一步是 Slice 0 冻结基线和 Slice 1 建立 Research 最小 owner，不应一次性重写现有回测。
