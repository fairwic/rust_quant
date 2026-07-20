# 量化通用逻辑归属

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
- 上位文档：[Rust Quant 目标架构](target-architecture.md)

## 1. 核心原则

“通用”不是一个目录，而是一个经过真实调用方证明的复用范围。代码不能因为名字看起来通用，就提前放进 `common`、`utils` 或共享 crate。

量化项目中的复用分为五类：

1. owner 无关的基础类型；
2. 纯数学与时间序列算法；
3. 通用技术指标；
4. 回测、分析和研究基础设施；
5. 外部协议与跨服务合同。

业务规则即使被多个策略使用，也应先判断它是否仍属于 Strategy、Portfolio、Risk、Execution 等明确 owner，不能直接升级成全局共享代码。

## 2. 通用层级与目录

| 复用类型 | 目录 | 允许内容 | 禁止内容 |
| --- | --- | --- | --- |
| Owner 无关基础 | `crates/platform/kernel` | Clock、时间戳、CorrelationId、稳定数值值对象 | Candle、PortfolioTarget、Order、Position、StrategySignal |
| 纯量化数学 | `crates/quant/math` | 滚动窗口、统计、回归、归一化、数值稳定算法 | 买卖判断、数据库、环境变量 |
| 技术指标 | `crates/quant/indicators` | EMA、RSI、MACD、ATR、布林带、形态 primitives | 策略入场结论、仓位和下单 |
| 确定性回测内核 | `crates/quant/backtest` | 时间推进、事件调度、Replay、撮合、费用、滑点、资金费 | Domain 编排、Experiment、某策略规则、promote 结论 |
| 绩效分析 | `crates/quant/analytics` | PnL 序列、回撤、Sharpe、Sortino、胜率 | 用户展示文案、生产发布状态 |
| 研究业务 | `crates/domains/research` | DatasetManifest、Experiment、Run、Checkpoint、SimulationProfile、Evidence、OOF/OOS | 原始行情、Strategy Definition、生产订单/账户事实 |
| 业务内部复用 | `crates/domains/<owner>` | 同一 owner 下多个用例稳定共用的模型和规则 | 无 owner 的万能 helpers |
| 跨服务数据 | `crates/contracts` | 带 owner 和版本的 wire DTO/Event | SQLx Model、页面 DTO、SDK 原始类型 |
| 外部系统实现 | `crates/adapters` | Postgres、Redis、HTTP、交易所、通知实现 | 策略、风控和订单状态机 |
| 进程技术能力 | `crates/platform` | 配置、日志、安全、取消、健康检查、消息运行时 | 业务判断和策略参数 |
| 测试复用 | `crates/platform/testkit` | Builder、Fixture、Fake Port、Deterministic Clock | 生产代码依赖 |

## 3. `kernel` 应该非常小

`crates/platform/kernel` 只放没有明确业务 owner、语义高度稳定、被多个业务 crate 使用的基础能力，例如：

```text
crates/platform/kernel/src/
├── time/
│   ├── timestamp.rs
│   └── clock.rs
├── decimal/
│   ├── percentage.rs
│   └── ratio.rs
├── ids/
│   └── correlation_id.rs
└── error/
    └── invariant_violation.rs
```

以下类型不应因为“很多地方使用”就进入 kernel：

- `Candle`、`Ticker`、`SymbolRules`：属于 Market；
- `StrategyDefinition`、`StrategyRelease`、`StrategyDecision`：属于 Strategy；
- `CapitalBudget`、`PortfolioTarget`、`TargetPosition`：属于 Portfolio；
- `Balance`、`Position`：属于 Account；
- `RiskApproval`：属于 Risk；
- `OrderIntent`、`ExecutionPlan`、`Order`：属于 Execution；
- `ReconciliationIssue`：属于 Reconciliation。

其他模块通过 owner 的稳定公开 API 使用这些类型，而不是把所有业务类型搬进 kernel。

## 4. `quant/math`：无交易语义的纯数学

适合放入：

- rolling sum、mean、min、max、variance；
- 分位数、标准差、z-score；
- 线性回归和相关系数；
- 数值归一化和缺失值策略；
- 固定窗口 ring buffer；
- 对数收益率等不包含买卖判断的数学变换。

判断标准：把类型名中的 BTC、ETH、buy、sell、position、strategy 全部删除后，算法语义仍然完整。

不适合放入：

- “RSI 超过 70 就做空”；
- “回撤超过 20% 禁止下单”；
- “Vegas 4H 只交易 ETH”；
- “没有止损不允许开仓”。

这些都是业务规则，分别属于 Strategy 或 Risk。

## 5. `indicators`：可复用指标，不产生交易结论

适合放入：

- EMA、SMA、RSI、MACD、ATR；
- Bollinger Bands、Keltner Channel；
- 成交量比率和波动率；
- 可独立解释的 K 线形态 primitive。

指标输出数值、状态或可解释特征，不直接输出 `Buy`、`Sell`、`OpenLong`。从指标组合到交易结论的逻辑属于具体策略。

例如：

```text
EMA 计算                         -> quant/indicators
EMA20 > EMA60                    -> domains/strategy/strategies/<name>
EMA 多头排列且风险允许开仓       -> Strategy + Risk use case
```

## 6. `backtest`：复用运行机制，不复用策略结论

适合放入：

- 历史时间推进；
- candle/tick replay；
- 撮合与成交模型；
- fee、slippage、funding 模型；
- 回测事件和交易记录；
- deterministic clock；
- 禁止未来数据访问的时间边界。

某个策略的参数搜索、专属过滤器、入场规则和跨域调用不进入 backtest。`quant/backtest` 只提供确定性内核，由 Research use case 调用同一 Strategy Evaluator、Portfolio Policy、Risk Policy 和所需的 Execution 纯 API。

依赖方向为：

```text
Production Domain -> quant/math + quant/indicators

Research Domain
  -> Production Domain 稳定公开 API
  -> quant/backtest + quant/analytics

quant/* -> 不依赖业务 Domain
```

Research 的 `SimulationLedger` 保存模拟现金、仓位、费用和权益，并生成 Portfolio/Risk 可消费的模拟 read model；它不是生产 AccountProjection。不得另造 `BacktestRisk`、`BacktestPositionSizer` 或策略规则，但成交模型可以按 SimulationProfile 明确近似。

## 7. `analytics` 与 `research` 的边界

`analytics` 负责对结果进行确定性计算：

- total return；
- max drawdown；
- Sharpe、Sortino；
- win rate、profit factor；
- exposure、turnover；
- 交易频率和持仓时长。

Research Domain 负责组织实验和证据：

- 数据集和样本窗口；
- 参数空间和实验编号；
- OOF/OOS；
- Artifact、Definition 与 Runtime Snapshot identity；
- candidate 比较；
- promote 建议和拒绝理由。

Analytics 不决定是否发布策略；Research 不直接修改 live 默认版本。Research 通过自己的 Port 持久化 Evidence，Strategy Release 只引用已完成 Evidence identity。

## 8. 业务模块内部的通用逻辑

被多个策略使用的代码不一定是全局通用。

| 代码 | 正确归属 |
| --- | --- |
| 多个策略共用的 StrategyDefinition 校验 | `domains/strategy/model` |
| 多个策略共用的 Signal 去重 | `domains/strategy/use_cases` 或 Strategy 内部共享模块 |
| 多个组合共用的资金预算、目标净额和冲突处理 | `domains/portfolio`；纯矩阵或优化算法可下沉 `quant/math` |
| 多个风险政策共用的敞口计算 | `domains/risk`；纯数学部分可下沉 `quant/math` |
| 多个交易所共用的订单精度处理 | 规格事实归 `domains/market/model`，协议映射归 exchange-gateway |
| 多个执行动作共用的订单状态迁移 | `domains/execution/model` |
| 多个对账任务共用的差异分类 | `domains/reconciliation/model` |

只有 owner 无关的那一小部分才继续下沉到通用 crate。

## 9. 常见代码归属示例

| 代码 | 应放位置 | 原因 |
| --- | --- | --- |
| Candle、Ticker、OrderBook | `domains/market/model` | 市场事实 |
| Symbol 标准化后的业务类型 | `domains/market/model` | Market 拥有标准交易品种 |
| 交易所原始 symbol 映射 | `adapters/exchange-gateway` | 外部协议差异 |
| EMA/RSI/ATR | `quant/indicators` | 通用指标 |
| RollingWindow、z-score | `quant/math` | 无交易语义纯算法 |
| Vegas 入场过滤 | `domains/strategy/strategies/vegas` | 策略专属规则 |
| Vegas 候选失效价/候选保护位 | `domains/strategy/strategies/vegas` | 解释信号何时失效，不代表最终风险审批 |
| 通用 StrategyEvaluator Trait | `domains/strategy/api` | Strategy owner 的公开协议 |
| 历史 `position_leverage=0.58` 的资金占用语义 | `domains/portfolio` 的 `allocation_ratio` | 是资本分配比例，不是交易所杠杆 |
| 真实交易所 leverage/margin mode | `domains/risk` 审批 + `domains/execution` 实现 | 账户风险与执行能力 |
| 多策略资本预算、目标仓位、信号净额 | `domains/portfolio` | 目标组合与资金分配 |
| 交易所实际余额、持仓和 PnL 投影 | `domains/account` | 外部账户事实 |
| 最大仓位、最大回撤规则 | `domains/risk` | 风险不变量 |
| 价格/数量 rounding 业务校验 | `domains/market/model` + `domains/execution/use_cases` | 规格事实与执行门禁分开 |
| 下单/撤单/保护单状态机 | `domains/execution` | 订单生命周期 |
| Exchange HTTP 请求 | `adapters/exchange-gateway` | 外部技术实现 |
| 订单与交易所结果差异 | `domains/reconciliation` | 对账事实 |
| fee/slippage/funding 模拟机制 | `quant/backtest` | owner 无关回测机制 |
| Sharpe/max drawdown | `quant/analytics` | 绩效计算 |
| 参数实验、数据指纹、Run/Checkpoint/Evidence | `domains/research` | 有独立身份、生命周期和持久化 owner |
| 模拟现金、仓位、working orders 与权益 | `domains/research/model/simulation_ledger` | 研究模拟事实，不是生产 AccountProjection |
| 同时点多币信号收集与决策屏障 | `domains/research/use_cases` | 防止 symbol 遍历顺序改变资金分配 |
| StrategySignalV1 JSON | `contracts/strategy/v1` | 跨进程协议 |
| FillEventV1 JSON | `contracts/execution/v1` | Execution 到 Account 的跨进程事实 |
| SQLx Row Model | `adapters/postgres/<owner>` | 数据库实现细节 |
| HTTP timeout/retry primitive | `platform` 或 Adapter 内部 | 进程技术能力 |

## 10. 共享晋升条件

代码从业务模块晋升到通用 crate 前，必须全部满足：

1. 已有至少两个真实、不同 owner 的调用方；
2. 两个调用方的语义一致，不只是代码长得相似；
3. 不依赖 owner 私有状态、数据库、环境变量和外部 SDK；
4. API 已经稳定，可以独立测试；
5. 下沉后不会产生反向依赖或循环依赖；
6. 能用一句不包含具体策略或服务名称的话描述职责。

如果不满足，保留在 owner 模块。对十几行简单逻辑，允许暂时重复，也不要制造错误抽象。

## 11. 明确禁止的“伪通用”目录

禁止新增以下兜底目录或同义变体：

```text
common/
utils/
helpers/
shared-services/
base-service/
misc/
```

允许业务模块内部存在范围明确的私有 helper，但文件名必须表达用途，例如 `price_rounding.rs`、`signal_dedup.rs`，不能只有 `utils.rs`。

## 12. 错误、配置和缓存不是全局业务通用

- 不建立一个包含所有错误的全局 `AppError`；每个业务模块定义稳定错误，App/Adapter 在边界映射。
- 不建立一个包含所有环境变量的全局配置；每个 App 只解析自己需要的强类型配置。
- 不建立全局 Repository Trait；Port 由实际使用它的业务模块定义。
- Redis 技术实现可以共享，cache key、TTL 和失效规则仍归业务 owner。
- 日志初始化可以共享，日志字段、审计内容和敏感信息规则仍由业务用例明确。
