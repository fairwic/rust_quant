# 阶段 0 Owner Ledger:现有 crate -> 目标 owner 映射矩阵

- 状态:已冻结
- 冻结日期:2026-07-27
- `architecture_baseline_git_sha`:`660407f438db52acde8318e705b26ad05948aa0a`
- 上位:[目标架构 §4.2/§5](../../target-architecture.md)、[依赖规则 §2](../../dependency-rules.md)、[迁移计划 阶段 0/4](../../migration-plan.md)

本文件把当前 14 个扁平 crate 的顶层 module 映射到目标架构的 owner 分区(`domains/quant/adapters/contracts/platform/apps`)。它是阶段 4"按业务链路迁移"的落点参照,不是施工顺序表(顺序见 migration-plan.md 阶段 2-4)。

## 1. crate 级落点概览

| 现有 crate | 主要落点 | 说明 |
|---|---|---|
| `common` | `platform/kernel` + `quant/math`(部分) + `domains/market` | 常量/错误/时间工具按 owner 无关性拆分；legacy `CandleItem` 是 Market OHLCV 事实，迁入 `domains/market/model`，不是 kernel 或 quant 类型 |
| `core` | `adapters`(db/cache)+ `platform`(config/logger) | `database/sqlx_pool.rs` 全局 PgPool -> adapters/postgres 基础 |
| `domain` | `domains/*` 各 owner 的 model + ports | 零依赖纯实体/trait,是各 owner model 与 port 的现成种子 |
| `infrastructure` | `adapters/postgres` + `adapters/exchange-gateway` | repositories -> postgres owner module;exchanges -> exchange-gateway |
| `services` | **拆分**(见 §2) | 最需按 owner 拆分的高价值目标 |
| `trading` | `domains/portfolio` + `domains/execution` 薄层 | audit/order/portfolio |
| `market` | `domains/market` + `adapters`(行情流) + `domains/strategy`(Velocity Signal) | Market 保留 snapshot/reference/stream 事实；Market Velocity `StrategySignal` 的业务 owner 是 Strategy，不能因数据来自 Market 而归 Market |
| `indicators` | `quant/indicators`(纯指标) + `domains/strategy`(vegas/strategy 部分) | 指标与策略当前耦合,需拆 |
| `strategies` | `domains/strategy` + `quant/backtest`(仅纯内核) + `domains/research` | 不得整块搬 `framework/backtest`；策略、状态机、止损/退出语义和 Domain 编排留在其 owner，只有 Clock/调度/Replay/撮合/成本模型下沉 Quant |
| `risk` | `domains/risk` + `quant/backtest`(仅模拟机制) | 风险判断、最终止损和审批不因文件名含 backtest 而下沉；legacy_signed_read_only 收敛见 allowlist V3 |
| `execution` | `domains/execution` | 纠正依赖方向(legacy-allowlist V2) |
| `orchestration` | `domains/control` + `apps`(调度/工作流) | jobs/scheduler/workflow -> app 组合根;control 事实 -> domains/control |
| `analytics` | `quant/analytics`(纯指标) + `domains/research` + `adapters` | 纯 equity/trade/event 指标下沉 Quant；策略特定 panel/training/证据工作流归 Research，SQLx/持久化归 Adapter，打断 analytics->strategies 反向依赖(legacy-allowlist V1) |
| `rust-quant-cli` | `apps/*` + `domains/research` + `quant/*` | 角色 bin -> apps;research/backtest 工具 -> research + quant |

## 2. services 拆分线(最高价值目标)

`crates/services` 的 `rust_quan_web` 子模块(约 45 文件)是 execution/account/reconciliation 三 owner 的当前合体,靠 `ExecutionWorkerLane` 枚举分流(见 [runtime-topology.md](runtime-topology.md))。目标拆分:

| services 顶层 module | 目标 owner |
|---|---|
| `exchange` | `adapters/exchange-gateway` |
| `market` | `domains/market`(snapshot/quality/reference)+ `domains/strategy`(Market Velocity Signal)+ `adapters` |
| `notification` | `adapters/notification` |
| `risk` | `domains/risk` |
| `strategy` | `domains/strategy` |
| `trading` | `domains/portfolio` / `domains/execution` |
| `rust_quan_web`(Execution lane) | `domains/execution` |
| `rust_quan_web`(Confirmation lane) | `domains/account` |
| `rust_quan_web`(ReportReplay lane) | `domains/reconciliation` |
| `rust_quan_web::execution_task_client` | `adapters/quant-web-client`(已 HTTP 化,直接归位) |
| `rust_quan_web::market_velocity_live_readiness` | `domains/market`(行情证据) / `domains/account`(session 证据) / `domains/execution`(执行可用性)；`domains/control` 只发布 release/kill-switch 并聚合只读诊断 |

## 3. owner 覆盖核对(目标 9 个 domain owner)

| 目标 owner | 现有来源 |
|---|---|
| control | orchestration(scheduler/config/release)、legacy `services::market_velocity_live_readiness` 中可拆出的 release/kill-switch 投影；Market/Account/Execution readiness 仍分别归其 owner |
| market | market crate、services::market 的行情 snapshot/quality/reference、indicators 数据侧 |
| strategy | strategies crate、indicators::trend::vegas::strategy、services::strategy、Market Velocity `StrategySignal` 与 handoff |
| portfolio | trading::portfolio |
| account | services::rust_quan_web(Confirmation lane)、risk::account |
| risk | risk crate、services::risk |
| execution | execution crate、services::rust_quan_web(Execution lane)、trading::order |
| reconciliation | services::rust_quan_web(ReportReplay lane)、三处 legacy_signed_read_only 收敛后 |
| research | rust-quant-cli 的 research/backtest 工具、strategies::framework::backtest |

owner-agnostic `quant/`:math(common 的纯数值部分)、indicators(纯指标)、backtest(从 `strategies::framework::backtest` 与 `risk::backtest` 中切出的 Clock/调度/Replay/撮合/费用/滑点/资金费机制)、analytics(纯结果指标)。任何依赖 Strategy/Risk 类型、SQLx、实验生命周期或证据发布的块都不能作为整 crate 搬入 `quant/`。

## 4. 冻结口径

- 本映射是目标落点,不代表已迁移;实际搬块须走 migration-plan.md 阶段 2 Golden Slice 模板 + 每切片 Manifest。
- "第一期只建用得着的 crate,别建空壳"——本 ledger 不触发提前创建目标目录。
- 禁止以 crate 名或目录名做整体搬迁决定：每个源模块先按“Market 事实 / Strategy 或 Risk 业务语义 / Research 生命周期 / Quant 纯机制 / Adapter I/O”拆分，再写入 Manifest 的 source/target 路径。`CandleItem` 的 canonical 目标是 Market public model；数据库 Row 仅在 Market Postgres Adapter 映射。

## 5. 跨仓库执行链治理基线

下列条目是未来 Program Registry/Owner 子 Manifest 的固定 owner 口径，不代表当前实现已经迁完：

| 事实或 Contract | 唯一 owner | 迁移约束 |
|---|---|---|
| `MarketSnapshot`、历史 DatasetManifest、公共行情采集 | Market | 固定服务 API Key 只在此边界做公共只读采集；必须保存 endpoint/method/权限 evidence hash，不能传入用户执行链 |
| Market Velocity `StrategySignal`、`StrategySignalHandoffV1` | Strategy | Market 只提供输入；Strategy 在本地事务写 handoff + Outbox，随后经 `CreateExecutionRequestFromSignalV1` 交给 Web |
| canonical `ExecutionRequest`、用户 credential 状态、claim lease | Web (`rust_quan_web`) | Execution 只能经 Claim/Renew/Release Contract 读取/续租/释放，不能直写 Web 表或创建自营请求 |
| `CoreExecutionIntake`、live `ExecutionPlan`、Order/Attempt/Protection | Execution (`rust_quant`) | B 阶段只生产 `ExecutionPlanningValue`；C1 才在 Execution 本地事务将其落实为持久 OMS aggregate |
| `ImmutableDecisionEvidenceBundleV1` | Execution test-only Adapter | 只组合各 owner 已冻结的 evidence；不拥有 Market/Account/Instrument 事实，不进 runtime、Paper、Live 或 scheduler |

跨仓库父 Program 以 [`../programs/registry.toml`](../programs/registry.toml) 为机器可读权威索引；父 Program 没有代码 owner。所有 child Manifest 必须标出 owner repository、`depends_on`、Contract snapshot object 和本地事务/Inbox/Outbox。历史 Research characterization 明确 `historical_dependency_eligible = false`，不得充当任何 future current-migration child 的依赖。
