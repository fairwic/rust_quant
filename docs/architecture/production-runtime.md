# Rust Quant 生产运行与恢复

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-21
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)

## 1. 目标

本规范定义代码在生产中实际如何启动、处理交易、应对重复和故障、恢复未完成状态并安全停止。目录结构只有同时遵守本运行协议，才构成可长期使用的生产架构。

## 2. 运行不变量

1. 数据面只使用已发布、带版本、不可变的配置快照；
2. Strategy、Portfolio 和 Risk 的纯计算路径不执行外部 I/O；
3. 外部 mutation 前，Risk 先持久化不可变审批，Execution 再原子持久化稳定身份、完整计划、`SubmitPending`、幂等和 Outbox；
4. 重试沿用原 `idempotency_key` 和 client order identity；
5. 网络超时表示结果未知，不等于交易所没有执行；
6. Account 只投影外部账户与成交事实；
7. Reconciliation 只发现差异和发出修复命令，不绕过 owner 状态机；
8. 没有有效 RiskDecision 和保护计划不得提交开仓订单；
9. 未有独立 Risk Reservation 协议前，Execution owner 以持久 `AccountOpeningSlot` 保证同一 `execution_account_ref` 只有一个未安全收敛的开仓 OrderIntent；保护、减仓和紧急平仓只在可证明 reduce-only 且先冻结风险增加 claim 时优先。

### 2.1 六角色运行拓扑（Phase 1）

Core 保持同一仓库、同一 runtime image 和同一 `quant_core` owner database，不按策略拆微服务。生产 Compose 的默认长期进程收敛为六个显式组合根：

| 运行角色 | Phase 1 装配职责 | 默认不装配 |
|---|---|---|
| `control-api` | 现有 Core internal HTTP API | 策略循环、行情扫描、执行轮询 |
| `market-worker` | symbol sync、Market Velocity radar、K 线 scanner、最多 2 天的在线缺口修复 | Web 执行 secret、交易 mutation、paper、60 天历史 backfill |
| `signal-worker` | Vegas 与 Vegas Universal 共享行情连接；按启动时精确 config ID 过滤；按 `strategy_key@preset` 装配不可变 Market Velocity handoff lane | Execution worker lane、Market radar |
| `account-worker` | 迁移期的 execution confirmation/fill observation lane | 新订单 claim 与 report replay |
| `execution-worker` | 新订单与风控平仓任务的 claim、lease、门禁和执行 | confirmation 与 report replay |
| `reconciliation-worker` | 迁移期的 execution report replay lane | 新订单 claim 与 confirmation |

`schema-tool`、paper observation、全市场只读成交量观察和大范围历史 backfill 必须通过 profile/短生命周期 Job 显式启动，不计入默认长期拓扑。旧的按策略、preset 和 scheduler 拆分的容器只保留在 `legacy-runtime` profile，供一次性迁移回退，不得与新角色并跑消费同一任务。

Phase 1 只完成运行入口收敛，不代表目标业务边界已经全部迁移：

- `account-worker` 当前复用 legacy confirmation 路径，其中仍可能执行成交后的止盈/止损同步 mutation；在该命令迁回 Execution owner 前，它不是最终的只读 AccountProjection capability boundary；
- `reconciliation-worker` 当前只承接 report replay，不等同于完整的 exchange/internal snapshot 差异检测与恢复编排；
- Market/Signal 外层 critical lane 意外退出会终止进程，但 worker 的 `kill -0` healthcheck 只证明进程存活；radar 内部 legacy detached task、lane freshness、checkpoint 和依赖 readiness 仍需继续显式化；
- Signal 启动必须能从 `strategy_configs` 精确加载每个 `strategy_key@preset`，且配置内 `strategy_slug` 与 lane 一致；缺失或错配时 fail-closed，不回退到另一策略配置。

第一次从 legacy 容器切换到六角色拓扑属于显式运维动作。发布脚本检测到旧容器时必须要求 `DEPLOY_SIX_ROLE_CUTOVER_CONFIRM=replace-legacy-runtime-with-six-roles`，并保存旧服务到镜像的拓扑快照；确认前不得停止旧容器。旧的单次/scheduler live-handoff 都属于待清退运行时，禁止与新 `signal-worker` 并跑。首次切换后的 rollback 在六角色前序镜像不完整时恢复这份旧拓扑，后续发布才使用六角色逐服务 previous image。部署脚本只判定六个进程在稳定窗口内未退出或重启；CI 随后必须执行 `verify_production.sh`，用 revision、非敏感配置、错误日志和 checkpoint 证据做只读运行验收，两者都不等同于尚待补齐的依赖级 readiness。

发布入口必须保持可维护：`promote_stable.sh` 与 `rollback.sh` 只能作为薄入口，六角色清单来自版本化的 `scripts/deploy/runtime-services.txt`，SSH/Compose、安全前置检查、镜像快照、清退和稳定性等待统一由共享部署实现负责。禁止通过 CI Secret 临时改写运行角色，也禁止在两个入口中复制远端安全逻辑。首次 cutover 与 legacy restore 属于迁移期兼容；完成六角色生产验收及约定的回滚窗口后，应按迁移计划删除该分支，而不是永久扩张日常发布路径。

## 3. 进程启动顺序

```text
解析本 App 的强类型配置
  -> 读取最小范围 Secret
  -> 初始化日志、指标和 Trace
  -> 创建必要 Adapter
  -> 校验 schema 与 contract 兼容性
  -> 保持外部 mutation Dispatcher 禁用
  -> 先订阅并缓冲交易所 User Stream
  -> 读取 signed account/order snapshot 或 query watermark
  -> 按 sequence/watermark 合并缓冲事件并补 gap
  -> 恢复 checkpoint、lease、attempt、outbox 和未完成订单
  -> Reconciliation 证明流、快照、订单、账户与保护闭合
  -> 建立/恢复行情流并处理 market checkpoint gap
  -> 完成 startup 检查
  -> 完成 readiness 检查
  -> 开始接收新任务
```

任一步失败时，进程不得进入 Ready。交易所没有可靠 sequence 时，必须重复 signed query/reconciliation 直到能证明订阅与快照之间没有事件缺口；闭合前 Dispatcher 保持禁用。研究、通知等非交易依赖可以按 App 策略降级，但账户新鲜度、交易规格、风险配置和 Execution Adapter 不允许静默降级。

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
  -> Risk owner 以 risk_evaluation_id 持久化不可变 RiskDecision
  -> 构建不可变 OrderIntent + ExecutionPlan + ProtectionPlan
  -> 准备期 readiness 与审批时效检查
  -> Execution owner 原子取得 AccountOpeningSlot
     + 提交 SubmitPending + 完整计划 + 幂等 + Outbox
  -> Dispatcher 执行提交时最终门禁
  -> 以 current fence 条件更新记录 SubmissionAttemptStarted
     + 签发短期 MutationPermit
  -> Fenced Exchange Mutation Gateway 原子消费 current permit
  -> 事务外 Exchange mutation
  -> 持久化确定结果或 Unknown
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
- 由 Risk Use Case 通过 owner Write Port 持久化不可变 `RiskDecision`；`risk_evaluation_id` 绑定 request、PortfolioTarget/PreTradeSnapshot hash、policy version 和 generation，同一 evaluation 重放返回同一决策，新评估使用新 generation；Risk Policy 本身仍是无 I/O 的纯计算；
- Execution 只保存该决策的稳定引用、摘要和批准约束，不写 Risk 私有表；
- 一个批准决策只能唯一绑定一个父 OrderIntent/plan hash；child order 总量不得越过批准边界；
- Execution 准备持久化时先检查审批与允许变化边界，Dispatcher 签发 permit 前基于当前事实再次检查；后一次才是提交时最终门禁。

## 6. 订单状态机与外部 mutation

建议的最小订单状态：

```text
Proposed（仅内存草案）
  -> SubmitPending（首个持久状态）

SubmitPending（尚无 attempt）
  -> Cancelled / Expired / Blocked（仅本地关闭提交任务，不调用交易所）

SubmissionAttemptStarted（attempt 事实，不是 Order 状态）
  -> Acknowledged / PartiallyFilled / Filled / Rejected / Unknown

Acknowledged
  -> PartiallyFilled / Filled / CancelPending
  -> Rejected / Expired / Cancelled（必须有 exchange evidence）

PartiallyFilled
  -> Filled / CancelPending
  -> Cancelled / Expired（保留 cumulative fill 与保护）

CancelPending
  -> Cancelled / PartiallyFilled / Filled / Unknown

Unknown
  -> 按原 identity 查询或对账
  -> Acknowledged / PartiallyFilled / Filled / Cancelled / Rejected / Expired
  -> 原 mutation kind 的 Pending state + 对应新 Outbox
     （仅持久 DefinitivelyAbsent + RecoveryAuthorized 且无可发送 permit 后）
```

规则：

- 状态只允许通过显式 transition 变化；
- `SubmitPending` 只表示完整提交任务已经持久化，不表示已经开始网络请求；
- `SubmitPending` 尚无 attempt 时收到取消，只有 CAS 同时满足 `expected_aggregate_version`、空 `send_claim` 和无未完成 attempt/permit，才能在 Execution 事务中进入 `Cancelled`；attempt claim 原子设置同一 `send_claim`/version，谁先提交谁获胜；
- `Acknowledged` 只由交易所同步响应、signed Query 或 User Stream 对原 client order identity 的明确接受证据推进，不能由 Outbox 已发布、socket write 或 attempt 记录推断；
- `Rejected`、`Expired`、`Blocked` 分别表示交易所拒绝、时间/交易所过期、本地不可恢复门禁失败，并保存 terminal source/reason/evidence；
- Execution owner 的一次原子事务必须覆盖 `AccountOpeningSlot` claim、Inbox/幂等与唯一身份、`RiskDecision` 引用及对父 OrderIntent/plan hash 的唯一绑定、`OrderIntent`、`SubmitPending`、完整 `ExecutionPlan`、初始为 `Planned` 的 `ProtectionPlan`、提交 Outbox 和审计字段；
- 只有该事务提交后才可确认上游或发布 Outbox；交易所 I/O 只能由 Fenced Exchange Mutation Gateway 在 Dispatcher 的 claim/attempt/permit 短事务提交后发起，数据库事务不得跨越网络调用；
- Dispatcher 必须执行提交时最终门禁并持久化所用 snapshot/capability/generation 引用与 checked time；超时进入 `Expired`，不可恢复失败进入 `Blocked`，可恢复 blocker 保持 `SubmitPending`，并在确认当前 delivery 的同一事务 rollover 到新 mutation generation 的 delayed Outbox 或 `MutationRetrySchedule`，由 durable scheduler/event 唤醒，禁止忙循环；
- 外呼前在短事务中以 `expected_aggregate_version`、原 mutation kind 对应的 expected Pending state（Submit 为 `SubmitPending`、Cancel 为 `CancelPending`、Protect 为原 `AttachedPending`/`PostFillPending`）、空 `send_claim` 与 current account/order fence 为条件原子设置 claim、记录 `ExecutionMutationAttempt(Started)` 并签发短期 `MutationPermit(Issued)`；
- Submit/Cancel/Protect mutation event 必须携带 `mutation_event_id`（等于 Envelope `event_id`）、`mutation_generation` 和 `expected_aggregate_version`；claim/attempt/permit 绑定三者，旧或重复 delivery 与 current generation/version 不匹配时只 ack/no-op；
- Dispatcher 只能把 permit 与固定 payload 交给 Fenced Gateway。Gateway 在真正网络 I/O 边界前原子校验 attempt/version/fence/gate generation/payload hash/expiry 并将 current permit 置为 `Consumed`；revoked/stale/expired/CAS 失败返回 `DefinitelyNotSent`，不得调用 SDK；raw SDK mutation capability/credential 对 Dispatcher 与其他 App 物理不可达；
- Attempt 至少记录 mutation kind、stable mutation identity、attempt number、`mutation_event_id`/`mutation_generation`/`expected_aggregate_version`、payload/plan hash、fence、gate evidence、Started/Confirmed/Indeterminate/DefinitelyNotSent/DefinitivelyAbsent 和 started/completed time；Permit 至少记录 attempt、上述三个 mutation 授权字段、fence/gate generation/payload hash/expiry 与 Issued/Consumed/Revoked/Expired。outcome、Order/Protection transition、permit 终态与后续 Outbox 在同一 Execution 事务提交；Unknown outcome 只允许 query/reconciliation/alert 等恢复 Outbox，不得直接创建同 kind 的 mutation Outbox；
- 任一 Submit/Cancel/Protect 路径确认当前 delivery 或推进 aggregate version 后仍需重试时，必须在同一 owner transaction supersede 当前本地 mutation generation、递增 generation，并写入带新 event/generation/version 的 delayed Outbox 或 durable `MutationRetrySchedule`；schedule 到期只能通过 owner transaction 幂等物化 Outbox，不能直接 claim；该规则也适用于可重试 `DefinitelyNotSent`；
- 外部请求重试不得创建新的业务订单；
- 只有 DefinitivelyAbsent 且不存在仍可发送的 permit 时，recovery transaction 才能原子关闭旧 attempt、revoke 未消费 permit 或确认 Gateway `DefinitelyNotSent`、持久化 RecoveryAuthorized、清除旧 send claim、supersede 旧 mutation generation 的本地授权/投递记录、推进 version，并恢复原 kind 的 Pending state。随后同一事务按 `Submit -> OrderSubmissionRequestedV1`、`Cancel -> OrderCancelRequestedV1`、`Protect -> ProtectionSubmissionRequestedV1` 写对应新 Outbox；新事件只滚动 `mutation_event_id`、`mutation_generation`、`expected_aggregate_version` 和 attempt number，保持原 mutation kind、stable mutation identity、目标 Order/Protection identity 与 payload/plan hash。旧 generation 的 Broker 重投只 ack/no-op；已 Consumed 且没有终态 Gateway 结果时保持 Unknown；单次 not-found、旧 Outbox 或内存扫描都不足以授权/唤醒重发；
- 交易所/订单类型若没有稳定 client identity 的 duplicate rejection，以及 signed query/可证明缺席能力，则 live mutation 必须标记为 `Unsupported`；
- `SubmissionAttemptStarted` 后崩溃或外呼结果不明时进入 `Unknown`，禁止盲目再次提交，必须先按原 identity 查询或对账；
- `MutationPermit(Issued)` 存在时，取消与 Gateway consume 竞争同一 permit CAS；revoke 先成功则在同一事务进入本地 `Cancelled`，迟到 Gateway 返回 DefinitelyNotSent。permit 已 Consumed 时只持久化 `cancel_requested`：证明订单存在后按稳定 cancel identity 进入 `CancelPending`；只有 DefinitivelyAbsent 且无可发送 permit 才可本地 `Cancelled`；仍不确定时保持 `Unknown` 并持久调度查询/人工升级；
- Exchange evidence 可让 Acknowledged/PartiallyFilled 直接 Rejected、Expired 或 Cancelled；迟到 Fill 不要求终态回退，但必须更新 cumulative fill、AccountProjection 和 Protection；
- 审批/计划已经 `Expired` 时不得原地复活，重新评估必须创建新的审批和订单 identity；
- 成交与撤单事件可重复投递，consumer 必须幂等。

`AccountOpeningSlot` 在 Order 进入表面终态时不自动释放。必须同时证明：全部 child mutation 结果确定、没有未完成 attempt/Unknown/可消费 permit、Account owner 的 typed watermark 已覆盖最终 cumulative fill，且剩余敞口已按政策保护；permit 从未被消费或 Gateway 已持久证明 DefinitelyNotSent 的 Cancelled/Expired/Blocked 才可以直接释放。

订单、撤单和保护单 mutation 的完整权威顺序见 [ADR-0006](adr/0006-at-least-once-idempotency-and-recovery.md)，本文不得定义不同顺序。

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
- 保护/减仓/紧急平仓旁路必须由 Gateway 证明 reduce-only 或等价不增加绝对敞口；数量不得超过当前可证明敞口；
- KillSwitch/紧急平仓先通过 typed Execution command 冻结新风险增加 claim、推进 account gate generation，再查询/取消或接管已有开仓订单；迟到 Fill 和保护完成对账前不得恢复新开仓。

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
2. 暂停外部 mutation Dispatcher，恢复持有或过期 lease/fence；
3. 先订阅并缓冲交易所 User Stream；
4. 读取 signed order/account snapshot 或 query watermark，按 sequence 合并缓冲事件并补 gap；没有可靠 sequence 时重复 query/reconciliation 直到闭合；
5. 查询 `SubmitPending`、`Acknowledged`、`PartiallyFilled`、`CancelPending`、`Unknown`、AccountOpeningSlot、attempt ledger 与 permit ledger；
6. 将“无 attempt 的 SubmitPending”与“存在未完成 attempt”分开：前者等待首次 fenced claim，后者进入/保持 `Unknown` 并先按原 identity 查询；
7. 用闭合后的交易所事实重建 AccountProjection，并恢复行情 checkpoint/缺口；
8. 运行 reconciliation，核对 opening slot、cumulative fill、Account watermark 与 Protection；
9. 恢复允许投递的 outbox，确认 readiness 后启用 Dispatcher；
10. 最后才接收新交易。

不允许仅因为进程启动成功就宣告 Ready，也不允许在流/快照 gap、attempt 和对账闭合前盲目重放外部 mutation Outbox。

## 10. Backpressure 与故障策略

- 所有 channel、批次、并发任务和重试次数都有上限；
- 达到容量上限时优先停止接收、合并可合并行情或进入降级，不丢弃订单和成交；
- 外部调用统一使用 timeout、有限重试、退避和 jitter；
- 外部 mutation 的可恢复门禁失败使用持久 `next_eligible_at`/事件触发重新唤醒，不使用 broker nack 或进程内循环制造热重试；
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
- `risk_evaluation_id`、`risk_decision_id`、`risk_policy_version`；
- `account_opening_slot_id/generation`、`mutation_id/kind`、`attempt_no`、`fence`、`gate_checked_at`；
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
10. 回测、paper 与 live 的策略、组合、风险和状态机 parity 成立；
11. 两个 staging worker 对同账户并发开仓时只有一个取得 opening slot，slot 不早于 Account watermark/保护闭合释放；
12. cancel/RecoveryAuthorized revoke 与 Gateway permit consume 的竞态只允许一方成功；旧 Dispatcher 携带 revoked/stale permit 时 Gateway 不调用 SDK，Consumed 且未知时不得本地终结；
13. Unknown outcome 不直接生成同 kind mutation Outbox；只有 DefinitivelyAbsent/RecoveryAuthorized 且无可发送 permit 时，recovery transaction 才 supersede 旧 generation，并保持原 mutation/目标 identity、按 Submit/Cancel/Protect kind 写入绑定新 `mutation_event_id`/`mutation_generation`/`expected_aggregate_version` 的对应 Outbox；旧 delivery ack/no-op，不支持缺席证明的能力不能 live；
14. Submit/Cancel/Protect 的 transient blocker、可重试 DefinitelyNotSent 或 fence/gate 变化确认当前 delivery 后，都原子 rollover 到新 generation 的 delayed Outbox/RetrySchedule；Scheduler 不能复用旧 delivery 或直接 claim；
15. User Stream/snapshot bootstrap 窗口注入 Fill/Cancel 不丢事件，闭合前 NotReady 且 Dispatcher off。
