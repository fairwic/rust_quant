# Rust Quant 依赖与代码归属规则

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
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
- 方法使用业务语言，例如 `persist_order_intent_with_outbox`，禁止泛型 `Repository<T>`、`update_by_id`、`save_json`；
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

## 9. Contract 规则

- 删除、改名或改变字段语义必须发布新版本；
- 同一版本只能增加经过兼容验证的 optional 字段；
- Contract 不得包含 SQLx derive、数据库主键细节或第三方 SDK 类型；
- 边界显式完成 `wire contract <-> use case input/output` 映射；
- 每个 Contract 有序列化快照、旧 payload 解析和未知字段测试；
- Command/Event 携带 event、correlation、causation、idempotency、aggregate、sequence、时间和 partition identity；
- producer 与所有 consumer 保持同一业务幂等身份；
- consumer 在业务 side effect 与消费确认之间必须具备可恢复状态。

## 10. 数据库与事务规则

- `crates/adapters/postgres` 默认是一个 crate，内部按 owner module 隔离；
- 一张表只能有一个 owner；跨 owner 查询走公开 Query API、版本化投影或事件，不直接 JOIN 私有表；
- Migration 使用单一目录，命名为 `YYYYMMDDHHMMSS__<owner>__<action>.sql`；
- 每个迁移头部声明 owner、用途、回滚/前滚策略和性能影响；
- 新表必须有表注释，新列必须有列注释；
- 新查询评估索引、过滤、返回行数、排序、分页和锁范围；
- 事务的业务原子性由 use case 说明，由一个 owner-scoped Adapter 方法实现；
- 业务状态、幂等记录和 outbox 需要原子性时写入同一事务；
- 跨 owner 一致性使用 outbox、幂等 command、状态投影和补偿，不建立跨 owner 大事务。

完整 Command/Query/Consumer 模板见[业务代码与数据访问放置规范](business-code-and-data-access.md)。

## 11. 策略、组合、风险与实盘规则

- Strategy evaluator、Portfolio policy 和 Risk policy 必须确定性可重放；
- 时间和随机源必须注入；
- backtest、paper、shadow、canary、live 复用同一业务实现；
- Strategy evaluator 不接收账户风险配置；候选失效价可以作为信号证据，最终仓位、止损和审批由 Portfolio/Risk 决定；
- 已产生运行证据的策略 Definition/Artifact 不得覆盖；
- Signal 携带 strategy version、definition hash 和 evidence cutoff；
- PortfolioTarget 记录 allocator/policy version 与输入 Signal identity；
- RiskDecision 记录政策版本、冻结输入、理由、边界和过期时间；
- OrderIntent 只能从有效批准结果生成；
- mutation 前完成 credential、instrument、账户/行情新鲜度、lease、风险和保护计划复核；
- 没有可执行保护性止损方案不得开仓。

## 12. Worker、并发与生命周期

- Worker use case 提供 `run_once` 或处理单个 typed message 的入口；
- 循环、间隔、消费、取消和关闭由 App/Platform 管理；
- 所有外部调用有 timeout；
- 重试有上限、退避、jitter 和错误分类；
- 竞争消费使用 lease，同一任务使用稳定幂等键；
- 默认优先 Postgres outbox；只有吞吐、延迟或隔离证据要求时引入独立消息中间件；
- 禁止持锁执行外部 I/O、无边界 channel 和无边界任务派生；
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

迁移期采用 ratchet：保存当前 legacy 违规基线，CI 只允许违规数下降，禁止新增。不得在门禁尚未实现时把本文写成“已经自动执行”。

## 14. 例外流程

架构例外必须记录：

- 真实调用方和阻塞证据；
- 为什么现有 API/Port/Adapter 无法表达；
- 风险、性能和故障范围；
- 测试、恢复和可观测性；
- owner、失效日期和删除条件；
- 对应 ADR 或迁移记录。

“开发更快”“AI 生成方便”“少写一个类型”“以后可能需要”不是有效例外理由。
