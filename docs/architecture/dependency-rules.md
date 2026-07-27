# Rust Quant 依赖与代码归属规则

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-23
- 上位文档：[Rust Quant 长期目标架构](target-architecture.md)
- CRUD 细则：[业务代码与数据访问放置规范](business-code-and-data-access.md)

## 1. 目标

所有新增代码都必须回答：

1. 它改变或读取哪一个业务事实；
2. 该事实由哪个 Domain 或外部服务拥有；
3. 它属于 model、policy、use case、port、adapter、contract 还是 app；
4. 它是否进入交易热路径；
5. 失败、重复、超时和重启后由谁恢复。

如果无法唯一回答，不得先放入 `common`、`utils`、`services`、`core`、中央 Scheduler 或现有大文件。

## 2. 物理分区

`crates` 下只允许五类一级分区：

| 分区 | 职责 |
| --- | --- |
| `domains` | 业务事实、不变量、政策、用例与所需 Port；包含终端离线 Research Domain |
| `quant` | owner 无关的数学、指标、回测内核和分析纯机制 |
| `contracts` | 真实跨进程/跨仓库 Wire Contract |
| `adapters` | Postgres、Redis、HTTP、交易所、对象存储和通知实现 |
| `platform` | 极小基础类型、消息运行时、生命周期、安全、可观测性和测试支持 |

`apps` 是独立于以上五类的组合根。现有扁平 crate 在迁移完成前属于 legacy，不能作为新代码默认落点。

## 3. 允许的依赖方向

```text
apps
  ├──> domains::use_cases / domains::api
  ├──> contracts（只在进程边界映射）
  ├──> adapters（装配实现）
  └──> platform（配置、生命周期、观测）

adapters
  ├──> domains::ports / domains::model
  ├──> contracts（仅协议适配需要）
  └──> platform 的薄技术能力

production domains::use_cases
  ├──> 本 Domain model / policies / ports
  ├──> 上游 Domain api
  └──> quant::math / indicators 的稳定纯计算 API

domains::research::use_cases
  ├──> Research model / policies / ports
  ├──> Market / Strategy / Portfolio / Risk / Execution 的稳定 API
  └──> quant::backtest / analytics / math 的纯 API

domains::model / policies
  └──> platform::kernel 或 quant::math / indicators 的批准 API

quant::math / indicators / backtest / analytics
  └──> platform::kernel 或自身纯计算依赖

contracts
  └──> 序列化依赖与极小 wire primitives
```

关键约束：Domain 内部不得依赖 `contracts`。Wire DTO 必须在 App 或入站 Adapter 映射为用例 Input；用例 Output 再在边界映射为 Contract。这样 HTTP/消息版本不会渗入业务模型。

没有列出的依赖默认禁止。App 可以装配多个 Domain，但不能因此访问其私有 module 或数据库表。

关键边界：

- `quant/*` 禁止依赖任何业务 Domain、Adapter、数据库 Row、环境变量或真实交易所；
- 生产 Domain 只依赖需要的 `quant/math`、`quant/indicators`，禁止依赖 Research；
- Research 是终端离线 Domain，可以调用生产 Domain 的稳定 API 和 Quant Kernel，但禁止访问私有 module、Repository Port 或生产 Adapter；
- `quant-lab` 只装配 Research use case 与 Adapter，不直接实现逐事件交易编排。

## 4. Domain 依赖顺序

```text
Market API
  ↓
Strategy API
  ↓
Portfolio API
  ↓
Risk API
  ↓
Execution API
  ↓
Reconciliation API

Market / Strategy / Portfolio / Risk / Execution API
  ↓
Research API（终端离线；无生产 Domain 反向依赖）

Account API ──> Portfolio / Risk / Execution / Reconciliation
Execution FillEvent ──进程边界 Contract──> Account consumer
```

- Market 不依赖 Strategy、Portfolio、Risk、Execution；
- Strategy 只依赖 Market 的稳定 API 与纯量化能力；
- Portfolio 可以依赖 Strategy Signal 与 Account Snapshot 的稳定 API；
- Risk 可以依赖 Market、Portfolio、Account 的公开事实；
- Execution 可以依赖 Market 规格、Account Snapshot 与 RiskDecision；
- Account 不依赖 Execution 私有 model，跨进程成交通过版本化 FillEvent 输入；
- Reconciliation 读取 owner 的公开查询结果，恢复必须发送 typed owner command；
- Research 可以只读调用生产 Domain 的稳定 API，并通过自己的 Port 保存 Experiment/Evidence；
- 生产 Domain 禁止依赖 Research 的 Experiment、Run、Simulation 或 Evidence 私有模型；
- 禁止业务 crate 循环依赖。

如果两个 Domain 需要双向协作，优先由 App 编排或通过 Event 解耦，不能把两边私有类型互相暴露。

## 5. 各层强制禁止

### 5.1 Model

Model 只保存实体、值对象、状态机和必须始终成立的不变量。禁止：

- SQLx、Redis、Reqwest、交易所 SDK；
- `std::env::var`、全局配置或连接池；
- async I/O、重试、日志编排；
- HTTP DTO、数据库 Row、第三方原始 DTO；
- 跨 owner Repository 或 Service。

### 5.2 Policies

Policy 是可替换、可版本化、确定性的纯决策。禁止：

- 读取数据库、缓存、网络或系统当前时间；
- 自己加载策略参数或风险阈值；
- 写入状态、发送消息或调用交易所；
- 把运行模式写成散落的 `if live/paper` 分支。

### 5.3 Use Cases

Use case 负责编排一个完整业务动作。禁止：

- 创建具体数据库、HTTP、Redis 或 Exchange Client；
- 依赖具体 Adapter；
- 直接写 SQL 或处理数据库 Row；
- 读取环境变量决定业务行为；
- 把 Wire Contract 当作内部模型贯穿；
- 绕过 owner use case 修改其他 Domain 状态。

Use case 可以定义事务必须原子完成哪些业务结果，但不能接收或传递 `sqlx::Transaction`。

### 5.4 Ports

- Port 由使用能力的 Domain 定义，不由 Adapter 反向定义；
- 方法使用业务语言，例如 `stage_order_submission_with_outbox`，禁止泛型 `Repository<T>`、`update_by_id`、`save_json`；
- Port 不暴露 SQL、表名、数据库事务类型、HTTP status 或 SDK 类型；
- 写 Port 与 Query Port 分开，避免一张万能 Repository 接口无限膨胀。

### 5.5 Adapters

- 只实现协议、持久化和技术语义；
- SQLx Row、SQL、锁、分页和具体事务只在 Postgres Adapter；
- 不承载策略判断、资本分配、风险政策或订单状态机；
- 禁止跨 owner 直接读写其他模块表；
- 不得静默丢弃交易所不支持的字段；
- 重试、超时和错误映射不得改变业务 identity。

### 5.6 Apps

- 只负责配置、Contract 映射、装配、循环、健康检查和关闭；
- Handler/CLI/Consumer 只做输入校验、鉴权上下文提取、DTO 映射和 use case 调用；
- 禁止在 `main.rs`、bootstrap、scheduler callback 或 consumer loop 中实现业务规则；
- 一个 App 只初始化本职责需要的连接、Secret 和 Adapter。

### 5.7 Contracts、Platform 与 Testkit

- Contract 只保存真实跨进程协议，不保存内部 DTO、数据库 Row 或 SDK 类型；
- `platform/kernel` 只保存无 owner 争议且长期稳定的基础值对象；
- Platform 不保存策略参数、风险阈值、cache key 或业务调度条件；
- Testkit 不得成为生产 dependency；
- 禁止以“以后可能复用”为理由提前创建 shared 抽象。

### 5.8 Quant 与 Research

`quant/math`、`quant/indicators`、`quant/backtest`、`quant/analytics`：

- 只做 owner 无关纯计算，不拥有 Experiment、BacktestRun、StrategySignal、RiskDecision 或生产 Order；
- `backtest` 只提供 DeterministicClock、EventScheduler、Replay、撮合、费用、滑点和资金费模型；
- `analytics` 只消费权益、成交和事件序列计算指标；
- 不读取环境变量、数据库、Redis、HTTP、系统当前时间或随机全局状态；
- 不依赖业务 Domain，不直接持久化；API 变更必须有确定性和数值回归测试。

`domains/research`：

- 拥有 Experiment、BacktestRun、DatasetManifest、SimulationProfile、Checkpoint、SimulationLedger、ResearchEvidence；
- 负责跨域离线编排，但所有 Strategy、Portfolio、Risk、Execution 判断仍调用对应 owner 的公开 API；
- 使用 `ResearchBar`、`PaperEvent`、`RecoveryHarness` 三种明确 profile，禁止用快速 K 线回测冒充 OMS 恢复验证；
- SimulationLedger 不是 AccountProjection，模拟事实不得写生产 Order/Fill/Account 表；
- Evidence 使用内容寻址对象加 Research owner 数据库事务实现原子可见发布，不宣称跨存储全局原子。

### 5.9 Rust 对象、函数与 Trait

代码形态不能替代 owner 和分层。新增代码按以下规则选择：

- 有稳定业务 identity、生命周期或必须一起维护的不变量，使用 Entity/Aggregate `struct`/`enum` + `impl`；
- 只有单位、范围、组合合法性而无 identity，使用不可变 Value Object；
- 无状态、无 I/O、完整输入决定输出，默认使用 module 内自由纯函数；
- 多个纯决策共享同一不可变版本配置，使用 Policy 对象；Policy 的字段只能是已解析的强类型快照或纯依赖；
- 跨市场事件维护滚动状态时，使用带完整 scope identity 的显式 State，并让 backtest/live 调用同一纯 transition；
- Use Case 可以是持有 Port 的对象；没有状态和依赖时，不得仅为了 `XxxService::method()` 语法创建零字段 Service；
- Adapter 可以持有连接池、Client、限流器和协议配置，但不得因此接管业务不变量；
- Trait 只用于真实多实现、消费方 Port、稳定 Domain API 或测试替身，不建立继承式 `BaseService`、万能 `Manager` 或每类型一个空 Trait。

Aggregate 中能够破坏不变量的字段不得向任意调用方公开可变访问；时间、随机值和外部事实必须作为参数或 Port 输入。把一个跨 owner 大函数移动到 `impl TradingState`、`TradingEngineService` 或其他对象中，不会改变其违规性质。

## 6. Owner 规则

| 事实 | 唯一 Owner |
| --- | --- |
| instrument、symbol、精度、合约能力 | Market |
| K 线、tick、盘口、资金费率、市场数据质量 | Market |
| StrategyDefinition、信号、预测和证据截止 | Strategy |
| StrategyArtifact、StrategyRelease 与 RuntimeSnapshot | Strategy |
| Experiment、BacktestRun、DatasetManifest、Checkpoint、ResearchEvidence | Research |
| 资本预算、目标权重、目标仓位和策略净额 | Portfolio |
| 实际余额、持仓、敞口、保证金和 PnL | Account |
| RiskDecision、持续风险、熔断和 RiskAction | Risk |
| OrderIntent、ExecutionPlan、订单、成交、撤单和保护状态 | Execution |
| 对账差异、恢复任务和处置证据 | Reconciliation |
| 外部协议、签名、第三方错误和能力映射 | 对应 Adapter / `crypto_exc_all` |
| 跨进程 payload 的版本与兼容 | 产生该事实的业务 Owner |

一份数据没有明确 owner 时，先补 ADR 或 owner registry，不得新增跨模块写入。

## 7. 新代码归属决策树

1. 定义业务对象或不变量 → owner `model`；
2. 基于完整输入作纯决策 → owner `policies`；
3. 编排一个状态变化 → owner `use_cases/commands`；
4. 编排一个只读业务查询 → owner `use_cases/queries`；
5. 消费事件后触发业务动作 → owner `use_cases/consumers`；
6. 表达数据库、交易所、HTTP、缓存等所需能力 → owner `ports`；
7. 实现 SQLx、Redis、HTTP、SDK → `adapters`；
8. 跨进程传输 → `contracts/<owner>/<version>`；
9. 进程启动、装配、取消、健康、关闭 → `apps` / `platform`；
10. 确定性时间推进、撮合、费用与滑点机制 → `quant/backtest`；
11. Experiment、Run、DatasetManifest、Checkpoint、SimulationLedger、Evidence → `domains/research`；
12. 当前系统兼容旧入口 → Adapter 与[迁移计划](migration-plan.md)，不得进入目标 model。

## 8. 新 crate 与新 App 判定

默认先创建 module。满足以下至少两项才拆独立 crate：

- 需要编译器强制依赖方向；
- 引入独立且较重的依赖；
- 有独立 owner；
- 有独立测试或发布生命周期；
- 需要阻止其他模块访问内部实现。

只有独立轮询/流消费、扩缩容、故障隔离、安全边界或部署生命周期出现时才创建 App。文件数量增加本身不是拆 App 的理由。

### 8.1 构建与发布影响边界

目标使用三个 Release Unit：

| Release Unit | 内容 | 生产部署资格 |
| --- | --- | --- |
| `core-runtime` | control-api、market-worker、signal-worker、account-worker、execution-worker、reconciliation-worker | 自动发布候选 |
| `core-maintenance` | schema-tool 与批准的有界维护 Job | 仅显式运行 |
| `quant-lab` | Research、Backtest、Analytics、PaperEvent 研究入口和 research CLI | 禁止生产部署 |

每个生产 App 是独立 Cargo package，但六个生产 binary 继续共享一个 `core-runtime` 镜像。Release Unit 是构建/工件边界，不是业务 owner、服务或仓库边界。

目标依赖图必须保证：

```text
apps/signal-worker、apps/execution-worker
  -> strategy-api + strategy-released
  -> other production domains + quant/math/indicators + required adapters
  -X-> strategy-candidates、domains/research、quant/backtest、quant/analytics、apps/quant-lab

apps/quant-lab
  -> strategy-api + strategy-released + strategy-candidates
  -> domains/research
  -> production domains stable APIs
  -> quant/backtest/analytics/math
```

`release-units/{core-runtime,core-maintenance,quant-lab}.toml` 至少声明 root packages、binary allowlist、forbidden packages、镜像、生产部署资格和必需测试集。CI、Dockerfile、Compose 与 deploy contract 必须消费同一清单，不得各自维护一份二进制列表。

CI/CD 通过 Git diff、owning package、`cargo metadata` 反向传递依赖和 Release Unit Manifest 计算影响范围。path filter 只能作为依赖图之上的优化，不能手工漏掉传递依赖；无法归属、依赖图失败或清单漂移时 fail closed：

| 变更范围 | 必须构建/验证 |
| --- | --- |
| `apps/quant-lab`、`domains/research`、`quant/backtest`、`quant/analytics` | Research/quant-lab 单元、回放与 Evidence 测试；不因该变更重建生产 App |
| Strategy owner 内的 `strategy-candidates` | 候选策略、quant-lab、Research 与候选回放；不构建生产 App |
| Strategy owner 内的 `strategy-api`、`strategy-released` | signal-worker、依赖其公开 API 的生产 App、Research 与 parity |
| Strategy、Portfolio、Risk、Execution、Market、`quant/math`、`quant/indicators` 的共享公开实现 | 所有受影响生产 App + Research + parity |
| `apps/signal-worker` 或其专属装配 | signal-worker 与 deploy contract |
| `apps/execution-worker`、生产 Execution Adapter/Contract | execution-worker、相关 consumer、contract、integration/recovery |
| Contract 或共享 Platform API | 依赖该 Contract/API 的全部 producer/consumer |
| Cargo.lock、toolchain、workspace/build script、Release Unit Manifest 或公共镜像基础 | 全部 Release Unit |

新增“只用于实验、尚未发布”的候选策略，其规则仍由 Strategy owner 管理，但物理放入 `strategy-candidates`，并由 Research Experiment 引用；它不能注册进 live `StrategyCatalog` 或进入生产 App 依赖图。候选晋级时进入 `strategy-released` catalog，创建 Definition/Artifact/Release/RuntimeSnapshot，并重新进入生产构建、parity 和发布门禁。不能通过让生产 App 依赖回测 crate 来共享代码；已发布策略的共享业务逻辑只能位于 Strategy released/API 或其他对应 production Domain。

生产镜像内容必须与 `core-runtime` binary allowlist 完全一致，禁止包含 quant-lab、Research/Backtest/optimizer、Paper 收益研究入口、strategy candidates、schema-tool 和未批准维护工具。每个工件记录 Git revision、Cargo.lock/toolchain、Manifest hash、传递依赖图 hash、binary checksum、镜像 digest 与 SBOM。完整规则见 [ADR-0010](adr/0010-build-impact-and-artifact-isolation.md)。

## 9. Contract 规则

- 删除、改名或改变字段语义必须发布新版本；
- 同一版本只能增加经过兼容验证的 optional 字段；
- Contract 不得包含 SQLx derive、数据库主键细节或第三方 SDK 类型；
- 边界显式完成 `wire contract <-> use case input/output` 映射；
- 每个 Contract 有序列化快照、旧 payload 解析和未知字段测试；
- Command/Event 携带 event、correlation、causation、idempotency、aggregate、sequence、时间和 partition identity；
- producer 与所有 consumer 保持同一业务幂等身份；
- consumer 在业务 side effect 与消费确认之间必须具备可恢复状态；
- `ExecutionRequest.risk_profile_ref/version` 只表达 Web 配置来源和授权；Core Risk 必须将其幂等解析为 Published `RiskPolicySnapshot`，缺失、不兼容或已撤销时 Blocked；
- `ExecutionDecisionContextSnapshot` 必须带四个 Domain Policy Snapshot 的稳定 id/version/hash；后续 RiskDecision、OrderIntent、Plan、Attempt 与恢复 Contract 必须可追溯到同一 `context_id + context_hash`。

## 10. 数据库与事务规则

- `crates/adapters/postgres` 默认是一个 crate，内部按 owner module 隔离；
- 一张表只能有一个 owner；跨 owner 查询走公开 Query API、版本化投影或事件，不直接 JOIN 私有表；
- Migration 使用单一目录，命名为 `YYYYMMDDHHMMSS__<owner>__<action>.sql`；
- 每个迁移头部声明 owner、用途、回滚/前滚策略和性能影响；
- 新表必须有表注释，新列必须有列注释；
- 新查询评估索引、过滤、返回行数、排序、分页和锁范围；
- 事务的业务原子性由 use case 说明，由一个 owner-scoped Adapter 方法实现；
- 业务状态、幂等记录和 outbox 需要原子性时写入同一事务；Execution 下单准备还必须同时取得持久 `AccountOpeningSlot`，写入不可变审批引用及 parent OrderIntent/plan hash 唯一绑定、完整 ExecutionPlan/ProtectionPlan 与首个持久状态 `SubmitPending`；
- 跨 owner 一致性使用 outbox、幂等 command、状态投影和补偿，不建立跨 owner 大事务。

完整 Command/Query/Consumer 模板见[业务代码与数据访问放置规范](business-code-and-data-access.md)。

## 11. 策略、组合、风险与实盘规则

- Strategy evaluator、Portfolio policy 和 Risk policy 必须确定性可重放；
- 时间和随机源必须注入；
- backtest、paper、shadow、canary、live 复用同一业务实现；
- Strategy evaluator 不接收账户风险配置；候选失效价可以作为信号证据，最终仓位、止损和审批由 Portfolio/Risk 决定；
- `StrategyRuntimeSnapshot` 只包含 Strategy owner 事实，不包含 account、user、credential、risk profile 或 Portfolio/Risk/Execution policy 内容；
- Portfolio、Risk、Execution planning 各自发布不可变 Policy Snapshot；Execution 只用 `ExecutionDecisionContextSnapshot` 绑定这些 Published 引用，不重新解析业务 JSON 或补默认值；
- 已产生运行证据的策略 Definition/Artifact 不得覆盖；
- Signal 携带 strategy version、definition hash 和 evidence cutoff；
- PortfolioTarget 记录 allocator/policy version 与输入 Signal identity；
- RiskDecision 由 Risk owner 按 `risk_evaluation_id = request + target/snapshot hash + policy version/generation` 幂等持久化；Execution 只保存不可变引用和审批证据，一个批准决策只绑定一个 parent OrderIntent/plan hash；
- OrderIntent 只能从有效批准结果生成；
- mutation 前先固定 OrderIntent、ExecutionPlan 和 ProtectionPlan，再由 Execution owner 原子提交 `SubmitPending + 完整计划 + Idempotency + Outbox`；
- 事务提交后才允许 Dispatcher 执行提交时最终门禁，复核 credential、instrument、账户/行情新鲜度、lease/fence、审批时效、release/kill switch 和保护能力，并签发短期 MutationPermit；只有 Fenced Exchange Mutation Gateway 在网络 I/O 边界原子消费 current permit 后，才可在数据库事务外调用交易所；
- Dispatcher 门禁失败必须分类持久化：超时 Expired、不可恢复 Blocked、可恢复则保留 SubmitPending 并写 durable `next_eligible_at`/唤醒条件；不得静默重算计划、nack 热循环或 ack 后丢任务；
- 没有可执行保护性止损方案不得开仓。

## 12. Worker、并发与生命周期

- Worker use case 提供 `run_once` 或处理单个 typed message 的入口；
- 循环、间隔、消费、取消和关闭由 App/Platform 管理；
- 所有外部调用有 timeout；
- 重试有上限、退避、jitter 和错误分类；
- 竞争消费使用 lease，同一任务使用稳定幂等键；
- 未有 Risk Reservation ADR 前，同账户独立开仓由 Execution owner 持久 opening slot/活跃唯一约束串行；worker lease 不能替代业务唯一约束；
- opening slot 只在全部 child 终结、无 attempt/Unknown/可消费 permit、Account typed watermark 覆盖 cumulative fill 且保护安全后释放；
- 保护/减仓/紧急平仓旁路必须由 Gateway 证明 reduce-only，并先通过 typed Execution command 冻结风险增加 claim、推进 gate generation；
- 默认优先 Postgres outbox；只有吞吐、延迟或隔离证据要求时引入独立消息中间件；
- 禁止持锁执行外部 I/O、无边界 channel 和无边界任务派生；
- 外呼前必须以 `expected_aggregate_version`、Pending state、空 `send_claim` 和 current account/order fence 条件更新，原子记录 attempt 并签发短期 permit；取消/恢复 revoke 与 Gateway consume 竞争同一 permit CAS；
- Fenced Gateway 是唯一装配 raw SDK mutation client/credential 的组件；Dispatcher 和其他 App 只能提交 permit 与固定 payload。Gateway 对 revoked/stale/expired permit 返回 DefinitelyNotSent，禁止触达 SDK；
- Submit/Cancel/Protect 的 mutation event、attempt 和 permit 必须共同绑定 `mutation_event_id`、`mutation_generation` 与 `expected_aggregate_version`；旧/重复 delivery 与 current generation/version 不匹配时只 ack/no-op；
- attempt 保存 mutation kind、stable identity/number、三个 mutation 授权字段、payload/plan hash、fence、门禁证据和 Started/Confirmed/Indeterminate/DefinitelyNotSent/DefinitivelyAbsent；permit 保存 attempt、三个 mutation 授权字段、fence/gate generation/payload hash/expiry 与 Issued/Consumed/Revoked/Expired；outcome、permit 终态、状态迁移和后续 Outbox 原子提交；Unknown outcome 只允许恢复类 Outbox，禁止直接生成同 kind mutation Outbox；
- 当前 delivery 已确认/终结或 aggregate version 已推进但同一 mutation 仍需未来重试时，owner transaction 必须 supersede 本地 current generation、递增 `mutation_generation` 并持久化 delayed mutation Outbox 或 `MutationRetrySchedule`；Scheduler 只能经 owner transaction 物化唯一新 Outbox，不能复用旧 event 或直接 claim；
- 结果不明时进入 `Unknown`；只有持久 `DefinitivelyAbsent + RecoveryAuthorized` 且没有可发送 permit，才可在同一 recovery transaction supersede 旧 generation，并按 `Submit -> OrderSubmissionRequestedV1`、`Cancel -> OrderCancelRequestedV1`、`Protect -> ProtectionSubmissionRequestedV1` 写入对应新 Outbox。恢复保持原 mutation kind、stable mutation identity、目标 identity 和 payload/plan hash，只滚动 mutation 三字段与 attempt number。Consumed 且无终态 Gateway 结果时保持 Unknown；没有稳定 client identity duplicate rejection 和 signed query/缺席证明能力的 live mutation 必须 Unsupported；
- restart 先订阅并缓冲 User Stream，再读取 signed snapshot/query watermark 并合并补 gap；闭合前 Dispatcher 禁用；
- shutdown 先停止接收，再等待安全点、刷出 outbox/checkpoint、释放 lease；
- restart 先恢复未完成状态和对账，再进入 Ready。

## 13. CI 架构门禁

目标命令 `cargo xtask arch-check` 至少检查：

1. 新增 crate/module 位置是否属于批准分区；
2. Domain model/policies/use_cases 是否引入 SQLx、Redis、Reqwest、环境变量或交易所 SDK；
3. Domain 是否依赖 Contracts 或其他 Domain 私有 module；
4. 非 App/Platform 是否读取环境变量；
5. Contract 是否依赖 SQLx、Domain 或第三方 SDK；
6. Postgres Adapter 是否出现无 owner SQL 或跨 owner 表访问；
7. testkit 是否被生产依赖；
8. 关键 Contract 是否发生未声明变化；
9. Event Envelope 是否缺少必需 identity；
10. 新增或触碰文件是否超过代码大小限制；
11. Quant 是否依赖业务 Domain、Adapter、数据库或环境变量；
12. 生产 Domain 是否依赖 Research，或 Research 是否访问其他 Domain 私有模块；
13. Strategy evaluator 是否接收账户资金、用户风险配置或生成最终订单数量；
14. 多币种 ResearchBar 是否缺少 decision-time barrier 或会受 symbol 遍历顺序影响；
15. ResearchBar 是否运行/声称覆盖 lease、outbox、Unknown 和 Reconciliation；
16. ResearchEvidence 是否缺少原子可见发布状态或被 Strategy 表直接拥有。
17. Dispatcher/其他 App 是否直接依赖或装配 raw SDK mutation client/credential；mutation capability 只能进入 Fenced Gateway。
18. 新增零字段 `*Service`、`*Manager`、`*Calculator` 是否只作为 associated function 命名空间；
19. Aggregate 是否公开能够绕过状态机或破坏不变量的可变字段，Model/Policy 是否直接读取系统时间、随机全局状态或进程全局业务缓存；
20. backtest/live 是否新增重复的 Strategy、Portfolio、Risk、止盈止损或 OrderPlan 实现，或把同一 JSON 解析为语义重叠的运行模式配置；
21. `signal-worker`、`execution-worker` 等生产 App 是否依赖 `strategy-candidates`、Research、`quant/backtest`、`quant/analytics` 或 `quant-lab`；
22. CI 影响范围是否与 Cargo 传递依赖和 Release Unit Manifest 一致，Research-only 变更是否错误进入生产镜像，或共享 Domain 变更是否漏掉生产构建/parity；
23. 生产镜像 binary 是否与 `core-runtime` allowlist 完全一致，是否混入 quant-lab、Research/Backtest/Paper、candidate、schema-tool 或未批准工具；
24. 每个生产 App 是否为独立 Cargo package，且传递依赖闭包不含 candidates、Research、Backtest、Analytics；
25. `StrategyRuntimeSnapshot` 是否混入 account/user/credential/risk profile 或其他 owner 的 policy 内容；
26. Execution 是否在请求 intake 时原子持久化完整 `ExecutionDecisionContextSnapshot`，后续决策、计划、attempt 与恢复证据是否缺少 `context_id + context_hash`；
27. 热路径是否重新读取可变配置、“最新版本”、环境变量或隐式默认值，ResearchRunSpec/Context 是否缺少规范 hash 与兼容测试。

迁移期采用 ratchet：保存当前 legacy 违规基线，CI 只允许违规数下降，禁止新增。不得在门禁尚未实现时把本文写成“已经自动执行”。

门禁落地状态（2026-07-27）：`cargo xtask arch-check` 已实现本节第 1/2（跨库直连）、3/4/11（依赖方向与 owner-agnostic 反向依赖）、10（文件行数）、17（legacy signed read-only 存续）项的静态检查与 ratchet，基线冻结在 [migrations/baseline-2026-07/legacy-allowlist.toml](migrations/baseline-2026-07/legacy-allowlist.toml)。其余需 AST/语义分析或运行时证据的项（5–9、12–16、18–27）仍为 TODO，覆盖清单见 [baseline-2026-07/README.md](migrations/baseline-2026-07/README.md)，未实现的项不得声称已自动执行。

## 14. 例外流程

架构例外必须记录：

- 真实调用方和阻塞证据；
- 为什么现有 API/Port/Adapter 无法表达；
- 风险、性能和故障范围；
- 测试、恢复和可观测性；
- owner、失效日期和删除条件；
- 对应 ADR 或迁移记录。

“开发更快”“AI 生成方便”“少写一个类型”“以后可能需要”不是有效例外理由。
