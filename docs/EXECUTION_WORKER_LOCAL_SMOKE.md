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

任务应是 Web backend 内部队列里的可执行状态。`rust_quant` worker 现在会带两个过滤维度去请求 lease：

- `task_type in (execute_signal, risk_control_close_candidate)`
- `task_status in (pending, pending_close)`

其中：

- `execute_signal + pending` 继续走原有开/平仓 dry-run 下单链路。
- `risk_control_close_candidate + pending_close` 代表 Admin/内部复核已确认的风控平仓候选。

## 一键运行 dry-run worker

在 `rust_quant` 目录执行：

```bash
./scripts/dev/run_execution_worker_dry_run.sh
```

如果想优先走更稳的“已有二进制 + 前置诊断”入口，执行：

```bash
./scripts/dev/run_execution_worker_local_preflight.sh
```

这个 preflight launcher 会先检查：

- `target/debug/rust_quant` 是否已经存在；存在时直接强制 `EXECUTION_WORKER_USE_EXISTING_BINARY=true`，避免再次触发 cargo 编译。
- 当前 PATH 是否落在 Homebrew 的 `/opt/homebrew/bin/cargo` / `rustc`；这是本地最常见的误判来源，会导致 `cargo test`/`cargo run` 看起来走了 `rustup`，但实际仍报 `rustc 1.89.0`。
- 当 `SQLX_OFFLINE=true` 且仓库缺少 `.sqlx/` cache 时，提示旧的 `sqlx::query!` 编译期 cache 风险。

因此，本地闭环默认建议先跑 preflight launcher；只有在确实需要重新编译 `rust_quant` 时，再处理 toolchain 和 `.sqlx` 风险。

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
EXECUTION_WORKER_TASK_TYPES=execute_signal,risk_control_close_candidate
EXECUTION_WORKER_TASK_STATUSES=pending,pending_close
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core
QUANT_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core
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

代码层还有一道 live 下单保护：即使绕过本地 dry-run 脚本直接启动 worker，
`EXECUTION_WORKER_DRY_RUN=false` 也必须同时显式设置：

```bash
EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS
```

否则 worker 会在构造 live exchange gateway 前拒绝启动。设置这个值前必须先确认
Web lease 过滤条件、用户 API Key、交易所环境（模拟盘或真实盘）、下单数量和
`risk_control_close_candidate` 的 reduce-only 平仓合约。

实盘 `execute_signal` 还必须带可验证的保护止损合同。即使 payload 没有显式设置
`protective_stop_loss_required=true`，只要 worker 处于 `EXECUTION_WORKER_DRY_RUN=false`，
也会要求 `risk_plan.selected_stop_loss_price` 为正数并且方向合法；缺失或无效时会在
API credential preflight、审计写入和任何交易所 mutation 之前失败关闭。

## 分批止盈实盘安全语义

策略 payload 带 `risk_plan.take_profit_legs` 时，live worker 只在主订单确认
`completed` 且有有效 `filled_qty` 后提交 reduce-only limit 止盈单。止盈单使用
稳定 client order id，重试前先按 client order id 查询已有订单，避免重复挂同一
止盈腿。

止盈单 ACK 和重试前查到的已有止盈单必须处于可接受状态，例如 `NEW`、`OPEN`、
`LIVE`、`PARTIALLY_FILLED`、`FILLED`、OKX 成功码 `0` 或本地 `dry_run`。如果 ACK 是
`REJECTED`、`EXPIRED`、`CANCELED` 等终态失败，worker 不会把 TP 同步标记为完成；
已有终态失败订单只作为证据记录，并继续尝试重新挂该止盈腿。

如果止盈单同步失败，主订单回报仍保留 `completed`，但 raw payload 会标记
`take_profit_sync.status=take_profit_order_retry_required` 和
`retry_required=true`。Web 会把 task 保持在 `pending_take_profit_sync`，confirmation
worker 后续继续 lease 并重试止盈单同步。

如果某个止盈腿成交后需要把 runner 止损移动到新价位，worker 必须先确认新的
保护止损单，再撤旧保护止损单。新止损未确认时旧止损保持不撤；旧止损撤单失败时，
新止损已经生效，raw payload 会记录 `manual_cleanup_required=true` 供人工清理。

## Market Velocity Rust-native handoff

Market Velocity 生产 handoff 不再使用 `scripts/dev/*.sh`。Core 只负责从
`market_rank_events` 和 15m K 线中筛出合格雷达事件，调用 Web owner API 做只读
preview 或创建 `execution_tasks`；真实执行继续复用 Vegas 风格的既有
`ExecutionWorker`，不得新写一套下单系统。

只读检查当前可执行候选和 Web readiness：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
RUST_QUAN_WEB_INTERNAL_SECRET=local-dev-secret \
MARKET_VELOCITY_LIVE_BUYER_EMAIL=723875705@qq.com \
MARKET_VELOCITY_LIVE_COMBO_ID=85 \
MARKET_VELOCITY_TASK_READINESS_CREDENTIAL_ID=1 \
MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS=24 \
MARKET_VELOCITY_LIVE_CANDIDATE_LIMIT=20 \
MARKET_VELOCITY_CREATE_TASK_APPLY=false \
cargo run -q -p rust-quant-cli --bin market_velocity_live_handoff
```

如果需要刷新 Web 的 signed read-only preflight 快照，只允许显式只读确认：

```bash
MARKET_VELOCITY_TASK_READINESS_REFRESH_APPLY=true
MARKET_VELOCITY_TASK_READINESS_REFRESH_CONFIRM=I_UNDERSTAND_THIS_REFRESHES_OKX_READONLY_TASK_READINESS
```

创建 Web `execution_tasks` 仍然不是下单。它必须同时满足 Web owner preview 无
blocker、15m 入场确认通过、live signal 显式允许，并带创建确认：

```bash
MARKET_VELOCITY_SIGNAL_AUTOMATION_MODE=live
MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED=true
MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED=false
MARKET_VELOCITY_CREATE_TASK_APPLY=true
MARKET_VELOCITY_CREATE_TASK_CONFIRM=I_UNDERSTAND_THIS_CREATES_WEB_EXECUTION_TASK
```

runner 成功创建 task 后只输出 scoped worker handoff manifest。真正 live worker
仍必须走现有 `ExecutionWorker`：

```bash
IS_RUN_EXECUTION_WORKER=true
EXECUTION_WORKER_ONLY=true
EXECUTION_WORKER_RUN_ONCE=true
EXECUTION_WORKER_DRY_RUN=false
EXECUTION_WORKER_TARGET_TASK_IDS=<web_execution_task_id>
EXECUTION_WORKER_TASK_TYPES=execute_signal
EXECUTION_WORKER_TASK_STATUSES=pending,leased
EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS
```

在 `ready_for_scoped_live_worker` 前，不得运行 live worker。若 runner 返回
`market_velocity_no_entry_confirmed_candidate`，查看 `skipped_summary`：
`VolumeNotConfirmed`、`PriceBelowAverages`、`TimingTriggerNotConfirmed` 分别表示
15m 成交量、均线位置和突破/回踩触发未通过。

生产 signal-only 降噪的 entry trigger 过滤可以用环境变量调整：

```bash
MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST=breakout_previous_high,reclaim_ema
MARKET_VELOCITY_ENTRY_TRIGGER_BLOCKLIST=
```

`MARKET_VELOCITY_ENTRY_TRIGGER_ALLOWLIST=all` 可以临时关闭 allowlist；blocklist
优先级高于 allowlist。过滤结果只决定是否向 Web 提交 Market Velocity strategy
signal，不会绕过 `signal_only`、`paper_trade_required=true` 或 worker dry-run/live
安全开关。

## Market Velocity 纸面后验回测

如果要评估历史 `market_rank_events` 是否经过 4h 趋势确认、15m 入场确认和追高过滤后仍有优势，
可以直接跑 Core Rust CLI，不需要 Python：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --sample-limit 0
```

当前生产观察 preset 是 `delta_rank >= 15`、`new_rank <= 30`、4h 均线保持多头确认、
15m 入场均线距离最多 `4%`、追高过滤
`new_rank <= 10 && price_change_pct >= 8%`、止损 `3%`，默认统计
`2.0R` 在 `24h`、`48h` 两个窗口内的结果。`48h` 是事件触发后的最大观察窗口：
先触发止盈或止损就立即结算，48 小时内都未触发才记为 timeout。

如果要对比不同 15m 入场触发的后验质量，可以在确认事件之后、生成收益统计和
paper outcome 之前加入口径过滤：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --sample-limit 0 \
  --entry-trigger-allowlist breakout_previous_high,reclaim_ema
```

或仅排除某类触发：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --sample-limit 0 \
  --entry-trigger-blocklist reclaim_ma
```

2026-06-20 本地 `quant_core` 对照结果：使用现有通用 backtest pipeline 的
`symbol_isolated_100u` 口径复核后，上一轮 `delta_rank >= 20`、4h 均线距离至少
`4%` 的高胜率组合只有 `16` 次 framework 开仓，未达到最低 `30` 次观察门槛。
上一轮默认 `breakout_previous_high,reclaim_ema,pullback_hold_ema`、`2.4R` 的
framework 胜率只有 `54.72%`，不满足 65% 胜率目标。当前生产默认改为不过拟合的
`momentum_03sl_20r_v5`：`breakout_previous_high,reclaim_ema`、
`delta_rank >= 15`、15m 均线距离最多 `4%`、止损 `3%`、目标 `2.0R`，
不默认使用历史 symbol blocklist，且关闭 stop-reentry，避免当前样本里出现的二次止损放大亏损。2026-06-21 补齐
`BICO-USDT-SWAP,RE-USDT-SWAP,RESOLV-USDT-SWAP,SAND-USDT-SWAP,O-USDT-SWAP`
的 15m K 线后，该组合扫描约 `62514` 个原始候选事件、覆盖
`55` 个 candle-pair symbols，技术入场通过 `2912` 个事件，entry trigger allowlist 后
剩余 `2494` 个确认事件。framework 开仓 `41` 次、覆盖 `27` 个实际交易 symbols、胜率
`65.85365853658537%`、`trade_sharpe=4.608339744751571`、`max_drawdown_pct=3.40882238`、
`total_profit=122.47823444`；48h 事件级结果为 `trades=41`、`win=26`、`loss=11`、
`timeout=1`、`incomplete=3`、`resolved_win_rate=70.27027027027027%`、
`avg_r_complete=1.090933346468652`。

2026-06-21 继续做通用不过拟合扫描：全排名 `price_change_pct` 过热 cap、放宽
`max_new_rank`、加入 `reclaim_ma` 或 `pullback_hold_ema`、提高 target R、调整止损宽度、
FVG 入场、放宽 15m 均线距离，都没有同时满足 framework 开仓不少于 `30`、胜率不少于
`65%`、高 Sharpe、最大回撤不超过 `30%`、总盈利不低于 `150U`。不要为了达到
150U 把历史 symbol blocklist 或单日/单币种过滤升为生产默认。

本轮新增研究参数 `--max-delta-rank`，用于验证极端 `delta_rank` 是否更像追涨噪声。
它默认关闭，不改变生产观察 preset。当前较稳结果是
`--max-delta-rank 70 --target-rs 2.0`：framework 开仓 `31` 次、胜率
`70.96774193548387%`、late split 胜率 `72.72727272727273%`、回撤
`3.40882238%`，但总盈利只有 `102.83844164U`；`--max-delta-rank 79`
总盈利也只有 `106.11559397U`。因此它只能作为降噪研究工具，不能替换当前默认。
放宽 15m 均线距离到 `8%` 的最高利润组合可到约 `141.21U`，但 full 胜率
`51.43%`、late 胜率 `50.0%`，明确拒绝。

过拟合验证可以追加 `--equity-split-report`，它不会改变策略，只会把同一个
`symbol_isolated_100u` framework replay 按入场时间切成 early/late 两段。2026-06-21
当前默认 split：early `trades=14`、`win_rate=71.42857142857143%`、
`total_profit=51.29822363`；late `trades=28`、`win_rate=64.28571428571429%`、
`total_profit=76.63505327`。历史高收益 `stop_reentry_03sl_30r_v3` + symbol blocklist
当前复跑 full `total_profit=169.73251503` 但 `win_rate=61.224489795918366%`，
late split `win_rate=59.375%`，不能作为不过拟合默认。

上一轮带历史 symbol blocklist 的 `stop_reentry_03sl_30r_v3` 可达到
`total_profit=178.84826603`，但这是研究性高利润候选，不作为生产默认，避免把历史差币种过滤器过拟合到未来。继续保持 signal-only /
paper observation，不直接升自动执行。

需要把纸面结果写入 Web 观察表时，先确认 Web backend 已加载当前代码和
`market_velocity_paper_outcomes` 迁移，再执行：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
EXECUTION_EVENT_SECRET=local-dev-secret \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --sample-limit 0 \
  --paper-outcome-sink web
```

该模式只调用 Web 的 observation-only paper outcome 接口；Core 会校验
Web 返回的 `generated_execution_task_count=0`，不创建 execution task，也不会触发 worker 或真实下单。

## pending_close 风控平仓说明

`risk_control_close_candidate` 任务在 Web 侧创建时先是 `manual_review`，复核 confirm 后会进入 `pending_close`。`rust_quant` 当前实现分两层处理：

1. lease/worker 契约层：

   - worker 会把 `task_type` / `task_status` 过滤条件带到 `/api/commerce/internal/execution-tasks/lease`。
   - `rust_quan_web` 后端已支持 `task_type` / `task_status` 查询参数，默认仍只领取 `execute_signal + pending`，worker 显式请求后才会放开到 `risk_control_close_candidate + pending_close`。
   - Web lease 会把任务返回状态更新成 `leased`；worker 会把 `risk_control_close_candidate + leased` 继续按 pending close 平仓路径处理。
   - `manual_review` 不会被 worker 自动领取，必须先由 Admin 或内部服务复核 confirm 后进入 `pending_close`。

2. 执行层：

   - dry-run 下，如果 `pending_close` payload 里还没有明确的 `close_order`，worker 会直接生成一个 `order_side=close` 的 dry-run 完成结果并回写，方便先打通 Admin 复核后的结果闭环。
   - live 下，worker 不会绕过安全开关。若 Web 尚未提供 `close_order`（至少需要 side 或 position_side，以及 size/qty 等 close 指令），worker 会把任务回写成 failed，并在错误信息里明确提示缺少 Web close contract。

建议的 Web close payload 最小字段：

```json
{
  "close_order": {
    "exchange": "binance",
    "symbol": "BTC-USDT-SWAP",
    "position_side": "long",
    "size": "0.01",
    "order_type": "market",
    "reduce_only": true
  }
}
```

有了这块后，dry-run 会优先走真实的 dry-run 下单/audit 路径，live 也能复用同一份 close 指令。

## 一键验证 pending_close worker 闭环

先启动 `rust_quan_web/backend`，并显式覆盖 Postgres 配置，避免误读本地 `.env` 里的旧占位值：

```bash
cd /Users/mac2/onions/crypto_quant/rust_quan_web/backend
DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_web \
PORT=8000 \
EXECUTION_EVENT_SECRET=local-dev-secret \
cargo run
```

然后在 `rust_quant` 目录执行：

```bash
WEB_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_web \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
EXECUTION_EVENT_SECRET=local-dev-secret \
./scripts/dev/run_pending_close_worker_e2e_smoke.sh
```

脚本会复用 Web risk close smoke，但设置 `RISK_CLOSE_SMOKE_STOP_AFTER_REVIEW=1`，让 Web 停在 `pending_close + close_order`；随后只运行 `risk_control_close_candidate + pending_close` worker，并验证 Web 侧订单和交易记录已回写。

如果本地已有旧的 `pending_close` 任务，脚本会查询 `pending_close_count` 并把本次 `effective_lease_limit` 提升到足够覆盖当前待处理任务，避免旧任务挡住刚创建的 smoke task。

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

边界说明：这是 one-shot startup smoke，不是 WebSocket 持续运行验证，也不是交易所下单 E2E。它需要本地基础初始化仍可用，包括 Redis 连接、`quant_core` Postgres 连接，以及可选的 `rust_quan_web` 本地地址；如果没有 Web backend，只有当策略产生信号并尝试分发时才会在分发步骤报错。

## 一键验证 Binance WebSocket 自然触发能推进到哪一段

如果要专门验证 Binance WebSocket 自然行情能否推进到策略链路，而不是依赖 `RUST_QUANT_SMOKE_FORCE_SIGNAL`，执行：

```bash
./scripts/dev/run_binance_websocket_natural_probe.sh
```

这个脚本现在会先执行 `./scripts/dev/check_binance_connectivity.sh` 作为 preflight，默认在 Binance futures REST/WebSocket endpoint 或代理 TLS 不通时直接停止，避免 natural probe 盲跑。

单独做 endpoint / 代理诊断时，可直接执行：

```bash
BINANCE_PROXY_URL='socks5h://127.0.0.1:7897' \
BINANCE_CONNECTIVITY_RETRIES='3' \
./scripts/dev/check_binance_connectivity.sh
```

常用覆盖项：

- `BINANCE_REST_ENDPOINTS`: 以空格或逗号分隔的 REST endpoint 列表，按顺序重试。
- `BINANCE_WS_ENDPOINTS`: 以空格或逗号分隔的 WebSocket/TLS endpoint 列表，按顺序重试。
- `BINANCE_PROXY_URL`: 显式覆盖代理，优先级高于 `ALL_PROXY` / `HTTPS_PROXY`。
- `BINANCE_CONNECTIVITY_RETRIES` / `BINANCE_CONNECTIVITY_RETRY_DELAY_SECS`: 单个 endpoint 的 curl 重试次数与间隔。
- `BINANCE_CONNECTIVITY_PREFLIGHT=false`: 跳过 natural probe 前置连通性检查。
- `BINANCE_CONNECTIVITY_ALLOW_FAILURE=true`: preflight 失败也继续 natural probe，仅用于确认本机网络问题之外的链路行为。

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
SMOKE_STRATEGY_VERSION='baseline-pg-191' \
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

- `execute_signal` 任务从 pending/leased 变成 completed。
- `risk_control_close_candidate` 任务在 Web lease 能返回后，会从 pending_close/leased 变成 completed。
- execution result 被写入。
- dry-run order/trade 结果回写成功，order status 通常为 `dry_run`。有 `close_order` 的 `pending_close` 会按 close order 回写真实方向，例如 long position 平仓回写 `order_side=sell`；尚未补齐 `close_order` 的旧任务才会退回 `order_side=close`。
- live `pending_close` 在真实 mutation 前会做 signed read-only 仓位对账；如果交易所账户没有匹配 `exchange + symbol + 持仓方向` 的非零仓位，会以 `pending_close_no_matching_position` 阻断，不会继续下平仓单。

如果 handled 为 `0`，常见原因有两类：

- 当前没有可 lease 的 `pending execute_signal` 任务。
- 已有 `risk_control_close_candidate` 任务但状态仍是 `manual_review`，需要先在 Admin 执行任务页点击“确认平仓”，或调用 Web 内部复核接口转成 `pending_close`。
