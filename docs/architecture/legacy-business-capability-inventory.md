# Legacy 业务能力全量盘点

本文冻结 rust_quant 到 rust_quant_alpha 迁移的 L1 业务能力基线。逐条机器登记、唯一目标路径和迁移状态以 rust_quant_alpha/architecture/business-capability-catalog.toml 为准。

- 盘点基线：rust_quant@9e8a23dab9b68519641f098be979ed528eca4121
- 目标仓库：rust_quant_alpha
- 盘点日期：2026-08-02
- 目标交易所范围：OKX、Binance
- 登记规模：124 个目标 capability，6 个 Domain Wave
- 当前工作树覆盖：不把基线之后未提交的 Research/策略实验改动宣称为已盘点完成；进入 W2 时单独做增量语义审查

## 1. 盘点口径

本次 L1 盘点回答：

- 业务为什么存在；
- 事实 owner 是谁；
- legacy 主要实现散落在哪里；
- 目标 capability 是什么；
- preserve、optimize、new、defer 或 retire；
- 进入哪个 Wave；
- 哪些语义必须在 L2 深挖。

L1 不宣称已逐函数证明行为一致，也不把目录、trait、README、迁移登记或测试名称当作迁移完成。

## 2. 四层认知框架

### 2.1 已知的已知

- legacy 已覆盖公共行情、K 线分表、指标、多个策略、回测、账户只读、风险、实盘 worker、订单保护和部分对账。
- legacy 业务语义分散在 domain、market、services、orchestration、risk、execution、trading、infrastructure 和 tests。
- crypto_exc_all 是交易所协议事实源；目标代码应复用 SDK。
- 第一版只迁移 OKX 与 Binance。
- 没有自营账户业务；固定 API Key 只用于公共行情配额或交易所要求，不代表用户交易账户。
- K 线继续按交易对和周期分表，market_candles 是废弃方案。
- 数据库和既有生产表优先复用，只有语义或治理不满足时才改表。
- 真实下单必须保留凭证准入、只读预检、InstrumentRules、Decimal 量化、lease、幂等、风险审批和保护单。

### 2.2 已知的未知

- legacy 各策略版本在生产、准生产、paper 和研究环境的真实使用范围，需要 W2 逐一冻结。
- 订单未知结果、保护单失败和重试在所有入口上的状态机是否完全一致，需要 W4 golden cases。
- 现有生产分表清单、owner、授权和历史异常表的最终冻结结果，需要 W1 数据库实证。
- quant_web 与 Core 之间仍有哪些跨库直连和隐式 contract，需要 W3、W4 沿真实调用方确认。
- OKX 与 Binance 同类 endpoint、错误和精度能力是否全部具备 parity，需要各 Wave 对照 SDK contract tests。

### 2.3 未知的已知

- 很多所谓 Service 实际同时包含 Strategy、Portfolio、Account、Risk 和 Execution 判断，文件名掩盖了多个 owner。
- 现有 domain/entities 和 enums 把数据库形态、跨服务 payload 与业务模型混在一起，不能整体复制。
- Backtest 不是单独业务 Domain；研究治理属于 Research，因果回放与撮合机制属于 Quant，具体信号语义属于 Strategy。
- Scheduler、worker 和 CLI 只是 App，不应决定业务顺序或状态转换。
- Reconciliation 不是 Execution 的收尾函数，而是独立检测、案例和恢复闭环。
- 公共行情固定 API Key 与用户交易凭证是两类安全边界，必须分开建模和配置。

### 2.4 未知的未知

- 历史数据中可能存在无法从源码推断的状态值、手工修正、孤儿订单或分表差异。
- 真实交易所可能返回文档未覆盖的状态转换和部分成功结果。
- 迁移期间双读或 shadow 可能暴露时间戳、精度、费用、排序和时钟语义差异。
- 未来策略或交易所扩展可能证明当前 capability 粒度仍需调整，但不得因此提前创建抽象。

## 3. L1 能力地图

### 3.1 Market

主要 legacy 来源：

- crates/market/src/models、repositories、streams、scanners
- crates/services/src/market
- crates/orchestration/src/jobs/data
- crates/infrastructure/src/repositories/candle_repository.rs
- crates/infrastructure/src/repositories/exchange_symbol_repository.rs
- crates/domain/src/entities/candle.rs、exchange_symbol.rs、funding_rate.rs

目标能力：

- market.reference.instrument
- market.reference.timeframe
- market.reference.instrument_rules
- market.reference.lifecycle
- market.reference.source_profiles
- market.reference.readiness
- market.reference.dataset_facts
- market.stream.bars
- market.stream.bars.history
- market.stream.bars.sync
- market.stream.bars.finalization
- market.stream.bars.snapshot
- market.stream.trades
- market.stream.order_book
- market.stream.funding
- market.stream.mark_index
- market.stream.quality

处置：

- 保留标的、K 线、成交、资金费和标记价格业务事实；
- 优化同步游标、确认 K 线、来源能力、数据质量和 InstrumentRules 生效时间；
- 新增明确的数据 readiness 与 dataset identity；
- 退役 market_candles 单表概念；
- W1 完成前不迁移 Strategy 扫描或实盘执行。

### 3.2 Strategy

主要 legacy 来源：

- crates/strategies/src/implementations
- crates/indicators/src/trend/vegas
- crates/services/src/strategy
- crates/orchestration/src/strategy、strategy_runner
- crates/domain/src/entities/strategy_config.rs
- crates/domain/src/value_objects/signal.rs

目标能力：

- strategy.definition
- strategy.catalog
- strategy.runtime
- strategy.evaluation
- strategy.signal
- strategy.signal_handoff
- strategy.release

处置：

- 保留已验证策略的业务语义和版本边界；
- 优化信号时点证据、过期、阻塞和 handoff；
- 把资金分配移出 Strategy；
- 把下单和保护移出 Strategy；
- 新增显式 promote、回滚和 PromotionReceipt；
- 不把所有策略塞进同一枚举、Registry 文件或万能 runner。

### 3.3 Research 与 Quant

主要 legacy 来源：

- crates/orchestration/src/backtest
- crates/services/src/strategy/backtest_service.rs
- crates/services/src/strategy/vegas_factor_research
- crates/analytics/src
- crates/indicators/src
- scripts/research
- docs/research_manifests、docs/evidence、docs/backtest_reports

Research 目标能力：

- research.experiment
- research.dataset
- research.simulation
- research.evaluation
- research.evidence
- research.qualification

Quant 目标能力：

- quant.math
- quant.indicators
- quant.backtest.clock
- quant.backtest.scheduler
- quant.backtest.replay
- quant.backtest.matching
- quant.backtest.costs
- quant.analytics.performance
- quant.analytics.attribution

处置：

- Research 拥有实验、L0 至 L3 gate 和晋级资格；
- Quant 只拥有无策略语义的确定性计算与模拟；
- Strategy 拥有具体入场、出场和信号语义；
- 保留因果回放、费用和分析能力，重写混入未来数据或组合无限资金的部分；
- 研究制品哈希继续保留，因为它证明数据集、universe、策略版本和机器结果身份。

### 3.4 Portfolio

主要 legacy 来源：

- crates/trading/src/portfolio
- crates/services/src/strategy/strategy_execution_service.rs
- crates/services/src/trading/order_creation_service.rs

目标能力：

- portfolio.policy
- portfolio.candidate_batch
- portfolio.ranking
- portfolio.allocation
- portfolio.netting
- portfolio.target

处置：

- legacy 没有清晰独立闭环，按 optimize 与 new 处理；
- 把候选排序、资金分配、容量和相关簇净额从策略与执行中拆出；
- 组合回测和生产必须消费相同排序、容量和资金政策版本。

### 3.5 Account

主要 legacy 来源：

- crates/risk/src/account、position
- crates/services/src/market/account_service.rs
- crates/orchestration/src/jobs/data/account_job.rs
- crates/risk/src/legacy_signed_read_only.rs
- crates/services/src/rust_quan_web/execution_capability.rs
- crates/domain/src/entities/exchange_api_config.rs

目标能力：

- account.session
- account.projection
- account.admission
- account.facts
- account.exposure
- account.recovery

处置：

- 保留用户交易账户只读事实和 signed read-only preflight；
- 凭证商业事实继续由 quant_web 持有，Core 只使用凭证引用和验证证据；
- 固定公共行情 API Key 不进入 Account；
- 优化余额、仓位、挂单和保证金的来源时间投影；
- Hyperliquid 不在第一版迁移范围。

### 3.6 Risk

主要 legacy 来源：

- crates/risk/src/policies、realtime、position、backtest
- crates/services/src/risk/risk_management_service.rs
- crates/orchestration/src/jobs/risk
- crates/orchestration/src/workflow/risk_order_job.rs
- execution worker 的 live guard 与 protection tests

目标能力：

- risk.policy
- risk.pre_trade
- risk.valuation
- risk.approval
- risk.continuous
- risk.action

处置：

- 保留仓位限制、回撤、止损和持续监控语义；
- 优化为带版本输入和明确拒绝原因的 RiskApproval；
- Risk 不直接下单或修改交易所保护单；
- 实际 mutation 由 Execution 执行并保留结果。

### 3.7 Execution

主要 legacy 来源：

- crates/execution/src/order_manager
- crates/trading/src/order
- crates/services/src/exchange
- crates/services/src/trading/order_creation_service.rs
- crates/services/src/rust_quan_web/execution_worker 及拆分 section
- crates/infrastructure/src/repositories/swap_order_repository.rs

目标能力：

- execution.intake
- execution.context
- execution.planning
- execution.intent
- execution.oms
- execution.order_lifecycle
- execution.protection
- execution.outbox
- execution.dispatch
- execution.recovery
- execution.safety_obligation

处置：

- 保留订单、成交、撤单、保护和 worker 的完整业务语义；
- 用不可变 intent、单向状态机和 Outbox 明确事务 owner；
- 进入 Execution 后按当时 InstrumentRules 用 Decimal 重新量化；
- SDK 只忠实表达请求，强制止损和 live gate 留在 Core；
- 未挂止损、未知结果和待补偿状态必须持久化，不能只写日志。

### 3.8 Reconciliation

主要 legacy 来源：

- execution_reconciliation_snapshot 系列
- execution_worker_reconciliation_section.rs
- execution_audit.rs
- 订单、仓位、保护相关 contract tests

目标能力：

- reconciliation.detection
- reconciliation.case
- reconciliation.recovery
- reconciliation.evidence

处置：

- 从 execution worker 的大型文件中抽取为独立闭环；
- Reconciliation 不直接覆盖 owner 数据；
- 差异必须形成案例、严重度、恢复 owner 和关闭证据。

### 3.9 Control、Platform 与运行入口

主要 legacy 来源：

- crates/core/src/config、logger、cache
- crates/orchestration/src/scheduler
- crates/infrastructure/src/cache、messaging
- crates/domain/src/entities/dynamic_config_log.rs
- crates/rust-quant-cli

目标能力：

- control.activation
- control.kill_switch
- control.publication
- platform.kernel
- platform.config
- platform.messaging
- platform.lifecycle
- platform.observability
- platform.security
- platform.testkit
- app.market_worker
- app.schema_tool
- app.signal_worker
- app.quant_lab
- app.account_worker
- app.execution_worker
- app.reconciliation_worker
- app.control_api

处置：

- App 只负责装配、调度和生命周期；
- Control 拥有启停、发布和 kill switch 事实；
- Platform 只保留无业务 owner 的技术 primitive；
- Redis 只能承载 lease 和短期运行状态，不成为订单或策略事实源。

## 4. 跨边界 Adapter 与 Contract

Adapter 能力分为：

- OKX、Binance 公共行情；
- signed read-only 私有账户；
- 经 Execution fence 的 mutation；
- 交易所 quota；
- 各 Domain PostgreSQL 持久化；
- 分表 DDL 与独立数据库角色；
- Redis lease 与运行状态；
- quant_web subscription、credential、execution request 和 result writeback；
- Research 证据对象存储；
- Risk 与 Execution 通知。

Contract 按 Market、Strategy、Research、Portfolio、Account、Risk、Execution、Reconciliation、Control 和通用 envelope 分版本管理。

处置原则：

- Adapter 复用 crypto_exc_all，不平行实现交易所协议；
- Contract 不复制数据库 Entity；
- 跨服务业务事实继续由 owner service 持有；
- 历史跨库直连只记录为待退役，不继续扩展。

## 5. 明确退役或不迁移的内容

| Legacy 形态 | 结论 | 替代位置 |
|---|---|---|
| market_candles 单表 | retire | Market 交易对与周期分表生命周期 |
| 全局 domain/entities | retire as structure | 各 owner capability 的 model |
| 全局 domain/enums | retire as structure | 枚举跟随业务事实或版本 contract |
| 万能 Service、Manager、Runner | retire as structure | capability command/query + 薄 App |
| Core 新增直连 quant_web 数据库 | forbidden | web-gateway 调 owner internal API |
| App 内业务判断 | forbidden | owner Domain |
| SDK 内用户授权、强制止损、lease | forbidden | Account、Risk、Execution |
| Hyperliquid 第一版能力 | defer | 第一版切换后独立 Wave/ADR |
| smoke-only 新路径 | retire | contract、runtime、browser 或显式 live validation |
| 每个小切片 Manifest/Registry/hash | archive | 能力总账 + Domain Wave gate |

## 6. 每个 Wave 进入前必须补齐的 L2 语义

### W1 Market

- 既有生产表、分表命名、主键、索引、owner 和 ACL；
- OKX、Binance endpoint、分页、限频和断线恢复；
- confirmed、未确认、修正、缺口和重复语义；
- Decimal 存储、f64 计算边界；
- source profile、固定 API Key 与 secret handle；
- 数据 readiness 和分表生命周期。

### W2 Strategy、Research、Quant

- 当前真实策略清单、版本和生产入口；
- Vegas 与其他策略的完整入场、出场、止损和冲突语义；
- signal time 可见字段与未来数据禁用；
- 回测撮合、成本、资金路径和 live parity；
- L0 至 L3 停止条件、数据集和 universe 身份；
- promote、回滚和旧版本并存。

### W3 Portfolio、Account、Risk

- strategy x symbol combo 与 Core 消费边界；
- 凭证验证、权限、余额和产品类型；
- 账户投影新鲜度和私有流恢复；
- 容量、排序、并发风险和相关簇；
- 风险审批、拒绝、持续监控和动作 owner。

### W4 Execution、Reconciliation

- 执行任务状态机和幂等键；
- 事务、Outbox、lease、fencing token 和未知结果；
- InstrumentRules 快照与 Decimal 量化；
- 保护单必需性、部分成功和补偿；
- 订单、仓位、余额和保护单对账；
- quant_web 回写 contract 和失败恢复。

### W5 Control 与统一切换

- 运行角色和唯一入口；
- readiness、kill switch、发布和回滚；
- 全链路 shadow、parity 和首个差异层；
- 数据库与消息积压；
- operator 证据和告警；
- 显式切流授权、回滚窗口和 legacy 退役条件。

## 7. 当前关键判断

当前最关键的判断是：目标 Domain 划分适合量化交易系统，但迁移不能继续按孤立文件或小切片衡量。必须按 Market → Strategy/Research → Portfolio/Account/Risk → Execution/Reconciliation 的业务因果顺序完成闭环。

最大风险不是目录不够多，而是：

- legacy 语义没有进入 L2 就被重写；
- 同一业务概念在 Domain、App、Adapter 各实现一份；
- 旧数据库和真实运行入口没有被纳入 parity；
- 静态检查通过后被误认为业务迁移完成；
- Execution 安全责任在多个 worker section 之间丢失。

下一步最值得采取的行动是完成 W0 验收，然后只进入 W1：冻结 Market L2 语义、生产分表事实、OKX/Binance SDK 能力和 Market golden cases。W1 未闭合前不继续扩展 Strategy、Research 或 Execution 的迁移范围。
