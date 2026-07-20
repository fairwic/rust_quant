# ADR-0005：分离控制面与交易数据面

- 状态：已接受
- 首次接受：2026-07-18
- 最近修订：2026-07-20
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

`control-api` 管理：

- StrategyDefinition、StrategyRelease、配置 schema 和 release pointer；
- 策略生命周期与 promote/rollback；
- 版本化配置快照；
- 运行资格、暂停、恢复和 kill switch；
- Worker desired state 和运营查询。

### 数据面

Market、Strategy、Portfolio、Account、Risk、Execution 和 Reconciliation Domain 构成交易数据面；它们不要求一一对应独立 Worker。默认运行角色是 market-worker、signal-worker、account-worker、execution-worker 和 reconciliation-worker。

数据面只消费已经发布、带版本、不可变的配置快照。实时处理过程中不得同步调用控制面获取策略参数、风险阈值或临时默认值。

每次业务事件记录：

- config snapshot version；
- strategy manifest hash；
- portfolio policy version；
- risk policy version；
- control decision 或 kill-switch generation。

控制面不可用时，数据面按已发布策略选择：

- 在有效期内继续运行；
- 停止接收新开仓但继续处理成交、撤单、保护单和对账；
- 配置过期或无法证明安全时 fail-closed。

Kill switch 必须使用高优先级、可确认、可审计的传播机制，但仍通过 RiskAction/Execution 状态机执行，不允许控制面绕过 owner 直接调用交易所。

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
- kill switch 可以在目标时限内传播并获得执行证据；
- 控制面没有真实交易所 mutation 权限。
