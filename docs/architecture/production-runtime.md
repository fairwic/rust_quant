# Rust Quant 生产运行与恢复

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)

## 1. 目标

本规范定义代码在生产中实际如何启动、处理交易、应对重复和故障、恢复未完成状态并安全停止。目录结构只有同时遵守本运行协议，才构成可长期使用的生产架构。

## 2. 运行不变量

1. 数据面只使用已发布、带版本、不可变的配置快照；
2. Strategy、Portfolio 和 Risk 的纯计算路径不执行外部 I/O；
3. 外部 mutation 前先持久化业务身份、审批依据和状态；
4. 重试沿用原 `idempotency_key` 和 client order identity；
5. 网络超时表示结果未知，不等于交易所没有执行；
6. Account 只投影外部账户与成交事实；
7. Reconciliation 只发现差异和发出修复命令，不绕过 owner 状态机；
8. 没有有效 RiskDecision 和保护计划不得提交开仓订单。

## 3. 进程启动顺序

```text
解析本 App 的强类型配置
  -> 读取最小范围 Secret
  -> 初始化日志、指标和 Trace
  -> 创建必要 Adapter
  -> 校验 schema 与 contract 兼容性
  -> 恢复 checkpoint、lease、outbox 和未完成订单
  -> 建立行情流或交易所用户流
  -> 完成 startup 检查
  -> 完成 readiness 检查
  -> 开始接收新任务
```

任一步失败时，进程不得进入 Ready。研究、通知等非交易依赖可以按 App 策略降级，但账户新鲜度、交易规格、风险配置和 Execution Adapter 不允许静默降级。

## 4. 行情处理

```text
原始外部消息
  -> 协议解析
  -> 标准 instrument 映射
  -> sequence 与重复检查
  -> 乱序、缺口和陈旧检测
  -> 数值与时间校验
  -> 更新 MarketSnapshot
  -> 发布版本化 MarketEvent
```

- 同一数据流的顺序由明确 sequence 或可证明的替代规则决定；
- 发现缺口时优先补快照或重建本地状态；
- 无法证明完整性和新鲜度的数据必须带 degraded/invalid 质量状态；
- StrategyDefinition 声明的数据要求未满足时，不运行或只产生明确的阻塞证据。

## 5. 策略到订单

```text
MarketSnapshot
  -> StrategyEvaluator
  -> StrategySignal
  -> 用户路径：Web ExecutionRequest 授权交接
  -> Portfolio allocation / netting
  -> PortfolioTarget
  -> 固定 PreTradeSnapshot
  -> RiskDecision
  -> OrderIntent
  -> 持久化 OrderIntent + Outbox
  -> ExecutionPlan
  -> 最终 readiness 与审批时效复核
  -> Exchange mutation
```

### 5.1 Strategy

- 只解释市场证据并产生 Signal；
- 使用 `evidence_cutoff_at` 阻止未来数据污染；
- 同一输入、配置、时钟和随机源必须产生同一输出。

### 5.2 Web 商业授权交接

用户自动交易路径必须区分 Web 商业任务和 Core 订单事实：

- Web 根据会员、`strategy x symbol` combo、凭证和产品资格创建 `ExecutionRequest`；
- `quant_web.execution_tasks` 在迁移期承载该请求，不是 Order/Fill 的事实源；
- Contract 只携带稳定 `execution_account_ref` 与 `credential_reference`，不传明文凭证，也不使用 email 推断交易身份；
- Contract 同时携带 `risk_profile_ref`、版本以及必要的不可变授权约束；Web 拥有用户配置，Core 拥有最终 RiskDecision 与下单金额；
- Core 接收请求后再为目标账户执行 Portfolio、Pre-trade Risk 和 OrderIntent 创建；
- Core 的 Order、Fill、Protection、Reconciliation 结果经 Core API/Event 投影给 Web；
- Core 更新 Web 请求状态时调用 Web owner API，不直写 Web 数据库。

### 5.3 Portfolio

- 合并同一账户下多个策略的目标；
- 处理相反信号、资本预算、策略优先级和目标仓位；
- 输出目标状态，不直接调用交易所。
- 账户级 Portfolio 只有在 `ExecutionRequest` 已提供稳定账户上下文后执行；`signal-worker` 不为未知用户账户提前计算最终数量。

### 5.4 Risk

- 使用冻结的 Market、Account、Portfolio 和 instrument snapshot；
- 返回 Approve、Reject 或 Resize；
- 记录政策版本、输入版本、理由、边界和过期时间；
- Execution 提交前必须确认审批未过期且账户/行情没有越过允许变化边界。

## 6. 订单状态机与外部 mutation

建议的最小订单状态：

```text
Proposed
  -> Persisted
  -> SubmitPending
  -> Submitted
  -> Acknowledged
  -> PartiallyFilled
  -> Filled

SubmitPending / Submitted / Acknowledged
  -> CancelPending
  -> Cancelled

任一外部请求超时
  -> Unknown
  -> 查询交易所
  -> 恢复到 Acknowledged / PartiallyFilled / Filled / Cancelled / Rejected
```

规则：

- 状态只允许通过显式 transition 变化；
- OrderIntent、幂等记录和 outbox 写入应处于同一事务边界；
- 外部请求重试不得创建新的业务订单；
- 交易所不支持等价 client order ID 时，必须通过本地状态、查询和对账强化去重；
- `Unknown` 状态禁止盲目再次提交，必须先查询或对账；
- 成交与撤单事件可重复投递，consumer 必须幂等。

### 6.1 保护状态机

开仓订单必须同时持有可审计的 `ProtectionPlan`。最小保护状态为：

```text
Planned
  -> AttachedPending / PostFillPending
  -> Active

PostFillPending
  -> PartiallyProtected
  -> Active

任一请求超时
  -> Unknown
  -> 查询交易所
  -> Active / PartiallyProtected / Failed

Failed / 超过最大未保护窗口
  -> RiskAction(Reduce / Close / KillSwitch)
  -> Recovering
  -> Active / Closed / ManualIntervention
```

规则：

- 优先使用交易所原生 attached stop，并由 Exchange Gateway 明确声明能力；
- 交易所只能成交后挂保护单时，ExecutionPlan 必须固定最大未保护时间、重试上限和失败动作；
- 如果交易所能力与当前恢复链路无法保证该窗口，目标账户/交易对的 live 开仓能力必须标为 Unsupported；
- 每次部分成交后立即按实际净敞口计算应保护数量，不等待全部成交；
- `protected_quantity < open_exposure` 时状态为 `PartiallyProtected`，禁止把它展示为保护完成；
- 保护单价格、数量或 reduce-only 语义无法等价映射时，Adapter 返回 Unsupported，不得静默降级成裸单。

### 6.2 部分成交与撤单竞态

- Fill、Cancel Ack 和 Order Query 可以乱序到达，按 exchange sequence 或可证明的时间/版本规则幂等合并；
- 收到 Cancelled 后仍可能出现已发生的 Fill，必须更新实际敞口并补齐保护；
- 撤单请求超时进入 `Unknown`，查询完成前不得假设剩余数量已撤销；
- 平仓单部分成交后，保护数量只能随实际剩余敞口减少，不能提前撤掉全部保护；
- 用户流断线时，以 signed query/reconciliation 恢复，禁止用本地推测覆盖交易所事实。

## 7. 成交、账户和持续风险

```text
Exchange User Stream / Query
  -> OrderEvent / FillEvent
  -> Execution 状态迁移
  -> AccountProjection
  -> 实际余额、持仓、保证金和 PnL
  -> Continuous Risk
  -> Continue / Reduce / Cancel / Close / KillSwitch
```

- Account 同时记录 exchange time、observed time 和 source；
- 用户流中断后 Account 进入 stale，依赖该账户的新开仓默认 fail-closed；
- RiskAction 通过 Execution 执行，不直接调用交易所；
- 平仓、减仓和保护单也必须沿用订单状态机、幂等和审计规则。

## 8. 对账与恢复

Reconciliation 周期性比较：

- 内部订单与交易所 open/history orders；
- 内部成交与交易所 fills/trades；
- Account 投影与交易所 balance/position snapshot；
- 保护单计划与交易所实际保护单；
- 实际已成交敞口与有效保护数量；
- 内部 lease、outbox、checkpoint 与实际任务进度。

差异分类至少包含：

```text
MissingInternal
MissingExternal
StateMismatch
QuantityMismatch
PriceMismatch
ProtectionMissing
DuplicateSuspected
StaleProjection
UnknownExternalResult
```

恢复只能通过 owner command：

- Execution command 修复订单和保护单；
- Account command 重建投影；
- Risk command 触发暂停、减仓或 kill switch；
- 无法自动证明安全的差异进入人工处置，不自动猜测。

## 9. 重启恢复

Worker 重启后按顺序处理：

1. 阻止新任务进入；
2. 恢复持有或过期 lease；
3. 重放未发布 outbox；
4. 查询 `SubmitPending`、`Submitted`、`Unknown` 订单；
5. 从交易所快照重建 Account；
6. 恢复行情 checkpoint 并处理缺口；
7. 运行一次 reconciliation；
8. 确认 readiness 后再接收新交易。

不允许仅因为进程启动成功就宣告 Ready。

## 10. Backpressure 与故障策略

- 所有 channel、批次、并发任务和重试次数都有上限；
- 达到容量上限时优先停止接收、合并可合并行情或进入降级，不丢弃订单和成交；
- 外部调用统一使用 timeout、有限重试、退避和 jitter；
- 行情、账户、订单和控制配置分别定义最大可接受陈旧时间；
- 交易路径依赖失效时默认 fail-closed；
- 只读分析、通知和报表可以 fail-open，但必须产生降级指标。

## 10.1 Backtest、Paper 与 Live 的运行一致性

- `ResearchBar`、`PaperEvent` 和 `RecoveryHarness` 是三个不同精度的运行协议，不共用模糊 `backtest/paper` 开关；
- ResearchBar 使用固定 DatasetManifest、RuntimeSnapshot、SimulationProfile、显式 Seed、费用、滑点和资金费，不读取生产当前时间或隐式环境变量；
- ResearchBar 精确复用 Strategy、Portfolio、Risk 和 OrderPlan，但不运行生产 lease、outbox、网络 Unknown 或 Reconciliation；
- PaperEvent 使用 Simulated Exchange 产生 Ack、PartialFill、Reject、Cancel、Protection 和延迟事件，并复用 Execution 纯订单状态迁移；
- RecoveryHarness 以 disposable storage/fault injection 验证 lease、outbox、Unknown、重放、保护缺失和对账恢复，不作为收益证据；
- Research 的 SimulationLedger 不是生产 AccountProjection；模拟 Order/Fill/Account identity 必须携带 SimulationRunId，且不得写生产事实表；
- StrategyEvaluationStateKey 必须包含 EvaluationScopeId、RuntimeSnapshotId 和 MarketStreamPartition，并行 backtest 不得共享可变状态；
- 多币种回测在同一 decision time 先收集全部 Signal，再统一进行 Portfolio/容量分配，不得受 symbol 遍历顺序影响；
- parity 至少比较 StrategySignal、PortfolioTarget、RiskDecision、OrderIntent/OrderPlan；Fill/PnL 只在相同 SimulationProfile 下要求可重放，不宣称与真实交易所相同；
- live 历史窗口不足、数据缺口或 RuntimeSnapshot 不匹配时必须重新预热或 fail-closed，不能通过截断窗口静默改变策略。

## 11. 健康检查

| 检查 | 含义 |
| --- | --- |
| Startup | schema、配置、恢复流程和关键 Adapter 是否初始化完成 |
| Readiness | 当前是否可以安全接收新工作 |
| Liveness | 进程主循环是否仍能推进 |

Liveness 不检查外部交易所短暂可用性，避免依赖波动造成无限重启；Readiness 必须反映行情、账户、配置和执行能力是否满足本 App 的安全条件。

## 12. 优雅关闭

```text
收到关闭信号
  -> 标记 NotReady
  -> 停止接收新任务
  -> 通知子任务取消
  -> 等待在途任务到安全点
  -> 刷新 outbox、checkpoint 和审计记录
  -> 释放 lease
  -> 关闭交易所和数据连接
  -> 刷新 telemetry
  -> 进程退出
```

关闭必须有总超时；超过总超时仍未完成时，记录未完成状态，依赖下次启动恢复，不无限等待。

## 13. 可观测性

每次交易链路至少可按以下字段关联：

- `service.name`、`service.version`、`deployment.environment`；
- `strategy_key`、`strategy_version`、`definition_hash`、`runtime_snapshot_version`；
- `account_id`、`exchange`、`instrument`；
- `correlation_id`、`event_id`、`order_id`、`client_order_id`；
- `risk_policy_version`、`risk_decision`；
- `event_time`、`observed_time`、`processing_latency`。

敏感凭证、Secret、passphrase 和未脱敏请求头禁止进入日志、metric label 和 trace attribute。

## 14. 生产验收

上线前至少验证：

1. 重复 Signal、Order command 和 FillEvent 不产生重复副作用；
2. 外部请求成功但响应丢失时不会重复下单；
3. execution-worker 在订单各状态崩溃后可以恢复；
4. account-worker 用户流断线后停止依赖陈旧持仓开仓；
5. 持续 Risk 运行角色可以触发减仓、撤单和 kill switch；初期可由 account-worker 装配，独立 risk-worker 需另有运行证据；
6. reconciliation-worker 可以识别差异，并通过 owner command 修复可自动恢复的状态；
7. 控制面不可用时数据面按已发布配置安全运行或停止；
8. 优雅关闭不会丢失已接受任务和未发布 outbox；
9. 部分成交、撤单/成交竞态和最大未保护窗口超时不会留下无保护敞口；
10. 回测、paper 与 live 的策略、组合、风险和状态机 parity 成立。
