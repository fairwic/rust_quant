# ADR-0004：分离 Strategy、Portfolio、Account、Risk 与 Execution

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
- 决策者：Rust Quant Core

## 背景

简单量化系统常把策略结论直接转换成订单。单策略、单账户阶段实现较快，但随着策略和账户增加，会出现：

- 策略同时承担方向判断、资金分配和订单 sizing；
- 多策略对同一 instrument 的相反目标无法统一净额；
- 回测中的虚拟仓位与生产实际账户事实混在一起；
- 风险难以判断“当前实际状态到目标状态”的变化；
- Execution 被迫理解策略优先级和资本预算。

这会使新增策略、资金分配方法或账户类型同时修改 Strategy、Risk 和 Execution。

## 决策

建立五个独立、直观的业务边界：

### Strategy

拥有 StrategyDefinition、信号、预测、置信度和证据截止。输出 `StrategySignal`，不直接输出交易所订单。

### Portfolio

拥有资本预算、策略权重、目标仓位、冲突处理和净额合并。输入 Strategy Signal 与 Account Snapshot，输出 `PortfolioTarget`。

### Account

拥有交易所实际余额、实际持仓、保证金、敞口、PnL 和数据新鲜度。Account 是观察到的实际状态，不保存策略希望达到的目标状态。

### Risk

比较 Market、Account 与 PortfolioTarget，产生带版本、理由、边界和过期时间的 `RiskDecision`。Risk 同时负责持续风险政策和 `RiskAction`。

### Execution

将批准后的目标变化转换为 `OrderIntent` 和 `ExecutionPlan`，维护订单、撤单、保护单和外部结果状态机。

### Reconciliation

拥有交易所与内部订单、成交、持仓和保护之间的差异、恢复任务与处置证据。只能通过 typed owner command 请求恢复，不直接修改 Execution、Account 或 Risk 私有状态。

标准方向：

```text
StrategySignal
  -> PortfolioTarget
  -> PreTradeSnapshot
  -> RiskDecision
  -> OrderIntent
  -> ExecutionPlan
  -> OrderEvent / FillEvent
  -> AccountProjection
  -> ReconciliationResult
```

对于用户自动交易，Web 的 `ExecutionRequest` 位于 StrategySignal 与账户级 Portfolio/Risk 处理之间。它证明商业资格、账户引用、凭证引用和用户风险配置版本，不成为 OMS 订单、最终下单金额、RiskDecision 或成交事实。账户级 Portfolio/Risk 默认由 `execution-worker` 装配；这不表示 Execution Domain 拥有 Portfolio/Risk 规则。

## 结果

### 正面影响

- 策略只表达 alpha，不绑定账户资金规模；
- 多策略可以统一分配、冲突处理和净额；
- 目标仓位与实际仓位不会混淆；
- 风险审批输入可冻结、重放和审计；
- Execution 不理解策略内部语义；
- Backtest 与 live 可以复用 Portfolio 和 Risk。

### 代价

- 增加 `PortfolioTarget`、`PreTradeSnapshot` 和映射边界；
- 单策略项目也需要一个简单 Portfolio Policy；
- Account 投影和成交反馈必须成为明确运行链路；
- 跨模块测试需要覆盖更多业务对象。

## 被否决的方案

### Strategy 直接产生 OrderIntent

无法长期支持多策略资本分配、净额和统一风险预算。

### 把 Portfolio 放进 Account

Account 是实际状态，Portfolio 是目标状态；混合后无法清楚表达“想要什么”和“已经有什么”。

### 把 Portfolio 放进 Risk

资本分配和风险审批是不同职责。Risk 可以缩减或拒绝目标，但不应决定策略组合的正常资本分配。

### 把 sizing 放进 Execution

Execution 负责如何成交，不负责为什么分配这些资金。

## 验证

- Strategy 单元测试不需要 Account 或 Exchange Connector；
- Portfolio 可以在相反 Signal 下产生确定性净额；
- Risk 可以使用固定 PreTradeSnapshot 重放；
- Execution 只接收有效批准后的目标变化；
- FillEvent 可以幂等重建 AccountProjection；
- Web ExecutionRequest 与 Core OrderIntent 使用不同身份且能通过 correlation/idempotency 链关联；
- Reconciliation 只能触发 owner command，不能直接修表。
