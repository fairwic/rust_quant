---
name: rust-quant-architecture
description: 约束 rust_quant 的领域归属、代码放置、数据库 CRUD、Ports/Adapters、Research/回测和生产运行边界。用于设计、评审、实现或迁移后端模块、Vegas、策略/组合/风险/执行链路、分级模拟、运行入口和架构文档，检查代码是否违反目标架构，以及识别并防止六类结构坏味道（空 struct 服务、指标层做决策、同名两义配置、依赖反向拉直连、env flag 热路径、时间戳内联）、七类工程纪律坏习惯（f64 金额、无类型 JSON 端口、SDK DTO 穿透、热路径 panic、压平错误、pub 泛滥、假测试）、八类基础设施/数据/运维坏习惯（fire-and-forget spawn、schema 三真相源、迁移改写、密钥裸传、配置散读、无可观测性、依赖不复用、契约无版本）、七类重复造轮子/未用外部标准版（手写重试、指标绕过、精度舍入脱节、回测/信号多路镜像、K线结构体泛滥、变体整文件复制、f64 金额）与研究/生产未隔离。
---

# Rust Quant 架构规范

## 目标

把架构文档转换成可执行的放置、依赖和验证流程，维护业务 owner、时序、交易安全和研究证据边界。

## 先定位仓库

1. 将 `/Users/mac2/onions/crypto_quant` 视为 umbrella workspace。
2. 将 `rust_quant/` 视为 Core owning repo；只在该目录执行 Git、Cargo、测试和提交。
3. 先检查当前分支、工作树和目标文件，保留用户已有改动。
4. 涉及 Web、Admin、News 或交易所 SDK 时，先确认 owner；跨仓库只使用 owner service API 或稳定 contract，不新增跨库读写。

## 按任务加载权威文档

先阅读[架构索引](../../../docs/architecture/README.md)，再按任务读取下列文件。不要只凭本 Skill 的摘要替代原文。

| 任务 | 必读文档 |
| --- | --- |
| 新模块、领域拆分、通用架构评审 | [目标架构](../../../docs/architecture/target-architecture.md)、[依赖规则](../../../docs/architecture/dependency-rules.md) |
| 业务逻辑、CRUD、SQL、事务、Consumer | [业务代码与数据访问](../../../docs/architecture/business-code-and-data-access.md)、[ADR-0007](../../../docs/architecture/adr/0007-owner-scoped-persistence-and-transaction-boundaries.md) |
| Research、Vegas、回测或模拟交易 | [ADR-0009](../../../docs/architecture/adr/0009-research-domain-and-tiered-simulation.md)、[Vegas 迁移实战](../../../docs/architecture/vegas-backtest-migration.md)、[通用量化逻辑归属](../../../docs/architecture/common-logic-placement.md) |
| Worker、订单、保护单、对账与恢复 | [生产运行与恢复](../../../docs/architecture/production-runtime.md)、[ADR-0004](../../../docs/architecture/adr/0004-portfolio-and-trading-domain-boundaries.md)、[ADR-0006](../../../docs/architecture/adr/0006-at-least-once-idempotency-and-recovery.md) |
| 新增代码、迁移 legacy、架构 Review | [AI 架构护栏](../../../docs/architecture/ai-coding-guardrails.md)、[AI 迁移执行协议](../../../docs/architecture/ai-migration-execution-protocol.md)、[迁移计划](../../../docs/architecture/migration-plan.md) |

ADR-0008 只保留为决策历史。遇到 Research 或 backtest 设计冲突时，以 ADR-0009 为准。

## 执行流程

### 1. 明确任务边界

区分分析、诊断、设计、修改、迁移和运行态验证；分析/诊断保持只读，实盘 mutation 必须取得明确授权。先声明假设、歧义和成功标准；存在多个合理 owner 或会改变产品语义时，停止并说明选择影响。

legacy、crate、Owner、事实源、运行入口或 Backtest/Live 双实现迁移必须先创建 Migration Manifest，锁定已提交架构基线、单一迁移模式、允许路径、业务不变量、验证和删除门。聊天计划不能替代 Manifest；diff 越界、模式混合、基线漂移或需要未授权 cutover 时立即停止。

### 2. 提交代码放置声明

新增或移动代码前输出：

```text
变更：
Owner：Market / Strategy / Portfolio / Account / Risk / Execution / Reconciliation / Research
切片：Command / Query / Event Consumer / Pure Policy / Simulation Kernel
入口：
Use Case：
Model / Policy：
Ports：
Adapters：
事务原子性：
跨进程 Contract：无 / 名称与版本
运行入口：
恢复 Owner：
验证：unit / integration / contract / parity / recovery
```

不能唯一填写时，不要先创建 `common`、`service`、`repository` 或万能 DTO。

### 3. 从真实调用链反向验证

使用当前代码、测试、数据库 contract 和运行入口复核假设。标记 legacy 边界，不把历史目录当成目标 owner。对照“迭代废物的产生模式”“工程纪律坏习惯”“基础设施/数据/运维纪律坏习惯”“重复造轮子/未用外部标准版”四节自查：新增代码是否在复制任一坏味道/坏习惯，是否在 `indicators`/`common`/`execution` 与 workspace 依赖已有实现的情况下另起炉灶，触碰的存量属于哪一类、收敛路径是什么。

对 Vegas 或回测至少追踪：

```text
入口
  -> Experiment / BacktestRun
  -> DatasetManifest
  -> StrategyRuntimeSnapshot
  -> Strategy Evaluator + scoped state
  -> decision-time barrier
  -> PortfolioTarget
  -> RiskDecision
  -> OrderPlan
  -> SimulationLedger / event simulation
  -> Analytics
  -> Completed ResearchEvidence
```

### 4. 实施最小垂直切片

优先完成一个可验证的 owner slice，不横向搬完整目录：

1. 定义内部 Input/Output、业务 identity 和 Model/Policy；
2. 在 Use Case 中编排业务动作，以业务语言定义 Port；
3. 在 Adapter 中实现 SQL、HTTP、Redis、对象存储或交易所协议；
4. 在 App 中完成配置、依赖注入和循环；
5. 增加对应层级测试与迁移/删除条件。

不要顺手清理相邻 legacy，不建立无真实调用方的兼容层或扩展点。

### 5. 按风险验证

- Pure Model/Policy：单元测试、边界测试、确定性测试；
- CRUD/事务：Postgres 集成测试、幂等、锁/版本、索引和注释检查；
- Contract：producer/consumer 快照与版本兼容测试；
- Vegas/Research：逐层 parity、严格时序、成本、Seed/Manifest 重放和 symbol 重排不变性；
- Execution：部分成交、撤单竞态、Unknown、保护数量、lease、outbox 和恢复测试；
- 运行入口：配置、compose、启动/关闭和 deploy contract；
- 多步骤闭环：同步相关架构/迁移文档、`task_plan.md` 与 `AGENT_PROGRESS.md`。

没有新鲜测试或运行态证据时，不声称完成、生产可用或可晋级。

## 硬边界

### Domain 与 Quant

- 业务规则只属于明确 Domain；Domain 不依赖 Wire Contract 或具体 Adapter，App 只负责组合根、配置和运行循环。
- `quant/math`、`quant/indicators`、`quant/backtest`、`quant/analytics` 只包含 owner 无关的纯机制。
- `quant/backtest` 只提供确定性时钟、事件调度、Replay、撮合和费用/滑点/资金费模型；不得依赖 Domain、数据库、环境变量或真实交易所。
- Research 是终端离线 Domain，可以通过稳定公开 API 编排 Market、Strategy、Portfolio、Risk、Execution 和 Quant；生产 Domain 不得依赖 Research。

### Strategy 与 Research

- Strategy 拥有 StrategyDefinition、StrategyArtifact、StrategyRelease 和 StrategyRuntimeSnapshot。
- Research 拥有 Experiment、BacktestRun、RunCheckpoint、DatasetManifest、SimulationProfile、SimulationLedger 和 ResearchEvidence。
- Strategy Release 只能引用 Completed ResearchEvidence，不复制、覆盖或接管研究证据。
- Evaluation state 使用：

```text
EvaluationScopeId + StrategyRuntimeSnapshotId + MarketStreamPartition
```

并行 run 或 deployment generation 不得共享可变 evaluator state。

### 三种模拟精度

- ResearchBar：验证 Strategy、Portfolio、Risk、OrderPlan 和成本后绩效；不声称覆盖 lease、outbox、Unknown 或恢复。
- PaperEvent：模拟 Ack、PartialFill、Reject、Cancel、Protection 和延迟，并复用 Execution 纯状态迁移。
- RecoveryHarness：使用 disposable storage 和 fault injection 验证 lease、outbox、Unknown、重放、保护与 Reconciliation；不作为策略收益证据。
- 多币种先收集同一 `decision_time` 的全部候选，再统一执行排序、净额、容量和风险；symbol 输入重排不得改变结果。
- SimulationLedger 不得写入生产 AccountProjection、Order 或 Fill 事实表。

### 数据访问

- Handler、Scheduler、Consumer 不直接执行 SQL 或调用交易所 SDK。
- Use Case 定义业务原子性，Port 使用业务动作命名，Postgres Adapter 实现事务、Row、SQL、锁和错误映射。
- 禁止 `Repository<T>`、`BaseService`、`update_by_id`、无条件 upsert 和 runtime DDL。
- 同 owner 原子提交状态、幂等/Inbox、Outbox 和审计；跨 owner 使用本地事务、Outbox/Inbox、幂等与补偿/Reconciliation，不使用跨域大事务。
- ResearchEvidence 先按内容哈希上传不可变对象，再由 Research owner 数据库事务发布 manifest、引用、指标、幂等和 Completed；只保证原子可见，不虚构跨存储全局原子事务。
- 新表和新列必须有数据库原生注释；每条 SQL 都检查索引、基数、锁和扫描成本。

### 交易安全

- 区分 read-only preflight、dry-run、paper/sim 和 live mutation。
- 未经明确授权，不下单、撤单、平仓或修改交易所账户。
- 实盘订单必须先验证凭证、权限、symbol filters、数量精度、风险、worker lease 和保护单计划；没有止损不允许下单。
- 研究收益、胜率、Sharpe、回撤和 PnL 必须带数据范围、版本、费用、滑点、资金费和时序证据。

## 迭代废物的产生模式与新架构防线

以下六个坏味道是 legacy Core 在反复迭代 / 策略调参中真实滚雪球形成的，均有 `dependency-rules.md` §13 对应条款。写新代码、评审或迁移时，**先自查是否在复制这些模式**；发现存量按“最小修订方案”给出收敛路径，不就地扩大。

| # | 坏味道模式（当初的“图快”动机） | 存量证据 | 违反 | 新架构防线 |
| --- | --- | --- | --- | --- |
| 1 | **空 struct 冒充服务**：迁 legacy 自由函数时统一包 `pub struct XxxService;` + 一堆 async fn，零字段 | 22 个零字段 `*Service/*Manager/*Calculator`；`services/market/` 下 8 个同构文件；`risk_management_service.rs` 方法体是 TODO 占位 | §13.18 / §11 | 无状态逻辑是 use case 函数或 Policy；不为“命名”造类型。只有持有 Port/配置快照才允许成对象 |
| 2 | **指标旁边就地做决策**：指标迭代中就地写开平仓判断，滚成隐藏策略引擎并反向 `use domain` | `indicators/trend/vegas/strategy/` 11572 行；`trade_signal.rs` 直接产 `should_buy/stop_loss_price`；`indicators` 21 处 `use rust_quant_domain` | §3 / §13.11 | owner-agnostic 层（`quant/*`）零业务 Domain 依赖；`SignalResult` 等决策产物只能在 Strategy Domain 生产 |
| 3 | **同名两义配置**：domain 有一份配置，加字段嫌麻烦就在别处另起同名 struct，用 `pub use as` 别名互相掩盖 | `BasicRiskStrategyConfig` domain 版 7 字段 vs strategies 版 20+ 字段；账户字段 `account_risk_fraction_per_trade`/`position_leverage` 穿透进 `get_trade_signal` 入参 | §13.13 / §13.20 / §13.25 | 一个 JSON = 一个 owner = 一个类型；evaluator 入参禁含账户资金 / 用户风险 / 最终数量口径字段 |
| 4 | **依赖箭头图省事拉直连**：要复用就直接加 Cargo 依赖，跳过 domain 抽象；下单直接 `use okx` 裸 SDK | `execution` 依赖 strategies/indicators/risk 却不依赖 domain（V2）；`swap_order_service.rs` 直调 `OkxApiTrait` mutation | §3 / §13.17 | 执行层只依赖 `domain::ports` 与上游 api；mutation capability 只进 Fenced Gateway，App/Dispatcher 不装配 raw SDK client |
| 5 | **env flag 当临时开关不回收**：调策略行为直接加 `VEGAS_XXX` env “临时”验证，验证完不删 | 决策热路径 40+ 个 `VEGAS_*` env，每次现读 `std::env::var`（`long_rule_helpers`/`short_rule_helpers`） | §13.4 / §13.27 | 决策开关是带规范 hash 的显式配置输入；非 App/Platform 禁读 env；热路径禁读 env / “最新版本” / 隐式默认 |
| 6 | **时间戳写死在 mutate 方法**：聚合根每个状态迁移内联 `Utc::now()` | `order.rs`/`position.rs`/`strategy_config.rs` 数十处；策略信号 `signal.ts = Utc::now()` | §13.19 | Model/Policy 注入 `Clock` Port；decision-time 由上游传入，回测/live 用同一注入时钟保 parity |

## 工程纪律坏习惯（编码级，同样在新架构堵住）

结构坏味道之外的另一批，危害集中在**资金精度、类型安全、实盘 panic**三处。根节点是「边界懒得建映射层」——A/B/C 同源：Domain 用 f64 与 `serde_json::Value` 定义、SDK DTO 一路透传，导致类型安全和资金精度在最该强的交易/订单路径反而最弱。对应目标架构 §10 / §10.1 / §10.2 / §12。

| 键 | 坏习惯 | 存量证据 | 新架构防线 |
| --- | --- | --- | --- |
| A | **f64 做金额，从 domain 向上污染** | 110 处金额 f64 字段；risk/execution 对 Decimal 引用为 **0**（全 f64），services 已用 Decimal → 混用带；`position.rs` 实体 pnl/margin 即 f64；`breakeven_stop_loss.rs:213` 止损价 f64 | 金额在 **Domain 层就用 Decimal 定义**；止损价/下单量/保证金率全程 Decimal，禁 f64 中间态（§10） |
| B | **domain 端口返回 `serde_json::Value`** | `exchange_trait.rs` 7 处端口吐无类型 JSON；全库 473 处 Value 当万能类型；实体裸 Value 字段 | Port 用领域类型命名；无类型 JSON 只在 Adapter 内部与 Contract wire 层；存证用命名 `raw_payload` 且不参与决策（§10.1） |
| C | **SDK DTO / DB Row 穿透业务层** | `okx::dto::*` 63 处；execution 直接吃 `okx::dto::account_dto::Position`；market 把 `CandleOkxRespDto` 当内部模型 | SDK DTO 只在 exchange-gateway 映射，Row 只在 Postgres Adapter 映射；业务 crate 禁 `use okx::*`/sqlx Row（§10.1） |
| D | **热路径 unwrap，全在实盘链** | `swap_order_service.rs:309` 下单量 `.unwrap()`；`swap_order.rs:54` 订单 key `split.nth(0).unwrap()`；`gateway.rs:761` 用 `panic!` 表达未支持交易所 | 交易/执行/风控热路径禁 unwrap/expect/panic；未支持分支返回 `Err` 并 fail-closed（§10.2） |
| E | **压平错误换“能跑”** | `unwrap_or_default()` services 74 处 → 失败余额静默变 `0.0` 令风控误判无仓位；`scheduler_service.rs:84` 关停/join 结果 `let _ =` 丢弃 | 金额/仓位 Result 必须传播由 Use Case 决定 fail-closed；后台任务失败必须被感知记录（§10.2） |
| F | **默认全 pub，可见性零收敛** | domain 294 pub / **0 pub(crate)**；全库 pub:pub(crate)≈7:1；371 处 re-export；439 处 glob import | Domain/业务 crate 对外只经 `api`/`lib.rs` 出口；内部 `pub(crate)` 或私有；禁 `pub mod` 全敞开 + glob re-export（§12） |
| G | **研究脚本固化成 #[ignore] 假测试** | 60 处 `#[ignore]`（`scalper_research.rs` 单文件 19 个）；根 `tests/` 大量 0 断言只 println；111 文件连真实 PG、46 打真实 OKX | 测试确定性 + 有断言；参数扫描/探索归 `examples/`；依赖真实基础设施的测试归 integration，不作默认 CI 门禁（§12） |

次要但要盯：粗粒度 `Arc<Mutex<整块状态>>` 跨 await 持有（`deep_stream_manager`、全局 `SCHEDULER` 把并发退化为串行 → 拆状态粒度或消息传递/`ArcSwap`）；MVP 硬编码沉淀为业务规则（ETH-only 白名单写死进 execution、风控阈值写死进 `Default` → 走配置）；注释即删除（execution 成段 `// let` 旧逻辑 → 删掉靠 git 找回）。

## 基础设施/数据/运维纪律坏习惯（第三批，迁移期尤其致命）

危害集中在**数据一致性、可复现构建、密钥安全、静默失败**，H/J/O 直接影响迁移期环境重建。对应目标架构 §10.3 / §12 / production-runtime §11-13。三个贯穿性根因：①正例存在但未强制下沉（`task_scheduler`/`websocket_service` 会正确 join+abort，新代码却仍 fire-and-forget）；②平台迁移未收尾（MySQL→Postgres 旧债不回填，靠"当前库已是目标态"掩盖）；③静默失败链（fire-and-forget + 无 metrics + 无 health + span 不贯穿 = 进程活着但业务停摆，监控看不见）。

| 键 | 坏习惯 | 存量证据 | 新架构防线 |
| --- | --- | --- | --- |
| H | **fire-and-forget spawn** | 48 处 spawn，生产约 20 处丢 JoinHandle；`bootstrap.rs:152-524` worker/雷达/同步循环无句柄；K线 upsert spawn 失败静默丢写 | 统一 task supervisor（JoinSet/结构化并发），禁裸 spawn 丢句柄；任务失败必须被感知记录（§12） |
| I | **schema 三套真相来源** | migrations 40 表 / `sql/postgres_quant_core.sql` 44 表 / 运行时 DDL（candle/backtest repo）并存，44≠40，靠 contract test `.contains()` 硬顶 | 表结构只由 `migrations/` 定义，禁运行时 DDL 与旁路整库脚本；分表用声明式分区/统一注册器（§10.3） |
| J | **历史迁移改写 + 不可重放** | 3+ 处已应用迁移被后续 commit 重写（checksum 崩）；8 个迁移含 MySQL 语法从零重放必炸；无 down | 迁移严格 append-only、可空库重放、清理不可移植语法、提供回滚；新表新列带 COMMENT（§10.3） |
| K | **密钥当普通数据传** | 51 处密钥裸 `String`；`ExchangeApiConfig` 在 domain `derive(Debug,Serialize)`+明文 secret；穿透 web contract；`local-dev-secret` 进部署脚本 | 凭证用 redacting 包装类型（脱敏 Debug、禁默认 Serialize），不进 Domain/Contract；内部 secret 无安全默认值，缺失 fail-closed（§10.3） |
| L | **配置去中心化到极致** | 616 次 `env::var` × 163 文件；无类型化 Config；无 `.env.example`；同变量多处默认值不一致 | App/Platform 集中解析强类型 Config 注入；业务 crate 不散读 env；单一登记处 + 配置清单（§10.3） |
| M | **无 metrics / 无 health / span 不贯穿** | prometheus/otel 命中 0；`#[instrument]` 仅 2 处；只有 DB `health_check()`，无 `/health`；错误无统一 `ErrorKind` | 关键路径（下单/成交/对账/lease/readiness）埋 metrics + 贯穿 span + 标准 `/health`；错误分类 Retryable/Fatal（§12） |
| N | **有依赖不复用，各写各的** | 依赖 `tokio-retry` 却手写 6+ 套重试（99 处）；幂等键分散；`hyperliquid_rust_sdk` git 无 rev（不可复现） | 统一 retry/退避/去重封装；workspace 内 `workspace = true`；git 依赖钉 rev（§12） |
| O | **同表多套 Row + 表名漂移** | `SwapOrderEntity` 定义 3 次跨 3 crate；`CandlesEntity` 2 次；`strategy_config` vs `strategy_configs` 并存 | 一表一 `FromRow` 归属其 owner Adapter；表名集中常量，不留旧名别名（§10.3） |
| P | **契约无版本化** | internal HTTP API 无 `/v1/`、无 apiVersion；版本混进业务字符串 `entry_rule_version:"..._v2"`；必填字段无 `#[serde(default)]` | internal API 显式版本化，可选字段带 default 兼容，版本不混业务字符串；handoff/Outbox N/N-1（§7.3/§12） |

## 重复造轮子 / 未用外部标准版（第四批，代码泛滥主因）

全量实证台账见 [duplication-and-wheel-reinvention](../../docs/architecture/migrations/baseline-2026-07/duplication-and-wheel-reinvention.md)（含 `文件:行`）。两个根因：①**已引入依赖却不用到位**（`tokio-retry`/`rust_decimal`/`ndarray` 在依赖树里，业务代码仍手写）；②**同一件事跨 crate 各写一份**（回测循环、指标、止损、K线模型、精度舍入、信号评估）。新增代码必须先查 `indicators`/`common`/`execution` 与 workspace 依赖是否已有实现，禁止再复制一份。

| 键 | 坏习惯 | 存量证据 | 新架构防线 |
| --- | --- | --- | --- |
| Q | **手写重试/退避** | `tokio-retry` 已依赖零引用；9+ 份逐字 `0..4u64`+`250*(attempt+1)` 退避（`okx_historical_15m_backfill.rs:997`、`market_velocity_backfill.rs:756` 等） | 统一 `tokio-retry`/`backoff`，合并成一个共享 helper（xtask WARN 局部退避） |
| R | **指标绕过 `indicators` crate 手写** | `atr_at` 逐字 6 份、`ema_at` 4 份、`compute_rsi`/Bollinger/MACD 多份，权威版都在 `indicators/src/` | 指标只来自 `indicators`，删除全部 `*_at`/`compute_*`；契约测试禁 indicators 外定义指标函数 |
| S | **精度舍入 + 真实量化脱节** | `round_price` 逐字 9 处硬编码 `*10_000.0).round()`，与交易所 filters 量化（`execution_order_filters.rs:208`）完全平行 | 单一 `execution::precision::quantize(price/qty, tick/step)`，回测与 live 都传 filters；禁硬编码精度魔数 |
| T | **回测/信号评估多路镜像** | 回测主循环 5+ 套；止损 live vs backtest 平行（`market_velocity_signal.rs:1490` vs MVE `stop_loss.rs:123`）；信号评估 paper/backtest/live 近 4000 行镜像 | 单一 `backtest-engine` + 纯函数 `SignalEvaluator` + `StopLossPolicy`，三路只换数据源/执行后端；live-parity 升为强制门禁 |
| U | **K线/OHLCV 结构体 8+ 份** | `common CandleItem`(权威) 外还有 `domain Candle`/`market CandlesEntity`/MVE `BacktestCandle`/tv-parity `Candle` 等；对齐逻辑两份 | `common::CandleItem` 唯一内存模型，其余仅 DB 实体 + `From`；对齐/聚合统一到 `market::candle` |
| V | **变体用整文件复制而非参数化** | `filtered_volume_rsi_ema_macd/` 16 个 `*_vN.rs`(v1..v13)；`*_research.rs` 24 / `*_vN.rs` 19；单家族 37 个 MANIFEST | 差异抽成声明式 `StrategyParams`(TOML/DB 行)，v1..v13 收敛为 1 参数化策略 + N 配置；research 结果落表非 markdown |
| W | **金额用 f64 而非 `rust_decimal`** | 金额语义字段 f64:Decimal≈275:27；`domain position.rs:56` pnl、`swap_order_service.rs:568` entry_price 全 f64 | 成交价/数量/盈亏/余额用 `Decimal`（已就绪），指标/统计保留 f64（与第二批 A 项合并） |

> 与第三批的关系：N（不复用）/O（同表多 Row）是"配置/依赖/行结构"视角，本批 Q–V 是"算法与领域逻辑"视角，互补不重叠；W 与第二批 f64 金额是同一债，此处给量化底数。

## 研究/迭代与生产必须物理隔离

上面六个坏味道之外，**最大的永久废物来源是“研究试错的沉渣堆进了生产代码仓”**。存量：4–5 套并行回测实现（`framework/backtest` 活，`orchestration/backtest`、`risk/backtest`、`cli/market_velocity_event_backtest` 40K 行自建、`btc_eth_strategy_family` 各重造）；策略版本以文件演进（`filtered_volume_rsi_ema_macd/` v1→v13、docs `V10→V27`、vegas `V61→V77`）；83 个 `src/bin/` 里 39 个是一次性 research/probe，与 8 个生产 `quant_core_*_worker` 混住；188 个 docs（136 个 `*_MANIFEST.md`）大量未跟踪。

新架构强制规则：

1. **策略版本走配置不走文件**：策略即数据（参数化 + registry），新版本 = 新 `version`/`strategy_key` + 配置，不复制源文件。禁止 `*_v2`/`*_v13` 命名的策略源文件家族。
2. **单一回测引擎**：所有研究入口复用 `quant/backtest` 的确定性时钟/事件调度/撮合/费用/滑点/资金费，不在 CLI 或其它 crate 重造 equity/PnL/撮合。
3. **研究 bin 剥离**：一次性 research/panel/probe 入口归 `apps/quant-lab` 与 Research Domain，不进 `core-runtime` 镜像依赖图（§13.23/24）。生产 `src/bin/` 只留生产角色 worker。
4. **研究产物不进代码仓**：`*_MANIFEST.md`/`*_EVALUATION.md`/`_RESULT.md` 与跑批输出写 gitignore 的 artifacts 目录；进仓的只有 Completed ResearchEvidence 的稳定引用。
5. **一次性脚本用完即删**：`fix_*.sh`/`migrate_phase*.sh` 这类迁移脚本完成后删除，不沉淀为永久噪声。
6. **禁 Python 研究栈进主线**：迭代/回测禁用 Python（见 CLAUDE.md）；并行的 `.py`/`.mjs` 研究脚本不作为事实源。

## Review 输出格式

架构评审按以下顺序输出：

1. 结论：可接受 / 需调整 / 阻塞；
2. Owner 与目标代码位置；
3. 当前实现证据和 legacy 差异；
4. 违反的依赖、数据、时序或运行边界；
5. 最小修订方案；
6. 必须补充的测试、迁移和运行证据。

把“目录看起来整齐”与“真实业务闭环可验证”分开判断。最终必须能回答：谁拥有事实、谁作决策、谁持久化、谁恢复，以及 backtest/paper/live 在哪一层保持 parity。
