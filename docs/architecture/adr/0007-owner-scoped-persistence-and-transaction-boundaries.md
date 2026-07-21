# ADR-0007：采用 Owner-scoped 数据访问与业务事务边界

- 状态：已接受
- 日期：2026-07-21
- 决策者：Rust Quant Core

## 背景

“使用 Repository”本身不能保证架构清晰。泛型 Repository、`update_by_id`、共享数据库 Row 和跨 owner SQL 会隐藏业务动作、绕过状态机，并让事务散落在 Service、Handler 和多个 Repository 之间。

第一版目标为每个 owner 建立独立 Postgres Connector crate，隔离很强，但在同一数据库、同一发布生命周期下会造成过多 crate、连接装配和迁移维护。另一方面，把所有 SQL 放进一个无边界 Infrastructure crate 又会回到当前问题。

## 决策

### 一个 Postgres Adapter crate，按 Owner 分 module

默认结构：

```text
crates/adapters/postgres/src/
├── market/
├── strategy/
├── portfolio/
├── account/
├── risk/
├── execution/
└── reconciliation/
```

每个 module 只实现本 owner 的 Port，只访问 owner 表或明确批准的只读投影。只有独立编译、重依赖、规模或发布证据出现时才拆 crate。

### Port 使用业务语言

Domain 定义自己需要的 Port。写方法描述原子业务结果，例如：

```text
stage_order_submission_with_outbox
claim_mutation_attempt
consume_mutation_permit
complete_mutation_attempt
recover_unknown_and_enqueue_retry
rollover_mutation_delivery
apply_fill_and_record_event
transition_release_generation
```

禁止泛型 `Repository<T>`、`save(entity)`、`update_by_id`、`save_json` 和向 Domain 暴露 `sqlx::Transaction`。

### Use Case 定义原子性，Adapter 实现事务

Use Case 说明哪些状态、幂等记录和事件必须一起成功；一个 business-named Port 方法在 Postgres Adapter 内建立并提交 SQLx transaction。

Execution 下单准备使用 `stage_order_submission_with_outbox` 表达单一原子结果：`AccountOpeningSlot` claim、不可变 `RiskDecision` 引用及 parent OrderIntent/plan hash 唯一绑定、`OrderIntent`、完整 `ExecutionPlan`、`ProtectionPlan(Planned)`、首个持久订单状态 `SubmitPending`、Inbox/幂等、提交 Outbox 与审计字段一起提交。

Dispatcher 使用 `claim_mutation_attempt` 在短事务中 CAS `mutation_event_id`/`mutation_generation`/`expected_aggregate_version`/`send_claim`/fence，记录 attempt 并签发绑定这些字段的短期 permit；旧 delivery 只 ack/no-op。取消/恢复 revoke 与 Gateway consume 竞争同一 permit CAS。`consume_mutation_permit` 只供 Fenced Gateway 在网络 I/O 边界调用，校验 current permit 后原子置为 Consumed；外部调用不属于数据库事务。`complete_mutation_attempt` 再把 attempt outcome、permit 终态、Order/Protection transition 和后续 Outbox 原子提交；Unknown outcome 只能生成 query/reconciliation/alert 等恢复任务。`rollover_mutation_delivery` 用于所有 Submit/Cancel/Protect 的可重试路径：确认当前 delivery 的同一事务 supersede 旧 generation，并创建 delayed Outbox 或 durable RetrySchedule，Scheduler 不能直接 claim。`recover_unknown_and_enqueue_retry` 只在 DefinitivelyAbsent 且无可发送 permit 时，原子持久化 RecoveryAuthorized、supersede 旧 generation 的本地授权/投递记录、恢复原 kind 的 Pending state，并按 Submit/Cancel/Protect 映射写对应新 Outbox；原 mutation/目标 identity 与 payload hash 不变。外部 mutation 的完整顺序以 [ADR-0006](0006-at-least-once-idempotency-and-recovery.md) 为唯一权威。

跨 owner 不使用数据库大事务。通过本地 State + Outbox、下游 Inbox/幂等、补偿和 Reconciliation 达成最终一致。

### Command 与 Query 分离

不引入复杂 CQRS 框架，但接口和目录必须区分：

- Command Use Case + Write Port：执行业务状态变化；
- Query Use Case + Query Port：返回业务 Snapshot 或专用 Read Model；
- Event Consumer：验证 Contract/幂等后调用 owner Command。

Query 不隐藏业务写入；Read Model 不冒充写 Aggregate。

### 单一 Migration 序列

所有 SQLx migration 保持一个有序目录，文件名包含 owner：

```text
YYYYMMDDHHMMSS__<owner>__<action>.sql
```

每个迁移声明 owner、用途、rollout/rollback 与性能影响，并为新表和新列添加数据库注释。不为每个 owner 建立互不确定顺序的 migrations 子目录。

## 结果

### 正面影响

- CRUD 位置和业务意图可以唯一定位；
- Domain 不依赖 SQLx，事务仍能覆盖状态、幂等与 Outbox；
- 一个 Postgres crate 不造成包爆炸，owner module 又能被门禁；
- Query 可以针对 UI/运营性能构建 Read Model，不污染写模型；
- 跨 owner 状态变化有明确消息与恢复边界。

### 代价

- 需要为每个业务动作设计明确 Port；
- 少量场景会出现专用持久化输入类型；
- 单个 Postgres crate 需要 CI 扫描跨 owner SQL 和可见性；
- 跨 owner 查询不能依靠方便的私有表 JOIN。

## 被否决的方案

### 泛型 Repository 和 BaseService

隐藏业务语义并鼓励绕过状态机，最终会演变成无 owner CRUD。

### Use Case 直接操作 SQLx Transaction

让数据库技术进入 Domain，并使测试和存储替换困难。

### 每个表一个 Repository

表不是业务边界；一个业务动作常需要原子修改多个同 owner 表和 Outbox。

### 每个 Owner 默认一个 Postgres crate

隔离收益暂时不足以覆盖包、装配和构建成本。先 module，后按证据拆分。

### 跨 Owner 数据库事务

把模块耦合固化在数据库内，未来无法独立恢复、扩缩容或拆服务。

## 验证

- Domain crate 不依赖 SQLx；
- 新 SQL 可以映射到唯一 owner module；
- Command 的状态、幂等和 Outbox 原子提交有集成测试；Execution 测试还要证明 opening slot、完整计划、审批引用和 `SubmitPending` 同生共死，cancel/recovery revoke 与 Gateway permit consume 互斥，stale permit 不触达 SDK，且 Unknown 恢复事务会写入新的提交 Outbox；
- Query 有索引、分页/上限和计划证据；
- 跨 owner 状态同步有 Contract、幂等和 recovery test；
- CI 拒绝新增泛型 Repository、跨 owner SQL 和无 owner migration。
