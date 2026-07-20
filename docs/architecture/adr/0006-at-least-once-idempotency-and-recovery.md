# ADR-0006：采用至少一次交付、幂等订单、保护闭环与显式恢复

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
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

### 外部 Mutation 前的持久化

1. 生成稳定业务 identity 与 client order identity；
2. 固定 RiskDecision、ExecutionPlan 和 ProtectionPlan；
3. 在同一事务写入 OrderIntent/Order 初始状态、幂等记录和 Outbox；
4. 异步提交交易所；
5. 根据 Ack、Query、User Stream 或 Reconciliation 推进状态；
6. 只有业务状态可恢复后才能确认消费。

### 订单状态

最小状态包含：

```text
Proposed
Persisted
SubmitPending
Submitted
Acknowledged
PartiallyFilled
Filled
CancelPending
Cancelled
Rejected
Unknown
```

`Unknown` 表示外部结果无法确认。必须先按 client order identity 查询或对账，禁止生成新身份盲目重试。

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
- Cancelled 后收到合法 Fill 仍须更新订单、账户和保护；
- 撤单超时进入 Unknown，查询完成前不释放全部风险占用；
- Event 按 exchange sequence 或明确替代规则幂等合并；
- User Stream 断线后以 signed query 与 Reconciliation 恢复。

### 启动恢复

进程启动时：

- 重放未发布 Outbox；
- 恢复过期 Lease；
- 查询 SubmitPending、Submitted、CancelPending 与 Unknown；
- 幂等处理重复 Ack、Fill、Cancel 和 Reject；
- 重建 AccountProjection；
- 核对实际敞口与有效保护数量；
- 运行 Reconciliation 后再进入 Ready。

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
- 需要交易所能力矩阵和最大未保护窗口政策；
- Recovery 与 Reconciliation 测试成为发布门禁。

## 被否决的方案

### 宣称全局 exactly-once

数据库与交易所不共享同一事务边界，内部消息不能覆盖所有外部副作用。

### 网络失败后生成新订单 ID

原请求可能已成功，会形成重复订单。

### 只依赖 Broker 去重

不能覆盖数据库提交、外部调用、消费确认和交易所执行之间的失败窗口。

### 等全部成交后再挂保护

部分成交期间会形成未保护敞口，不符合实盘安全底线。

### 把保护计划当成保护成功

计划、请求、Ack 和实际有效数量是不同事实，必须分别跟踪。

### Reconciliation 直接修改订单表

绕过 Execution 状态机并破坏 owner、不变量和审计。

## 验证

- 相同 Order command 重放不会产生第二笔订单；
- 请求成功但响应丢失时可以查询并恢复；
- 每个订单与保护状态崩溃后都可续跑；
- FillEvent 重复和乱序不会重复增加仓位；
- 部分成交后保护数量在规定窗口内覆盖真实敞口；
- Cancelled 后迟到 Fill 会重新计算保护；
- Outbox 发布前后崩溃均不会丢失业务事件；
- Reconciliation 可以识别 Missing、Mismatch、Duplicate、Unknown 和 ProtectionMissing；
- 无法证明安全时系统 fail-closed 并留下人工处置证据。
