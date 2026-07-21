# ADR-0006：采用至少一次交付、幂等订单、保护闭环与显式恢复

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-21
- 决策者：Rust Quant Core

## 背景

真实交易所 mutation 跨越数据库、进程、网络和外部系统。请求可能已成功，但响应因超时或连接中断丢失；消息也可能重复或乱序投递。开仓单还会部分成交，撤单与成交可能竞态，保护单可能提交失败或数量不足。

因此系统不能依靠“只发送一次”，不能把超时直接解释为失败，也不能把“计划挂止损”当成“实际敞口已经受保护”。

## 决策

### 至少一次交付

内部 Command/Event 使用至少一次交付。Producer、Consumer 和下游调用共同保持稳定 `idempotency_key`，重复处理不得产生重复业务副作用。

统一 Envelope 至少包含：

- `event_id`、`event_type`、`schema_version`；
- `correlation_id`、`causation_id`；
- `idempotency_key`、`aggregate_id`、`sequence`；
- `occurred_at`、`ingested_at`；
- `partition_key`、`trace_context`。

### 外部 Mutation 前的唯一持久化顺序

本节是订单、撤单和保护单外部 mutation 顺序的唯一权威。其他文档和模板只能引用或同步本节，不得定义另一套先后关系。

Risk 与 Execution 保持 owner 边界：

1. Risk Policy 使用冻结的 `PreTradeSnapshot` 纯计算 `RiskDecision`，Risk Use Case 再通过 owner Write Port 持久化该不可变决策；Reject 到此结束，不创建订单；
2. 每次评估具有稳定 `risk_evaluation_id`，至少绑定 `execution_request_id`、`PortfolioTarget`/`PreTradeSnapshot` hash、risk policy version 和 evaluation generation；同一 evaluation 重放返回同一 `RiskDecision`，重新评估必须使用新的 generation；
3. Approve/Resize 提供稳定 `risk_decision_id`、摘要、批准边界和过期时间，并绑定审批主题、账户、instrument、方向和批准的总数量/风险边界；它不是可重复套用到其他订单的通用许可；
4. 一个 Approve/Resize `risk_decision_id` 只能绑定一个父 `OrderIntent` 和一个 immutable plan hash；多个 child order 只能属于该固定计划且总量不得越过审批边界；
5. Execution 只能保存上述不可变引用和审批证据，不得在 Execution 事务中写 Risk 私有表；
6. Risk 与 Execution 不建立跨 owner 数据库事务，跨 owner 后续状态通过 typed command/event、幂等和对账收敛。

Risk 事务已提交但 Execution 准备失败时，允许留下未使用且可审计的 `RiskDecision`；它由幂等重放继续使用或按期限失效，不通过跨 owner 回滚删除。

Execution 按以下顺序准备和提交订单：

1. 校验有效的 Approve/Resize，并依据原 `execution_request_id`/幂等身份生成或复用稳定 `OrderIntentId`、`OrderId` 与 `client_order_id`；同一命令重放必须解析到同一组 identity；
2. 基于冻结输入构建不可变 `OrderIntent`、`ExecutionPlan` 和 `ProtectionPlan`，完成数量精度、交易所能力和保护可执行性校验；
3. 完成准备期 readiness 与审批时效检查；此时仍不得调用交易所；
4. 调用一个 business-named Execution Write Port，在单一 Execution owner 事务中原子写入：
   - Inbox/幂等记录和唯一业务键；
   - 通过 account gate row CAS 或等价活跃唯一约束取得持久 `AccountOpeningSlot`；
   - `RiskDecision` 引用、摘要、批准边界和过期时间；
   - `risk_decision_id -> parent OrderIntentId + plan hash` 唯一绑定；
   - `OrderIntent`、当前可执行 child Order 和首个持久状态 `SubmitPending`；
   - 完整不可变 `ExecutionPlan`；
   - `ProtectionPlan` 及初始 `Planned` 状态；
   - 携带 `mutation_generation=1`、`expected_aggregate_version` 的 `OrderSubmissionRequestedV1` Outbox；
   - correlation、causation 和必要审计字段；
5. 数据库事务提交后，才允许确认上游消费或发布 Outbox；提交只让任务具备投递资格，交易所 I/O 仍只能由后续 Dispatcher 与 Fenced Exchange Mutation Gateway 按第 6～10 步发起，准备 Use Case、Write Adapter 和上游调用方都不得直接外呼；事务回滚时不得存在可发布的提交任务；
6. Dispatcher 消费 Outbox，执行提交时最终门禁，取得并复核当前 account/order lease/fence、审批期限、账户/行情允许漂移、凭证、instrument、release/kill-switch generation 与保护能力；mutation 事件必须携带自身 `mutation_event_id`（等于 Envelope `event_id`）、`mutation_generation` 和 `expected_aggregate_version`，不得原地重算或静默改写计划；
7. 外呼前在一个短事务中，以 `mutation_event_id`、`mutation_generation`、`expected_aggregate_version`、expected Pending state、空 `send_claim` 和 current account/order fence 为条件更新；成功时原子设置 `send_claim`、记录 `ExecutionMutationAttempt(Started)` 并签发短期 `MutationPermit(Issued)`，attempt/permit 同时绑定这三个 mutation 授权字段。取消与 submit attempt claim 竞争同一 version/claim；旧或重复 delivery 不匹配 current generation/version 时只能幂等 ack/no-op；记录失败、条件更新失败或事务回滚时一律禁止继续，数据库事务不得跨越网络 I/O；
8. Dispatcher 只能把 permit 和固定 payload 交给 `FencedExchangeMutationGateway`；该 Gateway 是唯一持有 raw SDK mutation capability/credential 的组件。Gateway 在真正网络 I/O 边界前，通过 permit authority 的短事务原子校验并消费 current permit：`attempt_id`、aggregate version、account/order fence、gate generation、payload hash、expiry 和 `Issued` 状态必须全部匹配；
9. permit 为 revoked、stale、expired 或 CAS 失败时，Gateway 返回 `DefinitelyNotSent` 且不得触达 SDK；只有 `MutationPermit(Consumed)` 事务提交成功后，Gateway 才能在事务外调用 raw SDK。permit 一旦 Consumed，即使还没有 socket/Ack 证据，也必须按“可能已发送”处理；
10. 根据明确 Ack/Reject、Gateway `DefinitelyNotSent`、signed Query、User Stream 或 Reconciliation，在单一 Execution 事务中一起完成 attempt outcome、Order/Protection transition、permit 终态和后续 Outbox；
11. 只有确定结果或 `Unknown` 等可恢复状态已经持久化后，才能确认 Dispatcher delivery。

“固定计划”表示该计划不可变并与 `SubmitPending`、幂等和 Outbox 一起持久化，不是只存在于进程内存。`OrderSubmissionRequestedV1` 只表示持久提交任务，不证明请求已经发送或交易所已经受理。

`ExecutionMutationAttempt` 是可恢复事实，至少包含 `mutation_kind(Submit/Cancel/Protect)`、stable mutation id、attempt number、Order/Protection identity、`mutation_event_id`、`mutation_generation`、`expected_aggregate_version`、payload/plan hash、account/order fence、`Started/Confirmed/Indeterminate/DefinitelyNotSent/DefinitivelyAbsent`、started/completed time。与之绑定的 `MutationPermit` 至少包含 permit id、attempt id、上述三个 mutation 授权字段、account/order fence、gate generation、payload hash、`expires_at` 和 `Issued/Consumed/Revoked/Expired`；数据库唯一约束必须保证同一 mutation attempt 只有一个 permit，且同一 aggregate version/mutation kind 只有一个 current `Issued/Consumed` permit。最终门禁使用的 account/market snapshot ref、credential/instrument capability ref、release/kill-switch generation 与 `checked_at` 也必须持久化。只保存引用、版本或 hash，不保存明文凭证。

attempt outcome 为 `Indeterminate`/mutation 进入 `Unknown` 时，outcome transaction 只允许写 query、reconciliation、人工告警或等价 durable recovery Outbox，禁止直接重建同 `mutation_kind` 的 mutation Outbox。新的同类 mutation Outbox 只能由下述 `DefinitivelyAbsent + RecoveryAuthorized` recovery transaction 创建。

mutation kind 与事件类型固定映射：

- `Submit -> OrderSubmissionRequestedV1`；
- `Cancel -> OrderCancelRequestedV1`；
- `Protect -> ProtectionSubmissionRequestedV1`。

恢复不得改变原 `mutation_kind`、stable mutation identity、目标 Order/Protection identity 或 payload/plan hash，只允许滚动 `mutation_event_id`、`mutation_generation`、`expected_aggregate_version` 和 attempt number。

Submit、Cancel、Protect 统一遵守 mutation delivery rollover：凡 owner transaction 已确认/终结当前 delivery，或推进 aggregate version 使当前 `expected_aggregate_version` 失效，但业务仍需未来重试同一 mutation，必须在同一事务 supersede 当前 mutation generation 的本地授权/投递记录、递增 `mutation_generation`，并持久化唯一的后续授权：

- 可以直接写带新 `mutation_event_id`、新 generation、事务后 `expected_aggregate_version` 和 `available_at` 的 mutation Outbox；
- 或写带同样三字段、`next_eligible_at`/wake condition 的 durable `MutationRetrySchedule`，到期/唤醒后只能由 Execution owner transaction 幂等物化一条对应 Outbox；Scheduler 不得直接 claim mutation。

旧 Broker delivery 与 current generation/version 不匹配时只能 ack/no-op，不能读取 current version 后继续执行。该规则覆盖 transient final-gate blocker、可重试的 `DefinitelyNotSent`、lease/fence/gate generation 变化和 Unknown recovery；Cancelled/Expired/Blocked 等不再重试的终态不得创建新 mutation 授权。

最终门禁失败必须在 Execution 事务中分类并记录证据：审批/计划超时进入 `Expired`；对本计划不可恢复的本地失败进入 `Blocked`；可恢复失败保留 `SubmitPending`，并按上述 rollover 规则在确认当前 delivery 的同一事务写入 blocker 和 delayed Outbox，或写入带 `next_eligible_at`/明确唤醒条件的 `MutationRetrySchedule`。随后只由 durable scheduler/event 经 owner transaction 物化新 Outbox；禁止 nack 热循环、复用旧事件、Scheduler 直接 claim，也禁止 ack 后没有持久唤醒条件。

撤单和保护单 mutation 遵守同一协议：先以稳定 mutation identity 将本地 `CancelPending` / `AttachedPending` / `PostFillPending`、幂等、计划和 Outbox 原子提交，再以各自 expected Pending state/version/fence claim attempt、签发 permit，并由 Fenced Gateway 消费 permit 后在事务外调用交易所；不允许先调用 SDK、成功后再补状态。

### 同账户并发开仓的范围边界

本 ADR 解决外部 mutation 的持久化、幂等和恢复，不把 `RiskDecision` 偷换成账户级风险容量预留。未有单独接受的 Risk Reservation ADR 前，`stage_order_submission_with_outbox` 必须在同一 Execution 事务中取得 `execution_account_ref` 的持久 `AccountOpeningSlot`；worker lease 只表示处理权，不能替代该业务唯一约束。

活跃槽位至少覆盖 `SubmitPending`、`Acknowledged`、`PartiallyFilled`、`CancelPending`、`Unknown`，以及虽然 Order 已终结但成交尚未完成 AccountProjection/保护确认的窗口。只有全部 child mutation 结果确定、没有未完成 attempt/Unknown/可消费 permit、Account owner 的 typed watermark 已覆盖最终 cumulative fill，且剩余敞口保护满足政策后，Execution 才能释放槽位；permit 从未被消费或 Gateway 已持久证明 `DefinitelyNotSent` 的本地 Cancelled/Blocked/Expired 才可以直接释放。

同一已批准 `ExecutionPlan` 可以包含受总审批边界约束的确定性 child order，但不能并发创建多个彼此独立的开仓意图。若产品需要同账户并发独立开仓，必须先定义 Risk Reservation owner、Held/Bound/Released 生命周期、过期/崩溃恢复、Fill/Cancel/Unknown 释放规则和并发测试；在此之前不得仅凭多个基于相同 AccountSnapshot 的 Approve 决策启用并发 live mutation。

保护、减仓和紧急平仓可以绕过开仓槽位，但 Gateway 必须证明 `reduce-only` 或等价语义，数量上限不得增加绝对敞口。KillSwitch/紧急平仓必须先通过 typed Execution command 冻结该账户新的风险增加 claim、推进 gate generation，并查询/取消或接管已有风险增加订单；在这些订单和迟到 Fill 完成对账前，账户保持禁止新开仓。无法证明 reduce-only 的交易所/订单类型不得启用该旁路。

### 订单状态

最小状态包含：

```text
Proposed                    # 仅内存草案，不是可恢复状态
SubmitPending               # 首个持久状态；完整计划、幂等和提交 Outbox 已提交
Acknowledged
PartiallyFilled
Filled
CancelPending
Cancelled
Rejected
Expired
Blocked
Unknown
```

`SubmitPending` 只表示订单已持久排队并具备被 Dispatcher 尝试提交的资格，不表示网络请求已经开始。它可以携带可重试的本地 blocker，但审批或计划过期后必须进入 `Expired`。

`Acknowledged` 表示交易所的同步响应、signed Query 或 User Stream 已明确证明原 client order identity 被接受；不能由 Outbox 已发布、socket write 或 `SubmissionAttemptStarted` 推断。交易所首次确定证据也可能直接推进到 `PartiallyFilled`、`Filled`、`Rejected` 或 `Expired`。

`Rejected` 只表示交易所拒绝证据，`Expired` 表示本地审批/计划超时或交易所过期证据，`Blocked` 表示提交前不可恢复的本地门禁失败；三者必须保存 `terminal_source`、reason 和 evidence，不能互相代用。

`Unknown` 表示请求可能已经到达交易所，但外部结果无法确认；`SubmissionAttemptStarted` 后进程崩溃而没有确定结果也按 `Unknown` 恢复。必须先按原 client order identity 查询或对账，禁止生成新身份盲目重试。

只有 Exchange Adapter 根据已声明能力，在规定可见性窗口后以 signed query/reconciliation 形成 `DefinitivelyAbsent` 证据，且 Execution 能证明不存在仍可首次发送的 permit，才允许恢复。Execution recovery transaction 必须原子关闭旧 attempt、撤销尚未消费的 permit 或确认 Gateway 已持久化 `DefinitelyNotSent`、持久化 `RecoveryAuthorized`、清除旧 `send_claim`、supersede 旧 mutation generation 的本地授权/投递记录、推进 aggregate version，并恢复到原 mutation kind 对应的 Pending state：Submit 为 `SubmitPending`，Cancel 为 `CancelPending`，Protect 为原 `AttachedPending`/`PostFillPending`。同一事务按上述固定映射写入对应的新 mutation Outbox。新事件使用新的 `mutation_event_id`，递增 `mutation_generation`，携带恢复后的 `expected_aggregate_version`，并保持原 mutation kind、stable mutation identity、目标 identity 与 payload/plan hash；下一次 claim/attempt/permit 必须绑定这三个授权字段且只增加 attempt number。旧 generation 的 Broker 重投只能幂等 ack/no-op，不能读取 current version 后代替新事件 claim。已 `Consumed` 且没有终态 Gateway 结果的 permit 不能被事后撤销；即使交易所当前 not-found，也必须保持 `Unknown`，禁止本地终结或重发。单次 not-found 不是证明，也不得依赖已确认的旧 Outbox、内存扫描或偶然重启再次唤醒。

若交易所/订单类型不能提供稳定 client identity 的 duplicate rejection，以及查询原身份或可证明缺席的能力，则 live mutation 必须标记为 `Unsupported`。

本地审批/计划已经 `Expired` 的订单不得原地复活；重新评估必须产生新的 `RiskDecision` 和新的订单 identity，并保留旧链路。

恢复时必须区分两个窗口：`SubmitPending` 且没有 attempt 的任务从未签发 permit，在当前门禁通过后可以参与首次 claim；存在未完成 `SubmissionAttemptStarted` 的任务必须先转入/按 `Unknown` 查询原 identity。禁止仅凭 Outbox 尚未确认就把两者都重新发送。

`SubmitPending` 尚无 attempt 时收到取消，取消事务只有在同一 aggregate version 上满足 `SubmitPending + send_claim 为空 + 无未完成 attempt/permit` 才能转为 `Cancelled`；attempt claim 与取消谁先提交谁获胜。即使原提交 Outbox 已发布，Dispatcher 的 claim 也必须幂等 no-op，此时禁止发送交易所下单或 cancel。

存在 `MutationPermit(Issued)` 时，提交前取消与 Gateway consume 必须竞争同一 permit CAS：取消先提交则在同一事务原子 revoke permit 并进入本地 `Cancelled`，迟到 Gateway 必须返回 `DefinitelyNotSent`；consume 先提交则 permit 已代表“可能发送”，取消只能持久化 `cancel_requested` 并按 `Unknown` 查询原订单。证明订单存在后，以稳定 cancel identity 进入 `CancelPending`；只有同时形成 `DefinitivelyAbsent` 且不存在可发送 permit 时，才可本地 `Cancelled` 且原提交永不重发；仍不确定则保持 `Unknown` 并升级人工或按 durable query schedule 继续核对。raw SDK mutation 对 Dispatcher 和其他 App 必须物理不可达，否则 permit fence 只是约定而不是安全边界。

### 保护状态

最小状态包含：

```text
Planned
AttachedPending
PostFillPending
PartiallyProtected
Active
Unknown
Failed
Recovering
Closed
ManualIntervention
```

保护规则：

- 开仓前必须有经过交易所能力校验的 ProtectionPlan；
- 优先使用原生 attached stop；不能等价映射时 Adapter 返回 Unsupported；
- 只能成交后挂保护时，Plan 固定最大未保护窗口、重试上限和失败 RiskAction；
- 每次部分成交后，立即按实际净敞口计算和提交保护数量；
- `protected_quantity` 小于 `open_exposure` 时必须显示 `PartiallyProtected`；
- 超过最大未保护窗口后停止该账户新开仓，并触发 Reduce、Close 或 KillSwitch；
- 平仓部分成交时，只能按剩余敞口调整保护，不能提前撤销全部保护。

### 撤单与成交竞态

- Cancel Ack 不能证明此前未产生 Fill；
- 带 exchange evidence 的事件可以让 `Acknowledged` 直接进入 `Rejected`、`Expired` 或 `Cancelled`，也可以让 `PartiallyFilled` 的剩余量进入 `Cancelled` 或 `Expired`；必须保留 cumulative fill；
- Cancelled 后收到合法 Fill 仍须更新订单、账户和保护；
- `PartiallyFilled` 的未成交余量允许进入 `CancelPending`，但已成交敞口继续保留并补齐保护；
- 一次查询暂时未找到 Unknown 订单不能证明请求从未到达；恢复策略必须考虑交易所查询可见性窗口，再决定沿用原 identity 重试或人工处置；
- 撤单超时进入 Unknown，查询完成前不释放全部风险占用；
- Event 按 exchange sequence 或明确替代规则幂等合并；
- User Stream 断线后以 signed query 与 Reconciliation 恢复。

### 启动恢复

进程启动时：

- 暂停外部 mutation Dispatcher，恢复过期 Lease/fence；
- 按 exchange-specific bootstrap 先订阅并缓冲 User Stream，再读取 signed order/account snapshot 或 query watermark；
- 按 exchange sequence/watermark 合并缓冲事件并补 gap；无可靠 sequence 时，重复 signed query/reconciliation，直到可以证明 snapshot 与流之间闭合；
- 查询 SubmitPending、Acknowledged、PartiallyFilled、CancelPending、Unknown 与未完成 attempt；
- 将无 attempt 的 SubmitPending 与未完成 attempt 分开恢复，后者先按原 identity 查询或对账；
- 幂等处理重复 Ack、Fill、Cancel 和 Reject；
- 重建 AccountProjection；
- 核对实际敞口与有效保护数量；
- 只有 User Stream/snapshot gap、attempt、AccountProjection 与保护对账全部闭合后，才恢复允许投递的 Outbox、启用 Dispatcher 和进入 Ready。

### Reconciliation

Reconciliation 比较内部订单、成交、持仓、风险占用和保护单与交易所事实。可以自动证明安全的差异通过 typed owner command 修复；无法证明安全的差异进入阻塞和人工处置。Reconciliation 不直接修改 owner 表。

## 结果

### 正面影响

- 网络超时和重复消息不会直接产生重复订单；
- Worker 可以在订单与保护各状态重启续跑；
- 部分成交后保护与真实敞口同步；
- 撤单/成交竞态不会静默留下裸露仓位；
- Order、Fill、Account、Protection 和处置证据具有统一因果链；
- 交易所最终事实可以纠正内部投影。

### 代价

- 需要幂等表、Inbox/Outbox、订单与保护状态机；
- Unknown、PartiallyProtected 等中间态增加实现与运营复杂度；
- 在 Risk Reservation ADR 完成前，同账户 opening slot 会限制并发开仓吞吐；
- 缺少稳定 client identity、signed query/缺席证明或可靠恢复 bootstrap 的交易所能力不能直接启用 live；
- 需要交易所能力矩阵和最大未保护窗口政策；
- Recovery 与 Reconciliation 测试成为发布门禁。

## 被否决的方案

### 宣称全局 exactly-once

数据库与交易所不共享同一事务边界，内部消息不能覆盖所有外部副作用。

### 网络失败后生成新订单 ID

原请求可能已成功，会形成重复订单。

### 只依赖 Broker 去重

不能覆盖数据库提交、外部调用、消费确认和交易所执行之间的失败窗口。

### 用 Worker Lease 代替账户开仓唯一约束

Lease 只证明当前处理者，不能阻止两个 staging 事务各自创建开仓事实；业务串行必须由持久 opening slot/CAS 或活跃唯一约束保证。

### 单次查询 Not Found 后重发

交易所查询可能最终一致；没有 DefinitivelyAbsent 能力证据时重发会产生重复订单。

### 等全部成交后再挂保护

部分成交期间会形成未保护敞口，不符合实盘安全底线。

### 把保护计划当成保护成功

计划、请求、Ack 和实际有效数量是不同事实，必须分别跟踪。

### Reconciliation 直接修改订单表

绕过 Execution 状态机并破坏 owner、不变量和审计。

## 验证

- 相同 Order command 重放不会产生第二笔订单；
- 同一 `risk_evaluation_id` 重放返回同一决策，新 generation 产生新决策；一个批准决策不能绑定两个父 OrderIntent；
- `AccountOpeningSlot`、`RiskDecision` 引用/摘要/批准边界、`ExecutionPlan`、`ProtectionPlan`、`SubmitPending`、幂等和提交 Outbox 的原子事务任一写入失败时整体不可见；
- 两个 worker 并发 staging 同一账户的独立开仓时，最多一个能取得持久 opening slot；
- 事务提交前不会发布 Outbox 或调用交易所，提交后发布前崩溃可以从 Outbox 重放；
- Dispatcher 门禁失败不会签发 permit，也不会调用交易所或静默生成不同数量、价格、保护方案、client identity；
- 两个 Dispatcher 竞争同一提交任务时，只有 current fence 条件更新成功者能记录 attempt 并签发唯一 current permit；真正发送仍要求 Fenced Gateway 消费该 permit；
- 取消和 attempt claim 并发时，只有同一 aggregate version/send claim 条件的一个事务成功；
- 旧 Dispatcher 在 permit 被取消/恢复事务 revoke 后醒来时，Fenced Gateway 返回 DefinitelyNotSent 且 raw SDK 未被调用；Gateway consume 与 revoke 竞态只能有一方成功；
- permit 已 Consumed 但结果未知时，取消和恢复都不能本地终结或重发；
- Unknown outcome 只产生 query/reconciliation/alert 等恢复任务，不直接产生同 mutation kind 的新 Outbox；
- `SubmissionAttemptStarted` 的短事务未提交时不会外呼；提交后崩溃会进入 `Unknown` 并先查询，不直接重发；
- 单次 not-found 不会授权 Unknown 重发；只有持久 `DefinitivelyAbsent + RecoveryAuthorized` 且没有可发送 permit 时，恢复事务才会 supersede 旧 generation，并按原 Submit/Cancel/Protect kind 写入带新 `mutation_event_id`/`mutation_generation`/`expected_aggregate_version` 的对应 Outbox，沿原 mutation/目标 identity 创建下一 attempt；旧 Broker delivery 只能 ack/no-op；
- transient blocker、可重试 DefinitelyNotSent 和 lease/fence/gate 变化确认当前 delivery 后，会原子 rollover 到新 mutation generation 的 Outbox/RetrySchedule；Scheduler 不能复用旧事件或直接 claim；
- 可恢复门禁失败会持久化 `next_eligible_at`/唤醒条件并确认本次 delivery，不产生忙循环或丢失任务；
- 请求成功但响应丢失时可以查询并恢复；
- 每个订单与保护状态崩溃后都可续跑；
- FillEvent 重复和乱序不会重复增加仓位；
- 部分成交后保护数量在规定窗口内覆盖真实敞口；
- Cancelled 后迟到 Fill 会重新计算保护；
- Outbox 发布前后崩溃均不会丢失业务事件；
- startup 在订阅/快照 bootstrap 之间注入 Fill/Cancel 时不会遗漏事件，闭合前 Dispatcher 保持禁用；
- Reconciliation 可以识别 Missing、Mismatch、Duplicate、Unknown 和 ProtectionMissing；
- 无法证明安全时系统 fail-closed 并留下人工处置证据。
