# 阶段 0 Owner Ledger:现有 crate -> 目标 owner 映射矩阵

- 状态:已冻结
- 冻结日期:2026-07-27
- `architecture_baseline_git_sha`:`660407f438db52acde8318e705b26ad05948aa0a`
- 上位:[目标架构 §4.2/§5](../../target-architecture.md)、[依赖规则 §2](../../dependency-rules.md)、[迁移计划 阶段 0/4](../../migration-plan.md)

本文件把当前 14 个扁平 crate 的顶层 module 映射到目标架构的 owner 分区(`domains/quant/adapters/contracts/platform/apps`)。它是阶段 4"按业务链路迁移"的落点参照,不是施工顺序表(顺序见 migration-plan.md 阶段 2-4)。

## 1. crate 级落点概览

| 现有 crate | 主要落点 | 说明 |
|---|---|---|
| `common` | `platform/kernel` + `quant/math`(部分) | 常量/错误/共享类型/时间工具;按 owner 无关性拆分 |
| `core` | `adapters`(db/cache)+ `platform`(config/logger) | `database/sqlx_pool.rs` 全局 PgPool -> adapters/postgres 基础 |
| `domain` | `domains/*` 各 owner 的 model + ports | 零依赖纯实体/trait,是各 owner model 与 port 的现成种子 |
| `infrastructure` | `adapters/postgres` + `adapters/exchange-gateway` | repositories -> postgres owner module;exchanges -> exchange-gateway |
| `services` | **拆分**(见 §2) | 最需按 owner 拆分的高价值目标 |
| `trading` | `domains/portfolio` + `domains/execution` 薄层 | audit/order/portfolio |
| `market` | `domains/market` + `adapters`(行情流) | 内部按 reference/ 与 stream/ 分 module(目标 §4.2) |
| `indicators` | `quant/indicators`(纯指标) + `domains/strategy`(vegas/strategy 部分) | 指标与策略当前耦合,需拆 |
| `strategies` | `domains/strategy` + `quant/backtest`(framework/backtest 部分) | 文件数最多;backtest 归 quant |
| `risk` | `domains/risk` + `quant/backtest`(backtest 部分) | legacy_signed_read_only 收敛(见 legacy-allowlist V3) |
| `execution` | `domains/execution` | 纠正依赖方向(legacy-allowlist V2) |
| `orchestration` | `domains/control` + `apps`(调度/工作流) | jobs/scheduler/workflow -> app 组合根;control 事实 -> domains/control |
| `analytics` | `quant/analytics` | 打断 analytics->strategies 反向依赖(legacy-allowlist V1) |
| `rust-quant-cli` | `apps/*` + `domains/research` + `quant/*` | 角色 bin -> apps;research/backtest 工具 -> research + quant |

## 2. services 拆分线(最高价值目标)

`crates/services` 的 `rust_quan_web` 子模块(约 45 文件)是 execution/account/reconciliation 三 owner 的当前合体,靠 `ExecutionWorkerLane` 枚举分流(见 [runtime-topology.md](runtime-topology.md))。目标拆分:

| services 顶层 module | 目标 owner |
|---|---|
| `exchange` | `adapters/exchange-gateway` |
| `market` | `domains/market`(velocity signal)+ `adapters` |
| `notification` | `adapters/notification` |
| `risk` | `domains/risk` |
| `strategy` | `domains/strategy` |
| `trading` | `domains/portfolio` / `domains/execution` |
| `rust_quan_web`(Execution lane) | `domains/execution` |
| `rust_quan_web`(Confirmation lane) | `domains/account` |
| `rust_quan_web`(ReportReplay lane) | `domains/reconciliation` |
| `rust_quan_web::execution_task_client` | `adapters/quant-web-client`(已 HTTP 化,直接归位) |
| `rust_quan_web::market_velocity_live_readiness` | `domains/control`(readiness 快照)/ `domains/account`(session readiness) |

## 3. owner 覆盖核对(目标 9 个 domain owner)

| 目标 owner | 现有来源 |
|---|---|
| control | orchestration(scheduler/config/readiness)、services::market_velocity_live_readiness |
| market | market crate、services::market、indicators 数据侧 |
| strategy | strategies crate、indicators::trend::vegas::strategy、services::strategy |
| portfolio | trading::portfolio |
| account | services::rust_quan_web(Confirmation lane)、risk::account |
| risk | risk crate、services::risk |
| execution | execution crate、services::rust_quan_web(Execution lane)、trading::order |
| reconciliation | services::rust_quan_web(ReportReplay lane)、三处 legacy_signed_read_only 收敛后 |
| research | rust-quant-cli 的 research/backtest 工具、strategies::framework::backtest |

owner-agnostic `quant/`:math(common 部分)、indicators(纯指标)、backtest(strategies::framework::backtest + risk::backtest)、analytics(analytics crate)。

## 4. 冻结口径

- 本映射是目标落点,不代表已迁移;实际搬块须走 migration-plan.md 阶段 2 Golden Slice 模板 + 每切片 Manifest。
- "第一期只建用得着的 crate,别建空壳"——本 ledger 不触发提前创建目标目录。
