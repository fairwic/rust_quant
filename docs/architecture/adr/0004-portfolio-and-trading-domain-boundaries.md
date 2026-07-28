# ADR-0004：分离 Strategy、Portfolio、Account、Risk 与 Execution

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-28
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

#### `PortfolioEvaluationBatch`

`PortfolioEvaluationBatch` 是 Portfolio 的账户级、确定性输入单位：它只由已经取得有效 Web claim 的一个或多个 `ExecutionRequest` 构成，且全部属于同一 `execution_account_ref`、冻结的 `ExecutionAccountBindingV1` identity/version、`ExchangeAccountRef` 与 credential revision/revocation generation，以及明确的 decision window；单请求同样是 size=1 batch。Execution 向 Portfolio 交付稳定的 source request/claim receipt/context identity 与动态 evidence 引用，Portfolio 不读取 Web 表，也不创建或补造请求。

batch 至少固定账户/绑定/credential revision/revocation generation 引用、decision time/window、按规范排序的 source request identity、各请求 Context identity、Account/Market evidence 引用和 `batch_hash`。Portfolio 负责 batch 内排序、净额、容量分配和目标状态；它必须把每个来源请求的贡献、被净额抵消或 Blocked 原因映射回 source request。batch 不是新业务请求、不会获得 credential，也不能作为 Core 自营执行入口。

##### 窗口、claim 与结果协议

`PortfolioEvaluationBatch` 是 Portfolio 的纯输入值；Web claim 的获取、续约、释放和 batch admission/close 生命周期归 `execution-worker` 中的 Execution 编排 Use Case。该编排只固定输入集合，不拥有 Portfolio 的排序、净额、容量或目标仓位规则。

每个窗口使用稳定 `batch_window_key = execution_account_ref + execution_account_binding_version + credential_revision + credential_revocation_generation + portfolio_policy_snapshot_id + decision_window_id`。`decision_window_id` 由已冻结 Policy 与 `DecisionTime` 决定；`close_deadline_wall_clock` 只用于运行时活性，不参与 Portfolio 决策或 `batch_hash`。admission 必须把 source request identity、当前 Web claim receipt/fence 和单调 `admission_sequence` 原子绑定到一个未关闭窗口；同一有效 claim 不得同时进入两个窗口。

窗口达到已发布 Policy 的关闭条件或 `close_deadline_wall_clock` 后，Execution 原子写入 `close_watermark = last_admission_sequence` 并冻结 source identity、claim receipt、Context 与动态 Evidence 引用，生成不可变 `batch_hash`。关闭后的窗口不得重新打开、插入成员或修改 membership；在 watermark 之后取得的 claim、迟到信号或重新可用请求只能进入下一窗口，不能为了“凑满 batch”回填已关闭结果。

Execution 必须在开始 Portfolio/Risk 评估前续约并校验所有 source claim，并在 live OMS 初始化前再次校验所有实际贡献该目标的 claim。任一成员在此之前被 Web 取消、撤销或过期时，原 closed batch 整体失效：该成员以 `Cancelled` 或 `ClaimLost` 结束，其他仍有效成员释放或重排进下一窗口；不得在旧 `batch_hash` 上删成员后继续执行。已经形成 `SafetyObligation` 的既有敞口仍按其安全尾部处理，不因 Web claim 失效而被中断。

每个 source request 必须得到可审计的 `PortfolioBatchResult` 映射：`Allocated`、`Netted`、`Blocked`、`Deferred`、`Cancelled` 或 `ClaimLost`，并携带 `batch_id/hash`、source mapping、对应的 Risk/Planning identity 或 blocker。Execution 只把该用户自己的结构化 outcome 回写 Web；不得借 batch result 泄露其他用户的账户、credential、风险或策略细节。

### Account

拥有交易所实际余额、实际持仓、保证金、敞口、PnL 和数据新鲜度。Account 是观察到的实际状态，不保存策略希望达到的目标状态。

### Risk

比较 Market、Account 与 PortfolioTarget，产生带版本、理由、边界和过期时间的 `RiskDecision`。Risk 同时负责持续风险政策和 `RiskAction`。

### Execution

先将批准后的目标变化转换为纯、不可变的 `ExecutionPlanningValue`（含有序 child `OrderPlan` 与 `ProtectionPlanningValue`）；只有 live 的 Execution owner transaction 才以该值及其 hash 初始化 `OrderIntent`、持久 `ExecutionPlan` 与 `ProtectionPlan`，随后维护订单、撤单、保护单和外部结果状态机。

### Reconciliation

拥有交易所与内部订单、成交、持仓和保护之间的差异、恢复任务与处置证据。只能通过 typed owner command 请求恢复，不直接修改 Execution、Account 或 Risk 私有状态。

标准方向：

```text
StrategySignal
  -> 已 claim 的 ExecutionRequest 集合
  -> PortfolioEvaluationBatch
  -> PortfolioTarget
  -> PreTradeSnapshot
  -> RiskDecision
  -> ExecutionPlanningValue（child OrderPlan + ProtectionPlanningValue）
  -> live transaction 初始化 OrderIntent / ExecutionPlan / ProtectionPlan
  -> Execution submission lifecycle
  -> Account owner 消费 private event / signed query
  -> AccountProjection
  -> AccountFactV1（Execution Inbox/OMS transition）
  -> ReconciliationResult
```

这里仅表达 owner 与业务对象的交接方向，不定义数据库或交易所 mutation 的先后关系；外部 mutation 的唯一持久化与提交顺序以 [ADR-0006](0006-at-least-once-idempotency-and-recovery.md) 为准。

对于用户自动交易，Web 的 `ExecutionRequest` 位于 StrategySignal 与账户级 Portfolio/Risk 处理之间。它证明商业资格、账户引用、凭证引用和用户风险配置版本，不成为 OMS 订单、最终下单金额、RiskDecision 或成交事实。每个 request 的 Context 与 claim 仍保持独立；`PortfolioEvaluationBatch` 仅为同账户评估提供确定性的聚合边界，不合并 credential、商业授权或请求身份。账户级 Portfolio/Risk 默认由 `execution-worker` 装配；这不表示 Execution Domain 拥有 Portfolio/Risk 规则。

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
- 同一账户同一 decision window 的多个已 claim 请求，无论到达顺序如何都形成相同 `PortfolioEvaluationBatch`、source mapping 与 `PortfolioTarget`；
- admission/close test 证明 watermark 后的迟到 request 不会改变已关闭 `batch_hash`；claim 过期、撤销或取消会使未开始的 batch 失效并将其余成员重排到新窗口；
- 每个 source request 都能从 `PortfolioBatchResult` 追溯到 `Allocated`、`Netted`、`Blocked`、`Deferred`、`Cancelled` 或 `ClaimLost` 的唯一结果，且跨用户回写不泄露其他成员信息；
- Risk 可以使用固定 PreTradeSnapshot 重放；
- Execution 只接收有效批准后的目标变化，并在纯 `ExecutionPlanningValue` 与 live `OrderIntent`/`ExecutionPlan`/`ProtectionPlan` 初始化之间保持 hash 一致；
- 原始 private event/signed query 只能由 Account owner 以 current generation 幂等重建 AccountProjection，再发布至少带 source event/query identity、cursor/替代比较器、session generation、projection revision 与相关 watermark 的 `AccountFactV1`；Execution 只经 Inbox 更新 OMS；
- Web ExecutionRequest 与 Core OrderIntent 使用不同身份且能通过 correlation/idempotency 链关联；
- Reconciliation 只能触发 owner command，不能直接修表。
