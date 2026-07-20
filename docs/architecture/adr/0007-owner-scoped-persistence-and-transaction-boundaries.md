# ADR-0007：采用 Owner-scoped 数据访问与业务事务边界

- 状态：已接受
- 日期：2026-07-20
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
persist_order_intent_with_outbox
apply_fill_and_record_event
transition_release_generation
```

禁止泛型 `Repository<T>`、`save(entity)`、`update_by_id`、`save_json` 和向 Domain 暴露 `sqlx::Transaction`。

### Use Case 定义原子性，Adapter 实现事务

Use Case 说明哪些状态、幂等记录和事件必须一起成功；一个 business-named Port 方法在 Postgres Adapter 内建立并提交 SQLx transaction。

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
- Command 的状态、幂等和 Outbox 原子提交有集成测试；
- Query 有索引、分页/上限和计划证据；
- 跨 owner 状态同步有 Contract、幂等和 recovery test；
- CI 拒绝新增泛型 Repository、跨 owner SQL 和无 owner migration。
