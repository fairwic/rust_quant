# 业务代码与数据访问放置规范

- 状态：已接受
- 日期：2026-07-20
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 依赖规则：[Rust Quant 依赖与代码归属规则](dependency-rules.md)

## 1. 目的

本文专门回答最容易被写乱的两个问题：

1. 业务逻辑到底放在哪里；
2. 数据库增删改查、事务和 SQL 到底放在哪里。

本文是开发者和 AI 新增代码时的默认放置标准。现有 legacy 代码可以按迁移计划暂时保留，但不得继续扩大错误边界。

## 2. 一条固定调用链

所有新增入口必须落入以下三种垂直切片之一。

### 2.1 Command：改变状态

```text
HTTP Handler / Worker / Consumer
  -> Wire Contract 映射
  -> Command Input
  -> Command Use Case
  -> Model / Policy
  -> Write Port
  -> Postgres Adapter
  -> SQLx Transaction
  -> State + Idempotency + Outbox
```

### 2.2 Query：只读查询

```text
HTTP Handler / Worker
  -> Query Input
  -> Query Use Case
  -> Query Port
  -> Postgres / HTTP Adapter
  -> Read Model
  -> Wire Contract 映射
```

### 2.3 Event Consumer：事件驱动状态变化

```text
Message Contract
  -> Envelope/版本/幂等校验
  -> Event Consumer Use Case
  -> Command Use Case 或 Model/Policy
  -> Owner Write Port
  -> State + Inbox/Idempotency + Outbox
  -> Ack
```

不得从 Handler、Consumer 或 Scheduler 直接跳到 SQL/SDK，也不得从数据库 Row 直接构造跨进程响应后顺便修改业务状态。

## 3. 业务逻辑放置矩阵

| 问题 | 正确位置 | 示例 |
| --- | --- | --- |
| 始终必须成立的不变量 | `domains/<owner>/model` | 已 Filled 订单不能回到 Submitted；订单数量必须为正 |
| 基于完整输入作纯决策 | `domains/<owner>/policies` | 资本分配、风险缩减、执行算法选择 |
| 一个完整业务动作的顺序 | `domains/<owner>/use_cases/commands` | 创建 OrderIntent、申请撤单、发布策略版本 |
| 一个只读业务问题 | `domains/<owner>/use_cases/queries` | 查询未保护敞口、查询账户 readiness |
| 消费事件后的业务动作 | `domains/<owner>/use_cases/consumers` | FillEvent 更新订单并触发账户投影 |
| 用例需要的外部能力接口 | `domains/<owner>/ports` | OrderStore、ExchangePort、Clock、OutboxPort |
| SQL、Row、数据库锁与事务 | `adapters/postgres/<owner>` | `SELECT ... FOR UPDATE`、批量 upsert |
| 交易所协议与签名 | `adapters/exchange-gateway` + `crypto_exc_all` | OKX 下单、查询订单、symbol filter |
| HTTP/消息 DTO 映射 | `apps/<app>` 或入站 Adapter | `ExecutionRequestedV1 -> CreateOrderIntentInput` |
| 环境变量、连接池、任务循环 | `apps/<app>` / `platform` | 解析 WorkerConfig、建立 PgPool |

## 4. Command 的推荐目录

以“创建订单意图”为例：

```text
crates/domains/execution/src/
├── model/
│   ├── order_intent.rs
│   └── order_state.rs
├── policies/
│   └── execution_plan_policy.rs
├── use_cases/commands/create_order_intent/
│   ├── input.rs
│   ├── output.rs
│   ├── handler.rs
│   └── tests.rs
└── ports/
    ├── execution_write_port.rs
    └── account_snapshot_port.rs

crates/adapters/postgres/src/execution/
├── rows.rs
├── queries.rs
├── execution_write_adapter.rs
└── tests.rs

apps/execution-worker/src/
├── config.rs
├── wiring.rs
├── consumer.rs
└── main.rs
```

职责严格分开：

- `input.rs` 是内部用例输入，不带 HTTP/SQLx derive；
- `handler.rs` 编排读取、校验、模型变化和一次原子写入；
- `model` 决定状态是否合法；
- `port` 用业务语言表达必须持久化的原子结果；
- Postgres Adapter 使用 SQLx 实现 SQL、锁与事务；
- App 把 Contract 转成 Input 并注入具体 Adapter。

## 5. 数据库 CRUD 放置规则

### 5.1 Create

“创建”是 Command，不是通用 Repository 方法。

正确做法：

```rust
pub trait ExecutionWritePort {
    async fn persist_order_intent_with_outbox(
        &self,
        change: PersistOrderIntent,
    ) -> Result<PersistOrderIntentResult, ExecutionStoreError>;
}
```

Postgres Adapter 在一个 SQLx transaction 中写入：

1. OrderIntent/Order 初始状态；
2. 幂等记录或唯一业务键；
3. Outbox Event；
4. 必要审计字段。

错误做法：

```text
repository.save(entity)
generic_repository.insert<T>()
handler 直接 INSERT
先写订单，事务外再写 outbox
```

### 5.2 Read

读分两类：

- 为业务决策读取 Aggregate/Snapshot：Query Port 返回业务模型或稳定快照；
- 为 UI、运营和报表读取：Query Port 返回专用 Read Model，不强行加载完整 Aggregate。

禁止让 UI 查询复用写模型 Repository 并在内存中做无界筛选。高频查询必须明确：过滤条件、索引、最大返回行数、排序、游标/分页和允许陈旧时间。

### 5.3 Update

“更新”必须命名为业务动作并经过状态机：

```text
错误：update_order_by_id(id, fields)
正确：mark_order_acknowledged(order_id, exchange_order_id, observed_at)

错误：update_position_json(id, payload)
正确：apply_fill_to_account_projection(fill_event)
```

Adapter 使用乐观版本、唯一约束、状态条件或行锁防止丢失更新。不得用无条件 `ON CONFLICT DO UPDATE` 覆盖状态机、版本身份或关键审计字段。

### 5.4 Delete

交易事实默认不做业务硬删除：

- Order、Fill、RiskDecision、Release 和 Reconciliation Evidence 使用状态迁移或保留策略；
- 真正物理删除只用于已定义生命周期的缓存、临时数据、幂等记录或合规清理；
- 删除必须是 owner Command，并记录范围、保留期、审计和恢复/不可恢复说明；
- Admin 不得直接执行跨 owner `DELETE`。

## 6. 事务边界

### 6.1 谁定义事务

Use case 定义“哪些业务结果必须一起成功”，Adapter 实现数据库事务。

Use case 不接收 `sqlx::Transaction`，而是调用一个表达原子业务动作的 Port 方法。这样 Domain 不知道 SQLx，同时避免在多个细粒度 Repository 调用之间伪造原子性。

### 6.2 单 Owner 原子写

以下内容通常在同一事务：

- Aggregate 状态变化；
- 乐观锁/sequence 推进；
- Inbox 或幂等记录；
- Outbox Event；
- 同 owner 的审计事实。

### 6.3 跨 Owner 一致性

禁止跨 Domain 或跨服务大事务。使用：

```text
Owner A 本地事务：State A + Outbox
  -> 至少一次发送 Command/Event
  -> Owner B：Inbox/幂等 + State B + Outbox
  -> 失败时补偿或 Reconciliation
```

Reconciliation 不能直接修改 Owner B 的表，只能发送 Owner B 的 typed command。

### 6.4 ResearchEvidence 的原子可见发布

ResearchEvidence 由 Research Domain 拥有，不是 Strategy 表的附件，也不能由 `quant/backtest` 直接写数据库。新增研究写入使用固定链路：

```text
Research complete/publish use case
  -> ResearchEvidenceStore / ResearchRunStore
  -> adapters/object-store/research 先上传不可变内容寻址对象
  -> adapters/postgres/research 在单个数据库事务中写：
       EvidenceManifest + Metrics/EvidenceObjectRef + Idempotency + Run.Completed
```

这里保证的是“原子可见”，不是对象存储与 PostgreSQL 的全局原子事务：

- 查询、晋级和 StrategyRelease 只能引用 `Completed` evidence；
- 数据库事务失败时，已上传但未被 Completed manifest 引用的对象属于 orphan，由 Research GC 按保留期清理；
- 同一 `BacktestRunId + evidence kind + content hash` 必须幂等；
- Research Adapter 不得写生产 Order、Fill、Position 或 AccountProjection 表；
- Strategy 发布用例只能保存已完成 EvidenceManifest 的稳定引用，不能复制或改写证据内容。

## 7. Query 与 Command 分离的边界

不要求引入复杂 CQRS 框架，但必须在代码位置和接口语义上区分：

| 类型 | 是否改变业务状态 | 返回值 | 数据源 |
| --- | --- | --- | --- |
| Command Use Case | 是 | 业务结果/身份/状态，不返回任意数据库 Row | Owner Write Port |
| Query Use Case | 否 | Domain Snapshot 或专用 Read Model | Owner Query Port / 合法投影 |
| Event Consumer | 是或触发 Command | 幂等处理结果 | Inbox + Owner Port |

Query 不得产生隐藏写入。确需记录访问审计时，由边界明确写独立审计事件，不把它伪装成查询副作用。

## 8. Postgres Adapter 结构

默认只建立一个 Postgres Adapter crate：

```text
crates/adapters/postgres/src/
├── lib.rs
├── pool.rs
├── error.rs
├── market/
├── strategy/
├── portfolio/
├── account/
├── risk/
├── execution/
├── research/
└── reconciliation/
```

每个 owner module：

- 只实现该 owner 的 Port；
- 只访问该 owner 表或明确批准的只读投影；
- 保存自己的 Row、SQL、错误映射和集成测试；
- 不能导出通用 `PgRepository` 给所有 Domain 自由拼 SQL。

当某个 owner 的依赖、规模、编译时间或发布生命周期出现真实隔离需求时，再通过 ADR 拆 crate。

## 9. Migration 规范

SQLx migration 保持单一总序列：

```text
migrations/
├── 20260720090000__execution__create_orders.sql
├── 20260720090500__execution__create_order_outbox.sql
└── 20260720091000__account__create_position_projection.sql
```

每个文件头部至少写：

```sql
-- owner: execution
-- purpose: persist OMS order state and stable client identity
-- rollout: additive, backfill before read switch
-- rollback: forward-fix; do not drop while legacy consumer exists
-- performance: unique lookup by client_order_id; bounded pending-state scan
```

强制要求：

- 新表有 `COMMENT ON TABLE`；
- 新列有 `COMMENT ON COLUMN`；
- 每条新查询说明索引和预期基数；
- 大表变更说明锁风险、分批回填和发布顺序；
- 表名/字段变更先完成双读或投影切换，不在同一发布中先删后迁；
- Migration 文件不按 owner 分子目录，避免破坏 SQLx 全局顺序。

## 10. 跨仓库数据访问

### 10.1 Core 与 Web

- Web 拥有用户、会员、订单、combo 订阅、凭证配置和执行资格；
- Core 拥有行情、策略信号、OrderIntent、Order、Fill、Protection 和 Reconciliation；
- Core 读取 Web 商业事实必须使用 `quant-web-client` 调用 owner internal API；
- Web/Admin 读取 Core 交易事实必须使用 Core API、Event 或只读投影；
- 禁止新增跨库 SQL、共享 ORM Model 或让 Admin 直写业务表。

### 10.2 Execution Request 与订单事实

`quant_web.execution_tasks` 在迁移期映射为 `ExecutionRequest`：它证明“这个用户的这个 combo 被允许交给 Core 尝试执行”，不证明已经形成交易所订单。

Core 收到请求后生成自己的稳定 `OrderIntentId` 和 `client_order_id`。Core 的 Order/Fill/Protection 才是执行事实。Web 展示 Core 结果时保存的是投影，投影可重建且不能反向覆盖 Core 状态。

## 11. 三种模板的最低内容

### 11.1 Command Slice

- Input/Output；
- use case handler；
- model/policy 调用；
- business-named Write Port；
- Adapter transaction；
- 单元测试与 Postgres 集成测试；
- 如跨进程，Contract mapping 与快照测试。

### 11.2 Query Slice

- Query Input；
- 专用 Read Model；
- Query Port；
- 索引、分页、最大结果数和陈旧度；
- Handler mapping 与查询集成测试。

### 11.3 Event Consumer

- Versioned Contract；
- Envelope、幂等与顺序校验；
- Consumer Use Case；
- Inbox/状态/Outbox 原子写；
- Ack 时点；
- 重复、乱序、崩溃恢复测试。

## 12. AI 修改前的放置声明

AI 在新增或移动代码前，必须先给出以下简表；不能唯一填写时先停下澄清：

```text
变更：
Owner：
切片类型：Command / Query / Event Consumer / Pure Policy
入口：
Use Case：
Model/Policy：
所需 Ports：
Adapters：
事务原子性：
跨进程 Contract：无 / 名称与版本
恢复 Owner：
测试：unit / integration / contract / recovery
```

## 13. 典型功能归属示例

### 13.1 新增“撤销超时订单”

```text
Execution model             定义允许撤单的状态
Execution policy            判断是否超时（接收注入时间）
Execution command use case  读取订单、申请撤单、持久化状态与 outbox
Execution ports             OrderWritePort、ExchangeOrderPort、Clock
Postgres adapter            锁定并更新订单状态
Exchange gateway            映射撤单请求到 crypto_exc_all
execution-worker            调度和装配
Recovery test               覆盖请求成功但响应丢失、撤单与成交竞态
```

### 13.2 新增“用户查看自动交易阻塞原因”

```text
Core Query Use Case         返回执行/风险/保护事实的只读快照
Core Query Port             读取 owner 投影
Core Contract v1            对外暴露结构化阻塞证据
Web                         合并会员/combo/凭证事实并生成用户下一步
Admin                       调用 owner API，只做诊断展示
```

### 13.3 新增“成交后挂保护单”

```text
Execution model             保护计划和 Protection 状态机
Execution policy            计算已成交敞口应保护数量
Fill consumer use case      幂等应用 Fill，原子写订单/保护命令 outbox
Exchange gateway            映射 attached/conditional order 能力
Reconciliation              检测 ProtectionMissing/QuantityMismatch
Risk                        超过未保护窗口后发 Reduce/Close/KillSwitch
```

## 14. 禁止模式速查

```text
禁止：Handler -> SQL
禁止：Use Case -> SQLx/Reqwest/Redis/SDK
禁止：Domain -> Wire Contract
禁止：Adapter -> 策略/组合/风险决策
禁止：Repository<T> / BaseService / update_by_id / save_json
禁止：跨 owner JOIN 后直接修改
禁止：Query 隐藏写入
禁止：Reconciliation 直接修表
禁止：把数据库 Row 当 Domain 或 API DTO
禁止：没有 Outbox 的跨进程“先写库再发消息”
```
