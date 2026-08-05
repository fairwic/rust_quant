# Rust Quant 目标目录与代码放置规则

本文是 rust_quant_alpha 的物理目录权威规范，回答两个问题：

1. 已知业务能力应放到哪里；
2. 新发现的业务逻辑、共享代码或外部集成应如何确定唯一位置。

能力事实、迁移状态和唯一目标路径登记在 rust_quant_alpha/architecture/business-capability-catalog.toml。本文只定义稳定结构和放置规则，不复制能力状态。

## 1. 总体原则

### 1.1 先定业务事实 owner，再定技术层

代码放置必须按以下顺序判断：

1. 这段代码改变或判断了什么业务事实；
2. 哪个 Domain 对该事实负责；
3. 它是 Domain 规则、跨边界 contract、外部 adapter、纯量化 primitive、平台机制还是 App 装配；
4. 它属于 owner 下哪个稳定 capability；
5. capability 内由哪个文件职责承载。

不得从“这是一个 Service、DTO、trait、枚举或数据库查询”反推目录。

### 1.2 capability-first，不做全局技术分层

正确：

~~~text
crates/domains/execution/src/order_lifecycle/
  model.rs
  commands/
  queries/
  ports/
~~~

禁止：

~~~text
crates/domains/execution/src/models/
crates/domains/execution/src/services/
crates/domains/execution/src/repositories/
~~~

原因是订单状态、命令、查询和持久化边界需要共同维护同一组业务不变量。

### 1.3 目录按真实复杂度生长

不要提前创建空目录。一个 capability 初始只有一个稳定概念时，可以使用单文件：

~~~text
reference/
  instrument.rs
~~~

出现两个及以上独立变化原因，或同时存在状态、命令、查询、Port 时，再升级为目录：

~~~text
reference/
  lifecycle/
    mod.rs
    model.rs
    commands/
    queries/
    ports/
    tests/
~~~

拆目录不是完成迁移的证据；真实入口、业务行为和测试才是。

## 2. capability 内部文件职责

每个 capability 只使用实际需要的文件，不要求凑齐模板。

| 位置 | 唯一职责 | 禁止内容 |
|---|---|---|
| mod.rs | 声明子模块、最小 re-export、能力边界说明 | 复杂流程、数据库实现、巨型枚举集合 |
| model.rs 或 model/ | 实体、值对象、状态及其自身不变量 | 外部 DTO、SQL、跨 Domain 编排 |
| policy.rs 或 policies/ | 可复用的纯业务决策 | I/O、调度、环境变量 |
| commands/ | 改变 owner 事实的业务操作 | 直接依赖具体数据库或交易所客户端 |
| queries/ | owner 语义下的只读查询 | 绕过 owner 读取其他 Domain 数据库 |
| consumers/ | 把已验证的事件转换为本能力命令 | 外部消息 SDK 细节、第二套业务规则 |
| ports/ | capability 真正需要的最小输入或输出边界 | 按供应商 API 镜像出的巨大 trait |
| error.rs | 可行动的业务错误或边界错误 | 原始供应商错误全量泄露 |
| tests.rs 或 tests/ | capability 行为、不变量和失败路径 | 只验证构造器或目录存在 |

使用以下拆分触发条件：

- 单文件接近 400 行：检查是否存在多个业务概念；
- 单文件超过 600 行：Domain 或 Adapter 生产代码必须拆分；
- facade 超过 100 行：检查是否承载业务逻辑；
- facade 超过 150 行：必须拆分；
- 任意 Rust 文件不得超过 1000 行；
- 同一文件同时出现状态枚举、外部 DTO、SQL、流程编排和错误映射时，不等到行数上限，立即按职责拆分。

### 2.1 阅读成本优先于文件行数

拆分的首要判断不是文件有多少行，而是修改一个小规则时，是否被迫理解多个不相干的变化原因。以下情况即使尚未触发行数预算，也必须在原 capability 内收口：

- 有序规则链只保留一个短编排入口；每条规则使用业务命名的私有函数和强类型结果，顺序、首个阻塞与诊断输出必须由 parity 测试固定。不得先抽通用 Trait 框架；
- Domain 对象混合身份、游标、覆盖范围、有效期和摘要时，拆成不可变值对象；外层 Aggregate 保留业务入口，Postgres Row 可以继续按表结构扁平映射；
- 一条 SQL 的列、占位符和 bind 顺序超过人工可安全核对的范围时，由 owner-scoped Adapter Row 统一完成 Domain 到持久化值和 bind 的映射，并用真实 PostgreSQL 集成测试逐字段回读；
- App 运行时同时包含监督、live loop、重试和时间策略时，按这些运行职责拆文件，但保留唯一监督入口；错误分类、恢复决策和状态迁移仍归 Domain；
- 多版本配置只允许在 `raw -> canonical plan` 转换边界兼容；canonical plan 和后续业务链路不得继续分支解释旧版本；
- 多条 readiness/validation 规则不得分别维护 blocker、degradation 和最终状态；使用 capability 私有收集器统一去重与归约，规则函数只表达单条业务判断；
- `api`/`spi` 必须按 capability 提供可导航子模块，不得成为几十个无分组 re-export 的类型袋；没有真实调用方、存量数据、外部契约或迁移窗口时，不保留旧平铺出口或类型别名；
- 历史类型只有存在可验证的生产数据或契约消费者时才能进入 `legacy/`；否则调用方原子迁移到 canonical 类型后删除旧实现，不制造永久兼容层。

拆分文件必须以业务阶段、规则意图、值对象或运行职责命名。`part1.rs`、`helpers.rs`、`common.rs` 以及仅按行数切割的目录均不构成有效拆分。

枚举必须跟随它约束的业务事实：

- OrderStatus 放在 execution/order_lifecycle；
- CredentialStatus 放在 account/session 或 account/admission；
- RiskDecision 放在 risk/pre_trade；
- ExchangeOrderType 属于 adapter 的协议 DTO；
- 仅用于跨服务传输的枚举放在对应版本的 contract。

禁止建立全局 enums.rs、types.rs 或 models.rs 汇总所有 Domain 概念。

## 3. Domain 目标目录

以下目录是完整能力地图。只有迁移到相关能力时才创建实际目录。

### 3.1 Control

~~~text
crates/domains/control/src/
  activation/             # 运行角色、策略版本和交易能力启停事实
  kill_switch/            # 全局、账户、策略和交易对停机状态
  publication/            # 可发布配置及其生效窗口
  api.rs                  # 对 App 暴露的窄命令和查询
  spi.rs                  # Control 必需的外部能力
~~~

Control 不拥有订单、风控或策略规则；它只拥有启停和发布控制事实。

### 3.2 Market

~~~text
crates/domains/market/src/
  reference/
    instrument.rs         # 交易标的稳定身份
    timeframe.rs          # K 线周期值对象
    instrument_rules/     # 精度、步长、最小数量、状态和生效时间
    lifecycle/            # 标的发现、变更、退市和来源摄取
    source_profiles/      # OKX、Binance 公共数据源能力与限额配置
    readiness/            # 数据可用性和新鲜度事实
    dataset_facts/        # point-in-time 数据身份和指纹事实
  stream/
    bars/
      history/            # 分表历史读取、分页和版本
      sync/               # 增量同步游标、缺口和提交结果
      finalization/       # 未确认到已确认 K 线的状态转换
      snapshot/           # 可复现数据快照
      ports/              # K 线来源和存储边界
    trades/               # 成交流
    order_book/           # 盘口快照与增量
    funding/              # 资金费率
    mark_index/           # 标记价格与指数价格
    quality/              # 缺口、乱序、重复和时钟质量
  api.rs
  spi.rs
~~~

Market 拥有原始交易所市场事实及其数据质量，不拥有策略指标结论、账户余额或下单判断。

### 3.3 Strategy

~~~text
crates/domains/strategy/src/
  definition/             # strategy key、版本、参数 schema 和适用边界
  catalog/                # 可用策略版本与能力查询
  runtime/                # 生产可见的策略实例状态
  evaluation/             # 信号时点输入快照和因果评估状态
  signal/                 # 信号事实、证据与过期规则
  signal_handoff/         # 向 Portfolio 或 Execution 交付信号的业务合同
  release/                # promote、停用、回滚与 PromotionReceipt
  api.rs
  spi.rs
~~~

Strategy 只产生带版本和证据的信号事实。资金分配属于 Portfolio，账户准入属于 Account，交易前风控属于 Risk，订单生命周期属于 Execution。

### 3.4 Portfolio

~~~text
crates/domains/portfolio/src/
  policy/                 # 资金分配、容量和组合暴露政策
  candidate_batch/        # 同一决策时点的候选集合
  ranking/                # 仅使用信号时点可见信息的确定性排序
  allocation/             # 资金和风险预算分配
  netting/                # 同方向、反方向和相关簇净额
  target/                 # PortfolioTarget 事实与版本
  api.rs
  spi.rs
~~~

Portfolio 不读取交易所私有账户 DTO 直接做决策；它消费 Account 投影和 Strategy 信号。

### 3.5 Account

~~~text
crates/domains/account/src/
  session/                # 账户会话和凭证引用，不保存明文密钥
  projection/             # 余额、仓位、挂单和保证金统一投影
  admission/              # verified、权限、产品类型和交易资格
  facts/                  # 带来源时间的交易所账户事实
  exposure/               # 已占用保证金和账户级暴露
  recovery/               # 私有流断线、重连和快照恢复
  api.rs
  spi.rs
~~~

固定 API Key 获取公共 K 线属于 Market source profile，不属于自营账户。用户凭证和 signed read-only preflight 才属于 Account。

### 3.6 Risk

~~~text
crates/domains/risk/src/
  policy/                 # 风险政策版本和适用范围
  pre_trade/              # 下单前准入与拒绝原因
  valuation/              # 风险金额、名义价值和保证金口径
  approval/               # 带版本的 RiskApproval
  continuous/             # 持仓后的持续风险监控
  action/                 # 降杠杆、暂停、平仓建议和 kill switch 请求
  api.rs
  spi.rs
~~~

Risk 不负责调用交易所下单。它输出审批、拒绝或风险动作请求，并保留可审计理由。

### 3.7 Execution

~~~text
crates/domains/execution/src/
  intake/                 # 接收已批准目标或执行请求，完成幂等去重
  context/                # 决策时 InstrumentRules、账户和风险快照
  planning/               # 数量、价格、方向、订单类型和保护单计划
  intent/                 # 不可变 ExecutionIntent
  oms/                    # 订单聚合和命令入口
  order_lifecycle/        # OrderState 单向状态机
  protection/             # 强制止损、保护单状态和补偿义务
  outbox/                 # 待发送事实及事务一致性责任
  dispatch/               # mutation fence 之后的发送状态
  recovery/               # 超时、未知结果、重试和交易所回查
  safety_obligation/      # 未完成保护或未知订单的持久化安全义务
  api.rs
  spi.rs
~~~

进入 Execution 时使用 Decimal 和当时有效的 InstrumentRules 生成合法订单参数；行情分析内部可以使用 f64，但不得把未经重新量化的值直接提交交易所。

Execution 的 Port 以业务需要命名，例如 LoadInstrumentRules、ReserveExecutionLease、SubmitFencedOrder。禁止按 OKX 或 Binance 全量 API 创建万能 ExchangePort。

### 3.8 Reconciliation

~~~text
crates/domains/reconciliation/src/
  detection/              # 内部事实与交易所事实差异检测
  case/                   # ReconciliationCase 状态机
  recovery/               # 补拉、重放、人工确认和关闭
  evidence/               # 差异、来源时间和处置证据
  api.rs
  spi.rs
~~~

Reconciliation 只检测和推动 owner 修复，不直接覆盖 Account、Execution 或 Market 的事实表。

### 3.9 Research

~~~text
crates/domains/research/src/
  experiment/             # 假设、唯一变量、run 状态和 checkpoint
  dataset/                # point-in-time 数据集和 universe 身份
  simulation/             # Research 对回测引擎的实验请求
  evaluation/             # L0 至 L3 gate 与晋级资格
  evidence/               # 研究制品、指纹和结论证据
  qualification/          # 生成可供 Strategy release 审查的候选资格
  api.rs
  spi.rs
~~~

Research 可以调用 quant backtest，但不能直接发布生产策略、创建执行任务或修改订单事实。

## 4. Quant 共享能力

Quant 只承载无业务 owner、确定性且可复用的数学和模拟机制：

~~~text
crates/quant/
  math/                   # 数值稳定、统计和滚动窗口 primitive
  indicators/             # 无策略语义的指标计算
  backtest/
    clock/                # 因果时钟
    scheduler/            # 事件调度
    replay/               # point-in-time 回放
    matching/             # 模拟撮合
    costs/                # 手续费、滑点和资金费模型
  analytics/
    performance/          # EV、PF、Sharpe、回撤等口径
    attribution/          # 币种、时间、市场状态和事件簇贡献
~~~

以下内容不能下沉 Quant：

- Vegas 或其他具体策略的入场条件；
- 会员、订阅、用户 readiness；
- 账户余额是否足够；
- 是否允许实盘；
- 强制止损；
- 订单重试和 lease；
- 研究晋级结论。

## 5. 跨边界 Contracts

~~~text
crates/contracts/src/
  envelope.rs             # 通用版本、追踪和幂等信封
  market/v1/              # Market 跨进程事实
  strategy/v1/            # 信号及执行请求合同
  portfolio/v1/
  account/v1/
  risk/v1/
  execution/v1/
  reconciliation/v1/
  research/v1/
~~~

规则：

- contract 必须按 owner 和版本组织；
- Domain 内部 model 不因字段相似直接搬进 contracts；
- 外部交易所 DTO 不进入 contracts；
- contract 只表达跨进程必需字段，不复制数据库 Entity；
- contract 变更必须有 snapshot 或兼容测试；
- 无真实消费者时不创建共享 contract。

## 6. Adapters

~~~text
crates/adapters/
  exchange-gateway/src/
    public_market/        # OKX、Binance 公共行情能力
    private_account/      # signed read-only 账户能力
    fenced_mutation/      # 只能消费有效 permit/fence 的下单、撤单和保护单
    quota/                # 外部限频与退避机制
    mapping/              # SDK DTO 与内部边界类型映射
  postgres/src/
    market/
    strategy/
    portfolio/
    account/
    risk/
    execution/
    reconciliation/
    research/
  postgres-schema/src/
    catalog/              # 既有表、分表和 owner 清单
    plan/                 # 可审计 DDL plan
    apply/                # schema-tool 唯一 DDL 执行入口
    roles/                # 独立数据库角色与授权
  redis/src/
    leases/
    runtime_state/
  quant-web-client/src/
    subscription_read/
    credential_reference/
    execution_request/
    result_writeback/
  object-storage/src/
    research_evidence/
  notification/src/
    execution_alert/
    risk_alert/
~~~

Adapter 只负责协议、序列化、签名、错误归一、持久化和外部机制。业务校验放回 owner Domain。

交易所适配第一版只允许 OKX 和 Binance。crypto_exc_all 是协议 SDK 事实源；目标仓库 Adapter 必须复用 SDK，不平行重写签名、endpoint DTO、精度规则或错误映射。

数据库优先复用 legacy 生产表。Market K 线继续按交易对和周期分表；不得恢复已废弃的 market_candles 单表。新增或变更表前必须先登记 DDL owner、现有表差距、回填、回滚和 schema-tool 流程。

## 7. Platform

~~~text
crates/platform/
  kernel/                 # ID、时间、版本等无业务 owner primitive
  config/                 # 配置加载与来源校验
  messaging/              # 传输机制，不含业务事件定义
  lifecycle/              # 进程启动、关闭和健康状态
  observability/          # trace、metric、结构化日志
  security/               # secret handle、脱敏和审计 primitive
  testkit/                # 跨包测试基础设施，禁止生产依赖
~~~

Platform 不得成为 shared business logic。任何包含 order、signal、position、credential、risk 或 subscription 语义的代码，都必须重新确认 owner。

## 8. Apps

~~~text
apps/
  control-api/
  market-data-worker/
    main.rs
    job_runner/
    job_plans/
    config/
  signal-worker/
  account-worker/
  execution-worker/
  reconciliation-worker/
  schema-tool/
  quant-lab/
~~~

App 只允许：

- 加载和校验配置；
- 创建 Adapter；
- 注入 Domain API/SPI；
- 启动调度或消费循环；
- 处理进程生命周期、健康检查和观测上下文；
- 把已分类错误映射为退出码或运行状态。

App 中长时运行链路按 `supervisor`、`live_loop`、`retry`、`timing` 等真实运行职责组织；对外只保留一个监督入口。该拆分只改善生命周期可读性，不得把业务错误分类、账户 freshness、恢复谓词或交易决策搬入 App。

App 禁止：

- 判断是否买卖；
- 判断账户是否可交易；
- 计算风险额度；
- 改写订单状态机；
- 绕过 Core Gateway 直接实盘 mutation；
- 直接拼 SQL 修改其他 Domain 事实；
- 为方便而复制 Domain 枚举和校验。

## 9. 新代码放置决策表

| 问题 | 是 | 否 |
|---|---|---|
| 是否改变或判断某个业务事实？ | 放到该事实 owner 的 capability | 继续判断 |
| 是否只是跨进程传输 owner 事实？ | 放到 contracts/owner/version | 继续判断 |
| 是否只是交易所、数据库、Redis 或 HTTP 协议实现？ | 放到对应 adapter | 继续判断 |
| 是否是无业务语义的数学、指标或模拟 primitive？ | 查询总账后放到 quant | 继续判断 |
| 是否是配置、生命周期、观测或安全机制？ | 查询总账后放到 platform | 继续判断 |
| 是否只负责装配和进程入口？ | 放到 app | 不允许创建，先补充事实 owner |

发现缺失能力时：

1. 搜索 capability id、业务名和目标路径，确认总账不存在；
2. 写清改变的事实、唯一 owner 和至少一个真实消费者；
3. 判断是现有 capability 的子能力还是新 capability；
4. 在总账登记唯一 id、target、Wave 和复用策略；
5. 通过架构检查后再新增代码；
6. 若需跨 Domain 共享，优先暴露 owner API 或 contract，不移动业务实现。

## 10. 典型放置示例

| 需求 | 正确位置 | 原因 |
|---|---|---|
| OKX 公共 K 线签名或请求 DTO | crypto_exc_all 的 OKX SDK 域 | 交易所协议事实 |
| 选择 OKX 或 Binance 作为 K 线源 | market/reference/source_profiles | Market 数据来源政策 |
| K 线缺口和同步游标 | market/stream/bars/sync | Market 同步事实 |
| RSI 计算 | quant/indicators | 无策略语义的数学 primitive |
| Vegas RSI 阈值 | strategy/definition 或 evaluation | 具体策略语义 |
| 用户凭证是否 verified | account/admission | 账户准入事实 |
| Plus 最多三个 combo | quant_web owner service | 商业订阅事实，不进入 Core Domain |
| 下单数量按步长量化 | execution/planning，规则来自 Market | 执行决策，依赖当时规则快照 |
| Binance quantity 字段序列化 | exchange-gateway/fenced_mutation mapping | 外部协议映射 |
| 必须存在止损才能下单 | execution/protection 与 risk/pre_trade | 业务安全规则 |
| worker lease 持久化 | execution Port + redis/leases adapter | owner 规则与机制分离 |
| 订单结果未知后的回查 | execution/recovery | 订单恢复责任 |
| 内外订单差异 | reconciliation/detection | 对账事实 |
| 回测手续费模型 | quant/backtest/costs | 模拟机制 |
| 是否晋级 L3 | research/evaluation | 研究治理事实 |
| 策略版本 promote | strategy/release | 生产策略发布事实 |

## 11. 禁止命名与例外

生产代码禁止新增以下无 owner 容器：

- common
- utils
- helpers
- support
- misc
- shared
- services
- base
- manager
- `*_helpers`
- `*_support`
- generic services
- global models
- global enums

只有以下受控例外：

- 测试目录内局部 support；
- 生成代码要求的固定目录；
- 外部协议官方命名且被封装在单个 Adapter 内；
- 经 ADR 明确批准的基础设施能力。

例外不得对外暴露为默认扩展点，也不得承载业务判断。

## 12. 目录评审清单

新增或移动代码前逐项回答：

- capability id 是什么；
- 改变的业务事实是什么；
- owner 是谁；
- 目标路径是否已登记且唯一；
- 是否已有 `module-boundary-policy.toml` 登记的 canonical 实现可复用；
- 新增 canonical 实现时是否同时登记 capability、唯一路径、禁止别名和注入违规测试；
- 为什么不能放在现有 capability；
- 是否引入新的跨 Domain 依赖；
- Port 是否由使用方定义且足够窄；
- App、Adapter 是否混入业务判断；
- 是否复制了数据库 Entity、外部 DTO 或枚举；
- 是否需要状态机、幂等、事务、Outbox 或恢复 owner；
- 是否触发文件拆分阈值；
- 修改一个小规则是否仍需理解多个不相干概念；有序规则的执行顺序和首个阻塞输出是否有 parity 证据；
- 宽 Domain 对象是否按不变量拆成值对象，宽 SQL 是否由单一 Adapter Row/bind 映射并逐字段回读；
- 多版本兼容是否只停在 raw 输入转 canonical 模型的边界；无真实证据的旧类型、别名和 re-export 是否已删除；
- API/SPI 是否按 capability 可导航，App 监督循环是否只保留生命周期职责和唯一入口；
- 热路径是否复制完整窗口、头删 Vec、无界 collect/sort、读取 env/“最新配置”或重复序列化；
- provider lock/数据库事务内是否存在 snapshot 逐行 SQL await 或外部 SDK/HTTP I/O；
- 性能修改是否保留行为 parity、真实数据库原子性/并发证据和可重复 benchmark 口径；
- 是否有真实消费者和行为测试。

任何一项无法回答时，不得通过创建 common、utils、helpers、support、service 或临时 trait 绕过归属判断。
