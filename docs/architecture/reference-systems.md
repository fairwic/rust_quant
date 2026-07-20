# 开源交易系统架构参考

- 状态：参考资料，不构成本仓库规范
- 日期：2026-07-20
- 权威方案：[Rust Quant 长期目标架构](target-architecture.md)

## 1. 使用原则

本项目借鉴成熟系统的边界与运行思想，不复制任一仓库的完整目录。每个项目的交易频率、语言、部署方式、账户模型和产品边界不同，照搬目录只会把别人的历史约束带入本项目。

引入代码、协议或实现前必须单独审查 license；本文只引用公开架构与目录作为设计参考。

## 2. NautilusTrader

- [GitHub](https://github.com/nautechsystems/nautilus_trader)
- [官方架构说明](https://nautilustrader.io/docs/latest/concepts/architecture/)
- [Execution 源码目录](https://github.com/nautechsystems/nautilus_trader/tree/develop/crates/execution/src)

适合借鉴：

- Data、Risk、Execution 等 Engine 边界；
- Message Bus 的 command/event 与请求响应模式；
- 组件生命周期和可观测状态；
- Execution 内部明确的 order manager、protection、reconciliation；
- Backtest 与 live 复用核心业务组件。

本项目不照搬：

- 全部引擎、消息和缓存抽象；
- 与 Nautilus 运行模型绑定的组件数量；
- 为未来低延迟需求预建复杂基础设施。

对本方案的影响：Execution 必须把 OMS、Protection 和 Reconciliation 明确建模；运行组件必须有生命周期状态；ResearchBar/PaperEvent/live 复用 Strategy、Portfolio、Risk 与订单状态规则，但分别保留撮合精度和恢复协议边界。

## 3. QuantConnect LEAN Algorithm Framework

- [GitHub](https://github.com/QuantConnect/Lean)
- [Algorithm Framework 概览](https://www.quantconnect.com/docs/v1/algorithm-framework/overview)
- [Portfolio Construction](https://www.quantconnect.com/docs/v2/writing-algorithms/algorithm-framework/portfolio-construction/key-concepts)
- [Risk Management](https://www.quantconnect.com/docs/v1/algorithm-framework/risk-management)
- [Execution](https://www.quantconnect.com/docs/v1/algorithm-framework/execution)

适合借鉴：

```text
Alpha/Insight
  -> Portfolio Construction/Target
  -> Risk Management
  -> Execution
```

该链路直接支持本项目对 Strategy、Portfolio、Risk、Execution 的分离，尤其适合多策略目标合并和风险缩减。

本项目不照搬 LEAN 的 C# 框架类型、算法宿主和云平台产品结构。

## 4. Barter

- [GitHub](https://github.com/barter-rs/barter-rs)

适合借鉴：

- Rust typed component 与事件驱动 Engine；
- live/mock/backtest 能力替换；
- EngineState 审计流和只读副本思想；
- 按 data、execution、instrument、integration 等能力拆 Workspace。

对本方案的影响：Admin/Web 的非热路径查询优先使用可重建投影或审计流，不把查询需求反向塞入交易写模型。

注意：Barter 自身明确偏教育用途，因此只作结构参考，不把它视为生产安全证明。

## 5. Hummingbot

- [GitHub](https://github.com/hummingbot/hummingbot)
- [源码目录](https://github.com/hummingbot/hummingbot/tree/master/hummingbot)

适合借鉴：

- Exchange Connector 的标准接口与能力分类；
- REST、WebSocket、订单簿、用户流和交易规则适配的分离；
- Connector 能力差异保持显式。

本项目不照搬：

- 大量 `core/utils` 式公共目录；
- 多代 Strategy 目录长期并存的组织方式；
- 把交易所业务门禁下沉到 Connector。

对本方案的影响：`crypto_exc_all` 保持交易所协议事实源，Core `exchange-gateway` 只做业务 Port 到 SDK 能力的适配。

## 6. Tesser

- [GitHub](https://github.com/tesserspace/tesser)

适合借鉴：

- Rust Workspace 中 core、broker、strategy、indicators、portfolio、data、execution、events、backtest、connector 的紧凑划分；
- 小规模系统先保持模块化、避免过早微服务化。

本项目不照搬：

- Portfolio 同时容纳风险/PnL 的模糊边界；
- Execution 直接从 Signal 决定 sizing；
- 默认一个进程对应一个策略。

这些做法不适合本项目的多策略净额、多账户容量与统一 API 限频。

## 7. 最终采用矩阵

| 设计问题 | 主要参考 | 本项目决策 |
| --- | --- | --- |
| 数据、风险、执行引擎边界 | NautilusTrader | Domain 模块化单体，不强制每个 Domain 独立进程 |
| Strategy 到订单的业务链 | LEAN | Strategy -> Portfolio -> Risk -> Execution |
| 研究与生产规则复用 | NautilusTrader、Barter | Research Domain 编排稳定 Domain API；`quant/backtest` 只做纯内核，ResearchBar/PaperEvent/RecoveryHarness 分级验证 |
| OMS、保护和对账 | NautilusTrader | 全部归 Core；Reconciliation 不直接修表 |
| Rust typed event 与状态投影 | Barter | Contract + Inbox/Outbox + 可重建 Read Model |
| 交易所 Connector 标准化 | Hummingbot | `exchange-gateway` 包装 `crypto_exc_all`，能力差异显式 |
| 紧凑 Workspace | Tesser | 五类物理目录，按证据拆 crate/进程 |

## 8. 明确不采用

- 不照搬任一项目完整目录；
- 不默认每个 Domain、策略或交易所一个服务；
- 不建立无 owner 的 `common/utils/core/services`；
- 不让 Connector 决定会员、风控、强制止损或 live 门禁；
- 不用开源项目的“可运行”替代本项目的 contract、recovery、parity 和生产证据。
