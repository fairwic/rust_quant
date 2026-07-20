# ADR-0008：回测复用 Domain API 的双层 Quant 依赖

- 状态：已被 [ADR-0009](0009-research-domain-and-tiered-simulation.md) 取代
- 首次接受：2026-07-20
- 决策者：Rust Quant Core

> 2026-07-20 复核现有 Vegas 参数回测、Paper 执行和生产 OMS 后，确认本 ADR 让 `quant/backtest` 直接编排所有 Domain，仍可能形成新的 God Crate，并混淆快速研究回测与执行恢复验证。保留本文作为决策历史；当前规范以 ADR-0009 为准。

## 背景

`quant/math`、`quant/indicators`、`quant/backtest`、`quant/analytics`、`quant/research` 都位于 `quant` 分区，但它们不是同一个依赖层。

数学和指标是业务 Domain 的上游纯计算基础。回测则必须顺序驱动 Strategy、Portfolio、Risk、Execution 和 Account 的真实规则；如果强制回测也处于所有 Domain 上游，只能复制 `BacktestStrategy`、`BacktestRisk`、`BacktestPositionSizer` 和 `BacktestOrderState`，长期必然与 paper/live 漂移。

现有 Vegas 已经部分复用同一个 `get_trade_signal`，但后续回测 pipeline 又把风险配置、资金比例、开平仓、止损与模拟成交混在一起，证明“复用信号函数”不足以保证全链路 parity。

## 决策

将 Quant 明确分为两层：

```text
Quant Foundation
  = math + indicators

Quant Simulation Tooling
  = backtest + analytics + research
```

依赖方向为：

```text
Domains -> Quant Foundation

Quant Simulation Tooling
  -> Domain stable public APIs
  -> Quant Foundation

Apps/quant-lab
  -> Quant Simulation Tooling
  -> Domain commands
  -> Adapters
```

强制约束：

1. Domain 禁止依赖 `quant/backtest`、`quant/analytics`、`quant/research`；
2. Simulation Tooling 只能依赖 Domain 的稳定公开 API，不能访问私有 module、Port、Adapter、数据库 Row 或环境变量；
3. Backtest 必须调用与 paper/live 相同的 Strategy Evaluator、Portfolio Policy、Risk Policy 和 Execution 状态机；
4. 运行模式只替换 Clock、Market、Account 与 Exchange Adapter；
5. 模拟订单、成交和账户状态不写生产事实表，只形成 ResearchEvidence；
6. Quant Tooling 不直接持久化，完成结果通过 Strategy owner command 保存；
7. `quant-lab` 只负责配置映射、装配、调用和生命周期，不实现策略、资金或风险规则。

## 结果

### 正面影响

- backtest、paper、shadow、live 可以逐层比较，而不是只对最终 PnL；
- Strategy、Portfolio、Risk、Execution 的修复会自动进入回测路径；
- 回测机制仍可保持确定性、无外部副作用和独立性能优化；
- 不需要增加新的 Research Domain 或通用 Orchestration Service。

### 代价

- Cargo 依赖图不再是“所有 Quant 都在 Domain 下方”的单一直线；
- Domain 必须提供足够小且稳定的公开 API；
- arch-check 需要区分 Quant Foundation 与 Simulation Tooling；
- backtest 迁移必须拆掉现有万能 Context 和混合 `deal_signal`，不能只移动文件。

## 被否决的方案

### 为回测复制业务政策

短期依赖简单，长期产生第二套 Strategy、Risk 和订单语义，无法接受。

### 让 Domain 依赖回测 Trait

会把研究运行时带入生产业务模块，并形成反向或循环依赖。

### 把全部回测编排放进 App

App 会逐渐承载交易规则和状态机，重现当前大 Orchestration 问题。

### 新建万能 Application/Workflow crate

在当前只有 quant-lab 这一真实调用方时属于提前抽象。先由 `quant/backtest` 驱动公开 Domain API；如果未来出现两个非研究的跨域流程且无法由终端 owner use case 表达，再用新 ADR 评估。

## 验证

- Cargo/arch-check 阻止 Domain 依赖 Simulation Tooling；
- 固定 DatasetSnapshot 和 RuntimeSnapshot 下，Vegas backtest 与 paper/shadow 的 StrategySignal 一致；
- 相同模拟 AccountSnapshot、Portfolio/Risk 版本下，Target、RiskDecision 和 OrderIntent 一致；
- 模拟 FillEvent 驱动与 live 相同的订单和账户状态迁移；
- 回测失败或证据写入失败不会产生生产 Order/Fill/Account 记录；
- 现有 legacy pipeline 在 parity 切换完成后可按职责删除。
