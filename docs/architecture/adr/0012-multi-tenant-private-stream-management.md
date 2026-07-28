# ADR-0012：多租户私有流连接管理与容量分阶段

- 状态：已接受
- 首次接受：2026-07-24
- 决策者：Rust Quant Core
- 上位文档：[生产运行与恢复](../production-runtime.md)（§10.2）、[长期目标架构](../target-architecture.md)（§3.9、§5、§10、§11）
- 细化：[ADR-0005](0005-control-plane-and-data-plane.md)、[ADR-0006](0006-at-least-once-idempotency-and-recovery.md)、[ADR-0007](0007-owner-scoped-persistence-and-transaction-boundaries.md)

## 背景

多租户自动交易平台必须为每个用户的每个交易所账户维持一条**私有 WebSocket 连接**（User Stream / User Data Stream），以实时接收该账户的成交、余额、持仓和订单事件。私有流与公共行情流有本质差异：

- 公共行情流全市场共享，一条连接可订阅几十个币种，所有用户复用；
- 私有流是**账户私有**的，必须用该账户 API Key 鉴权，只推该账户事件，**不能多账户复用**；
- N 个用户配置自动交易 = 原则上需要 N 条独立私有连接（× 交易所数）。

这带来交易所侧的硬约束：

- 建连速率上限（OKX：每出口 IP 每秒 3 个新连接）；
- 连接维护成本（Binance listenKey 每 30 分钟需 REST 续期）；
- API Key 绑定 IP 数上限（OKX：每 Key 最多 20 IP）；
- 用户私有 REST、私有流维护与 mutation 使用 `PrivateQuotaKey(exchange, credential_reference, endpoint_group)`；它与 Market 公共数据的 `PublicQuotaKey` 完全隔离（见 [ADR-0013](0013-user-execution-request-and-public-market-data-credentials.md)）。

若不显式设计，会出现以下失败：私有流断线导致 Account 投影与交易所真实状态不一致却无人察觉；冷启动串行建连撞速率墙导致恢复缓慢；多实例并存时同一账户被两个实例同时持有（脑裂）；连接异常时用户体验在"完全停服"与"用脏数据继续开仓"之间没有中间态。

私有连接管理本质上是 Account owner 的 `ExchangeSession` readiness（[target §5](../target-architecture.md)）在工程上的落地，直接决定平台可支持的 monitored session 规模。

## 决策

### 1. 分三个容量阶段，按证据迁移，不一步到位

私有连接管理按实际需要监测的 `MonitoredExchangeAccountSession` 规模分三个 **AccountSession Phase**。它是“具有稳定物理 `ExchangeAccountRef` 的 `exchange × user account/subaccount × product scope` 会话”，其集合等于当前商业账户集合与未结 `SafetyObligation` 账户集合的并集；`credential_reference + credential_revision + credential_revocation_generation` 只是可轮换授权材料及失效版本，不能作为 shard、lease、fence 或风险归属主键。每阶段有明确触发条件、容量上限和迁移路径。不提前实现高阶段复杂度，也不把 AccountSession Phase 1 当作永久形态。本文所有 Phase 名称只描述账户会话容量；它们与 [production-runtime §2.1](../production-runtime.md) 的 `Topology rollout T1` 是正交迁移轴，不能互相替代或按同一个“第一阶段”解释。

| | AccountSession Phase 1 | AccountSession Phase 2 | AccountSession Phase 3 |
|---|---|---|---|
| 触发 | <100 monitored sessions | ≥100 或冷启动 >5 分钟 | ≥500 或需动态扩缩容 |
| 连接策略 | 全热连接 | 按需热/冷切换 | 同 AccountSession Phase 2 + 动态容量预算 |
| 拓扑 | 单实例单 IP | 多实例分片（2-4） | 多实例动态扩缩容 |
| 失败处理 | fail-closed + UI 提示 | 同左，并行恢复 | 降级态缓冲（degraded polling） |
| ExchangeSession 状态 | 二态 healthy/stale | 二态 | 三态 healthy/degraded/stale |
| 容量上限 | ~100 | ~500 | 500+ 水平扩展 |

AccountSession Phase 1 → AccountSession Phase 2 的唯一触发条件就是上表的“≥100 monitored sessions 或冷启动 >5 分钟”；200/300 会话、2 分钟预警或按 IP 估算只能是容量观测，不能形成第二套迁移门。退订后仍有安全义务的会话计入该规模，不能因不再商业活跃而绕过容量门。

AccountSession Phase 1/2 对外发布的 `ExchangeSession` readiness 只有 `healthy` 与 `stale`。`connecting`、`buffering`、`reconnecting`、`backoff` 是 stale 期间的内部连接生命周期；AccountSession Phase 2 的 `cold` 只是未承接活跃自动交易任务的调度分类，不是可开仓的第三种 readiness。`degraded` 仅在 AccountSession Phase 3 出现。

### 1.1 公共市场数据不属于私有账户容量

Market 使用平台固定的 `MarketDataAccessCredential` 时，只能访问 K 线、instrument、公共盘口/成交等公共只读 endpoint，并按 `PublicQuotaKey(exchange, endpoint_group, egress_identity, market_data_source_profile_id)` 受限。它不是任何用户 `credential_reference`，不能建立用户私有流、读取账户或发起 mutation；公共采集的配额与失败不得挤占或伪装成用户 `PrivateQuotaKey`。完整身份边界见 [ADR-0013](0013-user-execution-request-and-public-market-data-credentials.md)。

### 1.2 商业资格与安全监测义务共同决定会话集合

Web owner API 提供当前有活跃 combo、会员与可用用户 credential 的商业账户引用，并返回稳定 `ExchangeAccountRef`、当前 credential revision/revocation generation 与产品/子账户范围；Account 还必须消费 Execution 发布的版本化安全监测 Contract，以保留存在未结 `SafetyObligation` 的账户会话。二者的并集是需要私有流/签名查询监测的账户集合，但它们的语义不同：商业资格只授权新的风险增加，安全监测义务只允许对既有受管敞口执行受限的收敛动作。

退订、会员/claim 到期必须从商业集合移除并冻结新开仓/加仓，却不能在 SafetyObligation 未闭合时把账户会话冷却或关闭。`SafetyMonitoringV1` 使用 ADR-0006 定义的唯一 schema，由 Execution Outbox 发布 `Add|Update|Remove`，必须带稳定账户引用、obligation id/generation、单调 `monitoring_fence`、受管订单/敞口摘要、required fact kinds、最低 Account session generation/projection watermark 与原因；Account 以 Inbox 幂等消费、持久化最大 fence 后发布绑定同一 operation/fence、session generation 与 projection watermark 的 `SafetyMonitoringAckV1`。只有 current-fence Remove 已确认且 Execution 的闭合谓词成立，才允许会话进入可关闭候选；重启、漏收或 fence 间隙一律通过版本化全量 snapshot/replay 重建，并保守保留会话。用户 credential 被删除、撤销或 Gateway 不再能提供安全 capability 时，Execution 按 ADR-0006 持久化 `SafetyBlocked` 并发布用户/运营通知与人工处置证据；Account 只消费安全监测 Contract、保守保留会话并维持账户事实，不得借用平台公共市场数据材料、缓存 secret 或默认账户继续读取/下单，也不直读 Execution 私有表。

### 2. 单账户连接必须"先缓冲、后闭合水位"才进 Healthy

握手成功不等于数据完整。任何连接（首次或重连）都遵循：

```text
Disconnected -> Connecting -> Authenticating -> Subscribing
  -> Buffering（缓冲私有事件，不应用到投影）
  -> SnapshotReconciling（拉 signed 快照 + 水位线，切割缓冲队列）
  -> Healthy（水位闭合后实时应用）
```

- 水位闭合前账户保持 `stale`，依赖它的新开仓 fail-closed；
- **重连成功不直接回 Healthy**，必须重新走 Buffering → SnapshotReconciling（断线窗口可能已发生成交，否则永久丢失）；
- 与 [production-runtime §3.2/§9](../production-runtime.md) 中 Account 专属的“先订阅缓冲、再合并快照、闭合前 NotReady”同构；这不是所有 App 的共同启动要求。

#### 2.1 `AccountRecoveryClosedV1` 必须可证明闭合

`Healthy` 与新增风险 admission 只能消费 Account owner 发布的 `AccountRecoveryClosedV1`。该 Contract 至少包含 `ExchangeAccountRef`、`account_session_generation`、每个 private stream/source 的 sequence/cursor 或明确替代比较器、signed snapshot 的 scope/observed time、历史 query overlap window、closed account/order/fill watermarks、projection revision、session readiness、证据 expiry 与比较器版本。无可靠 sequence 的交易所不能只写“时间相近”：必须声明以何种 order/fill identity、查询范围和重叠窗口证明快照与缓冲流闭合；任何来源无法比较、query 覆盖不足、evidence 到期或 generation 不匹配时都不得发布 `AccountAdmissionReady`。

私有流和 signed query 的原始事实只由 Account 消费。Account 先以当前 generation 写入投影，再发布 `AccountFactV1 { source_event_or_query_id, cursor_or_comparator, generation, projection_revision, related_watermark }`；Execution 只经 Inbox 消费该 Contract 更新 OMS，绝不直接持有 User Stream 或重建 AccountProjection。

### 3. 心跳、listenKey 续期、lease 续约一律用 WallClock

所有运行时时效判定使用运行时墙钟，不使用注入 Clock（见 [target §10](../target-architecture.md) DecisionTime/WallClock 区分）：

- 心跳：记录最后收到消息的墙钟时间，超阈值判定连接异常；
- Binance listenKey：每 25 分钟（30 分钟过期留余量）REST 续期；进程重启不复用旧 listenKey；
- lease 续约：每 `lease_ttl / 3` 续约（AccountSession Phase 2+）。

### 4. 多实例分片：静态分片归属 + 动态 lease 持有 + generation 防脑裂

AccountSession Phase 2+ 支持多实例。同一账户在任意时刻只能被一个实例持有连接：

- **分片（静态归属）**：`account_shard_assignment(exchange_account_ref, shard_id)`，决定长期归属；用一致性哈希或显式分配表，**不用朴素 `hash % total_shards`**（N 变化时几乎全部账户换分片，制造迁移风暴）；
- **lease（动态持有）**：`account_lease(exchange_account_ref, instance_id, lease_until, generation)`，决定当前实际持有者；
- lease 获取用 CAS（条件：不存在、或已过期、或本实例）；持有者定期续约；过期即可被抢占；
- **generation 防脑裂与投影写 fence**：lease 带单调递增 generation；旧实例从卡死恢复后若发现自己 generation 已过期，必须立即停止应用事件、关闭连接、不写投影。即使旧实例尚未来得及观察 lease 丢失，AccountProjection 写入也必须以 generation 通过 `AccountProjectionWriteFence` 条件提交；低于已接受 generation 的事件/快照只能拒绝或隔离，不能覆盖新 holder 的闭合结果；
- 抢占接管后走完整 Buffering → SnapshotReconciling，不假设旧实例投影可信；
- 接管延迟上界 = `lease_ttl` + 闭合时间（AccountSession Phase 2 建议 lease_ttl=30 秒，接管 <1 分钟）；
- `total_shards` 变更是显式受控运维动作，不能被环境变量漂移悄悄改变。

AccountSession Phase 1 虽为单实例，也不能把“单实例”当作 fencing。Account owner 必须从唯一的持久 epoch allocator 原子签发单调 `account_session_generation`，并在进程重启、蓝绿重叠、crash-loop 或 session 重建时递增；旧实例/zombie 即使继续收消息，也只能被 `AccountProjectionWriteFence` 拒绝。Phase 1 的 epoch allocator、停止旧输入的顺序和 zombie 注入测试是启用 live admission 的前置证据。

### 5. AccountSession Phase 3 降级态：REST polling 兜底，但受配额与门禁约束

`degraded` 是 AccountSession Phase 3 专用的账户数据来源标记，不是 healthy 的放宽版。它只用于守住既有敞口，绝不成为“用脏数据开仓”的借口：

- **进入 degraded 的条件（全部满足）**：私有流断；该会话仍有 `ManagedExposure`、受管挂单或未结 `SafetyObligation`（仅 `ObservedExternalPosition` 的账户直接 stale）；REST 配额有余量；未超降级最大时长（默认 10 分钟）；
- degraded 下每 `poll_interval`（默认 30 秒）拉 signed 快照，投影标记 `account_data_source=degraded`；
- **degraded 禁止新开仓和加仓**：不得把动态会话状态写入不可变 `ExecutionDecisionContextSnapshot`，也不得通过“保守系数”放行。当前新鲜度、数据源与 polling watermark 只能作为 `AccountSnapshot`/`PreTradeSnapshot`/最终门禁 evidence；
- 仅允许 signed query 对账、持续 Risk、撤单、保护和可证明的 reduce-only（减仓/平仓）。RiskAction 必须经 Execution 的正常 planning/幂等路径，不能直接修改投影或调用 SDK；
- **私有配额协调**：同一 `PrivateQuotaKey` 内 mutation > 对账 > degraded polling；配额紧张时 polling 可退避（30→60→120 秒），但 polling 不得挤占 mutation 预算。大量账户同时 degraded 时按既有受管义务优先级（有 `ManagedExposure` > 有受管挂单 > 仅对账）分配有限 polling 名额；
- **降为 stale 的条件（任一）**：超降级时长；polling 连续失败；配额耗尽且优先级不足；signed query 出现无法自动和解的差异（交 reconciliation 人工处置）。

### 6. AccountSession Phase 3 动态扩缩容：最小迁移 + 先释放再接管 + 迁移风暴护栏

实例动态增减时的 rebalance：

- **最小化迁移**：只迁移达到平衡所必需的账户，一致性哈希增量迁移；
- **优先迁移低成本账户**：`cold` > `degraded` > `healthy`；有持仓 healthy 账户尽量不迁；
- **先停止、后释放、再接管**：源实例先停止接收该账户的新私有流输入，让已开始的带 generation 投影写入完成或被 fence 拒绝，再断开私有流，最后释放 lease；目标实例随后获取 lease（generation +1）并重新闭合水位。交接窗口对外一律是 `stale`，不能承诺或默认标为 `degraded`；只有 AccountSession Phase 3 且满足“私有流断、存在 `ManagedExposure`/受管挂单/未结 SafetyObligation、私有配额可用、未超最大降级时长”的全部条件时，才可由 signed polling 发布 `degraded`。绝不允许两实例同时持有（靠决策 4 的 CAS + generation + write fence 保证）；
- **限速迁移**：受目标实例建连速率（3/IP）约束，分批进行；
- **迁移风暴护栏**：全局并发迁移上限（≤总账户 5%）；两次 rebalance 最小间隔（≥5 分钟）冷却；拓扑变更持久化且版本化，实例以持久化当前版本为准；
- 优雅缩容：先标 NotReady、逐个迁走账户、再退出；崩溃退出回退到 lease 过期接管。

## 运行态与持久化对象

对象只有一个 owner 与一个权威存储/协调介质；不得再以“Redis/Postgres Adapter 均可”留下双真相。完整跨角色矩阵见 [production-runtime §3.3](../production-runtime.md)。

| 对象 | Owner | 权威存储 / 协调介质 | TTL 与重建 |
| --- | --- | --- | --- |
| `account_shard_assignment(exchange_account_ref, shard_id, topology_version, ...)` | Account | Account Postgres 表 | 无 TTL；版本化静态拓扑，显式运维变更，不能由 Redis 或环境变量覆盖。 |
| `account_lease(exchange_account_ref, instance_id, lease_until, generation, ...)` | Account | Redis 原子脚本 / lease | AccountSession Phase 2 默认 30 秒 TTL；到期才可接管，接管后必须重新 Buffering → SnapshotReconciling。不得双写为 Postgres 的第二个权威 lease。 |
| `ExchangeSession` readiness、最后消息墙钟时间、水位闭合、listenKey 续期与重连 metadata | Account | Redis 运行态键 | 短 TTL，不能超过 session 新鲜度预算；键丢失、Redis 重启或进程重启即 `stale`，必须由私有流 + signed snapshot 重建。它不是 Postgres 长期账户事实。 |
| `AccountProjectionWriteFence` / 已接受最大 `account_session_generation` | Account | 与 AccountProjection 相同的 Account Postgres 写事务 | 这是投影写 fencing receipt，不是第二个 lease 真相；旧 generation 的事件/快照写入拒绝或隔离。AccountSnapshot 与 AccountRecoveryClosed evidence 必须携带已应用 generation 与 closed watermark。 |
| `SafetyMonitoringV1` consumer receipt / 最大 `monitoring_fence` | Account | Account Postgres Inbox/receipt | 只消费 Execution 发布的 Contract；Add/Update 立即纳入会话集合，Remove 必须有匹配 Ack 与闭合谓词。事件缺失后通过全量 snapshot/replay 重建，不能把空缓存解释为无义务。 |
| live `StrategyEvaluationState` / evaluator checkpoint | Strategy | Redis，按 `EvaluationScopeId + RuntimeSnapshotId + MarketStreamPartition` 分键 | TTL 覆盖策略预热窗口；丢失后从 Market 已确认输入回放/预热，完成前 Signal lane NotReady。 |
| `RunCheckpoint`、`SimulationLedger` | Research | Research Postgres | 无 TTL 的研究事实；通过 Research Run 重放恢复，绝不进入 live session/lease 或生产 Account/Order/Fill 事实。 |

lease 与分片是“谁持有连接”的协调机制，**不替代业务唯一约束**（[ADR-0006](0006-at-least-once-idempotency-and-recovery.md) 的 AccountOpeningSlot 仍独立存在，防同账户并发开仓）。

## 一致性约束（不随 Phase 变化）

无论哪个 Phase，以下不变量恒成立：

1. 水位未闭合的账户投影不可信，依赖它的新开仓 fail-closed（[production-runtime §11](../production-runtime.md)）；
2. `stale` 或 `degraded` 账户的新开仓、加仓一律拒绝；Account 只投影/发布事实并触发 Risk 评估，已有受管持仓的持续风控、撤单、保护与可证明 reduce-only 必须由 Risk 产生 action、Execution 走正常 planning/幂等/permit/Gateway 路径，不得由 Account 或 Reconciliation 直接 mutation；
3. 同一账户任意时刻最多一个实例持有连接并写投影；即使 lease 观察存在延迟，`AccountProjectionWriteFence` 也不允许旧 generation 覆盖新 holder 的投影；
4. 运行时时效判定用 WallClock，不用注入 Clock；
5. degraded polling 仅纳入对应用户的 `PrivateQuotaKey` 协调，不挤占 mutation 预算；公共 `PublicQuotaKey` 与用户账户容量无关；
6. 商业资格移除会冻结新风险增加，但不会在未结 SafetyObligation 存在时移除安全监测会话；credential capability 缺失则进入 SafetyBlocked/人工处置，不能旁路；
7. AccountSession Phase 间迁移不改变 execution-worker / Risk / Execution 的 owner 边界——它们只消费 Account evidence、AccountAdmissionReady 与版本化安全监测 Contract，不关心连接池内部实现。

## 后果

**正面**：

- 平台可支持的 monitored session 规模有明确的分阶段容量模型与迁移触发条件；
- 连接异常有分级处理（fail-closed / degraded），但 degraded 只守住既有敞口，不放宽新风险增加；
- 多实例脑裂、迁移风暴、断线丢事件等典型事故有显式护栏；
- ExchangeSession readiness 有可落地的实现，与既有 fail-closed、配额、时钟约束一致。

**代价**：

- AccountSession Phase 2/3 引入分片、lease、降级态、rebalance，实现复杂度显著上升；
- degraded 态需要私有配额分级、signed snapshot 对账与 reduce-only 证明，跨 owner 协作增加；
- 出口 IP 成为需规划的运维资源（多 IP、IP 与 API Key 绑定管理）。

**未决**（留待后续 ADR 或实施细化）：

- 一致性哈希的虚拟节点数与 rebalance 精确算法；
- 跨交易所（OKX/Binance/Hyperliquid）连接能力差异在 exchange-gateway 的归一细节；
- 出口 IP 池的编排方式（多机 / VPC NAT / 容器网络）。

## 验收

上线各 Phase 前至少验证（对应 [production-runtime §14](../production-runtime.md) 风格）：

1. 首次连接与重连都经过水位闭合才 Healthy；断线窗口注入成交不丢事件；
2. Binance listenKey 过期/续期失败触发重建，不复用旧 key；
3. 心跳超时判定用墙钟，注入 Clock 不影响心跳；
4. （AccountSession Phase 1）重启、蓝绿重叠或 zombie 旧进程不能复用/伪造 session epoch，旧 generation 的投影写入必被 `AccountProjectionWriteFence` 拒绝；（AccountSession Phase 2）两实例对同账户并发获取 lease 只有一个成功，实例崩溃后账户在 lease_ttl + 闭合时间内被接管；
5. （AccountSession Phase 3）private 流断线且有持仓 → degraded → polling 更新投影 → 恢复或超时降 stale；degraded 新开仓/加仓被拒绝，持续 Risk 仅能经 Execution 生成 reduce-only/保护/撤单计划；
6. （AccountSession Phase 3）配额紧张时 polling 退避而非 fail-closed；mutation 优先级不被 polling 挤占；
7. （AccountSession Phase 3）新实例加入触发最小迁移、限速、先停止/断流/停写/释放再接管；交接期默认 stale，只有满足 degraded 全部条件才转 degraded；rebalance 全局并发不超上限；
8. 退订或会员/claim 到期后，未结 SafetyObligation 仍保留安全监测会话并冻结新风险；`SafetyMonitoringV1` Add/Update/Remove 的乱序、重放、漏收和 Account 重启不能形成监测空窗；credential 撤销导致安全能力不可用时产生 `SafetyBlocked`/人工处置，而不是使用平台公共数据材料。
9. `AccountRecoveryClosedV1` 在有 sequence 与无 sequence 的交易所都能展示逐来源 cursor/comparator、snapshot scope、overlap/query 覆盖和 evidence expiry；任一证明不完整时 AccountAdmissionReady 保持拒绝。
10. 容量压测、冷启动、rebalance 和告警按 `monitored_exchange_account_sessions` 统计，包含仅因 SafetyObligation 留存的会话；达到 Phase 触发阈值前不启用下一阶段能力。
