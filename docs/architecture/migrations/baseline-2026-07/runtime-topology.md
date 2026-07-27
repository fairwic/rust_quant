# 阶段 0 基线:生产运行拓扑冻结

- 状态:已冻结
- 冻结日期:2026-07-27
- `architecture_baseline_git_sha`:`660407f438db52acde8318e705b26ad05948aa0a`
- 上位:[生产运行与恢复](../../production-runtime.md)、[目标架构 §3.2/§4.2](../../target-architecture.md)、[迁移计划 阶段 0](../../migration-plan.md)

冻结迁移开始前的 Core 生产角色装配现状。`quant-core-worker` 是多入口运行时,靠不同 bin/command 切角色,共享同一份二进制。

## 1. 角色 bin -> app 入口 -> 装配

主二进制:`[[bin]] rust_quant -> src/main.rs`;`src/lib.rs` 只 `pub mod app;`。角色 worker 是 `src/bin/quant_core_*.rs` 薄 wrapper(先 `rust_quant_cli::app_init().await?` 再调 app/services 逻辑)。

| 角色 bin(`crates/rust-quant-cli/src/bin/`) | app 入口 | 装配 |
|---|---|---|
| `quant_core_control_api.rs` | `app::control_api::run_control_api()` | 控制面 API(orchestration/services) |
| `quant_core_market_worker.rs` | `app::market_worker::run_market_worker()` | 行情采集(market/services) |
| `quant_core_signal_worker.rs` | `app::signal_worker::run_signal_worker()` | 信号生成(indicators/strategies/services) |
| `quant_core_execution_worker.rs` | `app::execution_worker_runtime::run_execution_worker_lane(ExecutionWorkerLane::Execution)` | 下单执行 lane |
| `quant_core_account_worker.rs` | 同上,`ExecutionWorkerLane::Confirmation` | 账户/成交确认 lane |
| `quant_core_reconciliation_worker.rs` | 同上,`ExecutionWorkerLane::ReportReplay` | 对账/报告回放 lane |
| `quant_core_schema_ensure.rs` | 独立 53 行 main | DB schema 确保(core/infrastructure) |

## 2. 关键事实:三 lane 共用运行时

`execution / account / reconciliation` 三个角色**共用同一运行时** `app::execution_worker_runtime::run_execution_worker_lane`,靠 `ExecutionWorkerLane` 枚举(`Execution` / `Confirmation` / `ReportReplay`)分流。逻辑主体在 `crates/services/src/rust_quan_web/execution_worker.rs` 及其 `*_section.rs` 分片。

这是目标架构中 `domains/execution`(Execution lane)、`domains/account`(Confirmation lane)、`domains/reconciliation`(ReportReplay lane)三个 owner 的拆分线——现状是三者在一个 lane runtime 里合体。见 [owner-ledger.md §2](owner-ledger.md)。

## 3. app/ 角色与子命令(冻结时)

`app/mod.rs` 声明 28 个模块。运行时角色:`control_api`、`market_worker`、`signal_worker`、`execution_worker_runtime`、`internal_server`(1146 行内部 HTTP)、`bootstrap*`。

market_velocity 家族(最大体量):`market_velocity_backfill`(1514)、`market_velocity_event_backtest`(1998)、`market_velocity_kline_scanner`、`market_velocity_live_handoff`(1964)、`market_velocity_strategy_config`。

research/panel 家族:market_beta_*、market_cross_*、market_*_reversal_research、market_structure_choch_fvg_research 等 -> 目标 `domains/research` + `quant/*` + `apps`。

## 4. 与目标 apps 的对应

| 目标 app(target §4.2) | 现状来源 |
|---|---|
| control-api | quant_core_control_api |
| market-worker | quant_core_market_worker |
| signal-worker | quant_core_signal_worker(含雷达 handoff 消费 + 订阅扇出,尚未接入) |
| account-worker | quant_core_account_worker(Confirmation lane) |
| execution-worker | quant_core_execution_worker(Execution lane) |
| reconciliation-worker | quant_core_reconciliation_worker(ReportReplay lane) |
| schema-tool | quant_core_schema_ensure |
| quant-lab | src/bin/ 下 research/backtest 独立可执行 |

## 5. 冻结口径

- 新增或重命名 Core 角色必须同步生产 compose、deploy/rollback 脚本、deploy contract tests(CLAUDE.md 约束)。
- 本拓扑是静态盘点;生产实际 command/profiles/env 开关以生产 compose 为准。
