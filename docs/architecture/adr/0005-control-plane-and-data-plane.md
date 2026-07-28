# ADR-0005：分离控制面与交易数据面

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-28
- 决策者：Rust Quant Core

## 背景

策略版本、运行配置、发布状态、账户授权、Worker 管理和 kill switch 属于低频管理能力；行情、策略评估、风险审批和订单执行属于实时交易能力。

如果交易热路径同步调用管理 API 或临时读取可变配置，会产生：

- 控制面故障放大为交易中断；
- 同一事件在不同时间读取到不同参数；
- 无法证明订单使用的策略和风险版本；
- 管理请求和交易请求竞争资源；
- 回测、重放和故障恢复无法复现。

## 决策

### 控制面

`control-api` 是 Control owner 的组合入口，并路由其他 owner 的受权管理命令；它本身不因此接管所有策略事实。唯一 owner 分工如下：

- **Strategy**：StrategyDefinition、StrategyArtifact、StrategyRelease 生命周期、StrategyRuntimeSnapshot 及其发布，以及面向 Control 的不可变 `ActivationEligibilityV1`；
- **Control**：`ActivationPointer`、activation scope 的单调 `activation_generation`、`KillSwitchSnapshot`、全局 `kill_switch_catalog_generation`、每 scope 的 `scope_generation`、发布控制审计；Control 只消费 Strategy 发布的激活资格，不自行推断资格；
- **各业务 owner**：自己的 Policy Snapshot、运行时 readiness 与 typed control request；
- `control-api`：Worker desired state、运营查询、Control command，以及向 Strategy/Risk/Account 等 owner 转发的显式命令；不得通过管理入口绕过 owner 状态机。

`ActivationPointer` 只选择哪个已发布 RuntimeSnapshot 在明确运行范围生效；它不修改 Release，不是“当前 readiness”，也不等于 Kill Switch。它只能消费 Strategy owner 为该不可变 Snapshot 发布且尚未撤销的 `ActivationEligibilityV1`：资格必须同时绑定 `runtime_snapshot_id/hash`、`strategy_release_id/generation`、Completed Evidence 引用、`release_stage`、允许的 deployment channel、eligibility generation 和撤销状态。stage 与 channel 的最小矩阵固定为：`Research`/`Retired` 不允许任何 Pointer，`Paper -> paper`、`Shadow -> shadow`、`Canary -> canary`、`Live -> live`；Canary 仍需其额外范围与授权。Pointer 写入必须记录所消费资格的 identity/hash/generation；资格撤销、stage/channel 不匹配、Evidence 缺失或已撤销时拒绝写入，并让现有 Pointer 失效。Control 不得以“已发布”或可变 Release 状态自行猜测资格。Kill Switch 的 scope 固定为 `global`、`exchange`、`execution_account_ref`、`strategy_key@version` 或 Web combo identity；请求可来自运营、RiskAction、Account 故障或 Web 商业停用，但唯一可写入/发布的 owner 是 Control。Release/activation/kill 三类 generation 必须分开，不能通过同一个无 scope generation 判断。

这里的 Release/激活资格不是泛化的运行时 `Readiness` owner。Market 拥有行情/参考数据新鲜度，Account 拥有账户投影与 ExchangeSession，Execution 拥有执行链路可用性；控制面可以展示这些证据的只读聚合，但不得覆盖它们的事实、替它们在热路径放行，或把“控制 API 可用”当成交易可用。

### 数据面

Market、Strategy、Portfolio、Account、Risk、Execution 和 Reconciliation Domain 构成交易数据面；它们不要求一一对应独立 Worker。默认运行角色是 market-worker、signal-worker、account-worker、execution-worker 和 reconciliation-worker。

数据面只消费各 owner 已经发布、带版本、不可变的 Policy Snapshot。Execution 在 Web canonical request intake 时把某次决策使用的 `StrategyRuntimeSnapshot`、`PortfolioPolicySnapshot`、`RiskPolicySnapshot` 和 `ExecutionPlanningPolicySnapshot` 绑定为不可变 `ExecutionDecisionContextSnapshot`；Research 对自己的 `ResearchScenarioRef` 使用 `ResearchDecisionContextSnapshot`，不伪造 Web execution fields。实时处理过程中不得同步调用控制面获取策略参数、风险阈值、“最新版本”或临时默认值。

每次业务事件记录：

- decision context identity/hash；
- strategy manifest hash；
- 四个 Domain Policy Snapshot identity/version/hash；
- ActivationPointer identity/`activation_generation`；
- preparation 时的 `kill_switch_catalog_generation` 和命中 scope generation；
- live Context 的 `subject_binding_hash`，或 Research Scenario binding identity。

Market、Account 和 InstrumentRules 是本次决策的动态 Evidence，不是假装稳定的配置；它们与 Context 一起记录后才能重放决策。完整分层见 [ADR-0011](0011-layered-runtime-snapshots-and-decision-context.md)。

控制面不可用时，数据面按已发布策略选择：

- 在有效期内继续运行；
- 停止接收新开仓但继续处理成交、撤单、保护单和对账；
- 配置过期或无法证明安全时 fail-closed。

Kill Switch 必须使用高优先级、可确认、可审计的传播机制。Dispatcher 在最终门禁用最新发布 catalog 重新评估所有命中 scope；Control 不直接调用交易所，RiskAction/Execution 状态机仍负责 reduce-only、保护与恢复。

## 结果

### 正面影响

- 管理 API 故障不会自动破坏在途订单和成交处理；
- 每次交易可以追溯到不可变配置；
- 数据面可以独立压测、扩缩容和恢复；
- 回测、shadow 和 live 使用相同配置身份；
- 安全停止行为可以明确测试。

### 代价

- 需要配置发布、缓存、确认和过期机制；
- 配置变更不是任意共享内存修改，而是显式发布；
- kill switch 需要单独定义延迟、确认和失败策略；
- 运营查询接受投影的短暂最终一致性。

## 被否决的方案

### 每次评估同步调用 control-api

把低频管理依赖放入交易热路径，扩大延迟和故障面。

### Worker 直接读取可变数据库配置

无法固定一次决策使用的完整配置版本，也难以重放。

### 控制面直接调用 Exchange Connector

绕过 Risk、Execution 状态机、幂等、lease 和审计边界。

## 验证

- 断开控制面时，数据面按配置继续安全运行或停止；
- 相同事件重放使用相同配置版本；
- 配置过期后禁止新开仓；
- contract/integration test 证明 ActivationPointer 只接受当前 `ActivationEligibilityV1` 允许的 channel × stage，且资格撤销后旧 Pointer 不能继续被数据面消费；
- kill switch 可以在目标时限内传播并获得执行证据；
- 控制面没有真实交易所 mutation 权限。
