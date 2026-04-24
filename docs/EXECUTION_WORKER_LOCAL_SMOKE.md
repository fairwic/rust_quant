# Execution Worker Local Smoke

本文档用于本地手动验证 `rust_quant` 从 `rust_quan_web` 拉取 execution task，并以 dry-run 方式执行后回写结果的链路。

## 前置条件

1. 先启动 Web backend，默认地址：

   ```bash
   http://127.0.0.1:8000
   ```

2. 本地内部 secret 默认值：

   ```bash
   local-dev-secret
   ```

3. 如果需要 worker checkpoint 和 exchange request audit 写入本地 `quant_core`，准备 Postgres：

   ```bash
   postgres://postgres:postgres123@localhost:5432/quant_core
   ```

   worker 脚本会默认设置 `QUANT_CORE_DATABASE_URL`。如果本地没有这个库，任务 lease 和回写仍由 Web backend 负责；本地 audit 写入会在日志里显示失败告警。

## 准备任务

先通过下面任一方式让 Web backend 里存在 pending execution task：

- 运行 Web smoke，走订阅/信号到 execution task 的本地流程。
- 在 Admin 侧使用 seed 或页面操作创建 execution task。

任务应是 Web backend 内部队列里的 pending 状态。worker 当前只拉取 `execute_signal` 类型任务。

## 一键运行 dry-run worker

在 `rust_quant` 目录执行：

```bash
./scripts/dev/run_execution_worker_dry_run.sh
```

脚本默认环境变量：

```bash
RUSTUP_TOOLCHAIN=1.91.1
RUSTC=$(rustup which --toolchain 1.91.1 rustc)
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000
EXECUTION_EVENT_SECRET=local-dev-secret
EXECUTION_WORKER_ID=rust_quant_local_dry_run
EXECUTION_WORKER_LEASE_LIMIT=10
EXECUTION_WORKER_RUN_ONCE=true
EXECUTION_WORKER_ONLY=true
EXECUTION_WORKER_DRY_RUN=true
EXECUTION_WORKER_DEFAULT_EXCHANGE=binance
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core
```

脚本会优先执行：

```bash
rustup run "$RUSTUP_TOOLCHAIN" cargo run --bin rust_quant
```

脚本同时会把 `RUSTC` 指向对应 toolchain 的 rustc，避免本机 PATH 里的旧 rustc 被 cargo 选中。如果当前环境没有 `rustup`，会退回执行：

```bash
cargo run --bin rust_quant
```

并只启用 execution worker，不启用回测、WebSocket、实盘策略或数据同步。`EXECUTION_WORKER_DRY_RUN=false` 会被脚本拒绝，避免误触真实下单。

## 一键验证 quant_core audit 写入

如果只想验证 `QUANT_CORE_DATABASE_URL` 设置后，dry-run execution worker 会真实写入 `quant_core.public.execution_worker_checkpoints` 和 `quant_core.public.exchange_request_audit_logs`，执行：

```bash
./scripts/dev/quant_core_audit_smoke.sh
```

这个脚本会先复用：

```bash
./scripts/dev/ddl_smoke.sh
```

随后运行：

```bash
QUANT_CORE_AUDIT_SMOKE=1 \
cargo test -p rust-quant-services --test quant_core_audit_postgres_smoke -- --nocapture
```

该集成测试会在测试进程内启动一个本地 HTTP stub 来模拟 Web backend 的 lease/report 接口，驱动真实 `ExecutionWorker` 走 dry-run 下单路径，并使用 `PostgresExecutionAuditRepository` 写入本机 `quant_core`。它不会访问真实交易所，也不需要真实 API key；测试 payload 中的假 `api_key` 会被断言为已脱敏。

## 一键验证实时策略启动读取 quant_core

如果要验证 `rust_quant` 实时策略启动路径会加载 `quant_core.strategy_configs`，并从 `quant_core` K线分表读取预热/启动 snap，执行：

```bash
./scripts/dev/run_live_strategy_quant_core_smoke.sh
```

脚本默认环境变量：

```bash
CANDLE_SOURCE=quant_core
STRATEGY_CONFIG_SOURCE=quant_core
STRATEGY_SIGNAL_DISPATCH_MODE=web
IS_RUN_REAL_STRATEGY=true
IS_OPEN_SOCKET=false
IS_BACK_TEST=false
IS_RUN_SYNC_DATA_JOB=false
IS_RUN_EXECUTION_WORKER=false
EXIT_AFTER_REAL_STRATEGY_ONESHOT=true
EXECUTION_WORKER_DRY_RUN=true
SMOKE_TIMEOUT_SECS=90
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000
```

这个 smoke 默认会先复用：

```bash
./scripts/dev/ddl_smoke.sh
```

随后运行：

```bash
cargo run --bin rust_quant
```

安全边界：

- 不开启 WebSocket，因此不会长期等待实时行情。
- `EXIT_AFTER_REAL_STRATEGY_ONESHOT=true` 会让非 WebSocket 实时策略启动路径跑完后直接优雅退出。
- `STRATEGY_SIGNAL_DISPATCH_MODE=web` 会把策略信号提交给 `rust_quan_web` 生成 execution task，不走本进程直接交易所下单路径。
- `EXECUTION_WORKER_DRY_RUN=true` 是本地安全约定；该脚本不启动 execution worker，因此不会处理或真实提交 execution task。
- `SMOKE_TIMEOUT_SECS` 是外层兜底，避免本地依赖异常时进程长时间悬挂。

边界说明：这是 one-shot startup smoke，不是 WebSocket 持续运行验证，也不是交易所下单 E2E。它需要本地基础初始化仍可用，包括 legacy MySQL 连接、Redis 连接、`quant_core` Postgres 连接，以及可选的 `rust_quan_web` 本地地址；如果没有 Web backend，只有当策略产生信号并尝试分发时才会在分发步骤报错。

## 一键验证 Binance WebSocket 自然触发能推进到哪一段

如果要专门验证 Binance WebSocket 自然行情能否推进到策略链路，而不是依赖 `RUST_QUANT_SMOKE_FORCE_SIGNAL`，执行：

```bash
./scripts/dev/run_binance_websocket_natural_probe.sh
```

如果想先离线挑出更可能自然命中的候选 `symbol/timeframe/config`，再把建议参数交给上面的 probe，先执行：

```bash
./scripts/dev/suggest_binance_natural_probe_candidates.sh
```

这个候选脚本会：

- 从 `quant_core.strategy_configs` 读取当前启用的 Binance `vegas` 配置。
- 检查对应 `public.*_candles_*` 分表是否存在，以及已确认 K 线数量、最近时间、新近波动范围、volume spike。
- 输出 `recommended_candidates` 排名，并直接给出一行可执行的 natural probe 命令。

当前评分是启发式的，不是离线复跑策略：它更偏向“更快等到确认 K 线、最近波动更活跃、配置本身已存在”的组合。对于当前库里的数据，通常会比直接把 `ETH-USDT-SWAP 4H` 配置硬改成 `1m` 更接近真实自然命中场景。

脚本默认使用：

```bash
RUSTUP_TOOLCHAIN=1.91.1
CANDLE_SOURCE=quant_core
STRATEGY_CONFIG_SOURCE=quant_core
DEFAULT_EXCHANGE=binance
MARKET_DATA_EXCHANGE=binance
IS_RUN_REAL_STRATEGY=true
IS_OPEN_SOCKET=true
IS_BACK_TEST=false
IS_RUN_EXECUTION_WORKER=false
STRATEGY_SIGNAL_DISPATCH_MODE=web
EXECUTION_WORKER_DRY_RUN=true
SMOKE_SYMBOL=ETH-USDT-SWAP
SMOKE_PERIOD=1m
SMOKE_LIVE_TIMEOUT_SECS=150
```

这个 probe 会：

- 先同步 `quant_core` 里最新的 Binance 已确认 K 线，确保实时策略能完成预热。
- 临时插入一个 `1m` 的 Binance runtime strategy config，复用现有 `vegas` 配置。
- 启动 `rust_quant` WebSocket 实时模式，但不设置 `RUST_QUANT_SMOKE_FORCE_SIGNAL`。
- 读取日志和数据库增量，分别判断以下分段是否发生：
  - `websocket_connected`
  - `confirmed_kline_triggered`
  - `handler_started`
  - `strategy_executed`
  - `signal_dispatched`

判读方式：

- 如果只到 `websocket_connected=true`，说明已经连上 Binance，但在超时时间内还没等到一个可用的已确认 `1m` K 线。
- 如果 `confirmed_kline_triggered=true` 且 `strategy_executed=true`，说明 Binance 确认 K 线已经进入 `WebsocketStrategyHandler` 和策略执行层。
- 如果 `signal_dispatched=true`，说明已经自然生成策略信号并提交给 `rust_quan_web`，此时脚本还会打印新增的 `strategy_signal_inbox` / `execution_tasks` 记录。
- 如果到达策略执行但没有 `signal_dispatched`，当前卡点就是“自然行情没有命中策略入场条件”，不是 WebSocket 接入失败。

安全边界：

- 不启动 execution worker，因此不会处理 execution task。
- 始终保持 `EXECUTION_WORKER_DRY_RUN=true`。
- 不设置 `RUST_QUANT_SMOKE_FORCE_SIGNAL`，避免把强制信号误当成自然触发结果。

建议搭配方式：

1. 先跑 `./scripts/dev/suggest_binance_natural_probe_candidates.sh`
2. 复制 Top 1 或 Top 2 输出的 `SMOKE_SYMBOL / SMOKE_PERIOD / SMOKE_STRATEGY_VERSION / SMOKE_MIN_K_LINE_NUM / SMOKE_LIVE_TIMEOUT_SECS`
3. 再执行 `./scripts/dev/run_binance_websocket_natural_probe.sh`

例如：

```bash
SMOKE_SYMBOL='BCH-USDT-SWAP' \
SMOKE_PERIOD='15m' \
SMOKE_STRATEGY_VERSION='legacy-mysql-191' \
SMOKE_MIN_K_LINE_NUM='3600' \
SMOKE_LIVE_TIMEOUT_SECS='5400' \
./scripts/dev/run_binance_websocket_natural_probe.sh
```

## 常用覆盖项

单轮处理 1 条任务：

```bash
EXECUTION_WORKER_LEASE_LIMIT=1 ./scripts/dev/run_execution_worker_dry_run.sh
```

持续轮询：

```bash
EXECUTION_WORKER_RUN_ONCE=false ./scripts/dev/run_execution_worker_dry_run.sh
```

连接非默认 Web backend：

```bash
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:18000 \
EXECUTION_EVENT_SECRET=local-dev-secret \
./scripts/dev/run_execution_worker_dry_run.sh
```

## 成功现象

worker 成功 lease 到任务后，应在日志里看到 execution worker 单轮完成或轮询完成，并显示 handled 数量。

Web backend 侧应能看到：

- execution task 从 pending/leased 变成 completed。
- execution result 被写入。
- dry-run order/trade 结果回写成功，order status 通常为 `dry_run`。

如果 handled 为 `0`，通常表示当前没有可 lease 的 pending `execute_signal` 任务。先用 Web smoke 或 Admin seed 准备任务后再运行脚本。
