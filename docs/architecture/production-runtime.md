# Rust Quant 生产运行与恢复

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-28
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)

## 1. 目标

本规范定义代码在生产中实际如何启动、处理交易、应对重复和故障、恢复未完成状态并安全停止。目录结构只有同时遵守本运行协议，才构成可长期使用的生产架构。

## 2. 运行不变量

1. 数据面只使用已发布、带版本、不可变的领域政策快照和已持久化的 `ExecutionDecisionContextSnapshot`；不得在热路径读取“最新配置”、环境变量业务默认值或重新解释 Web JSON；
2. Strategy、Portfolio 和 Risk 的纯计算路径不执行外部 I/O；
3. 外部 mutation 前，Risk 先持久化不可变审批，Execution 再原子持久化稳定身份、完整计划、`SubmitPending`、幂等和 Outbox；
4. 重试沿用原 `idempotency_key` 和 client order identity；
5. 网络超时表示结果未知，不等于交易所没有执行；
6. Account 只投影外部账户与成交事实；
7. Reconciliation 只发现差异和发出修复命令，不绕过 owner 状态机；
8. 没有有效 RiskDecision 和保护计划不得提交开仓订单；
9. 未有独立 Risk Reservation 协议前，Execution owner 以持久 `AccountOpeningSlot` 保证同一 `ExchangeAccountRef` 只有一个未安全收敛的开仓 OrderIntent；Web wire field `execution_account_ref` 是商业 binding identity，必须经冻结的 `ExecutionAccountBindingV1` 解析到该稳定物理账户 ref，不能自身充当第二把 slot 的键，credential reference/revision 也不能替代它。保护、减仓和紧急平仓只在可证明 reduce-only 且先冻结风险增加 claim 时优先。

### 2.1 六角色运行拓扑（Topology rollout T1，非账户会话容量阶段）

Core 保持同一仓库、同一 `core-runtime` image 和同一 `quant_core` owner database，不按策略拆微服务。目标中六个角色分别是独立 App Cargo package，但共享一个只含生产 binary 的镜像；Research/Backtest/Paper 研究入口属于独立 `quant-lab` 工件，schema-tool 属于 `core-maintenance`。在总 CLI package 尚未拆除前，这一工件隔离仍属于迁移目标，不能宣称生产镜像已经满足。生产 Compose 的默认长期进程收敛为六个显式组合根：

| 运行角色 | T1 目标装配职责 | 默认不装配 |
|---|---|---|
| `control-api` | Core internal HTTP API | 策略循环、行情扫描、执行轮询 |
| `market-worker` | symbol sync、Market Velocity radar、K 线 scanner、最多 2 天的在线缺口修复 | Web 执行 secret、交易 mutation、paper、60 天历史 backfill |
| `signal-worker` | Vegas 与 Vegas Universal 共享行情连接；按启动时精确 config ID 过滤；按 `strategy_key@preset` 装配不可变 Market Velocity handoff lane | Execution worker lane、Market radar |
| `account-worker` | 每 `ExchangeAccountRef` 唯一私有 User Stream、signed account/order query、ExchangeSession、AccountProjection 与 `AccountFactV1`；为持续 Risk 提供账户事实 | OMS 任务 claim、订单 Outbox/attempt、直接 mutation capability |
| `execution-worker` | 新订单与风控平仓任务的 claim、lease、门禁和执行；经 Inbox 消费 `AccountFactV1` 更新 OMS | 原始私有流持有、AccountProjection 重建 |
| `reconciliation-worker` | 只读差异检测、恢复 schedule 与 typed owner command | 新订单 claim、私有流持有、Account/Execution 私有表直接写入 |

`schema-tool`、paper observation、全市场只读成交量观察和大范围历史 backfill 必须通过独立工件的 profile/短生命周期 Job 显式启动，不计入默认长期拓扑，也不得出现在 `core-runtime` binary allowlist。旧的按策略、preset 和 scheduler 拆分的容器只保留在 `legacy-runtime` profile，供一次性迁移回退，不得与新角色并跑消费同一任务。

Topology rollout T1 只完成运行入口收敛，不代表 Account、Execution 与 Reconciliation 的目标业务边界已经全部迁移。任何尚未迁完的 legacy 例外必须只记录在对应 Migration Manifest/Evidence 中，不能因为 T1 正在进行而成为长期运行能力或新的调用入口：

- `account-worker` 的目标边界是 AccountProjection、ExchangeSession 与持续 Risk 输入，绝不持有 OMS Outbox 或直接 mutation capability；
- `reconciliation-worker` 的目标边界是差异检测、恢复编排与 typed owner command，不是 report replay 的别名；
- Market/Signal 的 lane freshness、checkpoint、依赖 readiness 与结构化并发必须作为角色验收证据，进程存活不是 Ready；
- Signal 必须从已发布的精确 Strategy RuntimeSnapshot 装配 `strategy_key@preset` lane；缺失或错配时 fail-closed，不回退到另一策略配置。

第一次从 legacy 容器切换到六角色拓扑属于显式运维动作。发布脚本检测到旧容器时必须要求 `DEPLOY_SIX_ROLE_CUTOVER_CONFIRM=replace-legacy-runtime-with-six-roles`，并保存旧服务到镜像的拓扑快照；确认前不得停止旧容器。旧的单次/scheduler live-handoff 都属于待清退运行时，禁止与新 `signal-worker` 并跑。首次切换后的 rollback 在六角色前序镜像不完整时恢复这份旧拓扑，后续发布才使用六角色逐服务 previous image。部署脚本只判定六个进程在稳定窗口内未退出或重启；CI 随后必须执行 `verify_production.sh`，用 revision、非敏感配置、错误日志和 checkpoint 证据做只读运行验收，两者都不等同于尚待补齐的依赖级 readiness。

发布入口必须保持可维护：`promote_stable.sh` 与 `rollback.sh` 只能作为薄入口，六角色清单来自版本化的 `scripts/deploy/runtime-services.txt`，SSH/Compose、安全前置检查、镜像快照、清退和稳定性等待统一由共享部署实现负责。禁止通过 CI Secret 临时改写运行角色，也禁止在两个入口中复制远端安全逻辑。首次 cutover 与 legacy restore 属于迁移期兼容；完成六角色生产验收及约定的回滚窗口后，应按迁移计划删除该分支，而不是永久扩张日常发布路径。

## 3. 进程启动、角色就绪与运行态存储

### 3.1 所有 App 的共同启动基座

所有长期 App 都先完成同一最小基座；**共同基座不等于每个角色都要建立用户私有流或恢复 OMS**：

```text
解析本 App 的强类型配置
  -> 仅读取该角色所需的最小 Secret / capability
  -> 初始化日志、指标和 Trace
  -> 创建该角色必要的 Adapter
  -> 校验 schema 与其消费 Contract 的兼容性
  -> 恢复该角色拥有的 checkpoint / lease / durable work
  -> 执行该角色专属的流、快照、reconciliation 或 warm-up
  -> 完成该角色的 startup / readiness 检查
  -> 只开始接收该角色被授权的工作
```

所有有 mutation 能力的角色在完成自身最终门禁前保持 Dispatcher 禁用。任一**本角色必需**依赖失败时不得进入 Ready；研究、通知等非交易依赖可以按 App 策略降级，但不得让一个与其无关的私有流、用户凭证或 OMS 恢复阻塞 Market、Signal、Control 的 Ready。

### 3.2 角色 readiness 矩阵

| 角色 | Ready 前必须证明 | 明确不要求 / 不拥有 |
| --- | --- | --- |
| `control-api` | 自己的 Control 配置、只读投影和内部 Contract 路由可用 | 用户私有流、用户 `credential_reference`、OMS Outbox/attempt 恢复、交易 mutation |
| `market-worker` | `MarketDataAccessCredential`（若数据源需要）、`PublicQuotaKey` 协调、公共行情流/拉取新鲜度、Market checkpoint gap 已处理 | 用户私有流、用户 `credential_reference`、私有账户快照、OMS Outbox、mutation capability |
| `signal-worker` | 所需 Market 输入新鲜、精确的已发布 Strategy RuntimeSnapshot、lane checkpoint/warm-up 完整 | 用户私有流、用户凭证、账户 signed snapshot、OMS Outbox/订单恢复 |
| `account-worker` | ProcessReady：连接管理器、Account 运行态恢复、Gateway read/stream route 与配额协调可用；单个账户只有水位闭合后才发布其 `AccountAdmissionReady` | OMS 任务 claim、订单 Outbox/attempt 恢复、直接 mutation capability |
| `execution-worker` | ProcessReady：Web claim/幂等、自己的 Order/Protection/attempt/permit/Outbox 已恢复；每次新增风险另按目标账户的 `AccountAdmissionReady` 与最终 Gateway 门禁判断 | 用户私有流的持有与生命周期；原始用户 credential material |
| `reconciliation-worker` | 自己的 query/recovery schedule、所需的只读 Gateway capability、Account/Execution 事实读取与 owner command 路由 | 持有用户私有流、用户凭证 material、OMS Outbox 的 owner 恢复 |

`ExchangeAccountRef` 是稳定的物理账户身份，至少区分 `exchange × user account/subaccount × product scope`；`credential_reference + credential_revision + credential_revocation_generation` 是可轮换授权材料/失效版本，不得充当 session、lease、shard、fence、Risk 或 opening slot 的主键。`account-worker` 是唯一持有用户 `ExchangeSession`/私有流并产生 `AccountFactV1` 的角色；`execution-worker` 是唯一恢复和投递 OMS Outbox、attempt 与 permit、并消费 AccountFact 更新 OMS 的角色。任何角色需要用户交易所读取时，均只能调用受限 Gateway capability，不能读取或缓存原始用户凭证。

### 3.2.1 `ProcessReady` 与 `AccountAdmissionReady` 不是同一个状态

运行角色的健康状态分为两个作用域：

- **`ProcessReady(role)`**：该角色自己的 storage、Contract、调度、Gateway route 与恢复循环可安全推进。单一用户账户的私有流断线、会员变化或 credential 撤销不得把整个 `account-worker` / `execution-worker` 标成 NotReady，更不能阻塞其他健康账户或既有敞口的安全处置；
- **`AccountAdmissionReady(ExchangeAccountRef)`**：Account owner 对某账户发布的新增风险门禁，至少绑定当前 `account_session_generation`、闭合 watermarks、`ExchangeSession` 状态、signed preflight/能力证据与 observed wall clock。Web wire field `execution_account_ref` 必须解析为该同一稳定身份；它只决定该账户是否可以新开仓/加仓；
- **Safety path**：已有受管敞口的 Query、Cancel、Protect、Reduce、Close 与 Reconciliation 不得把 `AccountAdmissionReady=false` 误解释为“停止处置”。它们仍受各自的 reduce-only、credential capability、permit、配额与人工升级门禁约束。

`/ready` 必须报告角色作用域；按账户 admission 的拒绝通过结构化 Account evidence、指标和用户/运营投影呈现，而不是让一次账户故障导致进程重启或全局不可用。

### 3.3 运行态存储矩阵

运行态对象不得以“Redis/Postgres Adapter 均可”留下双真相。下表定义默认目标实现；Redis 丢失或 TTL 过期一律按未就绪处理并从 owner 的权威输入重建，不能把缓存当交易事实。所有 `account_*` 对象以 `ExchangeAccountRef` 而非 API Key/credential reference 为主键。

| 对象 | Owner | 权威存储 / 协调介质 | TTL、恢复与禁止事项 |
| --- | --- | --- | --- |
| `account_shard_assignment` | Account | Account Postgres 表 | 以 `ExchangeAccountRef` 为键、无 TTL；版本化拓扑事实，显式运维变更。不得以环境变量或 Redis 覆盖。 |
| `account_lease` | Account | Redis 原子脚本/lease | 以 `ExchangeAccountRef` 为键；AccountSession Phase 2 默认 30 秒 TTL；到期可接管，接管者必须重新闭合水位。它不是 Postgres 业务事实，也不允许双写成第二个权威 lease。 |
| `ExchangeSession` readiness、最后消息时间和 reconnect metadata | Account | Redis 运行态键 | 短 TTL，不能超过其新鲜度预算；失效、Redis 重启或 owner 重启都先标 `stale`，再由私有流 + signed snapshot 重建。不得写入 Postgres 伪装成长期账户事实。 |
| `AccountProjectionWriteFence` / 已接受的最大 `account_session_generation` | Account | 与 AccountProjection 同一 Account Postgres 写事务 | 它是投影写入 fencing receipt，不是第二个 lease 真相。旧 generation 的事件/快照写入必须被拒绝或隔离；AccountSnapshot 必须带已应用 generation 与闭合 watermark。 |
| `SafetyMonitoringV1` Inbox receipt / 最大 `monitoring_fence` | Account | Account Postgres Inbox/receipt | Add/Update 立即进入 monitored session 集合；只有匹配 Ack 的 Remove 加已验证 obligation 闭合才允许退出。漏收、重启或缓存丢失时用全量 snapshot/replay 重建并保守保留会话。 |
| live `StrategyEvaluationState` / evaluator checkpoint | Strategy | Redis，以 `EvaluationScopeId + RuntimeSnapshotId + MarketStreamPartition` 分键 | TTL 覆盖预热窗口；丢失时从 Market 已确认数据回放/预热，完成前 Signal lane NotReady。不得把它写入 Research Run 或 Account 事实。 |
| `RunCheckpoint`、`SimulationLedger` 和 `ResearchEvidence` | Research | Research Postgres（大对象另以内容哈希存对象存储） | 无 TTL 的可恢复研究事实；只由 Research Run 重放恢复，不进入 live Redis checkpoint 或生产 Account/Order/Fill 表。 |

公共市场数据的固定访问材料不是上述任何账户状态：它只在 Market/App/Gateway 公共读取边界存在，身份和配额规则见 §10.2 与 [ADR-0013](adr/0013-user-execution-request-and-public-market-data-credentials.md)。

`AccountProjectionWriteFence` 只消费单调 `account_session_generation`；在 AccountSession Phase 2+，该 generation 必须来自 Redis lease，Phase 1 则由唯一的持久 epoch allocator 在每次会话重建、重启或蓝绿重叠时原子递增签发，不能由进程内计数或环境变量推断。它不续约、不抢占、也不决定 holder，因此绝不形成第二个 lease 真相。它的作用是让暂停后恢复的旧实例即使尚未来得及观察 lease 丢失，也不能用旧流消息覆盖新 holder 已闭合的投影。Execution 的新增风险最终门禁必须拒绝 generation 或闭合 watermark 与当前 Account evidence 不匹配的 AccountSnapshot；Phase 1 也必须验证 zombie 旧实例无法写入或放行。

### 3.4 `AccountRecoveryClosedV1`：Account 与 Execution 的恢复交接

私有流、signed snapshot、gap 合并与 AccountProjection 只能由 `account-worker` 完成。水位闭合后，Account owner 先发布 `AccountFactV1`（原始 source event/query identity、cursor 或替代比较器、generation、projection revision、关联 watermark），再发布版本化 `AccountRecoveryClosedV1`（或等价 owner Contract）。后者至少包含 `ExchangeAccountRef`、`account_session_generation`、每个 source 的 sequence/cursor 或 comparator version、signed snapshot scope/observed time、history query overlap、closed account/order/fill watermarks、`ExchangeSession` readiness、AccountProjection revision、observed wall clock 与 evidence expiry。无可靠 sequence 的交易所必须以已声明的 order/fill identity、查询范围和 overlap 比较器证明闭合；任一来源覆盖不足、无法比较、证据过期或 generation 不匹配都不得产生 AccountAdmissionReady。

`execution-worker` 不订阅私有流，也不自行重建 AccountProjection；它只通过 Inbox 消费 `AccountFactV1` 更新 OMS，在恢复自己的 OMS 后，只能对持有与当前 Account generation/watermark 相匹配 close evidence 的账户恢复新增风险 Dispatcher。没有 close evidence、evidence 过期或 generation 不匹配时，新增风险保持 fail-closed，安全处置走上节 Safety path。

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

### 4.1 `BarFinalization` 与 `RequiredMarketEvidence`

`evidence_cutoff_at` 只能限制决策可见时间，不能单独证明 K 线已经最终确定。Market 为每个 `exchange + instrument + timeframe + market_data_source_profile_id` 发布版本化 `MarketEvent/BarFinalization`，至少带 source event identity、exchange event time、observed wall clock、sequence 或替代比较器、K 线 `[open, close)` 区间、`is_final`、revision/correction generation、continuity/gap generation、source profile/failover identity 与质量状态。策略若声明 bar-close 输入，只能消费 `is_final=true` 且未被后续同 generation correction 否定的 bar；若显式允许 intrabar，则必须把该能力、最大迟到和 revision 规则写入 Strategy RuntimeSnapshot，不得把未完成 K 线伪装成已完成历史输入。

Strategy RuntimeSnapshot 同时发布 `RequiredMarketEvidence`：其规范集合列出所有必需的 exchange/instrument/timeframe/source profile、bar/tick finality、最大年龄与允许的 fallback。PreTradeSnapshot 与 Dispatcher 必须对该集合逐项聚合 `MarketDecisionReadiness`；例如 1m 新鲜不能放行依赖 4H 未闭合或已 gap 的策略。乱序、修订、源切换、无法回补的 gap 或任一必需 evidence 过期时，新增风险 fail-closed；Research 以同一 finalization/correction 规则冻结 DatasetManifest，不能在回测时用事后修订 K 线补造 live 决策。

### 4.2 live 交易所能力与风险估值准入

Gateway owner 必须为每个启用的 `exchange × product × instrument capability` 发布版本化 `ExchangeExecutionCapabilityProfileV1`：stable client identity/duplicate rejection、signed query 与 `DefinitivelyAbsent` 可见性窗口、private-stream cursor/comparator、attached/post-fill protection、reduce-only、position/margin mode、精度/contract multiplier、rate limit、time-sync 与错误语义。任何 profile 无法证明 Unknown recovery、最大未保护窗口或 reduce-only 的组合必须标为 `Unsupported`，不得仅因 SDK endpoint 存在而开启 live。

Risk 在每次新增风险审批时固定 `RiskValuationSnapshotV1`，至少包含 `ExchangeAccountRef`、现货/线性/反向产品类型、settlement/collateral currency、position/margin mode、contract multiplier、mark/index/FX source 与 freshness、available equity、挂单/外部仓位占用、funding/fee 与 liquidation buffer。无法可靠估值的产品、账户或外部仓位默认 Blocked；首批只支持的产品类型必须写入 capability profile，不能让通用字段静默猜测。

## 5. 策略到订单

```text
MarketSnapshot
  -> StrategyEvaluator
  -> StrategySignal
  -> 用户路径：取得 Web `ClaimExecutionRequestV1` 的一个或多个 ExecutionRequest
  -> 每个请求解析四个 Published Domain Policy Snapshot
     + Execution 原子持久化各自的 ExecutionDecisionContextSnapshot + 请求幂等身份
  -> 按同账户、明确 decision window 形成 PortfolioEvaluationBatch
  -> Portfolio allocation / netting
  -> PortfolioTarget
  -> 固定 PreTradeSnapshot
  -> Risk owner 以 risk_evaluation_id 持久化不可变 RiskDecision
  -> Execution 纯规划：生成不可变 ExecutionPlanningValue（含 child OrderPlan）+ ProtectionPlanningValue
  -> 准备期 readiness 与审批时效检查
  -> Execution owner 原子取得 AccountOpeningSlot
     + 从同 hash planning value 初始化 live OrderIntent + ExecutionPlan + ProtectionPlan
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

`ExecutionPlanningValue` 是 Execution 纯、规范可序列化的规划结果，持有有序 child `OrderPlan` 与 `ProtectionPlanningValue`；它没有 OMS 身份、状态或恢复生命周期，Research/纯 parity 只比较该 value、child plans 与保护规划。只有 live intake 的 Execution owner transaction 才能从同一 planning hash 初始化持久 `OrderIntent`、`ExecutionPlan` 与 `ProtectionPlan`。`ExecutionPlan` 是唯一的 live OMS Aggregate，保存 planning value/hash、parent OrderIntent、child snapshot、身份、版本、风险决策引用和状态；`OrderPlan` 没有独立持久化生命周期，不能替代或单独冒充该 Aggregate。live/Paper 另验证 Aggregate 初始化与状态迁移，不把它误作 Research parity 对象。

### 5.2 Web 商业授权交接

用户自动交易路径必须区分 Web 商业任务和 Core 订单事实：

- Web 根据会员、`strategy x symbol` combo、凭证和产品资格创建 `ExecutionRequest`；
- `quant_web.execution_tasks` 在迁移期承载该请求，不是 Order/Fill 的事实源；
- `ExecutionAccountBindingV1` 只携带稳定 `execution_account_ref`/`ExchangeAccountRef` 与 `credential_reference + credential_revision + credential_revocation_generation` 等非敏感授权引用，不传明文凭证，也不使用 email、展示名或可变 slug 推断交易身份；ExchangeAccountRef 是 Account/Portfolio/Risk/Execution 的账户主键，credential revision/revocation generation 只作为 Gateway capability 与 admission Evidence 的授权版本；
- `ClaimExecutionRequestV1` 返回 `ClaimExecutionRequestReceiptV1`，至少带 `execution_request_id`、`claim_id`、单调 `claim_fence`、`claim_expires_at` 与 `execution_account_binding_ref`；稳定账户和 credential 版本只从该冻结 binding 解析，不在 receipt 复制第二份事实。`claim_fence` 本身就是同一 request 的 claim generation，不再维护第二个同义字段。Context、batch/source mapping 与 plan 只持久化 receipt 的规范引用/hash；Renew、Release 与 Outcome 使用同一 current fence，Web 以 CAS 接受迟到结果，避免旧 worker 覆盖新 claimant；
- Contract 同时携带精确 `risk_profile_ref + version` 以及必要的不可变授权约束；它是配置来源和授权引用，不是可直接执行的 Core 风险政策；
- Core Risk 校验单位、范围、默认值和账户/产品兼容性，将相同 profile version 幂等解析为相同 `RiskPolicySnapshot` hash；缺失、撤销或不兼容时 Blocked，不回退默认风险；
- Core 解析其余 Published Policy Snapshot 后，由 Execution 将四个引用绑定为 `ExecutionDecisionContextSnapshot`，与请求 intake/幂等身份一同持久化，再执行 Portfolio、Pre-trade Risk 和 OrderIntent 创建；
- Core 的 Order、Fill、Protection、Reconciliation 结果经 Core API/Event 投影给 Web；
- Core 更新 Web 请求状态时调用 Web owner API，不直写 Web 数据库。

#### 商业 claim 的 Safety Obligation 尾巴

`ClaimExecutionRequestV1` 只授权该 Web 请求在其有效期内增加风险。一旦已签发可能触达交易所的 Submit permit、已观察到成交/敞口，或仍有保护、Unknown result、撤单与对账责任，Execution 必须从原 `ExecutionRequest` 与 `ExecutionDecisionContextSnapshot` 建立或延续一个持久 `SafetyObligation`。

- `SafetyObligation` 只允许 Query、Reconciliation、Cancel、Protect、Reduce、Close 等收敛动作；每个动作仍须证明不增加绝对敞口，Cancel 还必须是取消未成交的风险增加量，或有已验证的保护替换，不能丢弃已有敞口的有效保护。它不是 Core 创建的 `ExecutionRequest`，不能用于新开仓、加仓或重新批准风险；
- 退订、会员到期、claim 到期或商业资格撤销只会冻结新的风险增加并把结果回传 Web，不能删除已有的安全处置责任、断开仍需监测的账户会话，或把已知敞口当作已关闭；
- 解除 `SafetyObligation` 只能在同一当前 Account generation 的 evidence 同时证明该 obligation 的 `ManagedExposure=0`、无受管开放订单、无 Unknown/未终态 attempt/可消费 permit、保护已 Closed 或被验证替换、且 Account watermark 覆盖最终 fill/资金/仓位后发生；仅订单终态或已有 stop 都不足以关闭；
- Execution 通过 Outbox 发布 `SafetyMonitoringV1 { safety_obligation_id, execution_account_ref, exchange_account_ref, operation, monitoring_fence, obligation_generation, managed_order_exposure_summary, required_fact_kinds, minimum_account_session_generation, minimum_projection_watermark, reason, issued_at, idempotency_key, causation_id }`；Account 用 Inbox 幂等消费并持久化最大 fence，回发绑定同一 operation/fence、session generation 与 projection watermark 的 `SafetyMonitoringAckV1`。只在严格闭合谓词成立且 current-fence Remove 得到 Ack 后，账户才可退出监测集合；漏收、重启或 fence 间隙用版本化全量 snapshot/replay 修复，并保守保留会话；
- 若用户 credential 被删除、撤销或 Gateway 无法提供所需的安全能力，必须持久化 `SafetyBlocked`/人工处置证据并通知 Web；默认阻止删除最终确认并冻结新风险。只有产品显式授权的“保留最小受限安全 capability 至收敛”或“责任人、通知 receipt、确认截止与人工 SLA 完整的 `ManualSafetyIntervention`”可以继续处理；不得以平台公共数据 Key、缓存的原始 credential 或任何默认账户绕过该限制。

### 5.3 Portfolio

`PortfolioEvaluationBatch` 是账户级、确定性的 Portfolio 评估输入单位。它只聚合已取得有效 Web claim、属于同一 `ExchangeAccountRef`（wire field `execution_account_ref` 的稳定语义）且落在明确 decision window 的请求；单个请求也形成 size=1 的 batch。其至少固定账户引用、decision time/window、按稳定顺序排列的 source request/claim receipt/context identity、所用 Account/Market evidence 引用及 `batch_hash`。

- Portfolio 只拥有 batch 的排序、净额、容量分配和 `PortfolioTarget` 语义；Execution 仍拥有每个 source request 的 claim receipt、商业状态回写与 OMS 结果；
- `PortfolioTarget`、RiskDecision 与后续计划必须可回溯到 `batch_hash` 及每个 source request 的贡献、被净额抵消或 Blocked 原因；不得以 batch 名义合成 Core `ExecutionRequest`，也不得读取 Web 表补齐请求；
- 相同 batch 输入在相同 Policy/Evidence 下必须产生相同 source ordering、净额和 source outcome 映射，不能受消息到达或 symbol 遍历顺序影响。

- 合并同一账户下多个策略的目标；
- 处理相反信号、资本预算、策略优先级和目标仓位；
- 输出目标状态，不直接调用交易所。
- 账户级 Portfolio 只有在 `ExecutionRequest` 已提供稳定账户上下文后执行；`signal-worker` 不为未知用户账户提前计算最终数量。

### 5.4 Risk

- 使用冻结的 Market、Account、Portfolio 和 instrument snapshot；
- 返回 Approve、Reject 或 Resize；
- 由 Risk Use Case 通过 owner Write Port 持久化不可变 `RiskDecision`；`risk_evaluation_id` 绑定生成该目标的 `PortfolioEvaluationBatch` hash/source 映射、各来源 Context hash、PortfolioTarget/PreTradeSnapshot hash、Market/Account/Instrument evidence 与 generation，同一 evaluation 重放返回同一决策，新评估使用新 generation；Risk Policy 本身仍是无 I/O 的纯计算；
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
- `Acknowledged` 只由交易所同步响应、signed Query，或 Account owner 根据 User Stream/query 发布的 `AccountFactV1` 对原 client order identity 的明确接受证据推进；Execution 不直接消费原始私有流，也不能由 Outbox 已发布、socket write 或 attempt 记录推断；
- `Rejected`、`Expired`、`Blocked` 分别表示交易所拒绝、时间/交易所过期、本地不可恢复门禁失败，并保存 terminal source/reason/evidence；
- Execution owner 的一次原子事务必须覆盖 `AccountOpeningSlot` claim、Inbox/幂等与唯一身份、`RiskDecision` 引用及对父 OrderIntent/planning hash 的唯一绑定、由同 hash `ExecutionPlanningValue` 初始化的 `OrderIntent`、完整持久 `ExecutionPlan`（含 child snapshot）与初始为 `Planned` 的 `ProtectionPlan`、`SubmitPending`、提交 Outbox 和审计字段；
- 只有该事务提交后才可确认上游或发布 Outbox；交易所 I/O 只能由 Fenced Exchange Mutation Gateway 在 Dispatcher 的 claim/attempt/permit 短事务提交后发起，数据库事务不得跨越网络调用；
- Dispatcher 必须执行提交时最终门禁并持久化所用 snapshot/capability/generation 引用与 checked time；对新增风险还必须验证每个 source 的 `ClaimExecutionRequestReceiptV1` request/claim/current `claim_fence`/`claim_expires_at` 仍为 Web 当前 receipt、`ExecutionAccountBindingV1` 的 ExchangeAccountRef/credential revision/revocation generation 未失效、`ResolvedMarketEvidenceSetV1` 全部 Ready 且 TTL 未过期、AccountAdmissionReady、Kill Switch 与 `ExchangeExecutionCapabilityProfileV1` 均满足。超时进入 `Expired`，不可恢复失败进入 `Blocked`，可恢复 blocker 保持 `SubmitPending`，并在确认当前 delivery 的同一事务 rollover 到新 mutation generation 的 delayed Outbox 或 `MutationRetrySchedule`，由 durable scheduler/event 唤醒，禁止忙循环；
- 外呼前在短事务中以 `expected_aggregate_version`、原 mutation kind 对应的 expected Pending state（Submit 为 `SubmitPending`、Cancel 为 `CancelPending`、Protect 为原 `AttachedPending`/`PostFillPending`）、空 `send_claim`、current account/order fence 与仍有效的 source claim set 为条件原子设置 claim、记录 `ExecutionMutationAttempt(Started)` 并签发短期 `MutationPermit(Issued)`；permit/capability 绑定 source claim receipt hash，并且 expiry 不得晚于最早 claim expiry；
- Submit/Cancel/Protect mutation event 必须携带 `mutation_event_id`（等于 Envelope `event_id`）、`mutation_generation` 和 `expected_aggregate_version`；claim/attempt/permit 绑定三者，旧或重复 delivery 与 current generation/version 不匹配时只 ack/no-op；
- Dispatcher 只能把 permit 与固定 payload 交给 Fenced Gateway。Gateway 在真正网络 I/O 边界前原子校验 attempt/version/fence/gate generation/source claim receipt hash/payload hash/expiry 并将 current permit 置为 `Consumed`；revoked/stale/expired/CAS 失败返回 `DefinitelyNotSent`，不得调用 SDK；raw SDK mutation capability/credential 对 Dispatcher 与其他 App 物理不可达；
- Attempt 至少记录 mutation kind、stable mutation identity、attempt number、`mutation_event_id`/`mutation_generation`/`expected_aggregate_version`、payload/plan hash、fence、source claim receipt hash/最早 claim expiry、gate evidence、Started/Confirmed/Indeterminate/DefinitelyNotSent/DefinitivelyAbsent 和 started/completed time；Permit 至少记录 attempt、上述三个 mutation 授权字段、fence/gate generation/source claim receipt hash/payload hash/expiry 与 Issued/Consumed/Revoked/Expired。Outcome/Release 回写 Web 时必须携带原 claim id 与 current `claim_fence` 并经 Web CAS；outcome、Order/Protection transition、permit 终态与后续 Outbox 在同一 Execution 事务提交；Unknown outcome 只允许 query/reconciliation/alert 等恢复 Outbox，不得直接创建同 kind 的 mutation Outbox；
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
  -> Account owner 解析、watermark 闭合与 AccountProjection
  -> AccountFactV1（经 Inbox）
  -> Execution OMS 状态迁移
  -> 实际余额、持仓、保证金和 PnL evidence
  -> Continuous Risk
  -> Continue / Reduce / Cancel / Close / KillSwitch
```

- Account 同时记录 exchange time、observed time 和 source；
- Account 是原始 User Stream/signed query 的唯一 consumer；Execution 只消费 AccountFactV1，绝不直接持有私有流或重建 AccountProjection；
- 用户流中断后 Account 进入 stale，依赖该账户的新开仓默认 fail-closed；
- Account/Reconciliation 只发布事实或触发 Risk 评估；RiskAction 必须通过 Execution 的正常 planning、幂等、permit 与 Gateway 路径执行，不直接调用交易所；
- 平仓、减仓和保护单也必须沿用订单状态机、幂等和审计规则。
- 保护/减仓/紧急平仓旁路必须由 Gateway 证明 reduce-only 或等价不增加绝对敞口；数量不得超过当前可证明敞口；
- KillSwitch/紧急平仓先通过 typed Execution command 冻结新风险增加 claim、推进 account gate generation，再查询/取消或接管已有开仓订单；迟到 Fill 和保护完成对账前不得恢复新开仓。

### 7.1 `ManagedExposure` 与 `ObservedExternalPosition` 不可混用

Account 必须投影交易所观察到的全部余额、持仓、订单与成交，但“看见”不等于 Core 获得自动处置权限：

- **`ManagedExposure`**：可以追溯到原 Web `ExecutionRequest`、live Context、Core Order/Plan 与对应 `SafetyObligation` 的敞口或挂单。只有它可以进入 Continuous Risk 的自动 Query、Cancel、Protect、Reduce、Close 路径，且每次 mutation 仍受计划、permit、账户 evidence 与 reduce-only 门禁约束；
- **`ObservedExternalPosition`**：只由交易所投影观察到、没有上述 Core 来源链的手动/外部仓位。Account 和 Reconciliation 可以读取、报告、标记风险与请求人工处置，但不得仅因拥有用户 API Key 就自动下保护、撤单、减仓或平仓；
- 来源链不完整或相互矛盾时必须归类为 **`UnknownOrigin`**，按 `ObservedExternalPosition` 的禁止自动 mutation 规则处理，直到 owner evidence 闭合；
- `ObservedExternalPosition`/`UnknownOrigin` 虽不能自动处置，仍必须进入 `RiskValuationSnapshotV1` 与 PreTradeSnapshot：至少占用同标的净敞口、保证金/可用权益、杠杆和风险预算。默认阻塞与其冲突或无法安全估值的新风险；禁止既不接管、又不计入风险；
- 若未来需要接管外部仓位，必须由 Web 创建单独、版本化、可审计的账户管理授权 Contract，明确账户、允许动作、有效期与撤销语义；不能通过补一个 Core request、复用过期 subscription 或平台公共数据 credential 推断授权。

## 8. 对账与恢复

Reconciliation 周期性比较：

- 内部订单与交易所 open/history orders；
- 内部成交与交易所 fills/trades；
- Account 投影与交易所 balance/position snapshot；
- 保护单计划与交易所实际保护单；
- 实际已成交敞口与有效保护数量；
- 内部 lease、outbox、checkpoint 与实际任务进度。

每个对账 run 必须持久化 `ReconciliationCoverageV1`：ExchangeAccountRef、查询 endpoint/profile、历史范围或 cursor、overlap、交易所可见性窗口、source watermark、比较器版本、开始/完成时间、覆盖 verdict 与下次 deadline。单次 not-found、过期历史窗口或未覆盖的 fill/order 不得被当作“无差异”；不能在 deadline 内证明覆盖完整时，新增风险 fail-closed，并保留查询/告警/人工处置证据。

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
- Risk command 只可对 `ManagedExposure` 触发暂停、减仓或 kill switch；
- 无法自动证明安全的差异进入人工处置，不自动猜测。

## 9. 按角色重启恢复

所有角色重启时先标记 NotReady、停止接收新工作并恢复自己拥有的运行态；后续步骤不得套用成“所有 Worker 都恢复私有流和 OMS”。

- **Account**：恢复或重新取得以 `ExchangeAccountRef` 为键的 `account_lease`，先订阅并缓冲其持有账户的 User Stream；读取 signed account/order snapshot 或 query watermark，按 sequence 合并缓冲事件并补 gap；没有可靠 sequence 时按 `AccountRecoveryClosedV1` 声明的 cursor/comparator、query scope 与 overlap 重复 query/reconciliation 直到闭合。闭合事实只能以当前 generation 经 `AccountProjectionWriteFence` 写入 `AccountProjection` 与 `ExchangeSession`，先发布 `AccountFactV1`，随后发布未过期的 `AccountRecoveryClosedV1`，才让该账户恢复 `AccountAdmissionReady`。
- **Execution**：保持 mutation Dispatcher 禁用，恢复 `SubmitPending`、`Acknowledged`、`PartiallyFilled`、`CancelPending`、`Unknown`、`AccountOpeningSlot`、attempt ledger、permit ledger 和自己的 Outbox；经 Inbox 重放 AccountFact 更新 OMS。无 attempt 的 `SubmitPending` 才可等待首次 fenced claim；存在未完成 attempt 的订单必须先按原 identity 查询并保持/进入 `Unknown`。新增风险的 Outbox 只有在核对 opening slot、cumulative fill、Protection、匹配当前 generation/watermark 的 `AccountRecoveryClosedV1` 与仍有效 source claim receipt 后才可恢复；已有 `SafetyObligation` 的安全路径不以 `AccountAdmissionReady` 替代其自身的 reduce-only、credential capability、permit 与人工升级门禁。
- **Market**：恢复 Market checkpoint，补齐公共行情 gap 并重新证明数据质量/新鲜度；不得为此建立用户私有流或读取用户凭证。
- **Signal**：按其策略快照和 MarketStreamPartition 恢复/预热 evaluator state；预热、窗口或 Market 输入未闭合时 lane 保持 NotReady，不恢复订单或账户状态。
- **Reconciliation**：恢复只读查询与差异处理 schedule；通过 owner command 发出修复请求，不接管 Account 的私有流或 Execution 的 Outbox。
- **Control**：恢复自身 Control read model、路由和配置投影；不因账户会话或 OMS 账本未恢复而不可用。

不允许仅因为进程启动成功就宣告 Ready。对 Account 而言，流/快照 gap 未闭合不得发布该账户 `AccountAdmissionReady`；对 Execution 而言，attempt、permit 和对账未闭合不得重放新增风险 mutation Outbox。两者都不妨碍其他健康账户或已有受管敞口的受限安全处置。

## 10. Backpressure 与故障策略

- 所有 channel、批次、并发任务和重试次数都有上限；
- 达到容量上限时优先停止接收、合并可合并行情或进入降级，不丢弃订单和成交；
- 外部调用统一使用 timeout、有限重试、退避和 jitter；
- 外部 mutation 的可恢复门禁失败使用持久 `next_eligible_at`/事件触发重新唤醒，不使用 broker nack 或进程内循环制造热重试；
- 行情、账户、订单和控制配置分别定义最大可接受陈旧时间；
- 交易路径依赖失效时默认 fail-closed；
- 只读分析、通知和报表可以 fail-open，但必须产生降级指标。

### 10.1 Backtest、Paper 与 Live 的运行一致性

- `ResearchBar`、`PaperEvent` 和 `RecoveryHarness` 是三个不同精度的运行协议，不共用模糊 `backtest/paper` 开关；
- Strategy entry/exit、Portfolio、Risk/final-stop 与 Execution planning 必须调用同一 Rust 业务 symbol；不允许 `BacktestRisk`/`LiveRisk`、`BacktestStopLoss`/`LiveStopLoss` 或两套 OrderPlanner；
- Strategy、Portfolio、Risk 和 Execution planning 分别发布自己拥有的不可变 Policy Snapshot；`StrategyRuntimeSnapshot` 不包含账户、用户或 risk profile。只有 Web 创建的用户请求才由 Execution 用 `ExecutionDecisionContextSnapshot` 绑定四个引用；Research 使用独立的 `ResearchScenario`/`ResearchRunSpec`，不伪造用户请求或 Context；
- Strategy 输出候选失效价、退出意图/候选止盈计划和证据；Risk 产生不可放宽的最终止损、风险边界与批准数量；Execution 根据 Strategy exit intent、RiskDecision 和交易所能力生成纯 `ExecutionPlanningValue`（及其确定性的 child `OrderPlan`）与 `ProtectionPlanningValue`；只有 live intake 在 Execution owner transaction 中从相同 planning hash 初始化持久 `OrderIntent`、`ExecutionPlan` 与 `ProtectionPlan`；
- ResearchBar 使用固定 `ResearchRunSpec`，其中显式引用 `ResearchScenario`、DatasetManifest、四个 Policy Snapshot、SimulationProfile、模拟账户初态、Clock 与 Seed；它只读 Market 的已冻结历史输入，不读取生产当前时间、用户凭证或隐式环境变量；
- ResearchBar 精确复用 Strategy、Portfolio、Risk 和纯 Execution planning（`ExecutionPlanningValue`、其 child `OrderPlan` 与 `ProtectionPlanningValue`），只把 planning value 持久为 Research Evidence；它不创建或持久化 live `ExecutionPlan` OMS Aggregate，也不运行生产 lease、outbox、网络 Unknown 或 Reconciliation；
- ResearchBar 在每个 Market/fill/funding 变化后都执行 `SimulationLedger -> simulated AccountSnapshot -> Continuous Risk -> RiskAction -> reduce-only ExecutionPlanningValue -> FillModel -> SimulationLedger`。已生效的模拟保护规划按 `SimulationProfile` 路径先结算；`KillSwitch`/`Close`/`Reduce` 优先于 Strategy exit 和任何新开/加仓；Strategy exit 只可与同方向 reduce-only 动作净额合并，RiskAction 不得直接改写 Ledger 或伪造成交；
- PaperEvent 使用 Simulated Exchange 产生 Ack、PartialFill、Reject、Cancel、Protection 和延迟事件，并复用 Execution 纯订单状态迁移；
- RecoveryHarness 以 disposable storage/fault injection 验证 lease、outbox、Unknown、重放、保护缺失和对账恢复，不作为收益证据；
- Research 的 SimulationLedger 不是生产 AccountProjection；模拟 Order/Fill/Account identity 必须携带 SimulationRunId，且不得写生产事实表；
- StrategyEvaluationStateKey 必须包含 EvaluationScopeId、RuntimeSnapshotId 和 MarketStreamPartition，并行 backtest 不得共享可变状态；
- 多币种回测在同一 decision time 先收集全部 Signal，再统一进行 Portfolio/容量分配，不得受 symbol 遍历顺序影响；
- Research/纯 parity 至少比较 StrategySignal、PortfolioTarget、RiskDecision、`ExecutionPlanningValue`、其 child `OrderPlan` 与 `ProtectionPlanningValue`；Fill/PnL 只在相同 SimulationProfile 下要求可重放，不宣称与真实交易所相同。live/Paper 另验证同值无损初始化的 OrderIntent、`ExecutionPlan`、`ProtectionPlan` 与状态迁移；
- exact parity fixture 还必须固定四个 Policy Snapshot hash、Market/Account/Instrument evidence、EvaluationState before 与 Clock。用户路径额外固定 `ExecutionDecisionContextSnapshot` hash；Research 路径固定 `ResearchDecisionContextSnapshot`/`ResearchRunSpec` hash，二者不得互相伪造或替代。随后比较 EvaluationState after、`ExecutionPlanningValue`、其 child `OrderPlan`、`ProtectionPlanningValue` 和 decision trace；live/Paper fixture 额外比较由同值初始化的 OrderIntent、`ExecutionPlan`、`ProtectionPlan` 与状态迁移。任一所需 identity 不同只能称 scenario comparison，首次输出差异层必须直接失败；
- fee、slippage、funding、latency 与 candle 内路径只能来自 SimulationProfile，不能混入 RiskPolicySnapshot；
- 对 live 新风险增加，`MarketDecisionReadiness` 只要是 `StaleOrGapped`、`ReferenceInvalid` 或 `Unknown`，或者 Account 的 `ExchangeSession`/`AccountAdmissionReady` 未满足，就必须 fail-closed；不得以临时探测、截断窗口或“保守系数”放行。保护、减仓和紧急平仓只能使用已声明的 mark/fallback 与 reduce-only 路径。Research 历史输入缺口、窗口不足或 RuntimeSnapshot 不匹配时，必须标记 Research input 不完整并拒绝 exact parity/推广结论，不能伪装成 live `stale` 恢复或静默改变策略。

### 10.2 多租户私有连接管理

多租户自动交易平台的核心容量约束之一，是**每个 `MonitoredExchangeAccountSession`（稳定 `ExchangeAccountRef`）需要独立的私有 WebSocket 连接**(User Stream)来接收该账户的成交、余额、持仓和订单更新事件。监测集合是商业活跃账户与未结 SafetyObligation 账户的并集；credential reference/revision 只是授权版本，不能把已撤销商业资格但仍需安全监测的会话从容量统计中排除。私有流连接的生命周期管理、出口 IP 分片、失败降级策略直接决定平台能同时支持多少 monitored sessions，以及连接异常时的用户体验。

本节定义三个 **AccountSession capacity phase** 的目标实现,每个阶段有明确的触发条件、容量上限和迁移路径。它们只描述单账户私有会话容量，和 §2.1 的 `Topology rollout T1` 是两个正交的迁移轴，绝不能把两者的“第一阶段”当成同一状态。完整决策记录、持久化对象、一致性约束与验收见 [ADR-0012](adr/0012-multi-tenant-private-stream-management.md)。

#### 问题陈述

**公共市场数据与私有账户流的根本差异**:
- 公共行情流（价格、盘口、K 线）是全市场共享的，一条连接可订阅几十个币种，所有用户复用。Market 可使用平台固定的、只读的 `MarketDataAccessCredential` 获取公共 endpoint；它不是用户账户、不能读取余额/仓位/私有流，也不能下单；
- 私有流（User Stream / User Data Stream）是**账户私有**的，必须用该用户账户的 API Key 鉴权，只推该账户的事件，**不能多账户复用**；
- N 个用户配置了自动交易 = 原则上需要 N 条独立的私有 WebSocket 连接（× 交易所数）。平台不存在自营账户或由 Core 自行创建的执行请求；固定公共数据 API Key 不构成例外。

**交易所限制**:
- **建连速率上限**:OKX 每个出口 IP 每秒最多 3 个新连接请求;Binance 类似;
- **连接维护成本**:Binance 的 listenKey 需要每 30 分钟 REST 续期,否则交易所单方面断开;
- **API Key 绑定 IP 数**:OKX 每个 API Key 最多绑 20 个 IP(支持网段);
- **两套不可替换的配额**：Market 的 K 线、instrument、盘口和公共成交使用 `PublicQuotaKey(exchange, endpoint_group, egress_identity, market_data_source_profile_id)`；用户 signed REST、私有流维护和 mutation 使用 `PrivateQuotaKey(exchange, credential_reference, endpoint_group)`。公共采集不得挤占用户 mutation 预算，也不得静默改用任一用户 credential；私有流的 REST query 备用路径只消耗其对应 `PrivateQuotaKey`。详见 [ADR-0013](adr/0013-user-execution-request-and-public-market-data-credentials.md)。

**Account 投影新鲜度与 ExchangeSession readiness 的依赖**:
- Account owner(§5)的职责包含"账户投影数据新鲜度"和"ExchangeSession 运行时 readiness";
- User Stream 是 Account 投影的主要输入源;流断线 → 账户投影 `stale` → 依赖该账户的新开仓 **fail-closed**(§11 底线);
- ExchangeSession readiness 包含"签名可用性、交易所冻结状态、User Stream 存活、signed preflight 结论"(§5 表注);流异常直接影响 readiness;
- 私有流的连接池管理,本质上是在管理"多少个账户能保持 readiness、什么时候必须 fail-closed、如何在容量不足时优雅降级"。

**账户会话初始目标只启用 AccountSession Phase 1（全热连接、单实例、fail-closed）**,适用 monitored sessions <100 的规模。当会话规模增长（≥100、冷启动变慢或≥500 需要降级态）时，再按 [ADR-0012](adr/0012-multi-tenant-private-stream-management.md) 迁移到 AccountSession Phase 2（多实例分片 + lease）与 AccountSession Phase 3（降级态 + 动态扩缩容）；后两个阶段的完整设计、持久化对象与验收都存档在 ADR-0012，本运行文档不重复。这里描述的是迁移目标与验收契约，不是落地声明。

#### AccountSession Phase 1：全热连接、单实例（初始迁移目标）

**适用场景**:产品初期,monitored sessions <100,单交易所为主(OKX 或 Binance)。

**连接策略**:
- 启动时，通过版本化 Web owner API 获取“有活跃 combo 订阅 + 已配置 API Key + 会员有效”的账户引用；不直连或查询 Web 订阅表。Account 另通过版本化 Execution-to-Account safety-monitoring Contract 接收未结 `SafetyObligation` 所需的账户引用；该 Contract 不传原始 credential，也不把安全义务伪装为商业资格。两者共同决定需要监测的账户集合，商业资格仍以 Web 为事实源；
- 对每个账户,按交易所建立私有 WebSocket:
  - 连接 → 鉴权(OKX/Bybit 发 login;Binance 先 REST 拿 listenKey) → 订阅私有频道(`orders`, `account`, `positions`) → 缓冲事件 → REST 拉签名快照 + 水位线闭合 → 标记 `healthy`;
- 建连速率:**串行限速**,每秒最多 3 个(OKX 单 IP 限制);100 账户冷启动 ≈ 33 秒;
- 连接维持:心跳监控(最后收到消息的时间,墙钟判定)、Binance listenKey 定期续期(每 25 分钟)、断线自动重连(指数退避);
- **不冷却**：只要账户仍有有效商业资格，或仍有未结 `SafetyObligation` 需要监测，且 Gateway capability 可用，私有连接一直保持。退订、会员到期和 claim 失效只会停止新风险增加；它们不能撤销已有敞口、保护、Unknown order 或对账仍未收敛的**监测义务**。只有商业资格已失效、且 `SafetyMonitoringV1` 的 Remove 已由 Account Ack 且已证明不存在未结义务时，Account 才可关闭该会话；credential 被撤销时，物理连接/签名查询可以停止，但 obligation 必须进入 `SafetyBlocked`/人工处置，而不得绕过 Gateway 使用任何替代凭证。

**失败处理**:
- User Stream 断线、心跳超时(>X 秒无消息)、listenKey 过期、交易所拒绝 login → 标记账户为 `stale`;
- `stale` 账户的新开仓请求,在 execution-worker 的 Dispatcher 最终门禁被拒绝(§8 标准链路);
- UI 通过 Web API 查询账户 ExchangeSession 状态,在用户的 combo 管理页明确提示"您的 OKX 连接异常,自动交易已暂停,请检查 API Key 或交易所状态";
- 后台持续尝试重连(指数退避,最长间隔 5 分钟),重连成功 → 重新闭合水位 → 恢复 `healthy`。

**容量观测（不是另一个迁移门）**:
- 单实例单 IP、建连速率 3/s 时，100 账户约 33 秒、200 账户约 67 秒；这些数字只用于容量预估和告警；
- 冷启动恢复超过 2 分钟是 AccountSession P2 预警，不能绕过或替换阶段迁移门；
- AccountSession Phase 1 → AccountSession Phase 2 **唯一**触发条件是 [ADR-0012](adr/0012-multi-tenant-private-stream-management.md) 的“monitored sessions ≥100，或实测/预估冷启动恢复 >5 分钟”。不另设 300 会话、2 分钟或按 IP 反推的隐式 cutover 门。

#### 配置参数参考（AccountSession Phase 1）

| 参数 | AccountSession Phase 1 值 |
|---|---|
| 建连速率上限(单 IP) | 3 连/秒(OKX 硬限,上线前以官方文档复核) |
| 心跳超时阈值 | 30 秒 |
| 冷却 | 不冷却（有商业资格或未结安全义务即保持热连接） |
| listenKey 续期(Binance) | 每 25 分钟(30 分钟过期留余量) |
| 重连退避 | 指数退避 + jitter,最长间隔 5 分钟 |
| 单实例管理账户数上限 | 不单列静态 bypass；Phase 迁移只使用 ADR-0012 的唯一门槛 |

#### 监控指标（AccountSession Phase 1）

AccountSession Phase 1 必须暴露以下指标(Prometheus/OpenTelemetry):

- `exchange_session_total{exchange, state}`:按交易所、状态(`healthy` / `stale`)统计账户数;
- `exchange_session_connection_duration_seconds`:连接存活时长分布;
- `exchange_session_reconnect_total{exchange, reason}`:重连次数,按原因分类(心跳超时/listenKey 过期/交易所拒绝);
- `exchange_session_coldstart_duration_seconds`:冷启动恢复时长(全部账户从启动到 healthy 的耗时);
- `exchange_session_connection_rate_limit_hit_total`:撞建连速率墙次数。

告警规则:
- `stale` 账户数 > 5% 总账户数,持续 > 5 分钟 → P1(可能交易所侧故障);
- 冷启动恢复时长 > 2 分钟 → AccountSession P2（容量预警，继续收集证据；不自动触发 Phase 迁移）。

> AccountSession Phase 2/3 的配置参数、监控指标（`degraded`/`cold`/rebalance 相关）与告警在启用对应阶段时按 [ADR-0012](adr/0012-multi-tenant-private-stream-management.md) 补充，此处不提前列入。

#### 10.2.1 AccountSession Phase 1 深度：单账户连接生命周期状态机

AccountSession Phase 1 的核心是“如何为一个账户建立、维持、恢复一条可信的私有流连接”。这里的魔鬼细节是：**任何一个环节做错，都会导致“以为连上了、其实漏了成交事件”，进而 Account 投影与交易所真实状态不一致，埋下资金安全隐患。**

**单账户私有流状态机**:

```text
Disconnected（初始/断开后）
  -> Connecting（发起 WebSocket 握手）
  -> Authenticating（OKX/Bybit 发 login；Binance 已在建连前取得 listenKey）
  -> Subscribing（订阅 orders/account/positions 私有频道）
  -> Buffering（开始缓冲私有事件，但尚未应用到 Account 投影）
  -> SnapshotReconciling（拉 signed REST 快照 + 水位线，合并缓冲事件）
  -> Healthy（水位闭合，实时应用事件）

Healthy
  -> Stale（心跳超时/交易所拒绝/listenKey 过期）

Stale（对外 readiness）
  -> Reconnecting（内部指数退避重连）
  -> Buffering（重连成功后重新闭合水位，不直接回 Healthy）

任一步失败
  -> Backoff（记录原因，指数退避）
  -> Connecting（重试）
```

AccountSession Phase 1/2 对外发布的 `ExchangeSession` readiness 只有 `healthy` 与 `stale`；`Connecting`、`Buffering`、`Reconnecting` 和 `Backoff` 都是 `stale` 期间的内部连接生命周期，不能被错误展示或消费为可开仓的第三种状态。`degraded` 仅在 AccountSession Phase 3 按 ADR-0012 启用。

**关键约束(每条都在防一个具体事故)**:

1. **必须"先缓冲、后闭合",不允许"连上就直接应用事件"**:WebSocket 握手成功不等于数据完整。握手到订阅之间、订阅生效到你拉快照之间,都可能已经发生了成交。所以连上后先把私有事件**缓冲在内存队列**,不立即应用到 Account 投影;

2. **水位线闭合(SnapshotReconciling)是 Healthy 的唯一入口**:
   - 拉一份 signed REST 账户快照(余额/持仓/open orders),这份快照带交易所时间戳或序号作为**水位线**;
   - 用水位线切割缓冲队列:水位线**之前**的事件丢弃(快照已包含其结果),水位线**之后**的事件按序应用;
   - 若交易所私有流没有可靠序号,则用"快照时间 + 事件时间"作为替代规则,并对边界附近的事件做幂等合并(同一 order/fill identity 不重复应用);
   - **闭合完成前,账户保持 `stale`,依赖它的新开仓 fail-closed**；这与 §3.2 的 Account readiness、§9 的 Account 专属恢复所要求的“先订阅缓冲、再合并快照、闭合前 NotReady”完全一致；

3. **Binance listenKey 的完整生命周期**:
   - 建连**前**先 REST `POST /api/v3/userDataStream` 取得 listenKey;
   - 用 listenKey 建连 `wss://.../ws/<listenKey>`;
   - **每 25 分钟**(交易所 30 分钟过期,留 5 分钟余量)REST `PUT` 续期一次;续期用墙钟(`WallClock`)调度,不用注入 Clock;
   - 续期失败要重试(有限次),连续失败视为连接不可信 → 标 `stale` → 重新走完整建连(取新 listenKey);
   - 进程重启后,旧 listenKey 可能已失效或被交易所回收,**不复用**旧 listenKey,一律重新取;
   - OKX/Bybit/Gate 无 listenKey,但 login 后有各自的鉴权有效期与心跳要求,按交易所能力在 gateway 归一;

4. **心跳与陈旧判定用墙钟**:
   - 每条连接记录"最后收到任何消息(含心跳 ping/pong)的墙钟时间";
   - 后台定时器（墙钟）检查：超过心跳超时阈值（AccountSession Phase 1 = 30 秒）无任何消息 → 判定连接异常 → 标 `stale` → 进入 Reconnecting；
   - **不能用注入 Clock 判定心跳**——心跳是运行时真实性,必须用 `WallClock`(见 target §10 DecisionTime/WallClock 区分);

5. **重连成功不直接回 Healthy,必须重新闭合水位**:
   - 断线期间可能发生了成交/撤单,本地缓冲是空的(因为断了);
   - 所以重连成功后回到 `Buffering` → `SnapshotReconciling`,重新拉快照闭合水位,而不是假设"断线期间什么都没发生";
   - 这一点最容易被写错:开发者常图省事在重连成功后直接 `Healthy`,导致断线窗口内的成交永久丢失;

6. **重连退避策略**:指数退避(如 1s → 2s → 4s → ... → 上限 5 分钟)+ jitter,防止交易所抖动时全部账户同时重连制造建连风暴(撞每秒 3 个的速率墙);

7. **`stale` 期间的行为**:
   - 依赖该账户的新开仓或加仓请求 fail-closed（execution-worker Dispatcher 最终门禁拒绝）；
   - **但已有 `ManagedExposure` 的持续风控不能停**：即使私有流断了，也只有 account-worker 可以通过受限 signed REST query 合并 AccountProjection 并发布 `AccountFactV1`；reconciliation-worker 只能发布 `ReconciliationEvidence` 和 typed owner command，不能成为第二个账户事实发布者。Risk 基于 Account owner 事实触发 action，再由 execution-worker 以正常 planning、幂等、permit 与 Gateway 路径执行受限的 Reduce/Close/Protect（减仓是 reduce-only，不受新风险 fail-closed 限制）。`ObservedExternalPosition` 只报告和人工处置，不因断线恢复而自动接管，但仍进入风险占用；
   - UI 通过 Web API 展示"您的 XX 交易所连接异常,自动交易已暂停",但不误导用户"仓位无人看管"。

**AccountSession Phase 1 单账户连接的可观测字段**：每条连接至少记录 `ExchangeAccountRef` 的脱敏/低基数关联标识、exchange、当前 credential revision/revocation generation、状态、最后消息墙钟时间、重连次数、最近一次水位闭合时间、listenKey 续期时间(Binance)；原始 credential reference 不得成为高基数 metric label 或日志 secret。

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
  -> 进入角色专属 drain：Account 停止接收新私有流输入、等待当前 fenced 投影写入安全结束并断开私有流；Execution 停止新的风险增加 claim、把未结 SafetyObligation/Unknown/Outbox 持久到恢复点
  -> 刷新 outbox、checkpoint 和审计记录
  -> Account 在私有流已断开且不再可能推进投影写入后才释放 account lease
  -> 关闭交易所和数据连接
  -> 刷新 telemetry
  -> 进程退出
```

关闭必须有总超时；超过总超时仍未完成时，记录未完成状态，依赖下次启动恢复，不无限等待。尤其 Account 的顺序固定为“停止接收该账户输入 → 让已开始的带 generation 写入完成或被 fence 拒绝 → 断开私有流 → 释放 lease”；不得先释放 lease 再继续写投影，以免新 holder 闭合水位后被旧实例覆盖。

## 13. 可观测性

每次交易链路至少可按以下字段关联：

- `service.name`、`service.version`、`deployment.environment`；
- `strategy_key`、`strategy_version`、`definition_hash`、`runtime_snapshot_version`；
- `decision_context_id`、`decision_context_hash`、四个 Policy Snapshot version/hash；
- `ExchangeAccountRef` 的脱敏关联标识、`exchange`、`instrument`；
- `correlation_id`、`event_id`、`order_id`、`client_order_id`；
- `risk_evaluation_id`、`risk_decision_id`、`risk_policy_version`；
- `account_opening_slot_id/generation`、`mutation_id/kind`、`attempt_no`、`fence`、`gate_checked_at`；
- `ExchangeAccountRef` 的脱敏关联标识、credential revision/revocation generation、source claim receipt hash/expiry、AccountFact source/cursor/comparator、AccountRecoveryClosed generation/projection revision；
- `MarketEvent` source/finalization/revision/continuity generation、`RequiredMarketEvidence` verdict、`ExchangeExecutionCapabilityProfileV1` 与 `RiskValuationSnapshotV1` identity；
- `event_time`、`observed_time`、`processing_latency`。

敏感凭证、Secret、passphrase 和未脱敏请求头禁止进入日志、metric label 和 trace attribute。

每次 live 范围启用前必须发布版本化 `OperationalSloProfileV1`，由运维和风险 owner 明确该范围的 kill-switch 传播/ack/negative-ack 时限、在途 permit 的 gate-recheck 证据、最大 Unknown 时长、最大无保护时长、对账 coverage deadline、AccountRecoveryClosed 最大 age、SafetyBlocked 通知/人工确认 deadline 及 monitored session 容量阈值。至少暴露并告警：`kill_switch_propagation_seconds`、`unknown_mutation_age_seconds`、`unprotected_exposure_age_seconds`、`reconciliation_coverage_age_seconds`、`account_recovery_evidence_age_seconds`、`safety_blocked_total/age`、`claim_gate_reject_total`、`market_evidence_reject_total`、`monitored_exchange_account_sessions` 与 source-claim/capability expiry reject。没有对应目标、告警路由和演练 evidence 的范围不得获得 live cutover。

## 14. 生产验收

上线前至少验证：

1. 重复 Signal、Order command 和 FillEvent 不产生重复副作用；
2. 外部请求成功但响应丢失时不会重复下单；
3. execution-worker 在订单各状态崩溃后可以恢复；
4. account-worker 用户流断线后停止依赖陈旧持仓开仓；
5. 持续 Risk 运行角色可以对 `ManagedExposure` 触发减仓、撤单和 kill switch；初期可由 account-worker 装配 Risk evaluation，但 Account 只发布事实，所有 mutation 仍由 execution-worker 的 plan/permit/Gateway 路径执行；独立 risk-worker 需另有运行证据；
6. reconciliation-worker 可以识别差异，并通过 owner command 修复可自动恢复的状态；
7. 控制面不可用时数据面按已发布配置安全运行或停止；
8. 优雅关闭不会丢失已接受任务和未发布 outbox；
9. 部分成交、撤单/成交竞态和最大未保护窗口超时不会留下无保护敞口；
10. 回测、paper 与 live 在四个 Policy Snapshot、Decision Context、动态 Evidence、EvaluationState before 和 Clock identity 一致时，先对纯 `ExecutionPlanningValue`/child `OrderPlan`/`ProtectionPlanningValue` 做逐层 parity；live/Paper 再证明同值初始化的 OMS Aggregate 与状态迁移一致；
11. 两个 staging worker 对同账户并发开仓时只有一个取得 opening slot，slot 不早于 Account watermark/保护闭合释放；
12. cancel/RecoveryAuthorized revoke 与 Gateway permit consume 的竞态只允许一方成功；旧 Dispatcher 携带 revoked/stale permit 时 Gateway 不调用 SDK，Consumed 且未知时不得本地终结；
13. Unknown outcome 不直接生成同 kind mutation Outbox；只有 DefinitivelyAbsent/RecoveryAuthorized 且无可发送 permit 时，recovery transaction 才 supersede 旧 generation，并保持原 mutation/目标 identity、按 Submit/Cancel/Protect kind 写入绑定新 `mutation_event_id`/`mutation_generation`/`expected_aggregate_version` 的对应 Outbox；旧 delivery ack/no-op，不支持缺席证明的能力不能 live；
14. Submit/Cancel/Protect 的 transient blocker、可重试 DefinitelyNotSent 或 fence/gate 变化确认当前 delivery 后，都原子 rollover 到新 generation 的 delayed Outbox/RetrySchedule；Scheduler 不能复用旧 delivery 或直接 claim；
15. User Stream/snapshot bootstrap 窗口注入 Fill/Cancel 不丢事件，闭合前 NotReady 且 Dispatcher off；
16. `core-runtime` 镜像内容与 Release Unit allowlist 完全一致，不含 Research/Backtest/Paper/candidate/schema-tool，Research workflow 无生产部署权限。
17. 旧 Account holder 在 lease 失效后恢复时，其旧 `account_session_generation` 无法通过 `AccountProjectionWriteFence` 写入投影；新 holder 闭合的 AccountRecoveryClosed evidence 不会被覆盖；
18. `ProcessReady(account-worker/execution-worker)` 与单账户 `AccountAdmissionReady` 可独立呈现：一个账户 stale、会员变化或 credential 撤销不会使其他账户停摆，也不会中断已有受管敞口的受限安全处置；
19. 退订、会员/claim 到期与 credential 撤销会阻止新风险增加；前两者仍保留经 `SafetyMonitoringV1` Add/Update/Remove fence/ack/replay 保证的监测/收敛，后者无可用安全能力时按默认阻止删除或产品授权的人工 SLA 转为可审计 `SafetyBlocked`，绝不使用平台公共数据 credential 或默认账户绕过；
20. 交易所观察到但缺少原 Web 请求/Context/Order 来源链的仓位被标记 `ObservedExternalPosition`/`UnknownOrigin`，不会自动 mutation，且已纳入 RiskValuationSnapshot 的敞口/保证金/风险预算；
21. 多个 Web request 进入同账户 Portfolio 时，`PortfolioEvaluationBatch` 的 source ordering、batch hash、净额结果与逐 source outcome 可确定重放；单请求也走 size=1 batch。
22. claim 到期、重新 claim、Renew fence 变化、Release/Outcome 迟到与 permit consume 并发时，旧 receipt 无法签发/消费 capability 或覆盖 Web 状态，permit/capability TTL 不超过最早 claim expiry；
23. 原始 User Stream/query 只由 Account owner 投影；AccountFactV1 的 Inbox 重放可确定更新 OMS，Execution 不持有私有流；AccountRecoveryClosedV1 在有/无 sequence 的交易所均能展示 cursor/comparator、snapshot scope、overlap、coverage 与 expiry，Phase 1 zombie epoch 无法放行新风险；
24. K 线乱序、修订、源切换、必需多周期 evidence 缺失及 bar 未 final 时，RequiredMarketEvidence 使新增风险 fail-closed；Research DatasetManifest 使用同一 finalization/correction 规则；
25. 每个启用的 exchange/product/instrument 组合均有 `ExchangeExecutionCapabilityProfileV1`、`RiskValuationSnapshotV1`、signed read-only preflight 与保护/Unknown recovery evidence；
26. kill switch、Unknown、无保护敞口、对账覆盖、SafetyBlocked、AccountRecoveryClosed 和 monitored session 容量均满足 `OperationalSloProfileV1`，且通过故障演练验证告警、ack/negative-ack 与人工升级路径。
