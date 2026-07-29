# 业务代码与数据访问放置规范

- 状态：已接受
- 日期：2026-07-23
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- 依赖规则：[Rust Quant 依赖与代码归属规则](dependency-rules.md)

## 1. 目的

本文专门回答最容易被写乱的三个问题：

1. 业务逻辑到底放在哪里；
2. 哪些逻辑应使用 Entity/Value Object/Policy/Use Case，哪些应使用函数；
3. 数据库增删改查、事务和 SQL 到底放在哪里。

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
| 始终必须成立的不变量 | `domains/<owner>/model` | 已 Filled 订单不能回到 Acknowledged；订单数量必须为正 |
| 基于完整输入作纯决策 | `domains/<owner>/policies` | 资本分配、风险缩减、执行算法选择 |
| 一个完整业务动作的顺序 | `domains/<owner>/use_cases/commands` | 创建 OrderIntent、申请撤单、发布策略版本 |
| 一个只读业务问题 | `domains/<owner>/use_cases/queries` | 查询未保护敞口、查询账户 readiness |
| 消费事件后的业务动作 | `domains/<owner>/use_cases/consumers` | FillEvent 更新订单并触发账户投影 |
| 用例需要的外部能力接口 | `domains/<owner>/ports` | OrderStore、ExchangePort、Clock、OutboxPort |
| SQL、Row、数据库锁与事务 | `adapters/postgres/<owner>` | `SELECT ... FOR UPDATE`、批量 upsert |
| 交易所协议与签名 | `adapters/exchange-gateway` + `crypto_exc_all` | OKX 下单、查询订单、symbol filter |
| HTTP/消息 DTO 映射 | `apps/<app>` 或入站 Adapter | `ExecutionRequestedV1 -> CreateOrderIntentInput` |
| 环境变量、连接池、任务循环 | `apps/<app>` / `platform` | 解析 WorkerConfig、建立 PgPool |

### 3.1 Rust 代码形态决策表

不要用“逻辑复杂就建对象、逻辑简单就写函数”判断。按以下顺序决策：

| 问题 | 是 | 否 |
| --- | --- | --- |
| 是否已经确定唯一业务 Owner？ | 继续判断 | 停止；不得先创建 `common`、Service 或 DTO |
| 是否有稳定 identity、生命周期或跨字段不变量？ | Entity/Aggregate | 继续判断 |
| 是否无 identity，但需要构造时保证单位、范围或组合合法？ | Value Object | 继续判断 |
| 是否为无状态、无 I/O、完整输入决定输出的计算？ | 自由纯函数 | 继续判断 |
| 是否有多个纯决策共享不可变、带版本配置？ | Policy 对象 | 继续判断 |
| 是否跨事件维护滚动状态？ | 显式 State + 同一 transition | 继续判断 |
| 是否编排读取、判断、持久化和事件？ | 持有 Port 的 Use Case 对象 | 继续判断 |
| 是否实现数据库、HTTP、Redis 或交易所协议？ | Adapter 对象 | 重新检查职责是否过宽 |

Trait 不是“面向对象基类”。只有以下场景才创建 Trait：

- Domain 定义自己需要的 Port；
- 存在 live/in-memory/simulated 等真实替换实现；
- 跨 Domain 暴露稳定进程内 API；
- 测试需要 Fake 且边界本身有业务意义。

只有一个实现、没有调用边界或只是希望以后扩展时，不创建 Trait。

### 3.2 Entity、纯函数、Policy 与 Use Case 示例

有身份和生命周期的订单使用 Aggregate，并通过方法保护状态迁移：

```rust
pub struct Order {
    id: OrderId,
    state: OrderState,
    quantity: Quantity,
    filled_quantity: Quantity,
}

impl Order {
    pub fn apply_fill(
        &mut self,
        fill: Fill,
        observed_at: Timestamp,
    ) -> Result<Vec<OrderEvent>, OrderError> {
        // 只验证不变量、推进状态并生成事件；不访问数据库或系统时间。
        todo!()
    }
}
```

无状态的止损候选选择使用自由纯函数：

```rust
pub fn select_tightest_valid_stop(
    side: PositionSide,
    entry: Price,
    candidates: &[StopCandidate],
) -> Option<Price> {
    todo!()
}
```

多个决策共享同一冻结风险配置时使用 Policy：

```rust
pub struct PreTradeRiskPolicy {
    snapshot: RiskPolicySnapshot,
}

impl PreTradeRiskPolicy {
    pub fn evaluate(&self, input: PreTradeRiskInput) -> RiskDecision {
        todo!()
    }
}
```

需要读取 Snapshot 并原子持久化结果时使用 Use Case：

```rust
pub struct EvaluatePreTradeRisk<A, S> {
    account_snapshots: A,
    risk_decisions: S,
}

impl<A, S> EvaluatePreTradeRisk<A, S> {
    pub async fn execute(
        &self,
        input: EvaluateRiskInput,
    ) -> Result<RiskDecisionId, RiskError> {
        todo!()
    }
}
```

Use Case 对象存在的理由是持有依赖并编排一个完整业务动作，不是给函数增加 `Service::` 前缀。没有字段和依赖的 `StopLossCalculator`、`OrderService` 或 `XxxStrategy` 静态方法容器，应改成语义明确的 module + function；确有冻结配置时再变成 Policy。

### 3.3 状态对象与纯 transition

跨 K 线或订单事件维护状态时，可以采用以下任一种形式：

```text
Aggregate method:
  order.apply(event, observed_at) -> domain events

Pure transition:
  transition(previous_state, event, policy_snapshot) -> next_state + effects
```

选择规则：

- 状态带业务 identity、version 且需要防止任意修改时，优先 Aggregate method；
- 状态是 evaluator 内部可重放值、需要 backtest/live 逐字节比较时，优先纯 transition；
- 两种形式都必须显式传入时间、随机值、配置快照和外部事实；
- 禁止在 Model/Policy 中读取 `Utc::now()`、`std::env`、全局随机源或进程全局业务 Map；
- 能破坏不变量的字段不得提供任意 setter 或公开可变访问。

`StrategyEvaluationState` 使用 `EvaluationScopeId + StrategyRuntimeSnapshotId + MarketStreamPartition` 作为 identity。live 只使用 Redis Adapter；Postgres 不得成为 evaluator state 的同源或回退事实库。backtest 使用内存 Adapter，只替换状态存储，不替换 evaluator transition。

### 3.4 Backtest、Paper 与 Live 的单实现规则

相同配置不是“同一 JSON 分别反序列化到两个相似结构体”。所有运行模式必须使用同一组强类型快照、同一 `DecisionContextCoreV1` builder 和同一业务 symbol：

```text
StrategyRuntimeSnapshot             -> Strategy evaluator/exit policy
PortfolioPolicySnapshot             -> Portfolio policy
RiskPolicySnapshot                  -> Risk policy
ExecutionPlanningPolicySnapshot     -> Order/Protection planning
              \                         /
               -> DecisionContextCoreV1
                  + 动态 MarketDecisionReadiness/Market/Account/Instrument Evidence
                  + live: ExecutionDecisionContextSnapshot(ExecutionRequest subject)
                  + research: ResearchDecisionContextSnapshot(ResearchScenario subject)
```

Owner 边界固定为：

- Strategy 输出候选失效价、退出意图/候选止盈计划和 Signal evidence；
- Portfolio 决定资本预算、净额和目标仓位；
- Risk 选择不可放宽的最终止损与风险边界、批准数量并生成 `RiskDecision`，不替 Strategy 发明盈利目标；
- Execution 根据 Strategy exit intent、RiskDecision 和 instrument capability 先生成纯、规范化的 `ExecutionPlanningValue`（内含有序 child `OrderPlan` 与 `ProtectionPlanningValue`）；只有 live intake 的单一事务才能由该值初始化持久 `ExecutionPlan` aggregate 与 `ProtectionPlan`；
- ResearchBar 只替换 Account/Market 的模拟输入和 Fill/fee/slippage/funding 机制，只产生/比较 `ExecutionPlanningValue`，不产生 `OrderIntent`、live `ExecutionPlan`、Outbox 或交易所 mutation；
- PaperEvent 只替换 Exchange Adapter，并复用 Execution 状态迁移；如需模拟 OMS 状态，只验证从同一 planning value 无损初始化的模拟 aggregate，不把它当作 Research parity 对象；
- live 使用真实 Adapter，但不得另写 `LiveRisk`、`LiveStopLoss` 或 `LiveOrderPlanner`。

`trade_fee_rate`、slippage、funding 和 candle 内路径归 `SimulationProfile`；分批止盈、ATR/固定目标等 alpha exit 规则归 Strategy RuntimeSnapshot；`allocation_ratio` 归 Portfolio；账户风险比例、最大损失、leverage/margin 限制和最终止损约束归 Risk；具体 leverage/margin mode、交易所精度与保护单能力映射归 Execution。禁止把这些字段继续混入一个 `BasicRiskStrategyConfig`，也禁止同一配置 payload 同时解析为两个语义重叠的风险类型。

完整配置边界固定为：

- `StrategyRuntimeSnapshot` 不包含账户、用户、凭证、risk profile 或其他 owner 的政策内容；
- `ActivationEligibilityV1` 由 Strategy owner 随 immutable RuntimeSnapshot、Release generation、Completed Evidence、allowed deployment channel、eligibility generation 与撤销状态发布；Control 的 `ActivationPointer` 只能消费该资格，且必须满足 channel×stage，不能从“已发布”或可变 Release 状态自行推断；
- Web risk profile 只能作为 `ExecutionRequest` 中的精确来源/授权引用，由 Core Risk 校验并解析为 Published `RiskPolicySnapshot`；
- Execution 在自己的事务中把四个 Published Policy Snapshot 绑定成 `ExecutionDecisionContextSnapshot`，并与请求 intake/幂等身份一起持久化；Research 使用 `ResearchScenarioRef`/`ResearchDecisionContextSnapshot`，不得伪造 Web execution fields；
- `MarketDecisionReadiness`、MarketSnapshot、AccountSnapshot、InstrumentRulesSnapshot 和 observed time 是动态决策证据，不得塞进 Policy Snapshot；非 `ReadyForNewRisk` 时只允许已声明且可证明的 reduce-only 路径，不能新增/加仓；
- 连续 `RiskAction` 必须按 `risk_action_decision_id = subject_binding_hash + trigger_event/evidence_hash + risk_policy_snapshot_hash + action_generation` 幂等；同一 identity 只能产生一次同语义 action/outcome，避免事件重放重复减仓/平仓；
- Research 取得历史 Market 输入只能使用 Market historical API/Contract，禁止直读 Market Storage、触发 backfill、使用生产 Adapter 或持有 `MarketDataAccessCredential`；
- Research 用 `ResearchRunSpec` 冻结 DatasetManifest、四个 Policy Snapshot、SimulationProfile、模拟账户初态和 Clock/Seed，再以同一 `DecisionContextCoreV1` builder 构造 ResearchScenario binding；
- 任一默认值只能由字段 owner 在发布 Snapshot 时展开；App、Research 和 Dispatcher 禁止自行补业务默认值或选择“当前最新配置”。

详细字段与 exact parity 定义见 [ADR-0011](adr/0011-layered-runtime-snapshots-and-decision-context.md)。

## 4. Command 的推荐目录

以“创建订单意图”为例：

```text
crates/domains/execution/src/
├── api.rs                           # 业务调用方只见稳定 Input/Output/API
├── spi.rs                           # Adapter/组合根只见 Port 与装配入口
├── order_lifecycle/
│   ├── model/
│   │   ├── order_intent.rs
│   │   └── order_state.rs
│   ├── commands/create_order_intent/
│   │   ├── input.rs
│   │   ├── output.rs
│   │   ├── handler.rs
│   │   └── tests.rs
│   └── ports/
│       ├── execution_write_port.rs
│       └── account_snapshot_port.rs
├── planning/
│   └── policies/
│       └── execution_plan_policy.rs
└── lib.rs                           # 只公开 api 与 spi

crates/adapters/postgres/src/execution/
└── order_lifecycle/
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
- App 把 Contract 转成 Input 并注入具体 Adapter；
- 其他 Domain/Handler/Consumer 只调用 `api`；Postgres Adapter 只实现 `spi`；App 只有 `wiring.rs` 可以看到 `spi`；
- 不建立 Owner 级全局 `model/`、`ports/`、`enums.rs` 或 provider 级大文件来容纳所有 capability。

### 4.1 Port 与 Use Case 的完成门

Port 是完整垂直切片中的 I/O 边界，不是先占位的目录资产。非测试 Port 进入 `verified` 前必须同时存在：

1. 调用它的生产 Use Case；
2. 至少一个生产 Adapter；
3. 业务命名的 Input/Output；
4. 失败、幂等/原子性、超时和恢复 Owner 测试。

Fake/Mock 只用于测试，不算生产实现。只有 Fake 的 Port 必须停在带后续承接的 `implementing` Manifest；不能被 re-export 成已经稳定的业务能力。纯 Policy、单一算法或“以后可能替换”的代码使用函数/对象，不创建 Trait。

一个公开 Use Case 只处理一个业务动词、一个主要业务结果和一个恢复 Owner。出现第二次事务提交、等待外部 receipt、第二个可独立补偿结果时，拆为同 Owner 的后续 Command/Consumer 或 durable process manager；App 不接管这段业务状态机。四个及以上有副作用 Port 是强制 Review 信号，禁止用万能 `Services`/`EverythingPort` 隐藏。

### 4.2 Enum、错误与表示类型

- Aggregate 状态和值枚举与 capability model 共置；
- Command/Query 选择枚举与 Input 共置；
- Port 技术失败分类与 Port 共置，再由 Use Case 映射为稳定业务错误；
- Wire、数据库 Row、交易所 SDK enum 分别留在版本化 Contract、owner-scoped Postgres Adapter、`crypto_exc_all`；
- `api.rs`/`spi.rs` 只重导出，不定义第二套同义 enum；
- 禁止 Domain 级 `enums.rs`、`types.rs`、`common.rs`、`shared.rs` 混装无关概念。

## 5. 数据库 CRUD 放置规则

### 5.1 Create

“创建”是 Command，不是通用 Repository 方法。

正确做法：

```rust
pub trait ExecutionWritePort {
    async fn stage_order_submission_with_outbox(
        &self,
        change: StageOrderSubmission,
    ) -> Result<StagedOrderSubmission, ExecutionStoreError>;
}
```

live Postgres Adapter 在一个 SQLx transaction 中写入：

1. Inbox/幂等记录和唯一业务键；
2. 通过 account gate row CAS 或活跃唯一约束取得 `AccountOpeningSlot`；
3. 不可变 `risk_evaluation_id`/`RiskDecision` 引用、摘要、批准边界和过期时间，并唯一绑定 parent OrderIntent/plan hash；
4. `OrderIntent` 与当前 child Order 首个持久状态 `SubmitPending`；
5. 由同一不可变 `ExecutionPlanningValue`/hash 无损初始化的完整 aggregate `ExecutionPlan`（含有序 child `OrderPlan` snapshot；child 不另建表、生命周期或 Contract）；
6. 由对应 `ProtectionPlanningValue` 初始化、初始为 `Planned` 的 `ProtectionPlan`；
7. `OrderSubmissionRequestedV1` Outbox；
8. correlation、causation 和必要审计字段。

只有事务提交后，Outbox Publisher 才能发布提交任务；Dispatcher 消费任务并取得 fenced send claim/attempt/permit，只有 Fenced Exchange Mutation Gateway 原子消费 current permit 后才能调用 raw SDK。Port 名中的 `stage` 表示“形成可恢复的持久提交任务”，不表示交易所已经收到或接受订单。完整顺序以 [ADR-0006](adr/0006-at-least-once-idempotency-and-recovery.md) 为唯一权威。

Dispatcher、Fenced Gateway 与 Recovery 的关键数据库写也必须使用业务 Port，而不是暴露 attempt/permit 表 CRUD：

```rust
pub trait ExecutionMutationWritePort {
    async fn claim_mutation_attempt(
        &self,
        claim: ClaimMutationAttempt,
    ) -> Result<IssuedMutationPermit, ExecutionStoreError>;

    async fn consume_mutation_permit(
        &self,
        permit: ConsumeMutationPermit,
    ) -> Result<ConsumedMutationPermit, ExecutionStoreError>;

    async fn complete_mutation_attempt(
        &self,
        outcome: CompleteMutationAttempt,
    ) -> Result<CompletedMutationAttempt, ExecutionStoreError>;

    async fn recover_unknown_and_enqueue_retry(
        &self,
        recovery: RecoverUnknown,
    ) -> Result<RecoveredMutation, ExecutionStoreError>;

    async fn rollover_mutation_delivery(
        &self,
        retry: RolloverMutationDelivery,
    ) -> Result<ScheduledMutationRetry, ExecutionStoreError>;
}
```

- `claim_mutation_attempt` 在短事务中同时校验 `mutation_event_id`、`mutation_generation`、`expected_aggregate_version`、Pending state、空 `send_claim`、account/order fence 和最终门禁证据，再写绑定这三个 mutation 授权字段的 `ExecutionMutationAttempt(Started)` 与短期 `MutationPermit(Issued)`；旧或重复 delivery 只 ack/no-op；
- `consume_mutation_permit` 只供 Fenced Gateway 在网络 I/O 边界调用，以 attempt/version/fence/generation/payload hash/expiry CAS 消费 current permit；失败返回 DefinitelyNotSent 且不得触达 raw SDK；
- 提交前取消/Recovery 使用同一 permit CAS；revoke 与 consume 只有一方成功。permit 已 Consumed 时不得本地终结或重发；
- `complete_mutation_attempt` 在一个事务中一起写 attempt outcome、permit 终态、Order/Protection transition 和后续 Outbox；Unknown outcome 只能创建 query/reconciliation/alert 等恢复任务，不能直接创建同 kind mutation Outbox；
- `recover_unknown_and_enqueue_retry` 只有在 DefinitivelyAbsent 且无可发送 permit 时，才原子写 RecoveryAuthorized、supersede 旧 mutation generation 的本地授权/投递记录、推进原 kind 的 Pending state，并按 Submit/Cancel/Protect 映射创建对应新 Outbox；只滚动 mutation 三字段和 attempt number，保持原 mutation/目标 identity 与 payload/plan hash，不能依赖旧 Outbox 或进程扫描；
- `rollover_mutation_delivery` 统一处理 Submit/Cancel/Protect 的 transient blocker、可重试 DefinitelyNotSent 和 fence/gate 变化：确认当前 delivery 的同一事务必须 supersede 旧 generation，并创建 delayed Outbox 或 durable RetrySchedule；Scheduler 到期只能经 owner transaction 物化唯一 Outbox，不能直接 claim；
- `ExecutionMutationAttempt` 与 Permit 至少保存 mutation kind、stable identity/number、payload/plan hash、fence/generation、expiry、门禁引用、结果/permit 状态和 started/completed time；不得保存明文凭证、`GatewayCredentialCapability`、原始 `MarketDataAccessCredential` 或其 Ref；
- raw SDK mutation client 与用户 credential material 只装配进 Fenced Gateway。Gateway 仅以 Web owner 签发、audience-bound、短 TTL/一次性的 capability 在内存解析材料；Dispatcher、Risk、Context、Outbox 和其他 App 不得读取、序列化或持久化它。原始 `MarketDataAccessCredential` 只允许 Market/Gateway 公共 read-only 配置内存使用；公共配额/证据/必要 Market Contract 仅可使用非敏感 `MarketDataAccessCredentialRef` 或 `market_data_source_profile_id`，绝不能复用为私有/执行能力；
- 暂时 blocker 必须持久化 `next_eligible_at`/唤醒条件并确认当前 delivery，由 durable scheduler/event 唤醒，不能 nack 热循环。

错误做法：

```text
repository.save(entity)
generic_repository.insert<T>()
handler 直接 INSERT
先写订单，事务外再写 outbox
先调用交易所，成功后再补订单状态
只持久化 OrderIntent，ExecutionPlan（含 child OrderPlan）/ProtectionPlan 留在内存
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

Execution 的 live 下单准备是该规则的严格实例：`RiskDecision` 由 Risk owner 先行持久化；Execution 不建立跨 owner 事务，只在自己的单一事务中取得 `AccountOpeningSlot`，保存不可变审批引用与 `ExecutionPlanningValue` hash，并原子保存由其初始化的 `SubmitPending + OrderIntent + ExecutionPlan（含 child OrderPlan snapshot）+ ProtectionPlan + Idempotency + Outbox`。该事务提交前禁止确认上游或发生外部 I/O；提交后由 Dispatcher 复核 `MarketDecisionReadiness`、账户新鲜度、资格与保护门禁并签发 permit，再由 Fenced Gateway 消费 current permit 后调用交易所。

Opening slot 也是 Execution owner 事实，不由 worker lease 代替。释放必须由业务用例验证全部 child mutation 已确定、没有 attempt/Unknown、Account owner typed watermark 已覆盖 cumulative fill，且剩余敞口保护满足政策；Adapter 只原子执行该决定。

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

### 6.5 Outbox、幂等、投递与恢复的 Owner

Outbox 不是一个可以接管业务的“消息服务”。职责固定如下：

| 责任 | 放置位置 |
| --- | --- |
| Event/Command 业务含义、幂等 identity、何时重试/补偿 | 原 Owner Model/Use Case |
| State + Inbox/幂等 + Outbox + Audit 原子写集 | 原 Owner Write Port 定义；owner-scoped Postgres Adapter 实现 |
| 通用轮询、投递、Ack、退避、transport telemetry | Messaging/Postgres Adapter 或 Platform |
| 连接、配置、循环监督、关闭 | App |
| 投递失败后的业务恢复决定 | 原 Owner Recovery Use Case |
| 跨 Owner 差异检测 | Reconciliation；发送 typed owner command |

因此：

- Publisher 只能投递已提交 Outbox，不根据 payload 内容决定业务状态；
- App 的 loop/callback 不通过 `match error` 发明重试、补偿或状态迁移；
- Adapter 可以归一技术错误与执行通用退避，但不能改变业务 identity、生成新的业务命令或越权更新 Aggregate；
- Reconciliation 不直接更新原 Owner 表；它提交带证据与幂等 identity 的恢复 Command，由原 Owner 决定 no-op、重试、补偿或人工升级；
- 跨 Owner 等待 receipt 的长流程属于发起 Owner 的 durable process manager/状态机，不属于 App 内存 Task。

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
├── control/
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
- 跨仓库业务 payload 随其唯一 owner 仓库发布：Core `crates/contracts` 只保存 Core owner payload 和 owner-neutral `ContractEnvelopeV1` primitive；Web/News payload 由各自 owner 发布，Core 仅在 Adapter 显式映射固定版本 binding。Envelope 只放传输 identity，不能携带业务字段；Envelope/payload 均须独立 N/N-1 兼容测试。
- `NewsInsightV1` 由 News owner 发布且带 version、`published_at`、不可变 `available_at` 与 evidence ref；它只可作为 `StrategyRuntimeSnapshot` 声明可消费的已发布 evaluator 输入，且仅当 `available_at <= DecisionTime` 时可消费为 `StrategySignal`，不得直接形成 Web `ExecutionRequest`。

### 10.2 Execution Request 与订单事实

`quant_web.execution_tasks` 在迁移期映射为 `ExecutionRequest`：它证明“这个用户的这个 combo 被允许交给 Core 尝试执行”，不证明已经形成交易所订单。表始终由 Web owner 管理，Core 既不直连、也不轮询该表。

其中 `risk_profile_ref + version` 只是 Web 配置来源和商业授权引用。Core Risk 必须按精确版本校验并幂等解析为不可变 `RiskPolicySnapshot`；不得把 Web JSON 直接当作业务政策，也不得缺失时回退默认风险。

Core 通过 `ClaimExecutionRequestV1` 取得带 claim fence/expiry 的 canonical request；长流程用 `RenewExecutionRequestClaimV1`，放弃/阻塞用 `ReleaseExecutionRequestClaimV1`，完成/用户可见 blocker 用 `ReportExecutionRequestOutcomeV1`。Web 在自己的事务中处理 claim、重复、过期与迟到 outcome；Core 的 worker lease、OpeningSlot 与 OMS 幂等独立存在，二者不构成跨库事务。

持有有效 claim 后，Execution 才把四个 Published Policy Snapshot 绑定为不可变 `ExecutionDecisionContextSnapshot`，再生成稳定 `OrderIntentId` 和 `client_order_id`。Core 的 Order/Fill/Protection 才是执行事实。Web 展示 Core 结果时保存的是投影，投影可重建且不能反向覆盖 Core 状态。

已由该 Web 请求触发且仍有成交、保护、Unknown、撤单或对账责任的交易，Execution 从原 request/context/order identity 持久派生 `SafetyObligation`。会员、订阅或 claim 到期只能冻结新风险，不能删除该责任；只有可追溯该来源链的 `ManagedExposure` 可在原始证据、Gateway capability、permit/fence 与 reduce-only 证明齐备时执行 Cancel/Protect/Reduce/Close。`ObservedExternalPosition`/`UnknownOrigin` 只能读取、告警和人工处置，除非 Web 另行发布版本化账户管理授权 Contract；不得借平台市场数据材料或其他用户 credential 继续自动 mutation。

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
代码形态：Entity / Value Object / Pure Function / Policy / Use Case / Port / Adapter
入口：
Use Case：
Model/Policy：
所需 Ports：
Adapters：
Backtest/Paper/Live 复用点与允许差异：
构建影响：Research-only / Strategy candidate / Shared Domain / Production App；受影响 App：
事务原子性：
跨进程 Contract：无 / 名称与版本
恢复 Owner：
测试：unit / deterministic / parity / integration / contract / recovery
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
Risk                        超过未保护窗口后发 Reduce/Close 与 Kill Switch typed request
```

## 14. 禁止模式速查

```text
禁止：Handler -> SQL
禁止：Use Case -> SQLx/Reqwest/Redis/SDK
禁止：Domain -> Wire Contract
禁止：其他 Domain/Research -> Domain SPI 或私有 capability
禁止：Adapter -> Domain 私有 model/use_case（只能实现 SPI）
禁止：Adapter -> 策略/组合/风险决策
禁止：Repository<T> / BaseService / update_by_id / save_json
禁止：零字段 Service/Manager/Calculator 仅作为函数命名空间
禁止：只有 Fake/Mock、没有生产 Use Case + Adapter 的 Port 被标为 verified
禁止：万能 Services/EverythingPort 隐藏大 Use Case 的副作用依赖
禁止：Domain 级 enums.rs/types.rs/common.rs/shared.rs 混装无关语义
禁止：把跨 owner 大函数移动进 impl 后冒充 Aggregate
禁止：Aggregate 公开可绕过状态机的可变字段
禁止：Model/Policy 读取系统时间、环境变量或全局业务缓存
禁止：backtest/paper/live 各自实现 Risk、止盈止损或 `ExecutionPlanningValue`（含 child `OrderPlan`）；禁止 Research 创建或比较 live-only `ExecutionPlan` aggregate
禁止：同一 JSON 解析为语义重叠的 backtest/live 风险配置
禁止：跨 owner JOIN 后直接修改
禁止：Query 隐藏写入
禁止：Reconciliation 直接修表
禁止：把数据库 Row 当 Domain 或 API DTO
禁止：没有 Outbox 的跨进程“先写库再发消息”
禁止：Outbox Publisher/App callback 自行决定业务重试或补偿
```
