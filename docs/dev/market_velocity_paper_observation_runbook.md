# Market Velocity Paper Observation Runbook

This runbook is for production observation of Market Velocity paper outcomes.
It runs the Rust-native `market_velocity_paper_observation` binary and writes
observation-only rows into Web `market_velocity_paper_outcomes`.

## Safety Boundary

- No Python production path is used.
- No exchange order, cancel, close, or account mutation is performed.
- No Web `execution_tasks` are created by this command.
- The production observation entry owns the paper outcome sink and entry-trigger
  filter. Experimental filter overrides must use `market_velocity_event_backtest`
  outside this production runbook.
- The default production observation command is locked to the named stable preset
  `momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1`.
  Do not pass ad-hoc target R, stop-loss, reentry,
  FVG, profit-protection, or runner-exit parameters to the production observer.
- 当前 preset 也固定本轮优化后的入场过滤：`min_delta_rank=15`、
  `trend_min_average_distance_pct=0.0`、`entry_max_distance_pct=4.0`。
- 生产默认不带历史 symbol blocklist；如果要研究性复现历史黑名单结果，只能在
  `market_velocity_event_backtest` 中显式传 `--symbol-blocklist`。
- `kline15m 0.52R/fvg50` 是独立 paper/shadow challenger。不要覆盖默认 observer；
  使用 `kline15m-paper-observation-scheduler` profile 单独启动。

## Required Environment

```bash
QUANT_CORE_DATABASE_URL=postgres://...
RUST_QUAN_WEB_BASE_URL=https://...
EXECUTION_EVENT_SECRET=...
MARKET_VELOCITY_PAPER_OBSERVATION_INTERVAL_SECS=21600
MARKET_VELOCITY_KLINE15M_PAPER_OBSERVATION_INTERVAL_SECS=21600
```

`DATABASE_URL` may also be set to the same value as `QUANT_CORE_DATABASE_URL`
for compatibility with existing Core deployment conventions.
`MARKET_VELOCITY_PAPER_OBSERVATION_INTERVAL_SECS` and
`MARKET_VELOCITY_KLINE15M_PAPER_OBSERVATION_INTERVAL_SECS` are optional and
default to 21600 seconds, or 6 hours, in the deploy compose scheduler services.

## Production Image Requirement

The runtime image must include both binaries:

- `/usr/local/bin/rust_quant`
- `/usr/local/bin/market_velocity_paper_observation`

`Dockerfile.runtime` builds `rust-quant-cli --bins` and copies both binaries.

## One-Shot Run

Using the production deploy compose file:

```bash
podman compose -f docker-compose.deploy.yml --profile observation run --rm quant-core-market-velocity-paper-observation
```

The service is behind the `observation` profile and has `restart: "no"`, so it
does not start with the live radar or execution worker services. The compose
command runs:

```bash
market_velocity_paper_observation --paper-strategy-preset momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1
```

## 15m K 线直抽样研究验证 - 2026-07-04

本轮动量候选不再把“异动雷达”作为策略本体的一部分验证，而是直接从
本地 15m K 线表中按 seed 抽样 20 个交易对，复用同一套 15m 入场逻辑：

- `event_source=kline_15m`
- `sample_limit=20`
- `sample_seed=kline15m_fvg30_v1`
- `trend_timeframe=off`
- `stop_loss_pct=0.04`
- `target_r=0.5`
- `entry_max_distance_pct=14.0`
- `entry_min_volume_ratio=1.3`
- `entry_min_rsi=50`
- `entry_max_rsi=90`
- `entry_bollinger_breakout=true`
- `entry_min_recent_drawdown_pct=3.5`
- `entry_recent_drawdown_lookback_candles=12`
- `entry_symbol_cooldown_candles=4`
- `entry_trigger_allowlist=breakout_previous_high`
- `fvg_entry_mode=m15_impulse_retrace`
- `fvg_impulse_retrace_fill_pct=30`
- `fvg_impulse_retrace_min_wait_candles=0`

对应 preset 已登记为：

```text
paper_strategy_preset=research_momentum_04sl_05r_kline15m_breakout_fvg30_vol13_dd35_v1
entry_rule_version=kline15m_mom04_05r_brk_fvg30_vol13_dd35_v1
```

`0.55R` 扩样复核后的上一版 challenger preset 为：

```text
paper_strategy_preset=research_momentum_04sl_055r_kline15m_breakout_fvg30_vol13_dd35_v1
entry_rule_version=kline15m_mom04_055r_brk_fvg30_vol13_dd35_v1
```

`0.52R + fvg50` 在 100 币种扩样后升级为当前主候选 preset：

```text
paper_strategy_preset=research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1
entry_rule_version=kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1
```

production-parity 修正：

- `kline_15m` synthetic event 已按 trade direction 过滤完成 K 线；当前 long
  主候选只生成 `close > open` 的上涨 K 线事件，和
  `market_velocity_kline_scanner` 的生产发现层一致。
- 旧实现用 `ABS(15m change)` 生成候选，long 回测会把下跌 K 线也送入后续 entry
  过滤。复测显示这些额外 raw candidate 基本被 15m entry 挡掉，交易结果没有变化，
  但 raw/open-rate 口径已改为 scanner parity。

本地显式窗口验证使用北京时间 `2026-05-04 00:00:00` 到
`2026-07-04 00:00:00`。下面保留初始 `0.5R/fvg30` 探索命令；复测当前主候选时要同时使用
`--target-rs 0.52` 与 `--fvg-impulse-retrace-fill-pct 50`，并按扩样验证使用
`--sample-limit 100`：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
./target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --sample-limit 20 \
  --sample-seed kline15m_fvg30_v1 \
  --event-start-ms 1777824000000 \
  --event-end-ms 1783094400000 \
  --trend-timeframe off \
  --target-rs 0.5 \
  --stop-loss-pct 0.04 \
  --entry-max-distance-pct 14 \
  --entry-min-volume-ratio 1.3 \
  --entry-min-rsi 50 \
  --entry-max-rsi 90 \
  --entry-bollinger-breakout \
  --entry-min-recent-drawdown-pct 3.5 \
  --entry-recent-drawdown-lookback-candles 12 \
  --entry-symbol-cooldown-candles 4 \
  --entry-trigger-allowlist breakout_previous_high \
  --fvg-entry-mode 15m_impulse_retrace \
  --fvg-impulse-retrace-fill-pct 30 \
  --fvg-impulse-retrace-min-wait-candles 0 \
  --equity-report \
  --equity-trigger-report \
  --equity-concentration-report \
  --equity-symbol-window-report \
  --min-trades 1
```

初始 `0.5R` 固定 seed 结果：

```text
raw_candidate_events=79743
effective_open_rate=0.1216%
24h outcome: trades=97 win=73 loss=23 timeout=1 complete_win_rate=75.26%
framework: trades=97 win_rate=73.20% max_drawdown=13.98% total_profit=23.43U
window q1/q2/q3/q4 win_rate=83.33%/58.33%/79.17%/72.00%
```

多 seed 稳定性检查显示 `0.5R + fvg30` 比上一轮 `0.6R + fvg20` 更贴近
快进快出目标，且 4 个 seed 都满足 `win_rate > 60%`、`max_drawdown < 15%`：

```text
seed=kline15m_fvg30_v1 framework_win=73.20% max_drawdown=13.98% profit=23.43U
seed=batch_a framework_win=71.26% max_drawdown=13.98% profit=11.14U
seed=batch_b framework_win=74.77% max_drawdown=12.86% profit=36.96U
seed=batch_c framework_win=78.05% max_drawdown=9.09% profit=45.45U
```

`0.5R` 在 50 个币种扩样时发现 `batch_b` 框架总收益为负，因此进一步扫
`0.55R/0.58R/0.6R`。当时 `0.55R + fvg30` 是更稳的折中：它修复了
`batch_b` 的负收益，同时没有触发 `0.58R/0.6R` 在 `batch_a` 上的回撤超线。

`0.55R` 上一版 challenger 在修正后的 long-only synthetic event 口径下，20 币种多 seed
结果为：

```text
sample_limit=20 seed=kline15m_fvg30_v1 target=0.55R raw=38265 trades=97 framework_win=72.16% max_drawdown=13.81% profit=31.81U
sample_limit=20 seed=batch_a target=0.55R raw=40091 trades=87 framework_win=68.97% max_drawdown=13.81% profit=10.64U
sample_limit=20 seed=batch_b target=0.55R raw=38912 trades=107 framework_win=72.90% max_drawdown=12.17% profit=40.12U
sample_limit=20 seed=batch_c target=0.55R raw=31871 trades=82 framework_win=75.61% max_drawdown=8.38% profit=45.69U
```

```text
sample_limit=50 seed=kline15m_fvg30_v1 target=0.55R raw=93367 trades=208 framework_win=72.60% max_drawdown=13.81% profit=76.16U remove_top5_profit=2.10U
sample_limit=50 seed=batch_a target=0.55R raw=85428 trades=173 framework_win=71.68% max_drawdown=13.81% profit=53.36U remove_top3_profit=7.73U
sample_limit=50 seed=batch_b target=0.55R raw=93716 trades=214 framework_win=67.29% max_drawdown=13.81% profit=4.98U remove_top1_profit=-8.03U
```

`fvg_fill_pct=50` 明显降低 concentration，`target=0.60 + fill50` 在 50 币种
样本中表现更强，但 100 币种 `batch_a` 的 `max_drawdown=15.55%` 超过目标线，
因此未升级为主候选。`target=0.52 + fill50` 在三组 100 币种样本均满足
`win_rate > 60%`、`max_drawdown < 15%`，且 remove_top5 后仍为正：

```text
sample_limit=100 seed=kline15m_fvg30_v1 target=0.52R fill=50 trades=367 framework_win=73.84% max_drawdown=12.24% profit=126.94U remove_top5_profit=60.94U q1/q2/q3/q4_profit=82.21U/20.08U/5.81U/22.22U
sample_limit=100 seed=batch_a target=0.52R fill=50 trades=360 framework_win=72.78% max_drawdown=13.91% profit=100.68U remove_top5_profit=35.06U q1/q2/q3/q4_profit=40.62U/11.62U/35.29U/16.46U
sample_limit=100 seed=batch_b target=0.52R fill=50 trades=341 framework_win=73.61% max_drawdown=13.91% profit=113.35U remove_top5_profit=63.37U q1/q2/q3/q4_profit=36.42U/25.03U/37.42U/16.81U
```

对比结论：

```text
target=0.5R batch_b sample_limit=50 framework_win=68.69% max_drawdown=13.98% profit=-5.15U
target=0.55R batch_b sample_limit=50 framework_win=67.29% max_drawdown=13.81% profit=4.98U
target=0.58R batch_a sample_limit=50 framework_win=69.36% max_drawdown=17.28% profit=44.20U
target=0.6R batch_a sample_limit=50 framework_win=69.36% max_drawdown=17.22% profit=54.25U
```

结论：

1. 15m K 线直抽样可以作为策略本体回测口径，不需要依赖 `market_rank_events`
   或生产异动雷达。
2. 当前 `04sl_052r/fvg50` 比 `04sl_055r/fvg30` 更适合作为 paper/shadow
   主候选：三组 100 币种随机样本均满足 `win_rate > 60%`、`max_drawdown < 15%`、
   remove_top5 后仍为正，且四分窗口总收益均为正。
3. 该版本仍只能进入 paper/shadow。生产 live promote 仍需 fresh paper outcomes、
   scanner/handoff E2E parity、重复 symbol 暴露复核和明确 promote approval。
4. 生产触发层仍应只负责低成本发现候选交易对；后续要用 15m K 线扫描事件替代
   旧 rank-velocity 雷达输入，避免把资源节省层误当成策略信号本体。

### 15m K 线扫描触发实现 - 2026-07-04

已新增 Rust-native `market_velocity_kline_scanner`：

- 输入只依赖 quant_core 已有的 `{symbol}_candles_15m` 分表和 `exchange_symbols`
  可交易 OKX perpetual 白名单。
- 默认 `dry-run`；生产 compose 使用 `--write`，但只写
  `market_rank_events` 候选事件，不生成执行任务、不触发下单。
- 事件合同保持 handoff 兼容：`event_type=rank_velocity`、`timeframe=15分钟`、
  `new_rank=0`、`delta_rank=0`、`source=kline_15m_scanner`、`detected_at`
  为该根 15m K 线收盘时间。
- 现有稳定生产 preset 默认 `MARKET_VELOCITY_SIGNAL_MIN_DELTA_RANK=18`，会自然忽略
  `delta_rank=0` 的 K 线扫描事件；只有显式切换到 `min_delta_rank=0` 的新
  paper/shadow/live 候选配置后，handoff 才会消费该来源。
- scanner 写入使用 `INSERT ... WHERE NOT EXISTS` 按
  `exchange + symbol + event_type + timeframe + detected_at + source` 去重，避免
  1 分钟调度重复写入同一根 15m K 线。

本地只读验证：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
./target/debug/market_velocity_kline_scanner \
  --dry-run \
  --max-symbols 20 \
  --lookback-minutes 4320 \
  --min-price-change-pct 0.0 \
  --per-symbol-limit 4
```

结果：

```text
market_velocity_kline_scanner: symbols_total=20 candidate_events=76 events_inserted=0 duplicate_events=0 lookback_minutes=4320 min_price_change_pct=0 max_price_change_pct=None per_symbol_limit=4 dry_run=true
```

生产拓扑已补齐：

- runtime 镜像复制 `/usr/local/bin/market_velocity_kline_scanner`。
- `docker-compose.deploy.yml` 新增
  `quant-core-market-velocity-kline-scanner-scheduler`，profile 为
  `kline-scanner-scheduler`，默认每 60 秒扫描最近 30 分钟已完成 15m K 线。
- `promote_stable.sh` / `rollback.sh` 默认服务列表和 compose profile 已包含该
  scheduler；CI production deploy contract 已固定二进制、compose 和 workflow 检查。

### kline15m 主候选 runtime 配置合同 - 2026-07-04

`MarketVelocityStrategySignalConfig` 已能从 `strategy_configs.config` 承载并传递
kline15m 主候选的 fast filters 到 `market_velocity_live_handoff` 的 live shell：

- `entry_min_rsi`
- `entry_max_rsi`
- `entry_min_rsi_delta`
- `entry_rsi_delta_lookback_candles`
- `entry_bollinger_breakout`
- `entry_min_bollinger_bandwidth_expansion_pct`
- `entry_min_recent_drawdown_pct`
- `entry_recent_drawdown_lookback_candles`

可用于 paper/shadow 的配置片段：

```json
{
  "strategy_slug": "market_velocity",
  "strategy_preset": "research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1",
  "entry_rule_version": "kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1",
  "min_delta_rank": 0,
  "max_delta_rank": null,
  "min_price_change_pct": 0.0,
  "max_price_change_pct": null,
  "stop_loss_pct": 0.04,
  "take_profit_r": 0.52,
  "max_holding_hours": 24,
  "require_technical_confirmation": false,
  "require_entry_confirmation": true,
  "trend_min_average_distance_pct": 0.0,
  "entry_confirmation_period": 20,
  "entry_confirmation_fetch_limit": 80,
  "entry_max_average_distance_pct": 14.0,
  "entry_min_volume_ratio": 1.3,
  "entry_min_rsi": 50.0,
  "entry_max_rsi": 90.0,
  "entry_bollinger_breakout": true,
  "entry_min_recent_drawdown_pct": 3.5,
  "entry_recent_drawdown_lookback_candles": 12,
  "fvg_entry_mode": "m15_impulse_retrace",
  "fvg_lookback_candles": 40,
  "fvg_max_wait_candles": 24,
  "fvg_impulse_retrace_fill_pct": 50.0,
  "fvg_impulse_retrace_min_wait_candles": 0,
  "entry_trigger_allowlist": ["breakout_previous_high"]
}
```

注意：本地 backtest 的 `entry_symbol_cooldown_candles=4` 是跨事件状态过滤，
当前 live handoff 单次 shell 不直接表达它；生产链路依赖 `live_handoff_state`
和 earliest-per-symbol 去重降低重复消费。切换到 live 前仍应先用该配置做
paper/shadow forward observation，并复核重复 symbol 暴露。

## Latest Local Research Verification

The production default above is unchanged. The following command is the latest
local owner-side verification for the current low-frequency `reclaim_ema`
research mainline, which adds a research-only `entry_max_signal_pullback_pct`
gate so late FVG fills cannot drift too far below the original signal price,
and now also tightens `entry_max_distance_pct` from `5.0` to `3.0` without
changing the realized owner-side trade set. A later verification also tightened
`entry_min_volume_ratio` from `1.0` to `1.1`, and then tightened
`fvg_impulse_retrace_fill_pct` from the default `20` to `10`, still without
changing the realized owner-side trade set.

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
EXECUTION_EVENT_SECRET=local-dev-secret \
cargo run -q -p rust-quant-cli --bin market_velocity_paper_observation -- \
  --paper-strategy-preset research_momentum_04sl_18r_reclaim_fvgwait14_pullback3_delta20_40_pchg5_10_v1
```

Verified on 2026-06-26 with owner-side Web writeback:

```text
entry_trigger_filter before=11 after=5
24h: trades=5 win=5 loss=0 timeout=0 avg_r_complete=1.8
48h: trades=5 win=5 loss=0 timeout=0 avg_r_complete=1.8
paper_outcomes_submitted=10
```

The matching Web summary for
`rank_radar_4h15m_r04_18r_rcm_fvg14_d3_pb3_vol11_fp10_d20_40_p5_10_v1` then returned:

```text
total_count=10
generated_execution_task_count=0
```

## Latest Hybrid FVG Plus Retest Verification

The current research follow-up keeps the same low-frequency `reclaim_ema`
shell, still uses `15m_impulse_retrace` as the first entry path, and only falls
back to `retest_after_signal` when the FVG branch does not fill. A same-day
de-coupling scan then confirmed that `pullback3` and `vol1.1` are still useful,
but the tighter `dist3` cap and explicit `fill10` are not needed for the final
owner-side trade set. The current preset therefore keeps:

- `entry_max_distance_pct=5.0`
- `entry_min_volume_ratio=1.1`
- `entry_max_signal_pullback_pct=3.0`
- default `fvg_impulse_retrace_fill_pct=20`

This is now
captured by:

```text
paper_strategy_preset=research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2
entry_rule_version=rank_radar_4h15m_r04_18r_rcm_fvg_rt1_pb3_vol11_d20_40_p5_10_v2
```

Local owner-side verification command:

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
EXECUTION_EVENT_SECRET=local-dev-secret \
CARGO_TARGET_DIR=/tmp/rust_quant_target_tdd \
cargo run -q -p rust-quant-cli --bin market_velocity_paper_observation -- \
  --paper-strategy-preset research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2
```

The old explicit `fvg_max_wait_candles=14` was removed after replay showed
`wait24` (the default) keeps the same `8 trades / 8 win / 1.8R` realized set,
so the preset no longer carries a redundant wait-specific coupling.

Verified on 2026-06-27:

```text
entry_trigger_filter before=43 after=8
24h: trades=8 win=8 loss=0 timeout=0 avg_r_complete=1.8
48h: trades=8 win=8 loss=0 timeout=0 avg_r_complete=1.8
paper_outcomes_submitted=16
```

Current realized set shows the expected split:

- FVG primary: `HMSTR / ORDI / BASED / AMD / CHIP`
- FVG fallback retest: `EIGEN / SLX / INJ`

The fallback rows are recorded with
`entry_trigger=reclaim_ema+retest_after_signal+fvg_fallback`, so downstream
analysis can distinguish them from the primary
`reclaim_ema+fvg_15m_impulse_retrace` fills.

## Current Production Shell TP/SL Recheck - 2026-06-28

在当前生产默认壳
`research_momentum_04sl_18r_reclaim_fvg_retest1_pullback3_delta20_40_pchg5_10_v2`
上，又补跑了一轮只改风控、不改入场语义的低耦合 TP/SL 扫描。

固定 `4%` 止损下的目标位结果：

- `1.6R`：`8 笔 / 8 胜 / 50.64U`
- `1.8R`：`8 笔 / 8 胜 / 57.04U`
- `2.0R`：`6 胜 1 负 1 timeout(48h) / 51.44U`
- `2.2R`：`4 胜 3 负 1 timeout(48h) / 31.44U`
- `2.4R`：`4 胜 3 负 1 timeout(48h) / 35.44U`
- `2.6R`：`4 胜 3 负 1 timeout(48h) / 39.44U`
- `3.0R`：`3 胜 4 负 1 timeout(48h) / 31.44U`

固定 `1.8R` 目标位下的止损宽度结果：

- `3%`：`5 胜 3 负 / 17.44U`
- `4%`：`8 胜 0 负 / 57.04U`
- `5%`：`4 胜 3 负 1 timeout(48h) / 29.44U`
- `6%`：`4 胜 2 负 2 timeout(48h) / 35.44U`

又补了一轮低耦合盈利保护，主要验证更高目标位能否靠简单保本/锁盈反超：

- `2.0R + protect_after=1R, stop=0R`：`55.51U`
- `2.0R + protect_after=1R, stop=0.5R`：`51.44U`
- `2.2R + protect_after=1R, stop=0R`：`34.92U`
- `2.2R + protect_after=1R, stop=0.5R`：`35.84U`

结论收敛为：

1. 当前生产壳里，最优且最低耦合的组合仍然是 `固定 4% 止损 + 固定 1.8R 全平`。
2. 更高目标位即便加简单盈利保护，也没有超过 `1.8R` 基线，因此暂不升级默认止盈。
3. `structure_or_fixed` 与 runner 在这个壳上都不应默认开启。前者本质上只会把止损收紧；而当前
   `15m FVG / retest fallback` 的结构锚点又偏近，容易把本来能完成 `1.8R` 的单子提前打掉。

继续按“不过度耦合”的方向，又补了一轮更圆整、更简单的复核：

- 固定 `1.8R` 下的整数止损宽度：
  - `3%`：`5 胜 3 负 / 17.44U`
  - `4%`：`8 胜 0 负 / 57.04U`
  - `5%`：`4 胜 3 负 1 timeout(48h) / 29.44U`
- 固定 `4%` 止损下的简单目标位：
  - `1.5R`：`8 胜 0 负 / 47.44U`
  - `1.8R`：`8 胜 0 负 / 57.04U`
  - `2.0R`：`6 胜 1 负 1 timeout(48h) / 51.44U`
  - `3.0R`：`3 胜 4 负 1 timeout(48h) / 31.44U`

这说明如果硬要把目标位简化成更“圆”的档位，当前壳里也仍然要为简化付出明确代价：

- `1.5R` 比 `1.8R` 少 `9.60U`
- `2.0R` 比 `1.8R` 少 `5.60U`

最后又测了一组最简单的两段止盈，保持基础目标仍是 `1.8R`，只让 `10%-20%` 的 runner 去看
`3R` 或 `4R`，runner stop 只用 `0R` 或 `1R`：

- `runner 10% -> 3R, stop 0R`：`54.915U`
- `runner 10% -> 3R, stop 1R`：`55.28U`
- `runner 20% -> 3R, stop 0R`：`52.79U`
- `runner 20% -> 3R, stop 1R`：`53.52U`
- `runner 10% -> 4R, stop 0R`：`55.66007682U`
- `runner 10% -> 4R, stop 1R`：`55.22507682U`
- `runner 20% -> 4R, stop 0R`：`54.28015365U`
- `runner 20% -> 4R, stop 1R`：`53.41015365U`

这 8 档全部低于 `1.8R 全平` 的 `57.04U`，而且都会把 `48h` 结果引入 `1-2` 笔 timeout。
另外这轮顺手复核了回测实现语义：`result ... horizon=48h` 这条统计链路在 runner 打到 base target 后，
会继续用 `48h` horizon 约束剩余 runner；但 `framework_equity_result` 的 runner replay 路径当前不带
`24h/48h` horizon 约束，而且把正收益的 runner timeout / forward_data_incomplete 按正 PnL 记成 win。
因此 runner 方向的 `framework total_profit / framework win_rate` 对当前壳来说是偏乐观的，真正比较
runner 是否值得开启时，应优先看 `result ... horizon=48h` 的 `win/loss/timeout/avg_r_complete`。在这个更严格口径下，
上述 8 档 simple runner 仍然全部低于 `1.8R 全平`，所以“不启用 runner” 的结论反而更稳。

因此当前生产壳的收敛结论可以再强化一层：

1. `固定 4% 止损 + 固定 1.8R 全平` 仍然是当前最优的低耦合默认。
2. 如果为了“更圆整”强行改成 `1.5R / 2R`，收益会可验证地下滑。
3. 即便把分批止盈限制到最简单的 runner 版本，也没有超过 `1.8R 全平`。

又把当前 hybrid 壳按入场子分支拆开复核了一次：

- `FVG-only`：
  - 做法：`fvg_entry_mode=15m_impulse_retrace`，关闭 `entry_retest_after_signal`
  - realized set：`5` 笔
  - `4% + 1.6R`：`31.65U`
  - `4% + 1.8R`：`35.65U`
  - `4% + 2.0R`：`27.65U`，并引入 `1 loss + 1 timeout`
  - 结论：FVG 主分支本身就明显更适合 `4% + 1.8R`
- `retest-only`：
  - 做法：`fvg_entry_mode=off`，开启 `entry_retest_after_signal`
  - realized set：`3` 笔
  - `4% + 1.8R`：`21.39U`
  - `4% + 2.0R`：`23.79U`
  - `4% + 2.2R`：`13.39U`
  - 结论：fallback retest 这 `3` 笔样本里，`4% + 2.0R` 比 `1.8R` 略强，但再往上就开始掉队

因此当前总壳的 `4% + 1.8R` 更准确的定义不是“每个子分支各自都最优”，而是：

1. 对当前样本量更大的 `FVG` 主分支，它本身就是最优点。
2. 对 `retest fallback`，`2.0R` 有轻微优势，但样本只有 `3` 笔。
3. 如果为了榨出这点优势，把止盈按 `entry subtype` 分成 `FVG=1.8R / retest=2.0R`，就会引入新的策略耦合。

所以在“找到更优 TP/SL，但不要过度耦合”的前提下，当前默认仍应保持：

- `固定 4% 止损 + 固定 1.8R 全平`

而唯一值得继续观察的轻度耦合候选，不是 runner，也不是结构止损，而是：

- 已单独固化的 research preset：
  `research_momentum_04sl_20r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1`
  - `entry_rule_version=rank_radar_4h15m_r04_20r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1`
  - 作用：只作为 `retest fallback` 的 paper challenger，不参与当前默认版本切换
- 仅当未来更长历史证明稳定时，才考虑 `FVG primary = 1.8R`、`retest fallback = 2.0R`

## Production Scheduler

For production observation, run the Rust-native scheduler profile:

```bash
podman compose -f docker-compose.deploy.yml --profile observation-scheduler up -d quant-core-market-velocity-paper-observation-scheduler
```

This starts `market_velocity_paper_observation --paper-strategy-preset
momentum_0375sl_17r_reclaim_ma_pullback_delta18_42_pchg5_10_v1 --loop-interval-seconds
${MARKET_VELOCITY_PAPER_OBSERVATION_INTERVAL_SECS:-21600}` with `restart:
unless-stopped`. A failed observation cycle is logged and retried after the next
interval; missing startup configuration still fails fast.

Run every 6-12 hours. The Admin diagnostic marks observation health as stale when
the latest production-filter paper outcome write is older than 48 hours.

The default Core CI/CD deploy and rollback scripts manage this scheduler
alongside `quant-core-market-velocity-radar` and `quant-core-execution-worker`.
After `up -d`, both scripts assert that every targeted long-running service has
a container and that Docker reports `.State.Running=true`; otherwise the deploy
or rollback exits non-zero instead of only printing `docker compose ps`.
If production overrides `DEPLOY_SERVICES`, keep
`quant-core-market-velocity-paper-observation-scheduler` in that list.

## kline15m 0.52R Challenger Scheduler

The `0.52R + fvg50` 15m-only candidate is available as an opt-in paper/shadow
observer. It is intentionally not part of the default deploy service list.

```bash
podman compose -f docker-compose.deploy.yml \
  --profile kline15m-paper-observation-scheduler \
  up -d quant-core-market-velocity-kline15m-paper-observation-scheduler
```

This starts:

```bash
market_velocity_paper_observation \
  --paper-strategy-preset research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1 \
  --loop-interval-seconds ${MARKET_VELOCITY_KLINE15M_PAPER_OBSERVATION_INTERVAL_SECS:-21600}
```

Use this for forward paper/shadow evidence only. Promotion to live still requires
fresh paper outcomes, trigger-layer end-to-end parity, duplicate-symbol exposure
review, and explicit promote approval.

### Local one-shot shadow verification - 2026-07-04

Current code was verified with a local mock Web endpoint before production
promotion. The one-shot command used the current challenger preset and local
`quant_core`, while `RUST_QUAN_WEB_BASE_URL` pointed at a local mock HTTP server
instead of production Web.

Observed output:

```text
candle_pairs=20
raw_candidate_events=43273
stage_counts entry_blocked=43167 entry_execution_blocked=640 entry_execution_pass=106 entry_pass=106 entry_signal_blocked=42527 entry_signal_pass=746 raw=43273 trend_pass=43273
paper_outcomes_submitted=150
```

The mock received exactly 150 requests, all on
`POST /api/commerce/internal/market-velocity/paper-outcomes`, all with
`entry_rule_version=kline15m_mom04_052r_brk_fvg50_vol13_dd35_v1`,
`target_r=0.52`, and horizons `24/48`. The mocked Web response returned
`generated_execution_task_count=0` for every request, which exercises the
observation-only guard in the Core submission path.

### Production readiness check - 2026-07-04

Production was checked before starting the opt-in scheduler. The running Core
image was `ghcr.io/fairwic/quant-core-worker:sha-5cd6c4a71bd00018315fbf92d7a683fce4505437`.
That image's `market_velocity_paper_observation --help` output did not include
the `research_momentum_04sl_052r_kline15m_breakout_fvg50_vol13_dd35_v1` preset,
and the remote production compose files did not contain
`quant-core-market-velocity-kline15m-paper-observation-scheduler`.

Do not start the production kline15m paper/shadow observer until this working
tree is committed, built by CI/CD, deployed through the normal promote flow, and
the production image/compose are rechecked for this preset and profile.

### Release staging checklist - 2026-07-04

When committing this challenger for CI/CD, use path-limited staging. The required
runtime/build set includes:

- `.github/workflows/cicd.yml`
- `Dockerfile.runtime`
- `docker-compose.deploy.yml`
- `scripts/deploy/promote_stable.sh`
- `scripts/deploy/rollback.sh`
- `crates/rust-quant-cli/src/app/market_velocity_backfill.rs`
- `crates/rust-quant-cli/src/app/market_velocity_event_backtest.rs`
- `crates/rust-quant-cli/src/app/market_velocity_event_backtest/`
- `crates/rust-quant-cli/src/app/market_velocity_kline_scanner.rs`
- `crates/rust-quant-cli/src/bin/market_velocity_kline_scanner.rs`
- `crates/rust-quant-cli/src/app/market_velocity_live_handoff.rs`
- `crates/rust-quant-cli/src/app/market_velocity_live_handoff/`
- `crates/rust-quant-cli/src/app/mod.rs`
- `crates/services/src/market/market_velocity_signal.rs`
- `crates/services/src/market/mod.rs`
- `crates/services/src/market/scanner_service/`
- `crates/services/tests/market_velocity_production_deploy_contract.rs`
- `docs/dev/market_velocity_paper_observation_runbook.md`

Do not include `.DS_Store` or `AGENTS.md` in the strategy release commit. Current
working tree also contains research/reference artifacts that are not required to
ship the kline15m paper/shadow observer:

- `docs/BTC_ETH_STRATEGY_FAMILY_ITERATION_LOG.md`
- `docs/VEGAS_NWE_BACKTEST_ANALYSIS.md`
- `crates/strategies/src/implementations/*DEPRECATED*.md`
- `scripts/research/market_velocity_5m_*`

## Success Criteria

The command output should include:

```text
entry_trigger_filter    before=<n>    after=<m>    allowlist=breakout_previous_high,reclaim_ema
paper_outcomes_submitted=<n>
```

Admin `GET /admin/quant/market-velocity/diagnostics` should then show:

- `paperOutcomeObservationHealth.status = ok`
- `paperOutcomeObservationHealth.cadenceStatus = ok` after at least three
  production-filter observation batches are present in the latest 48 hours
- `paperOutcomeObservationHealth.expectedFilterVersion = entry_trigger_allowlist_v1`
- `paperOutcomeObservationHealth.productionFilterSampleCount60d > 0`
- `paperOutcomeObservationHealth.productionFilterObservationBatchCount48h >= 3`
- `paperOutcomeObservationHealth.readyForExecutionTaskEvaluation = true`
- `paperOutcomeObservationHealth.generatedExecutionTaskCount = 0`
- `paperOutcomeObservationHealth.observationOnly = true`
- `paperOutcomeOptimizationRecommendation.status = candidate` once the selected
  production-filter target R / horizon has enough resolved samples. A lower
  status such as `insufficient_sample` means the recommendation is visible but
  should not be promoted.
- The optimization recommendation first prefers target R / horizon buckets that
  meet the minimum resolved-sample gate, then ranks by risk-adjusted win-rate
  edge. The edge is the Wilson lower bound of resolved win rate minus the
  target R breakeven win rate, so small raw high-win-rate buckets do not outrank
  statistically stronger candidates.
- `paperOutcomeOptimizationRecommendation.riskAdjustedWinRateEdge > 0`
- `paperOutcomeOptimizationRecommendation.generatedExecutionTaskCount = 0`
- `paperOutcomeExecutionReadiness.status = ready`
- `paperOutcomeExecutionReadiness.nextAction = evaluate_execution_task_creation`
- `paperOutcomeExecutionReadiness.blockers = []`
- `paperOutcomeExecutionReadiness.mutationAllowed = false`
- `paperOutcomeExecutionTaskCreationEvaluation.status = ready_for_web_dry_run`
- `paperOutcomeExecutionTaskCreationEvaluation.dryRunOnly = true`
- `paperOutcomeExecutionTaskCreationEvaluation.requiresSeparateAuthorization = true`
- `paperOutcomeExecutionTaskCreationEvaluation.wouldCreateExecutionTask = false`
- `paperOutcomeExecutionTaskCreationEvaluation.requiredWebChecks` includes
  strategy rules, risk filters, tradable symbol, user entitlement, API key
  readiness, signed read-only preflight, and execution-task idempotency.

## Alert Conditions

Investigate before enabling any execution-task creation if:

- `paperOutcomeObservationHealth.status` is `missing_filter_version`, `stale`,
  `empty`, or `unavailable`.
- `paperOutcomeObservationHealth.cadenceStatus` is not `ok`.
- `paperOutcomeObservationHealth.readyForExecutionTaskEvaluation` is not `true`.
- `productionFilterSampleCount60d` drops to zero.
- `productionFilterObservationBatchCount48h` is lower than 3.
- `paperOutcomeOptimizationRecommendation.status` is not `candidate`.
- `paperOutcomeOptimizationRecommendation.riskAdjustedWinRateEdge` is not
  positive.
- `paperOutcomeExecutionReadiness.status` is not `ready`.
- `paperOutcomeExecutionReadiness.blockers` is not empty.
- `paperOutcomeExecutionTaskCreationEvaluation.status` is not
  `ready_for_web_dry_run`.
- `paperOutcomeExecutionTaskCreationEvaluation.wouldCreateExecutionTask` is not
  `false`.
- `generatedExecutionTaskCount` is not zero.
- The command fails before `paper_outcomes_submitted=...`.

## Current Production Filter

The production observation entry currently tracks:

```text
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
```

This is based on the recent local `quant_core` framework sweep where
`breakout_previous_high,reclaim_ema` balanced trade count, win rate, Sharpe, and
drawdown better than the wider trigger set.

## Current Strategy Preset

The production paper observer currently uses:

```text
paper_strategy_preset=momentum_03sl_20r_v5
entry_rule_version=rank_radar_4h_trend_15m_momentum_03sl_20r_v5
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
symbol_blocklist=
stop_reentry_mode=off
stop_loss_pct=0.03
target_r=2.0
horizons=24h,48h
```

This preset promotes the stronger anti-overfit candidate from the recent
backtest sweep: it does not depend on historical symbol-specific exclusions and
it disables stop-reentry after the local sample showed three stop-reentry
attempts all became second stop losses. The
previous `stop_reentry_03sl_30r_v3` result reached 178.85U in the framework
report, but it depended on a historical symbol blocklist and should not be used
as the production default without forward observation.

Latest local verification, using the current local `quant_core` database:

```text
Core candidate scale: candle_pairs=55, raw_candidate_events=62514, entry_pass=2912, entry_trigger_after=2494
Core CLI 48h: trades=41, win=26, loss=11, timeout=1, incomplete=3, resolved_win_rate=70.2703%, avg_r_complete=1.090933
Framework symbol_isolated_100u: symbols=27, trades=41, win_rate=65.8537%, trade_sharpe=4.608339744751571, max_drawdown_pct=3.40882238, total_profit=122.47823444
```

`raw_candidate_events` can drift slightly while new `market_rank_events` rows are
inserted; the framework trade count is unchanged in this verification.

2026-06-21 no-overfit continuation: local 15m backfill added 9342 rows for
`BICO-USDT-SWAP,RE-USDT-SWAP,RESOLV-USDT-SWAP,SAND-USDT-SWAP,O-USDT-SWAP`,
raising candle-pair coverage from 50 to 55 symbols. The production filter result
did not change because the added symbols remained blocked by 4h insufficiency,
15m overextension, price below averages, or missing timing confirmation. Generic
scans of all-rank price-change caps, wider `max_new_rank`, wider trigger sets,
target R, stop-loss width, 15m/1h and 1h/4h FVG entries, and 15m average-distance
limits did not produce a candidate that simultaneously met trades >= 30,
framework win-rate >= 65%, high Sharpe, max drawdown <= 30%, and total profit
>= 150U. Keep `momentum_03sl_20r_v5` as the non-overfit default until forward
paper observation or a broader sample supports another generic rule.

2026-06-21 continuation added a research-only `--max-delta-rank` filter to test
whether extreme rank jumps are noisy chase entries. The strongest OOS-stable
results improved win rate but reduced profit, so they are not promoted:

```text
--max-delta-rank 70 --target-rs 2.0: trades=31, win_rate=70.96774193548387%, trade_sharpe=4.76318458727161, max_drawdown_pct=3.40882238, total_profit=102.83844164, late_win_rate=72.72727272727273%
--max-delta-rank 79 --target-rs 2.0: trades=33, win_rate=69.6969696969697%, trade_sharpe=4.65928250362462, max_drawdown_pct=3.40882238, total_profit=106.11559397, late_win_rate=69.56521739130434%
entry_max_distance_pct=3.0 --target-rs 2.0: trades=36, win_rate=69.44444444444444%, trade_sharpe=4.452, max_drawdown_pct=6.04575100, total_profit=108.95, late_win_rate=68.18181818181817%
```

Wider distance settings increased trade count and sometimes profit, but failed
the win-rate and late-split gates; the best wider-distance profit was
`entry_max_distance_pct=8.0 --target-rs 2.4` with `70` trades and `141.21U`,
but only `51.43%` full win rate and `50.0%` late win rate.

For overfit checks, run the research CLI with `--equity-split-report`. It prints
early/late time splits using the same `symbol_isolated_100u` framework replay.
On 2026-06-21, the current default split as:

```text
early: trades=14, win_rate=71.42857142857143%, trade_sharpe=3.618907914530351, max_drawdown_pct=3.07000000, total_profit=51.29822363
late: trades=28, win_rate=64.28571428571429%, trade_sharpe=3.371265362245003, max_drawdown_pct=3.07000000, total_profit=76.63505327
```

The historical high-profit `stop_reentry_03sl_30r_v3` + symbol-blocklist
candidate no longer meets the full-sample win-rate gate on the current database
and degrades in the late split:

```text
full: trades=49, win_rate=61.224489795918366%, trade_sharpe=4.044472281948052, max_drawdown_pct=6.40196478, total_profit=169.73251503
early: trades=18, win_rate=66.66666666666666%, trade_sharpe=3.9490239669120095, max_drawdown_pct=6.40196478, total_profit=96.71270181
late: trades=32, win_rate=59.375%, trade_sharpe=2.4476985110437828, max_drawdown_pct=6.04575100, total_profit=81.89242033
```

2026-06-21 目标放宽为 framework 胜率 `>55%` 后，先补齐当前验收
symbol 的 15m K 线缺口。回填写入 `9600` 根旧缺口 15m K 线，剩余
48h 覆盖缺口均来自事件过近、未来 48h 尚未走完，而不是历史 K 线缺失。
回填后的当前默认基线为：

```text
momentum_03sl_20r_v5: symbols=33, trades=51, win_rate=58.82352941176471%, trade_sharpe=3.5516554411025902, max_drawdown_pct=7.55972182, total_profit=112.87621520
early: trades=18, win_rate=66.66666666666666%, trade_sharpe=3.3077875670257013, max_drawdown_pct=3.07000000, total_profit=58.24150839
late: trades=34, win_rate=52.94117647058824%, trade_sharpe=1.9260080478895372, max_drawdown_pct=4.75683918, total_profit=51.46292655
```

Under the relaxed `>55%` gate, generic scans that increased target R, widened
entry distance, widened `max_new_rank`, lowered volume confirmation, or included
all triggers increased neither robust profit nor late-split quality. The first
clean structural candidate was to remove the weaker `reclaim_ema` trigger and
run only `breakout_previous_high`; after fine scanning target R and the
framework-backed profit-protect stop update, the best breakout-only variant was:

```text
breakout_previous_high only, target_r=3.3, profit_protect_after=2.8R, profit_protect_stop=1.0R:
symbols=29, trades=37, win_rate=59.45945945945946%, trade_sharpe=3.7284868207002813, max_drawdown_pct=3.40882238, total_profit=137.74550261
early: trades=13, win_rate=61.53846153846154%, trade_sharpe=3.115743445763077, max_drawdown_pct=3.07000000, total_profit=66.16253770
late: trades=25, win_rate=56.00000000000001%, trade_sharpe=2.2149256472480614, max_drawdown_pct=3.07000000, total_profit=68.84681163
```

This candidate keeps both split profits positive and improves drawdown, but it
reduces trade count from `51` to `37` and still does not reach `150U`.

The first full-sample candidate that satisfied `55%+` win rate, `150U+` profit,
and non-small trade count used the existing trigger allowlist with a narrower
generic chase filter and late profit protection:

```text
command shape:
--target-rs 3.3
--entry-trigger-allowlist breakout_previous_high,reclaim_ema
--entry-max-distance-pct 4.0
--entry-min-volume-ratio 1.0
--min-delta-rank 15
--max-new-rank 30
--chase-top-rank 5
--chase-price-change-pct 8.0
--profit-protect-after-r 2.8
--profit-protect-stop-r 2.0
--equity-report --equity-split-report --min-trades 30

candle_pairs=56
raw_candidate_events=68416
entry_pass=3621
entry_trigger_filter_after=3107
framework: symbols=35, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.645696446371883, max_drawdown_pct=7.55972182, total_profit=158.04193447
early: symbols=14, trades=18, win_rate=61.111111111111114%, trade_sharpe=3.463678049044337, max_drawdown_pct=3.07000000, total_profit=90.30123180
late: symbols=26, trades=36, win_rate=52.77777777777778%, trade_sharpe=2.1423213742336467, max_drawdown_pct=6.04575100, total_profit=74.56631459
```

This is a research candidate, not a production preset yet. The full sample meets
the user's relaxed target and exceeds `150U`, but the late split win rate is
below `55%`; keep it in forward paper observation before promotion. The
`top=5` chase filter was stable across `chase_price_change_pct=4.0..10.0` in the
current sample, while `top=10` stayed below the win-rate gate and `top=15/20`
over-filtered. The no-protect `top=5` variant reached `154.29088330U` but only
`53.84615384615385%` win rate, so the late profit-protect update is the part
that crosses the relaxed win-rate gate.

Continuation scans after this candidate did not find a better non-overfit
promotion preset:

```text
profit-protect sweep, top=5:
best remains after=2.8R, stop=2.0R, target=3.3R with full win_rate=55.769230769230774%, total_profit=158.04193447, late win_rate=52.77777777777778%.
after=2.8R stop=1.5R kept full win_rate=55.769230769230774% but only total_profit=154.82329406 and the same late win_rate=52.77777777777778%.
after=3.0R and after=3.2R mostly reverted to no-protect behavior: full win_rate around 53.84615384615385% and late win_rate around 50%.

entry-distance sweep:
dist=3.5 reached full win_rate=55.319148936170215% but only total_profit=140.48569261 and late win_rate=53.125%.
dist=4.5/5.0/6.0 increased trades but degraded full and late win rates below 50%.

volume sweep:
vol=0.8/0.0 increased trades but late split degraded sharply; vol=1.2+ over-filtered and reduced both win rate and profit.

rank sweep:
min_delta=12 improved profit to 167.05573535 with 62 trades, but full win_rate=53.2258064516129% and late win_rate=52.38095238095239%.
min_delta=12 + max_delta=80 improved late win_rate to 54.285714285714285% but only full win_rate=54.71698113207547% and total_profit=147.99922309.
min_delta=12 + max_new=20 had full/late win rates above 59%/60%, but only total_profit=85.47273354 and 32 trades.
```

Trigger decomposition explains the tradeoff:

```text
breakout_previous_high only:
full trades=39, win_rate=58.97435897435898%, total_profit=136.93737158, max_drawdown_pct=3.55002519
late trades=27, win_rate=55.55555555555556%, total_profit=67.59172485

reclaim_ema only:
full trades=24, win_rate=37.5%, total_profit=26.02152552
late trades=17, win_rate=29.411764705882355%, total_profit=0.97990709

breakout_previous_high,reclaim_ema:
full trades=52, win_rate=55.769230769230774%, total_profit=158.04193447
late trades=36, win_rate=52.77777777777778%, total_profit=74.56631459
```

So the robust path is still unresolved: `breakout_previous_high` is the stable
entry trigger, while `reclaim_ema` provides the extra trade count/profit needed
to clear `150U` but introduces the late-split weakness. Avoid promoting a
reclaim-inclusive default until forward observation confirms that the late split
weakness is sample noise rather than structural decay.

2026-06-21 当前目标调整为 framework 胜率 `>55%`、优先提高盈利和开仓次数、
同时保持低回撤后，新增 `--equity-quartile-report` 做四分位时间切片，避免
只看 early/late 两段造成收益集中误判。当前本地 `quant_core` 样本已经轻微
漂移到 `raw_candidate_events=68483`；以下结果都使用同一套
`symbol_isolated_100u` framework replay，不使用 portfolio-level 共享资金。

高收益候选仍然只适合作为 research/paper observation：

```text
breakout_previous_high,reclaim_ema, target_r=3.3, profit_protect_after=2.8R, profit_protect_stop=2.0R
candle_pairs=56, raw_candidate_events=68483, entry_pass=3621, entry_trigger_after=3107
framework: symbols=35, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.645696446371883, max_drawdown_pct=7.55972182, total_profit=158.04193447
early: trades=18, win_rate=61.111111111111114%, total_profit=90.30123180
late: trades=36, win_rate=52.77777777777778%, total_profit=74.56631459
q1: trades=6, win_rate=66.66666666666666%, total_profit=25.19422684
q2: trades=13, win_rate=53.84615384615385%, total_profit=60.73227702
q3: trades=19, win_rate=47.368421052631575%, total_profit=52.30591919
q4: trades=19, win_rate=52.63157894736842%, total_profit=15.06774223
```

它满足全样本 `55%+`、`150U+`、交易数不少、回撤低，但 q3/q4 胜率弱，
尤其 q4 Sharpe 只有 `0.6821737744188278`，不能作为非过拟合默认参数。

当前更稳的候选是只保留 `breakout_previous_high`：

```text
breakout_previous_high only, target_r=3.3, profit_protect_after=2.8R, profit_protect_stop=2.0R
entry_trigger_after=2364
framework: symbols=31, trades=39, win_rate=58.97435897435898%, trade_sharpe=3.6639645901641575, max_drawdown_pct=3.55002519, total_profit=136.93737158
early: trades=13, win_rate=61.53846153846154%, total_profit=66.62053704
late: trades=27, win_rate=55.55555555555556%, total_profit=67.59172485
q1: trades=4, win_rate=50.0%, total_profit=8.85130784
q2: trades=10, win_rate=60.0%, total_profit=55.00101019
q3: trades=15, win_rate=53.333333333333336%, total_profit=48.43814202
q4: trades=14, win_rate=57.14285714285714%, total_profit=24.86399231
```

这组没有达到 `150U`，但比 reclaim-inclusive 候选更稳：late 胜率高于
`55%`，最大回撤只有 `3.55002519%`，四分位没有亏损段。它是当前更适合
继续 forward paper observation 的保守候选。

触发器拆分确认 `reclaim_ema` 是后段弱点：

```text
breakout_previous_high only with min_delta=15,max_delta=80:
symbols=26, trades=32, win_rate=59.375%, total_profit=113.51981010, late_win_rate=57.14285714285714%

reclaim_ema only with min_delta=15,max_delta=80:
symbols=16, trades=21, win_rate=42.857142857142854%, total_profit=35.23152552, late_win_rate=33.33333333333333%, q4_total_profit=-5.42392067
```

继续扫描结果：

```text
min_delta=12,max_delta=80,target=3.3: trades=53, win_rate=54.71698113207547%, total_profit=147.99922309, late_win_rate=54.285714285714285%
min_delta=15,max_delta=80,target=3.3: trades=43, win_rate=58.139534883720934%, total_profit=138.98542221, late_win_rate=55.172413793103445%, q4_win_rate=44.44444444444444%
max_new=28 with min_delta=12,max_delta=80,target=3.3: trades=49, win_rate=55.10204081632652%, total_profit=130.19962193, late_win_rate=54.54545454545454%
breakout-only dist=4.5/5.0/6.0: increased trades but late win rate fell to about 40-42.5%
breakout-only volume=0.8: trades=47 but late win_rate=48.148148148148145%
```

Conclusion for this iteration: do not promote the `158.04193447U` candidate yet.
Use it as a research/paper-observation candidate; use breakout-only as the
current non-overfit baseline. The unresolved work is to find a non-time,
non-symbol-specific filter that can admit a subset of `reclaim_ema` without
reintroducing the q3/q4 weakness.

Continuation found a better reclaim-inclusive research candidate by adding a
generic overheat cap on rank jumps. This does not use a symbol blocklist or a
time split:

```text
breakout_previous_high,reclaim_ema, target_r=3.3, min_delta_rank=12, max_delta_rank=72, max_new_rank=30, profit_protect_after=2.8R, profit_protect_stop=2.0R
candle_pairs=58, raw_candidate_events=71755, entry_pass=4057, entry_trigger_after=3644
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
early: trades=19, win_rate=52.63157894736842%, total_profit=72.73980197
late: trades=34, win_rate=55.88235294117647%, total_profit=75.29818102
q1: trades=7, win_rate=57.14285714285714%, total_profit=22.57051689
q2: trades=14, win_rate=50.0%, total_profit=52.11690623
q3: trades=18, win_rate=44.44444444444444%, total_profit=45.53263319
q4: trades=20, win_rate=50.0%, total_profit=14.45832131
```

Compared with the earlier `158.04193447U` candidate, this version gives up
about `6.97U` but fixes the late split from `52.77777777777778%` to
`55.88235294117647%` while keeping `52` trades and `150U+` profit. Adjacent
checks show the overheat cap is not a single-point `75` artifact:

```text
max_delta_rank=70: trades=52, win_rate=53.84615384615385%, total_profit=146.05777341, late_win_rate=52.94117647058824%
max_delta_rank=72: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
max_delta_rank=75: same framework result as 72 in the current sample
max_delta_rank=78: trades=53, win_rate=54.71698113207547%, total_profit=147.99922309, late_win_rate=54.285714285714285%
```

Target-R neighbor checks keep the same win rate but only `3.3R` clears the
current `150U` profit objective:

```text
target=3.1R: trades=52, win_rate=55.769230769230774%, total_profit=140.39442985, late_win_rate=55.88235294117647%
target=3.2R: trades=52, win_rate=55.769230769230774%, total_profit=145.72904404, late_win_rate=55.88235294117647%
target=3.3R: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
target=3.4R: trades=52, win_rate=55.769230769230774%, total_profit=138.99942377, late_win_rate=55.88235294117647%
```

Promotion caveat: this is now the best numeric fit for the user's relaxed
objective (`55%+` full win rate, `150U+` profit, non-small trade count, low
drawdown), but q3/q4 win rates are still below `55%`. Treat it as a stronger
research candidate than the prior `158U` version, not as a fully de-risked
production default, until forward paper observation confirms the q3/q4 weakness
does not persist.

Follow-up with `--equity-trigger-report` confirmed the trigger tradeoff under
the updated user target of `>=55%` framework win rate, high profit, non-small
trade count, and low drawdown. The local sample drifted to
`raw_candidate_events=71241`, but the framework result stayed unchanged:

```text
candidate:
breakout_previous_high,reclaim_ema, target_r=3.3, min_delta_rank=12, max_delta_rank=72, max_new_rank=30, chase_top_rank=5, chase_price_change_pct=8.0, profit_protect_after=2.8R, profit_protect_stop=2.0R
candle_pairs=58, raw_candidate_events=71241, entry_pass=4033, entry_trigger_after=3620
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
early: trades=19, win_rate=52.63157894736842%, total_profit=72.73980197
late: trades=34, win_rate=55.88235294117647%, total_profit=75.29818102
q1: trades=7, win_rate=57.14285714285714%, total_profit=22.57051689
q2: trades=14, win_rate=50.0%, total_profit=52.11690623
q3: trades=18, win_rate=44.44444444444444%, total_profit=45.67828893
q4: trades=20, win_rate=50.0%, total_profit=14.45832131
```

Trigger reports are standalone trigger replays, not additive attribution of the
combined sequence. They show why a simple trigger removal is not enough:

```text
breakout_previous_high standalone:
trades=40, win_rate=52.5%, trade_sharpe=3.047059450246726, max_drawdown_pct=4.89118170, total_profit=110.85371915

reclaim_ema standalone:
trades=27, win_rate=40.74074074074074%, trade_sharpe=1.3381271712433855, max_drawdown_pct=7.81241293, total_profit=40.18135478
```

Adjacent scans did not find a stronger non-overfit replacement:

```text
profit-protect:
after=2.6R stop=1.5R: trades=52, win_rate=55.769230769230774%, total_profit=131.60797648, late_win_rate=55.88235294117647%
after=2.6R stop=2.0R: trades=52, win_rate=55.769230769230774%, total_profit=139.22532242, late_win_rate=55.88235294117647%
after=2.8R stop=1.5R: trades=52, win_rate=55.769230769230774%, total_profit=147.83678948, late_win_rate=55.88235294117647%
after=3.0R stop=2.0R: trades=52, win_rate=53.84615384615385%, total_profit=147.23541273, late_win_rate=52.94117647058824%

chase top-rank:
top=4: trades=53, win_rate=54.71698113207547%, total_profit=147.66719457, late_win_rate=55.88235294117647%
top=5: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
top=6: same framework result as top=5 in the current sample
top=7: trades=51, win_rate=54.90196078431373%, total_profit=139.75257156, late_win_rate=54.54545454545454%

entry distance:
distance=3.8: trades=50, win_rate=54.0%, total_profit=142.96200542, late_win_rate=54.54545454545454%
distance=4.0: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
distance=4.2: trades=53, win_rate=52.83018867924528%, total_profit=145.63092117, late_win_rate=51.42857142857142%
distance=4.5: trades=57, win_rate=47.368421052631575%, total_profit=126.53738153, late_win_rate=44.73684210526316%

min_delta_rank with max_delta_rank=72:
min_delta=11: trades=54, win_rate=53.70370370370371%, total_profit=152.59850737, late_win_rate=52.77777777777778%
min_delta=12: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309, late_win_rate=55.88235294117647%
min_delta=13: trades=49, win_rate=55.10204081632652%, total_profit=143.68879850, late_win_rate=54.83870967741935%
min_delta=15: trades=42, win_rate=59.523809523809526%, total_profit=142.05542221, late_win_rate=57.14285714285714%

max_new_rank / chase threshold:
max_new=28: trades=48, win_rate=56.25%, total_profit=133.26962193, late_win_rate=56.25%
max_new=29: trades=48, win_rate=56.25%, total_profit=136.73673534, late_win_rate=59.375%
max_new=31: trades=55, win_rate=52.72727272727272%, total_profit=144.52277580, late_win_rate=52.77777777777778%
chase_price_change_pct=6.0: same framework result as 8.0 in the current sample
chase_price_change_pct=10.0: same framework result as 8.0 in the current sample
```

Conclusion: keep the `min_delta=12,max_delta=72,target=3.3R` candidate as the
current best research fit for the updated `55%+` objective. It clears the
numeric gates with `52` trades and `151.06922309U`, but q3/q4 still argue
against declaring it production-stable without forward observation.

Continuation added `--equity-concentration-report` so symbol concentration can
be checked inside the same `symbol_isolated_100u` framework replay instead of
with ad hoc shell parsing. The current best research candidate has material
symbol concentration:

```text
framework: trades=52, win_rate=55.769230769230774%, total_profit=151.06922309
remove top 1 positive symbol H-USDT-SWAP:
remaining_trades=49, remaining_win_rate=53.06122448979592%, remaining_total_profit=123.28979515

remove top 3 positive symbols H-USDT-SWAP,JTO-USDT-SWAP,XPL-USDT-SWAP:
removed_profit=72.92816358, removed_share_pct=48.27466646887941%
remaining_trades=43, remaining_win_rate=46.51162790697674%, remaining_total_profit=78.14105951

remove top 5 positive symbols H-USDT-SWAP,JTO-USDT-SWAP,XPL-USDT-SWAP,UNI-USDT-SWAP,KAT-USDT-SWAP:
removed_profit=101.41160282, removed_share_pct=67.12922774330893%
remaining_trades=40, remaining_win_rate=42.5%, remaining_total_profit=49.65762027
```

The inverse test, removing the top three losing symbols
`TON-USDT-SWAP,ASTER-USDT-SWAP,MRVL-USDT-SWAP`, reached `45` trades,
`64.44444444444444%` win rate, `3.07000000%` max drawdown, and
`169.84878971U` profit. Do not promote this directly: it is a historical symbol
blocklist and therefore a likely overfit path unless a non-symbol-specific
reason can be proven.

Narrow-window inspection of representative win/loss rank events did not reveal
a simple robust feature. Losing samples include both high and low `delta_rank`
and both high and low `price_change_pct`; winning samples also include high
`price_change_pct`, so a blanket chase cap is too blunt. Reusing the existing
`chase_top_rank=30` setting as an all-rank price-change cap confirmed this:

```text
all-rank price cap=30: trades=47, win_rate=53.191489361702125%, total_profit=124.80694351
all-rank price cap=25: trades=44, win_rate=54.54545454545454%, total_profit=123.60182010
all-rank price cap=20: trades=38, win_rate=50.0%, total_profit=88.77215627
all-rank price cap=15: trades=37, win_rate=43.24324324324324%, total_profit=73.59497349
all-rank price cap=12: trades=31, win_rate=41.935483870967744%, total_profit=47.99213226
all-rank price cap=10: trades=26, win_rate=46.15384615384615%, total_profit=49.13403550
all-rank price cap=8: trades=25, win_rate=48.0%, total_profit=50.83693063
```

Updated conclusion: the candidate still meets the user's numeric goal, but the
symbol concentration report strengthens the anti-overfit caveat. Treat it as a
paper-observation candidate, not a production default. The next useful research
step is not a symbol blocklist; it is finding a generic reason for
`TON/ASTER/MRVL`-type failures that does not remove the high-conviction winners.

2026-06-21 latest target adjustment: the active research gate is now framework
win rate `>=55%`, high total profit, non-small trade count, and low drawdown.
The current local sample used `58` candle pairs and `71317` raw rank events:

```text
stage_counts: raw=71317, trend_pass=63730, trend_blocked=7587, entry_pass=4033, entry_blocked=59697
entry_trigger_filter: before=4033, after=3620, allowlist=breakout_previous_high,reclaim_ema
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
```

The candidate still clears the numeric gate, but the feature report shows why it
should remain research-only:

```text
delta_rank=12_24: trades=32, win_rate=50.0%, total_profit=68.21089671
delta_rank=25_48: trades=20, win_rate=65.0%, total_profit=86.98804181
delta_rank=49_plus: trades=9, win_rate=55.55555555555556%, total_profit=28.82221074
new_rank=1_10: trades=7, win_rate=42.857142857142854%, total_profit=9.68666625
new_rank=11_20: trades=24, win_rate=58.333333333333336%, total_profit=66.32411145
new_rank=21_30: trades=28, win_rate=42.857142857142854%, total_profit=55.10172542
price_change_pct=lt5: trades=16, win_rate=37.5%, total_profit=13.40311751
price_change_pct=5_10: trades=16, win_rate=50.0%, total_profit=41.48305379
price_change_pct=10_20: trades=17, win_rate=58.82352941176471%, total_profit=56.53124763
price_change_pct=20_plus: trades=17, win_rate=64.70588235294117%, total_profit=69.12938275
```

Generic perturbations did not find a better replacement. `max_new_rank=25`
reduced profit to `112.77547025U`, `max_new_rank=20` dropped below the
`30`-trade gate, `min_delta_rank=15` improved win rate to
`59.523809523809526%` but only reached `142.05542221U`, and raising `target_r`
above `3.3R` lowered profit. Profit-protect sweeps also kept the current
`after=2.8R, stop=2.0R` as the only setting in this group that reached
`150U+` while preserving `>=55%` win rate.

Continuation added research-only `--min-price-change-pct` after the feature
report showed weak standalone results for `price_change_pct < 5`. This is a
generic event filter, not a symbol blocklist, and it is locked out of the
production preset override path. The scan showed it improves win rate and
time-split stability but gives up too much total profit to replace the current
`151.06922309U` research candidate:

```text
min_price_change_pct=2: trades=49, win_rate=57.14285714285714%, total_profit=145.66332704, late_win_rate=56.25%, q3_win_rate=43.75%, q4_win_rate=50.0%
min_price_change_pct=5: trades=42, win_rate=59.523809523809526%, total_profit=143.43244199, late_win_rate=58.620689655172406%, q3_win_rate=56.25%, q4_win_rate=60.0%
min_price_change_pct=8: trades=33, win_rate=63.63636363636363%, total_profit=123.13295089
min_price_change_pct=10: trades=31, win_rate=64.51612903225806%, total_profit=117.51299616
```

The best conservative branch was to combine the minimum price-change filter with
a slightly wider rank-jump floor:

```text
min_delta_rank=11, max_delta_rank=72, min_price_change_pct=5, target_r=3.3:
candle_pairs=54, raw_candidate_events=59867, entry_pass=3378, entry_trigger_after=3012
framework: symbols=27, trades=45, win_rate=57.77777777777777%, trade_sharpe=3.7652834461775884, max_drawdown_pct=7.55972182, total_profit=149.14914524
late: trades=34, win_rate=58.82352941176471%, total_profit=105.93154004
q1: trades=3, win_rate=33.33333333333333%, total_profit=0.50918469
q2: trades=10, win_rate=60.0%, total_profit=48.11488234
q3: trades=19, win_rate=57.89473684210527%, total_profit=77.68677536
q4: trades=16, win_rate=56.25%, total_profit=23.77040977
```

This branch is more time-stable than the `151U` candidate, but it still misses
the `150U` objective and does not solve symbol concentration:

```text
remove top 3 positive symbols H-USDT-SWAP,JTO-USDT-SWAP,UNI-USDT-SWAP:
removed_profit=68.29615335, removed_share_pct=45.79050938858432%, remaining_trades=37, remaining_win_rate=48.64864864864865%, remaining_total_profit=80.85299189
```

Target-R neighbors did not recover the missing profit: `3.32R` dropped to
`146.15390720U`, `3.35R` to `143.50746025U`, `3.4R` to `140.94903225U`, and
`3.5R` to `144.97882742U`. Do not promote this as the main candidate; keep it
as a conservative forward-observation branch if the next priority becomes
time-split stability over total profit.

After the target was relaxed to `win_rate >= 55%` with higher priority on
profit, trade count, and low drawdown, the CLI gained
`--equity-symbol-window-report`. It replays the same `symbol_isolated_100u`
framework equity report inside the existing quartile windows and prints the top
profit symbols per window. This is a diagnostic only; it does not change entry
or exit behavior.

Current `150U` profit candidate:

```text
params: min_delta_rank=12, max_delta_rank=72, target_r=3.3, no min_price_change_pct
candle_pairs=58, raw_candidate_events=71352, entry_pass=4033
paper 24h: trades=52, resolved_win_rate=55.00000000000001%, avg_r_complete=0.8866904314838523
paper 48h: trades=51, resolved_win_rate=59.09090909090909%, avg_r_complete=1.1065604654229744
framework: symbols=30, trades=52, win_rate=55.769230769230774%, trade_sharpe=3.553650213392905, max_drawdown_pct=7.81102242, total_profit=151.06922309
q1: trades=7, win_rate=57.14285714285714%, total_profit=22.57051689
q2: trades=14, win_rate=50.0%, total_profit=52.11690623
q3: trades=18, win_rate=44.44444444444444%, total_profit=45.67828893
q4: trades=20, win_rate=50.0%, total_profit=14.45832131
top window contributors: q1=H, q2=H/HOME/ENA, q3=XPL/JTO/KAT, q4=UNI/JTO/BICO
remove top 3 positive symbols H,JTO,XPL: removed_share_pct=48.27466646887941%, remaining_trades=43, remaining_win_rate=46.51162790697674%, remaining_total_profit=78.14105951
```

This is the only branch currently above `150U` while meeting `>=55%` framework
win rate and `>=30` trades, but it is still not clean enough for promotion:
`q3` and `q4` are weak by win rate, and the top three symbols explain nearly
half of profit.

Current conservative branch:

```text
params: min_delta_rank=11, max_delta_rank=72, min_price_change_pct=5, target_r=3.3
candle_pairs=54, raw_candidate_events=59880, entry_pass=3378
paper 24h: trades=44, resolved_win_rate=55.55555555555556%, avg_r_complete=1.0509472765480348
paper 48h: trades=44, resolved_win_rate=56.09756097560976%, avg_r_complete=1.0844600256157642
framework: symbols=27, trades=45, win_rate=57.77777777777777%, trade_sharpe=3.7652834461775884, max_drawdown_pct=7.55972182, total_profit=149.14914524
q1: trades=3, win_rate=33.33333333333333%, total_profit=0.50918469
q2: trades=10, win_rate=60.0%, total_profit=48.11488234
q3: trades=19, win_rate=57.89473684210527%, total_profit=77.68677536
q4: trades=16, win_rate=56.25%, total_profit=23.77040977
top window contributors: q1=INJ, q2=H/BNB/ENA, q3=JTO/KAT/TAO, q4=UNI/JTO/BICO
remove top 3 positive symbols H,JTO,UNI: removed_share_pct=45.79050938858432%, remaining_trades=37, remaining_win_rate=48.64864864864865%, remaining_total_profit=80.85299189
```

This branch is the better forward-observation candidate under the new `55%`
target because q2/q3/q4 all clear `55%`, Sharpe is higher, drawdown remains
low, and trades stay at `45`. It misses `150U` by `0.85085476U`, so do not call
the optimization complete. Parameter checks around it did not find a better
generic replacement:

```text
target_r scan with min_delta=11,min_price=5: 3.3R remains best; 3.1R=139.19753118, 3.2R=144.17146091, 3.25R=146.65983375, 3.35R=143.50746025, 3.4R=140.94903225, 3.5R=144.97882742
min_price scan with min_delta=11/12: lower min_price increases weak late-window trades or lowers win rate; min_delta=12,min_price=5 improves win_rate to 59.523809523809526% but drops to 42 trades and 143.43244199U
min_delta scan at min_price=5: min_delta=8 gives 62 trades but only 50.0% win; min_delta=15 gives 35 trades and 65.71428571428571% win but only 143.07318759U; min_delta=20/25 are too few or too low profit
max_new_rank scan at min_delta=11,min_price=5: tightening to 24/26/28 drops profit to 96.13386384/113.25882880/130.09621907U; keep max_new_rank=30
```

`--profit-protect-*` now changes `symbol_isolated_100u` framework equity replay
through same-side framework stop updates and is covered by focused tests,
including a conservative guard that refuses to place a new protected stop if the
trigger candle closes below that stop. `--runner-*` and `--stop-reentry-mode`
still only affect the CLI outcome simulation and should not be used as framework
equity evidence until mapped into the existing framework risk system.

`avg_r_complete` excludes incomplete rows, matching the Core CLI result output.
The Web summary API `avg_result_r` includes all non-null `result_r`, including
incomplete rows, so that value is lower and should not be compared directly to
CLI `avg_r_complete`.

2026-06-21 chase filter audit: the `new_rank <= 5 &&
price_change_pct >= 8` exclusion was originally a generic anti-chase guard. Its
purpose was to avoid entries that had already jumped into the top ranks after a
large same-event price move. The latest local evidence shows that this guard is
too blunt for the relaxed target. With the conservative branch unchanged except
for disabling the chase filter via `--chase-top-rank 0`, the framework replay
crossed the profit target but weakened q4:

```text
min_delta_rank=11, max_delta_rank=72, min_price_change_pct=5, target_r=3.3, chase disabled
candle_pairs=54, raw_candidate_events=64439, entry_pass=3478, entry_trigger_after=3112
framework: symbols=27, trades=47, win_rate=57.446808510638306%, trade_sharpe=3.7811324199202496, max_drawdown_pct=7.55972182, total_profit=151.65980730
q1: trades=4, win_rate=50.0%, total_profit=6.43918469
q2: trades=9, win_rate=55.55555555555556%, total_profit=38.28488234
q3: trades=20, win_rate=60.0%, total_profit=87.43579249
q4: trades=17, win_rate=52.94117647058824%, total_profit=20.50068431
```

The 48h paper-outcome diff against the filtered branch added only two settled
events:

```text
LAB-USDT-SWAP, new_rank=4, delta_rank=21, price_change_pct=54.09166054, reclaim_ema, +2.0R
ALLO-USDT-SWAP, new_rank=5, delta_rank=33, price_change_pct=97.48468656, reclaim_ema, -1.0R
```

So the old overheat filter did not simply remove bad trades; it removed one
good high-momentum follow-through and one bad chase. A blanket top-rank
price-change cap is therefore not the right generic explanation.

The better simplified research candidate is to remove the chase filter and
raise the generic rank-jump floor instead:

```text
params: min_delta_rank=13, max_delta_rank=72, max_new_rank=30, min_price_change_pct=5, target_r=3.3, chase disabled, no profit-protect
candle_pairs=54, raw_candidate_events=56375, entry_pass=2993, entry_trigger_after=2671
paper 24h: trades=41, resolved_win_rate=55.88235294117647%, avg_r_complete=1.3408920140693348
paper 48h: trades=41, resolved_win_rate=56.41025641025641%, avg_r_complete=1.4162710947769892
framework: symbols=27, trades=43, win_rate=60.46511627906976%, trade_sharpe=4.034706652682123, max_drawdown_pct=7.55972182, total_profit=161.05575703
early: trades=15, win_rate=60.0%, total_profit=70.65064943
late: trades=29, win_rate=58.620689655172406%, total_profit=85.05309245
q1: trades=5, win_rate=60.0%, total_profit=18.55417809
q2: trades=11, win_rate=63.63636363636363%, total_profit=62.22825234
q3: trades=16, win_rate=56.25%, total_profit=67.19231019
q4: trades=15, win_rate=60.0%, total_profit=23.77921676
```

Adjacent checks support this as a region rather than a single fragile point.
`min_delta_rank=13,target=3.3` without chase but with `profit_protect_after=2.8R`
still produced `148.56271569U` and all quartiles above `56%`; moving the
profit-protect activation to `3.0R` produced `157.16714096U`. Setting no
profit-protect matched late activation near the target, so the cleanest version
is the no-protect command above.

Remaining caveat: symbol concentration is still material. Removing the top three
positive symbols `H-USDT-SWAP,JTO-USDT-SWAP,UNI-USDT-SWAP` removes
`73.00057862U` (`45.326277042410126%` of profit), leaving `35` trades,
`51.42857142857142%` win rate, and `88.05517841U` profit. This candidate is
the current best research fit for the user's relaxed objective, but it should
still go through forward paper observation before being promoted as a production
default.

Follow-up neighbor scans did not find a cleaner replacement. Trigger replay
confirmed that `breakout_previous_high` is the stable trigger and `reclaim_ema`
is a lower-win-rate profit extender:

```text
breakout_previous_high standalone: trades=30, win_rate=60.0%, total_profit=110.86083951
reclaim_ema standalone: trades=21, win_rate=47.61904761904761%, total_profit=59.50537895
combined sequence replay: trades=43, win_rate=60.46511627906976%, total_profit=161.05575703
```

Generic neighbor scans around the simplified candidate showed:

```text
target_r: 3.2R=155.39358913U, 3.3R=161.05575703U, 3.4R=146.27300167U, 3.5R=151.26762696U but q3 win_rate=46.666666666666664%, 3.6R full win_rate=54.761904761904766%
max_new_rank: 24/26/28 all reduced profit to 119.74811540U / 136.84811476U / 144.26045117U; keep 30
min_price_change_pct: 6/8/10/12 improved some headline win rates but reduced profit to 145.59392074U / 137.33840578U / 128.72304104U / 134.86304104U
entry_max_distance_pct: 3.5 improved win rate but only 147.10092705U; 4.5/5.0/6.0 added trades but degraded full/q4 win rate; keep 4.0
entry_min_volume_ratio: 0.0/0.8 added many low-quality trades and hurt q4; 1.2/1.5 over-filtered and collapsed profit; keep 1.0
```

Current interpretation: the best simplification is not an overheat chase cap,
not profit-protect tuning, and not trigger removal. It is a stricter generic
rank-jump floor (`min_delta_rank=13`) plus the existing distance and volume
quality gates. The unresolved weakness is still cross-symbol concentration, not
time-split instability.

2026-06-21 refresh scan against the live local `quant_core` sample confirmed
the same direction. The host-side CLI connection issue was environmental:
`docker-compose.yml` still documents `example`, but the current persistent
Postgres volume uses the older local password from the repo runbooks. The replay
was run with the current working connection string and the same
`symbol_isolated_100u` framework path.

```text
no_chase_filter:
  candle_pairs=54, raw_candidate_events=56406, entry_trigger_after=2671
  trades=43, symbols=27, win_rate=60.46511627906976%, trade_sharpe=4.034706652682123, max_drawdown_pct=7.55972182, total_profit=161.05575703
  q1/q2/q3/q4 win_rate=60.0%/63.63636363636363%/56.25%/60.0%

old_chase_filter (new_rank<=5 and price_change_pct>=8 excluded):
  candle_pairs=54, raw_candidate_events=52671, entry_trigger_after=2571
  trades=41, symbols=27, win_rate=60.97560975609756%, trade_sharpe=3.972352350037274, max_drawdown_pct=7.55972182, total_profit=154.65647890
  q1/q2/q3/q4 win_rate=50.0%/63.63636363636363%/60.0%/53.333333333333336%
```

The old chase filter still improves headline win rate by only `0.510493477%`
while removing two trades and reducing profit by `6.39927813U`. It also weakens
q1 and q4. Therefore the reason to keep it is not supported by the current
sample; it is a conservative anti-chase guard, not an optimal strategy filter.

The "make it more nuanced" tail-rank hypothesis also did not beat the simple
candidate. The tested rule was: if `new_rank` is in the tail bucket, require a
higher same-event `price_change_pct`; otherwise keep the base filters.

```text
tail21_price8:  trades=38, win_rate=63.1578947368421%,  sharpe=4.10559024022797,  max_dd=7.55972182, profit=153.28930022, q2_win=44.44444444444444%, q4_win=50.0%
tail21_price10: trades=37, win_rate=62.16216216216216%, sharpe=3.854419026876245, max_dd=7.55972182, profit=141.69428160, q2_win=37.5%, q4_win=53.333333333333336%
tail21_price12: trades=36, win_rate=63.888888888888886%, sharpe=4.014699619115618, max_dd=7.55972182, profit=144.76428160, q2_win=37.5%, q4_win=57.14285714285714%
tail21_price15: trades=35, win_rate=62.857142857142854%, sharpe=3.7042337773056997, max_dd=7.74122662, profit=129.79273953, q2_win=44.44444444444444%, q4_win=53.84615384615385%
tail21_price20: trades=29, win_rate=62.06896551724138%, sharpe=3.2278521885887272, max_dd=7.74122662, profit=101.25662535, min_trades=false

tail25_price8:  trades=40, win_rate=60.0%,              sharpe=3.9374199357825668, max_dd=7.55972182, profit=150.32145082, q2_win=42.857142857142854%, q4_win=53.333333333333336%
tail25_price10: trades=39, win_rate=58.97435897435898%, sharpe=3.6943993089982214, max_dd=7.55972182, profit=138.72643220, q2_win=33.33333333333333%, q4_win=50.0%
tail25_price12: trades=38, win_rate=60.526315789473685%, sharpe=3.8411986383922385, max_dd=7.55972182, profit=141.79643220, q2_win=37.5%, q4_win=53.333333333333336%
tail25_price15: trades=38, win_rate=60.526315789473685%, sharpe=3.7946510879089135, max_dd=7.55972182, profit=139.77112752, q2_win=33.33333333333333%, q4_win=53.333333333333336%
```

The tail-rank variants can raise headline win rate, but they either lose
`7.76645681U` to `59.79913168U` versus the simple no-chase candidate, or drop
below the 30-trade floor. They also introduce a poor q2 split. The current best
tradeoff remains:

```text
min_delta_rank=13
max_delta_rank=72
max_new_rank=30
min_price_change_pct=5
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
entry_max_distance_pct=4.0
entry_min_volume_ratio=1.0
target_r=3.3
stop_loss_pct=0.03
chase_top_rank=0
stop_reentry_mode=off
profit_protect=off
```

The next scan tested whether the improvement should come from exit geometry
instead of more entry filtering. With the same no-chase entry set, changing
`stop_loss_pct` changes both the stop price and the target price used by the
framework replay, so it is a real risk/exit change rather than a cosmetic R
rescale.

Coarse stop/target grid:

```text
stop_loss=0.025:
  best nearby: target=3.5R, trades=48, win_rate=52.083333333333336%, sharpe=3.0143711900292396, max_dd=7.19385133, profit=105.20204509
  verdict: too many low-quality exits; below 55% win-rate and far below profit target.

stop_loss=0.030:
  target=3.2R, trades=43, win_rate=60.46511627906976%, sharpe=3.991091233539193, max_dd=7.55972182, profit=155.39358913
  target=3.3R, trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
  target=3.4R, trades=42, win_rate=57.14285714285714%, sharpe=3.721401095957837, max_dd=7.55972182, profit=146.27300167
  verdict: 3.3R is the Sharpe-first local optimum.

stop_loss=0.035:
  target=2.8R, trades=42, win_rate=64.28571428571429%, sharpe=3.9438887488027543, max_dd=8.51094850, profit=161.13258507
  target=2.9R, trades=41, win_rate=63.41463414634146%, sharpe=3.9354819580125184, max_dd=8.51094850, profit=162.53670679
  verdict: slight profit lift, but higher drawdown and lower Sharpe than 0.03/3.3R.

stop_loss=0.0375:
  target=2.6R, trades=42, win_rate=61.904761904761905%, sharpe=3.958164886224208, max_dd=8.98471703, profit=162.12393256
  target=2.7R, trades=41, win_rate=60.97560975609756%, sharpe=3.9587117307028654, max_dd=8.98471703, profit=164.05047908
  verdict: best profit found in this scan, but it pays for the extra 2.99472205U
  versus the base with higher drawdown and lower Sharpe.

stop_loss=0.040:
  best nearby: target=2.4R, trades=42, win_rate=57.14285714285714%, sharpe=3.717357630927375, max_dd=9.45725569, profit=153.26828599
  verdict: wider stop becomes worse on win-rate, Sharpe, and profit.
```

Full replay on the two wider-stop challengers:

```text
base_sharpe 0.030/3.3R:
  trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
  early/late win_rate=60.0%/58.620689655172406%, profit=70.65064943/85.05309245
  q1/q2/q3/q4 win_rate=60.0%/63.63636363636363%/56.25%/60.0%
  remove top3 positives: remaining_trades=35, remaining_win_rate=51.42857142857142%, remaining_profit=88.05517841

wider_stop_profit_mid 0.035/2.9R:
  trades=41, win_rate=63.41463414634146%, sharpe=3.9354819580125184, max_dd=8.51094850, profit=162.53670679
  early/late win_rate=66.66666666666666%/59.25925925925925%, profit=82.84156449/74.31425185
  q1/q2/q3/q4 win_rate=60.0%/72.72727272727273%/57.14285714285714%/60.0%
  remove top3 positives: remaining_trades=33, remaining_win_rate=54.54545454545454%, remaining_profit=89.04428246

wider_stop_profit_high 0.0375/2.7R:
  trades=41, win_rate=60.97560975609756%, sharpe=3.9587117307028654, max_dd=8.98471703, profit=164.05047908
  early/late win_rate=66.66666666666666%/55.55555555555556%, profit=81.03428123/77.47794172
  q1/q2/q3/q4 win_rate=60.0%/72.72727272727273%/57.14285714285714%/53.333333333333336%
  remove top3 positives: remaining_trades=33, remaining_win_rate=51.515151515151516%, remaining_profit=91.35378129
```

Current decision after the exit scan:

- Sharpe-first / balanced default remains `0.03 stop_loss_pct + 3.3R target`.
- Profit-first research candidate is `0.0375 stop_loss_pct + 2.7R target`, but
  it is not a strict upgrade because drawdown rises to `8.98471703%`, Sharpe
  falls below the base, and q4 win-rate drops to `53.333333333333336%`.
- A more conservative profit-lift candidate is `0.035 stop_loss_pct + 2.9R
  target`: it improves top-line win-rate and keeps q4 at `60%`, but the extra
  profit is only `1.48094976U` over the base with higher drawdown.

Therefore the best current framing is two presets, not one forced replacement:
keep `0.03/3.3R` as the cleaner Sharpe/default candidate, and track
`0.0375/2.7R` as the PnL-seeking paper-observation challenger.

Profit protection was then tested as a way to make the wider-stop challenger
less fragile. This is still framework-level per-symbol replay: once a trade has
moved far enough in R, the strategy emits an updated stop-loss signal for that
same symbol position. It does not use portfolio-level capital sharing.

Base `0.03/3.3R` with protection:

```text
after=1.4, stop=0.4: trades=49, win_rate=67.3469387755102%, sharpe=3.415977215043625, max_dd=7.55972182, profit=116.46887617
after=1.4, stop=0.8: trades=49, win_rate=65.3061224489796%, sharpe=3.4194074957603187, max_dd=7.55972182, profit=105.06698143
after=1.4, stop=1.2: trades=48, win_rate=66.66666666666666%, sharpe=3.899867399886567, max_dd=7.55972182, profit=104.41601312
after=1.8, stop=0.4: trades=45, win_rate=62.22222222222222%, sharpe=3.646954878203686, max_dd=7.55972182, profit=135.58149312
after=1.8, stop=0.8: trades=45, win_rate=62.22222222222222%, sharpe=3.340548963984261, max_dd=7.55972182, profit=113.73320805
after=1.8, stop=1.2: trades=43, win_rate=62.7906976744186%, sharpe=3.076584016385959, max_dd=7.55972182, profit=88.79874356
after=2.2, stop=0.4: trades=44, win_rate=59.09090909090909%, sharpe=3.451892131094753, max_dd=7.55972182, profit=129.89569571
after=2.6, stop=0.4: trades=43, win_rate=60.46511627906976%, sharpe=3.676393389467742, max_dd=7.55972182, profit=141.88666482
```

Profit-first `0.0375/2.7R` with protection:

```text
after=1.2, stop=0.3: trades=47, win_rate=65.95744680851064%, sharpe=3.1680416505272184, max_dd=8.98471703, profit=113.21438375
after=1.5, stop=0.3: trades=44, win_rate=63.63636363636363%, sharpe=3.343959665730574, max_dd=8.98471703, profit=126.63852746
after=1.8, stop=0.3: trades=43, win_rate=60.46511627906976%, sharpe=3.4093688044223636, max_dd=8.98471703, profit=138.43910982
after=2.1, stop=0.3: trades=42, win_rate=61.904761904761905%, sharpe=3.498088361812747, max_dd=8.98471703, profit=140.66861461
after=2.1, stop=0.6: trades=42, win_rate=61.904761904761905%, sharpe=3.296955985056664, max_dd=8.98471703, profit=128.82913145
after=2.1, stop=1.2: trades=42, win_rate=61.904761904761905%, sharpe=3.321762933561542, max_dd=8.98471703, profit=123.47347713
```

The protection layer raises headline win-rate in some cases, but it does not
reduce the max drawdown on this replay and it gives up too much profit. It is
therefore not the right fix for the wider-stop candidate. The remaining
tradeoff is unchanged: `0.03/3.3R` is cleaner on Sharpe and drawdown;
`0.0375/2.7R` is a PnL-seeking challenger with higher drawdown.

2026-06-21 further replay added a framework trade-level report
(`--equity-trade-report`) so overheat attribution can use the actual
`symbol_isolated_100u` closed trades instead of standalone feature-bucket
replays. The exact full-combination attribution for the simple `0.03/3.3R`
candidate was:

```text
all trades: trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
new_rank<=5 and price_change_pct>=8: trades=2, wins=1, losses=1, win_rate=50.0%, profit=6.45821900
other trades: trades=41, wins=25, losses=16, win_rate=60.97560975609756%, profit=154.59753804
```

Both overheat trades were `reclaim_ema` entries. The old broad filter
`new_rank<=5 && price_change_pct>=8` removes one winner and one loser, but the
net removed bucket is positive. That explains why the broad chase filter raises
headline win-rate slightly while reducing total PnL. It is not a good optimal
filter on the current sample.

A refined chase scan kept the same entry set and only changed
`chase_top_rank/chase_price_change_pct`:

```text
no_chase:     trades=43, win_rate=60.46511627906976%, sharpe=4.034706652682123, max_dd=7.55972182, profit=161.05575703
top3_pct8-80: identical closed-trade metrics to no_chase; no actual trade removed
top5_pct8:   trades=41, win_rate=60.97560975609756%, sharpe=3.972352350037274, max_dd=7.55972182, profit=154.65647890
top5_pct50:  trades=42, win_rate=61.904761904761905%, sharpe=4.178398928166582, max_dd=7.55972182, profit=164.45778555
top5_pct80:  trades=42, win_rate=61.904761904761905%, sharpe=4.178398928166582, max_dd=7.55972182, profit=164.45778555
top10_pct8:  trades=37, win_rate=59.45945945945946%, sharpe=3.821499961127567, max_dd=7.55972182, profit=146.43071424
top10_pct80: trades=42, win_rate=61.904761904761905%, sharpe=4.178398928166582, max_dd=7.55972182, profit=164.45778555
```

The useful nuance is therefore not "top rank plus 8% is too hot"; it is only
the very extreme same-event jump. In the current closed-trade set, `top5_pct80`
removes `ALLO-USDT-SWAP` event `1653251` (`new_rank=5`,
`price_change_pct=102.08968412`, `reclaim_ema`, `-3.37178100U`) and slightly
improves a later same-symbol trade because the symbol lock no longer starts
from the losing entry.

Combining the refined extreme-chase filter with the exit geometry scan produced
two stronger candidates:

```text
balanced_extreme_chase 0.035/2.9R + top5_pct80:
  trades=40, win_rate=65.0%, sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380
  early/late win_rate=69.23076923076923%/64.28571428571429%, profit=76.69142049/98.09566180
  q1/q2/q3/q4 win_rate=60.0%/77.77777777777779%/66.66666666666666%/53.333333333333336%
  remove top3 positives: remaining_trades=32, remaining_win_rate=56.25%, remaining_profit=92.98893946
  remove top5 positives: remaining_trades=30, remaining_win_rate=53.333333333333336%, remaining_profit=70.27828031

pnl_extreme_chase 0.0375/2.7R + top5_pct80:
  trades=40, win_rate=62.5%, sharpe=4.13105376665384, max_dd=8.98471703, profit=168.25947388
  early/late win_rate=69.23076923076923%/60.71428571428571%, profit=75.18338223/101.42409295
  q1/q2/q3/q4 win_rate=60.0%/77.77777777777779%/66.66666666666666%/46.666666666666664%
  remove top3 positives: remaining_trades=32, remaining_win_rate=53.125%, remaining_profit=95.56277608
  remove top5 positives: remaining_trades=30, remaining_win_rate=50.0%, remaining_profit=72.90269584
```

Current decision: use `0.035/2.9R + chase_top_rank=5 +
chase_price_change_pct=80` as the stronger balanced research candidate. It
beats the previous simple default on PnL, win-rate, and post-top3 concentration,
while keeping drawdown far below 30%. The `0.0375/2.7R + top5_pct80` version is
the pure-PnL challenger, but q4 falls below 50%, so it should not replace the
balanced candidate without more forward paper evidence. This refined filter is
still sample-small and mostly justified by one extreme ALLO loss; treat it as a
paper-observation candidate, not production proof.

Follow-up robustness scans kept the balanced candidate as the current best
tradeoff. A full-combination q4 trade-level cut showed the weakest recent bucket
was low same-event movement:

```text
q4 actual closed trades after entry_ts>=1781582400000:
  trades=13, wins=8, losses=5, win_rate=61.53846153846154%, profit=22.48072370
  price_change 5_10: trades=4, wins=1, losses=3, win_rate=25.0%, profit=-10.48967040
  price_change 10_20: trades=4, wins=4, losses=0, win_rate=100.0%, profit=16.57867767
  price_change 20_plus: trades=5, wins=3, losses=2, win_rate=60.0%, profit=16.39171643
```

Raising the global `min_price_change_pct` improves q4 but over-filters the full
sample:

```text
min_price=5:  trades=40, win_rate=65.0%,              sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%, top3_removed_profit=92.98893946
min_price=6:  trades=36, win_rate=66.66666666666666%, sharpe=3.9546222252002257, max_dd=8.51094850, profit=151.52527679, q4_win=66.66666666666666%, top3_removed_profit=78.03285245
min_price=8:  trades=32, win_rate=65.625%,            sharpe=3.7038963246218675, max_dd=6.89059169, profit=131.07163927, q4_win=58.333333333333336%, top3_removed_profit=59.83404964
min_price=10: trades=30, win_rate=66.66666666666666%, sharpe=3.6256665960556225, max_dd=3.57000000, profit=122.68291102, q4_win=66.66666666666666%, top3_removed_profit=60.89200500
```

Tail-rank variants were also weaker than the balanced candidate. The best
tail-filtered profit in this pass was `tail25_price8` with `156.40637991U`,
still below `166.48136380U`, and most tail variants either reduced q4 or cut too
many trades.

Entry-quality neighbor scans did not improve the tradeoff either:

```text
distance=3.5, volume=1.0: trades=35, win_rate=65.71428571428571%, sharpe=3.8183571920867347, max_dd=8.51094850, profit=150.46332669, q4_win=58.333333333333336%
distance=4.0, volume=1.0: trades=40, win_rate=65.0%,              sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%
distance=5.0, volume=1.0: trades=45, win_rate=60.0%,              sharpe=3.7363653728534647, max_dd=8.51094850, profit=164.74915474, q4_win=50.0%
distance=4.0, volume=0.8: trades=49, win_rate=55.10204081632652%, sharpe=3.097666076354121,  max_dd=9.31258359, profit=147.11458866, q4_win=33.33333333333333%
distance=4.0, volume=1.2: trades=35, win_rate=54.285714285714285%, sharpe=2.869615487369861, max_dd=6.89059169, profit=109.44354870, q4_win=63.63636363636363%
```

`stop_reentry_mode=breakout_reclaim` was a no-op on this candidate: it produced
the same `40` trades, `65.0%` win rate, `166.48136380U` profit, Sharpe
`4.096343901750493`, and `8.51094850%` max drawdown as `stop_reentry_mode=off`.

Current practical decision is unchanged: keep `0.035/2.9R + top5_pct80` as the
balanced paper candidate; keep `0.0375/2.7R + top5_pct80` as a profit-only
challenger; do not raise the global price-change floor or tighten entry distance
unless forward paper data confirms the q4 low-price-change weakness persists.

2026-06-21 follow-up refresh with the current local snapshot kept the same
decision. The active data now spans `54` candle pairs and `55,422` filtered
candidate events for this parameter family, from
`2026-05-29 05:57:58.203531+00` to `2026-06-21 10:29:15.44423+00`. The closed
trade report for the balanced candidate spans `40` trades from
`2026-05-30 12:30:00` to `2026-06-21 09:45:00` Beijing time.

The latest trade-level overheat attribution makes the old
`new_rank <= 5 && price_change_pct >= 8` filter look too broad:

```text
no_chase, 0.035/2.9R:
  trades=41, win_rate=63.41463414634146%, sharpe=3.9354819580125184, max_dd=8.51094850, profit=162.53670679
  bucket new_rank<=5 && price_change_pct>=8:
    LAB-USDT-SWAP rank=4 price_change=48.87948597 reclaim_ema +10.08000000
    ALLO-USDT-SWAP rank=5 price_change=102.08968412 reclaim_ema -3.92985600
    bucket_total=+6.15014400, bucket_win_rate=50.0%

old_filter_top5_pct8:
  trades=39, win_rate=64.1025641025641%, sharpe=3.888392315439138, max_dd=8.51094850, profit=156.53420633

current_filter_top5_pct80:
  trades=40, win_rate=65.0%, sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380
  remaining old bucket after current filter:
    LAB-USDT-SWAP rank=4 price_change=48.87948597 reclaim_ema +10.08000000
```

So the `8%` cap removes a net-positive high-rank bucket. The narrower `80%`
cap removes only the extreme ALLO failure in this closed-trade set and keeps the
LAB follow-through win. This is still not production proof because the
justification is small-sample and mostly one loser, but it is a better paper
candidate than the old blanket `8%` anti-chase rule.

Profit-protection scans did not improve the balanced candidate under the same
framework equity replay. Early protection raised headline win rate but cut too
much trend follow-through:

```text
base_no_protect:              trades=40, win_rate=65.0%,              sharpe=4.096343901750493,  max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%, top3_removed_profit=92.98893946
protect_after_1.4_stop_0.3:  trades=44, win_rate=68.18181818181817%, sharpe=3.5353705792456154, max_dd=8.51094850, profit=131.82143204, q4_win=52.94117647058824%,  top3_removed_profit=82.16644074
protect_after_2.2_stop_0.0:  trades=41, win_rate=65.0%,              sharpe=3.739914211836979,  max_dd=8.51094850, profit=149.46615467, q4_win=53.333333333333336%, top3_removed_profit=88.18827758
protect_after_2.6_stop_0.0:  trades=41, win_rate=65.85365853658537%, sharpe=3.96079296532936,   max_dd=8.51094850, profit=161.68070192, q4_win=53.333333333333336%, top3_removed_profit=88.18827758
protect_after_2.6_stop_1.0:  trades=41, win_rate=65.85365853658537%, sharpe=3.9425845966679858, max_dd=8.51094850, profit=158.68250845, q4_win=53.333333333333336%, top3_removed_profit=85.19008411
```

No tested protection setting beat `166.48136380U`, and max drawdown stayed
unchanged at `8.51094850%`, so protection is not buying a meaningful risk
reduction here. The runner flags were also sanity checked and currently leave
`framework_equity_result` unchanged (`40` trades, `65.0%`, `166.48136380U`) for
sample `4R/4.5R/5R` runner settings. Do not use runner output as comparable
framework evidence until partial-runner behavior is mapped into the existing
Vegas-style replay path.

A refreshed stop/target neighborhood scan also kept the same balanced
candidate:

```text
0.0325/3.1R: trades=40, win_rate=62.5%, sharpe=3.983635272132361,  max_dd=8.03595010, profit=157.91230058, q4_win=53.333333333333336%
0.0350/2.7R: trades=41, win_rate=65.85365853658537%, sharpe=4.046050711002911,  max_dd=8.51094850, profit=158.12318006, q4_win=53.333333333333336%
0.0350/2.9R: trades=40, win_rate=65.0%, sharpe=4.096343901750493, max_dd=8.51094850, profit=166.48136380, q4_win=53.333333333333336%
0.0375/2.7R: trades=40, win_rate=62.5%, sharpe=4.13105376665384,  max_dd=8.98471703, profit=168.25947388, q4_win=46.666666666666664%
0.0400/2.5R: trades=40, win_rate=57.49999999999999%, sharpe=3.902078264206322, max_dd=9.45725569, profit=160.28097721, q4_win=40.0%
```

`0.0375/2.7R` remains the pure-PnL challenger, but it fails the robustness smell
test because q4 win rate is below `50%` and post-top3 concentration win rate is
only `53.125%`. Keep `0.035/2.9R + chase_top_rank=5 +
chase_price_change_pct=80` as the current balanced research candidate.

2026-06-21 later scan: widening the 15m entry distance, then adding a
trigger/rank research blocklist, produced a stronger candidate without adding
symbol-specific rules. The first step was to relax `entry_max_distance_pct`
from `4.0` to the `5.25-5.75` neighborhood and re-scan exits:

```text
distance=5.25, stop=0.0375, target=2.7R: trades=45, win_rate=62.22222222222222%, sharpe=3.81281391465377,  max_dd=8.98471703, profit=176.35564812, q4_win=57.14285714285714%
distance=5.50, stop=0.0375, target=2.7R: trades=46, win_rate=63.04347826086957%, sharpe=3.9102594530141572, max_dd=8.98471703, profit=180.69231859, q4_win=57.14285714285714%
distance=5.75, stop=0.0375, target=2.7R: trades=48, win_rate=60.416666666666664%, sharpe=3.717325826337162, max_dd=8.98471703, profit=178.40968598, q4_win=53.333333333333336%
distance=6.00, stop=0.0375, target=2.7R: trades=49, win_rate=59.183673469387756%, sharpe=3.625912947690713, max_dd=8.98471703, profit=176.02193582, q4_win=50.0%
```

`5.5/0.0375/2.7R` is therefore a better high-trade-count base than the earlier
`4.0/0.035/2.9R` candidate, but its trade-level attribution showed one weak
bucket:

```text
reclaim_ema + new_rank 11-20:
  trades=7, wins=2, win_rate=28.571428571428573%, profit=-18.55061472
```

To test that as a reusable research filter, the CLI now supports
`--entry-trigger-rank-blocklist trigger:min-max`, for example
`--entry-trigger-rank-blocklist reclaim_ema:11-20`. This only filters confirmed
events in `market_velocity_event_backtest`; it is not part of the production
paper observer preset.

The conservative blocklist candidate improves both return and risk:

```text
params:
  stop_loss_pct=0.0375
  target_r=2.7
  entry_max_distance_pct=5.5
  entry_min_volume_ratio=1.0
  min_delta_rank=13
  max_delta_rank=72
  max_new_rank=30
  min_price_change_pct=5
  chase_top_rank=5
  chase_price_change_pct=80
  entry_trigger_allowlist=breakout_previous_high,reclaim_ema
  entry_trigger_rank_blocklist=reclaim_ema:11-20

result:
  trades=43, win_rate=62.7906976744186%, sharpe=4.131473618558458, max_dd=7.37274802, profit=184.26477392
  early/late win_rate=58.333333333333336%/65.625%, profit=55.12916184/138.78470167
  q1/q2/q3/q4 win_rate=66.66666666666666%/50.0%/68.42105263157895%/57.14285714285714%
  remove top3 positives: remaining_trades=36, remaining_win_rate=61.111111111111114%, remaining_profit=127.71674999
  remove top5 positives: remaining_trades=32, remaining_win_rate=59.375%, remaining_profit=98.66855857
```

Neighbor checks suggest the idea is not a single-threshold accident:

```text
distance=5.25, 0.0375/2.7R, block 11-20: trades=42, win_rate=61.904761904761905%, sharpe=4.030466224165957,  max_dd=7.37274802, profit=179.92810345, q4_win=61.53846153846154%
distance=5.75, 0.0375/2.7R, block 11-20: trades=46, win_rate=58.69565217391305%,  sharpe=3.7363130716706876, max_dd=7.49407600, profit=175.57155029, q4_win=57.14285714285714%
distance=5.50, 0.0350/2.9R, block 11-20: trades=44, win_rate=61.36363636363637%,  sharpe=4.091146859498289,  max_dd=6.89059169, profit=180.35389347, q4_win=53.333333333333336%
```

The more aggressive adjacent rank ranges scored even higher, but should be
treated as challengers until forward paper data confirms the rank boundary:

```text
block reclaim_ema:13-20: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block reclaim_ema:11-22: trades=42, win_rate=64.28571428571429%, sharpe=4.2906542736637565, max_dd=3.82000000, profit=187.94366873, q4_win=66.66666666666666%
```

2026-06-21 follow-up neighborhood scan kept the same broad shape but found a
better middle-rank `reclaim_ema` block. First, `min_price_change_pct=5` and
`entry_min_volume_ratio=1.0` stayed best. Raising the price-change floor to `6`
or tightening volume improved some recent slices but reduced full-sample profit
too much; loosening volume to `0.9` added weak trades and pushed q4 below the
target:

```text
block 11-20, min_price=5, vol=0.9: trades=49, win_rate=55.10204081632652%, sharpe=3.370892422663588,  max_dd=7.49407600, profit=158.67533783, q4_win=46.666666666666664%
block 11-20, min_price=5, vol=1.0: trades=43, win_rate=62.7906976744186%,  sharpe=4.131473618558458,  max_dd=7.37274802, profit=184.26477392, q4_win=57.14285714285714%
block 11-20, min_price=6, vol=1.0: trades=40, win_rate=62.5%,              sharpe=3.843919482315516,  max_dd=7.37274802, profit=166.04234420, q4_win=61.53846153846154%
block 13-20, min_price=5, vol=1.0: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block 11-22, min_price=5, vol=1.0: trades=42, win_rate=64.28571428571429%, sharpe=4.2906542736637565, max_dd=3.82000000, profit=187.94366873, q4_win=66.66666666666666%
```

Distance and exit neighborhood checks also favored the same region:

```text
block 13-20, distance=5.25, 0.0375/2.7R: trades=42, win_rate=64.28571428571429%, sharpe=4.158529966947208,  max_dd=7.37274802, profit=184.36693095, q4_win=61.53846153846154%
block 13-20, distance=5.50, 0.0375/2.7R: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block 13-20, distance=5.50, 0.0350/2.9R: trades=44, win_rate=63.63636363636363%, sharpe=4.218999044078994,  max_dd=6.89059169, profit=184.77805614, q4_win=53.333333333333336%
block 13-20, distance=5.75, 0.0375/2.7R: trades=46, win_rate=60.86956521739131%, sharpe=3.8483688592034606, max_dd=7.49407600, profit=180.01037779, q4_win=57.14285714285714%

block 11-22, distance=5.25, 0.0375/2.7R: trades=41, win_rate=63.41463414634146%, sharpe=4.187645867333588,  max_dd=3.82000000, profit=183.60699826, q4_win=66.66666666666666%
block 11-22, distance=5.50, 0.0375/2.7R: trades=42, win_rate=64.28571428571429%, sharpe=4.2906542736637565, max_dd=3.82000000, profit=187.94366873, q4_win=66.66666666666666%
block 11-22, distance=5.75, 0.0375/2.7R: trades=45, win_rate=60.0%,              sharpe=3.868927872958535,  max_dd=7.49407600, profit=179.25044509, q4_win=61.53846153846154%
```

The adjacent rank-boundary scan then showed `reclaim_ema:12-22`,
`reclaim_ema:13-21`, and `reclaim_ema:13-22` all produce the same strongest
closed-trade result in the current snapshot:

```text
block 12-20: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=7.37274802, profit=188.70360142, q4_win=57.14285714285714%
block 12-22: trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767,  max_dd=3.82000000, profit=192.38249622, q4_win=66.66666666666666%
block 13-21: trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767,  max_dd=3.82000000, profit=192.38249622, q4_win=66.66666666666666%
block 13-22: trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767,  max_dd=3.82000000, profit=192.38249622, q4_win=66.66666666666666%
block 14-22: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=3.82000000, profit=188.56249622, q4_win=66.66666666666666%
block 15-22: trades=43, win_rate=65.11627906976744%, sharpe=4.260803895629817,  max_dd=3.82000000, profit=188.56249622, q4_win=66.66666666666666%
```

`reclaim_ema:13-22` is the clearest label for that result because it blocks the
middle-rank reclaim bucket while keeping rank-11 mixed evidence and rank-23+
tail reclaim entries. Full report:

```text
params:
  stop_loss_pct=0.0375
  target_r=2.7
  entry_max_distance_pct=5.5
  entry_min_volume_ratio=1.0
  min_delta_rank=13
  max_delta_rank=72
  max_new_rank=30
  min_price_change_pct=5
  chase_top_rank=5
  chase_price_change_pct=80
  entry_trigger_allowlist=breakout_previous_high,reclaim_ema
  entry_trigger_rank_blocklist=reclaim_ema:13-22

result:
  trades=42, win_rate=66.66666666666666%, sharpe=4.426683134074767, max_dd=3.82000000, profit=192.38249622
  early/late win_rate=66.66666666666666%/64.28571428571429%, profit=73.97808104/113.57103129
  q1/q2/q3/q4 win_rate=60.0%/72.72727272727273%/64.70588235294117%/66.66666666666666%
  remove top1 positive: remaining_trades=39, remaining_win_rate=64.1025641025641%, remaining_profit=170.36508313
  remove top3 positives: remaining_trades=35, remaining_win_rate=62.857142857142854%, remaining_profit=132.53867897
  remove top5 positives: remaining_trades=30, remaining_win_rate=63.33333333333333%, remaining_profit=103.02859534
```

Additional stress checks show `13-22` is materially stronger than the previous
balanced and conservative-complex candidates after removing top contributors:

```text
old_balanced 4.0/0.035/2.9R:
  base: trades=40, win=65.00%, profit=166.48136378
  remove top3 symbols: trades=32, win=56.25%, profit=92.98893944
  remove top5 symbols: trades=30, win=53.33%, profit=70.27828030
  remove top5 trades:  trades=35, win=60.00%, profit=107.32296949
  remove positive singleton symbols: trades=31, win=54.84%, profit=94.95916082

block 11-20:
  base: trades=43, win=62.79%, profit=184.26477390
  remove top3 symbols: trades=36, win=61.11%, profit=127.71674998
  remove top5 symbols: trades=32, win=59.38%, profit=98.66855855
  remove top5 trades:  trades=38, win=57.89%, profit=113.16617557
  remove positive singleton symbols: trades=33, win=51.52%, profit=79.83937793

block 13-22:
  base: trades=42, win=66.67%, profit=192.38249620
  remove top3 symbols: trades=35, win=62.86%, profit=132.53867895
  remove top5 symbols: trades=30, win=63.33%, profit=103.02859532
  remove top5 trades:  trades=37, win=62.16%, profit=121.28389787
  remove positive singleton symbols: trades=32, win=56.25%, profit=87.95710023
```

Single-symbol and single-trade concentration is acceptable for a small sample:
the largest positive symbol is `H-USDT-SWAP` with `22.01741309U`; removing it
still leaves `39` trades, `64.10%` win rate, and `170.36508311U` profit. The
largest single winning trade contributes `19.17030790U` (`9.96%` of total
profit), top three winning trades contribute `47.03952737U` (`24.45%`), and top
five winning trades contribute `71.09859833U` (`36.96%`). The main remaining
risk cluster is a five-loss run from `2026-06-14 11:45:00` to
`2026-06-16 11:45:00`, totaling `-17.10629145U`.

The `13-22` improvement over `11-20` is not simple set subtraction because the
symbol-isolated replay lock changes which neighboring signal gets used. In the
current closed trades, `13-22` removes four trades totaling `-14.38489611U` and
adds three trades totaling `-6.56828872U`, a net improvement of about `7.82U`:

```text
removed versus 11-20:
  H breakout_previous_high rank=10 profit=-2.18019217
  BSB reclaim_ema rank=22 profit=-4.10332529
  UNI breakout_previous_high rank=11 profit=-4.42248384
  ASTER reclaim_ema rank=21 profit=-3.67889481

added versus 11-20:
  H reclaim_ema rank=11 profit=+0.81448625
  BSB reclaim_ema rank=23 profit=-4.10332529
  UNI reclaim_ema rank=11 profit=-3.27944968
```

`stop_reentry_mode=breakout_reclaim` and late profit-protection settings
(`2.2/0.0`, `2.6/0.0`, `2.6/0.3`) were no-ops on this candidate: all reproduced
the same `42` trades, `66.66666666666666%` win rate, `192.38249622U` profit,
Sharpe `4.426683134074767`, and `3.82000000%` max drawdown.

Current practical decision: keep `5.5/0.0375/2.7R +
entry_trigger_rank_blocklist=reclaim_ema:13-22` as the strongest complex
research candidate, with `reclaim_ema:11-20` retained as the more conservative
explanation-first candidate. The `13-22` result is strong enough to paper
observe, but it is still a sample-derived middle-rank filter and must not be
used as a production-live rule without forward paper evidence.

## Episode 专用参数扫描 - 2026-06-22

本轮重新按 `--event-source episodes` 做专用扫描。核心结论是：
`raw_state` 下最强的 `reclaim_ema:13-22` 不是 episode 专用候选；它在
episode 口径下只剩 `2` 笔交易，不能作为独立机会回测结论。episode
样本里主要有效触发来自 `breakout_previous_high`，但 `pullback_hold_ema`
和少量 `reclaim_ema` 在当前样本中不是负贡献，直接做 breakout-only 会
降低总收益。

基线提醒：

```text
default episodes:
  raw_candidate_events=125
  stage_counts: raw=125, trend_pass=61, entry_pass=7
  2R equity: trades=7, win_rate=57.14285714285714%, max_dd=6.04575100, profit=14.42219800

raw_state research best reclaim_ema:13-22 on episodes:
  raw_candidate_events=31
  entry_pass=3, after trigger rank block=2
  2.7R equity: trades=2, win_rate=100%, max_dd=0, profit=20.11000000
```

更适合 episode 的方向是放宽 episode 入场过滤，而不是继续使用 raw_state
的中段 `reclaim_ema` blocklist。小网格扫描覆盖：
`min_delta_rank=5/8/10/12`、`max_new_rank=20/30/40/50`、
`entry_max_distance_pct=4.0/5.5/7.0`，固定
`stop_loss_pct=0.03`、`entry_min_volume_ratio=0.8`、
`chase_top_rank=5`、`chase_price_change_pct=80`，并扫
`target_r=2.0/2.2/2.4/2.6/2.7`。

推荐 episode research 候选：

```text
balanced episode candidate:
  event_source=episodes
  stop_loss_pct=0.03
  target_r=2.4
  entry_max_distance_pct=7.0
  entry_min_volume_ratio=0.8
  min_delta_rank=5
  max_new_rank=30
  chase_top_rank=5
  chase_price_change_pct=80

  raw_candidate_events=248
  stage_counts: raw=248, trend_pass=111, entry_pass=25
  equity: trades=20, win_rate=70.0%, sharpe=3.7954335353236894,
          max_dd=6.04575100, profit=81.33033807
  early/late: trades=10/10, win_rate=70.0%/70.0%,
              profit=40.70000000/40.48110900
  q1/q2/q3/q4: trades=6/4/6/4,
               win_rate=66.67%/75.0%/50.0%/100.0%,
               profit=22.38000000/18.32000000/12.18000000/28.52000000
  remove top1 positive: remaining_trades=17, win_rate=70.58823529411765%,
                        profit=70.08535800
  remove top3 positives: remaining_trades=15, win_rate=66.66666666666666%,
                         profit=55.82535800
  remove top5 positives: remaining_trades=13, win_rate=61.53846153846154%,
                         profit=41.56535800
```

更保守、解释更简单的候选：

```text
conservative episode candidate:
  event_source=episodes
  stop_loss_pct=0.03
  target_r=2.4
  entry_max_distance_pct=7.0
  entry_min_volume_ratio=0.8
  min_delta_rank=5
  max_new_rank=20
  chase_top_rank=5
  chase_price_change_pct=80

  raw_candidate_events=158
  stage_counts: raw=158, trend_pass=81, entry_pass=17
  equity: trades=13, win_rate=84.61538461538461%,
          sharpe=5.234284636997629, max_dd=3.07000000,
          profit=71.85221800
  early/late: trades=7/6, win_rate=85.71428571428571%/83.33333333333334%,
              profit=39.71000000/32.36110900
```

保守候选做 breakout-only 后仍成立，但收益下降：

```text
max_new_rank=20 + breakout_previous_high only:
  target_r=2.4
  trades=12, win_rate=83.33333333333334%,
  sharpe=4.737642092344642, max_dd=3.07000000,
  profit=64.94110900
```

`max_new_rank=30` 上直接屏蔽 `breakout_previous_high:21-30` 会提升
收益和胜率，但它是更强的样本内过滤，当前只保留为研究观察，不作为推荐
默认：

```text
block breakout_previous_high:21-30:
  target_r=2.4
  trades=14, win_rate=85.71428571428571%,
  sharpe=5.730542875847696, max_dd=3.07000000,
  profit=78.98221800
```

保护止盈 sanity check：

```text
balanced candidate + profit_protect_after_r=1.8 + profit_protect_stop_r=0.8:
  target_r=2.4
  trades=20, win_rate=75.0%, sharpe=3.501183519467381,
  max_dd=6.04575100, profit=67.53033807
```

保护止盈提高 resolved/complete 胜率，但牺牲 `13.80U` 左右利润；当前不作
episode 默认优化，只作为“更平滑但收益更低”的风控备选。

当前 practical decision：

- episode 专用首选研究候选：`min_delta_rank=5 / max_new_rank=30 /
  entry_max_distance_pct=7.0 / entry_min_volume_ratio=0.8 / 0.03SL / 2.4R`。
- episode 专用保守候选：`max_new_rank=20` 版本，利润略低但回撤和胜率更好。
- 不把 raw_state 的 `reclaim_ema:13-22` 升级为 episode 结论；它仍保留为
  raw_state/paper observation research candidate。
- 由于 episode 当前最多只有 `20` 笔候选交易级别，本轮结论必须继续标记为
  research，需要 forward paper evidence 后再考虑生产默认。

## Episode Forward Paper Observation - 2026-06-22

新增 episode 专用 forward paper observation 预设：

```text
paper_strategy_preset=research_episode_momentum_03sl_24r_rank5_30_v1
entry_rule_version=rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1
event_source=episodes
entry_trigger_filter_version=unfiltered_v1
stop_reentry_mode=off
stop_loss_pct=0.03
target_r=2.4
entry_max_distance_pct=7.0
entry_min_volume_ratio=0.8
min_delta_rank=5
max_new_rank=30
chase_top_rank=5
chase_price_change_pct=80
```

这不是生产默认。该 preset 用于前向纸面观察，把样本内 episode 候选固定成
可审计的 entry rule version；生产默认仍保持当前 production preset。preset
会锁定 `--event-source` 和核心调参项，避免用同一个研究名称混入
`raw_events` / `raw_state` 或手工调参结果。

本地只读干跑可以先用 `jsonl` sink 复核口径，不向 Web 写入：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source episodes \
  --paper-outcome-sink jsonl \
  --paper-outcome-entry-rule-version rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1 \
  --stop-loss-pct 0.03 \
  --target-rs 2.4 \
  --entry-max-distance-pct 7.0 \
  --entry-min-volume-ratio 0.8 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 5 \
  --max-new-rank 30 \
  --chase-top-rank 5 \
  --chase-price-change-pct 80 \
  --entry-trigger-allowlist all
```

2026-06-22 local dry-run result:

```text
raw_candidate_events=248
stage_counts: raw=248, trend_pass=111, entry_pass=25
framework_equity_result: trades=20, win_rate=70.0%, sharpe=3.7954335353236894,
  max_drawdown_pct=6.04575100, total_profit=81.33033807
early/late: trades=10/10, win_rate=70.0%/70.0%,
  profit=40.70000000/40.48110900
paper_outcomes_generated=40
```

Forward observation 写入 Web 时使用：

```bash
market_velocity_paper_observation \
  --paper-strategy-preset research_episode_momentum_03sl_24r_rank5_30_v1
```

需要配置 `QUANT_CORE_DATABASE_URL`、`RUST_QUAN_WEB_BASE_URL` 或
`QUANT_WEB_BASE_URL`、以及 `EXECUTION_EVENT_SECRET` /
`RUST_QUAN_WEB_INTERNAL_SECRET` / `ALPHA_EXECUTION_INTERNAL_SECRET`。该命令仍是
observation-only：Core 不下单、不撤单、不平仓；Web 如果返回
`generated_execution_task_count != 0`，CLI 会报错退出。

## Episode Backtest Detail - 2026-06-22

Vegas 回测会写 legacy `back_test_log` / `back_test_detail`，所以可以在
Admin 或 SQL 中查看每次开仓和平仓。动量 episode 回测此前是 CLI research
路径：默认只打印汇总，`--equity-trade-report` 只把单笔交易输出到 stdout，
没有落 legacy 明细表。

现在可以在一次性 `market_velocity_event_backtest` 中显式加
`--save-backtest-detail`。该开关默认关闭，不属于
`market_velocity_paper_observation` forward loop，避免观察任务重复写同一批历史
回测明细。每个 `target_r` 会写一条 `back_test_log`，每笔交易写两条
`back_test_detail`：

- `option_type=long, full_close=false`：开仓记录。
- `option_type=close, full_close=true`：平仓记录。

`signal_value` 中保留 `rank_event_id`、`entry_trigger`、`new_rank`、
`delta_rank`、`price_change_pct`、`target_r` 和 `entry_rule_version`，便于回看每笔
交易对应的动量事件。

确认允许写本地 `quant_core` 后运行：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source episodes \
  --paper-outcome-entry-rule-version rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1 \
  --stop-loss-pct 0.03 \
  --target-rs 2.4 \
  --entry-max-distance-pct 7.0 \
  --entry-min-volume-ratio 0.8 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 5 \
  --max-new-rank 30 \
  --chase-top-rank 5 \
  --chase-price-change-pct 80 \
  --entry-trigger-allowlist all \
  --save-backtest-detail
```

查询最近写入的动量 episode 回测：

```sql
select id,
       strategy_type,
       inst_type,
       time,
       win_rate,
       open_positions_num,
       final_fund,
       profit,
       created_at
from back_test_log
where strategy_type = 'market_velocity_episode'
order by id desc
limit 5;
```

查看每笔开平仓：

```sql
select id,
       inst_id,
       option_type,
       open_position_time,
       close_position_time,
       open_price,
       close_price,
       quantity,
       profit_loss,
       full_close,
       close_type,
       signal_value
from back_test_detail
where back_test_id = <back_test_log_id>
order by open_position_time, id;
```

## Episode FVG And Early Exit Experiment - 2026-06-22

本轮在 episode 首选 research 参数上继续测试两个方向：

- 开启已有 FVG 入场过滤：`--fvg-entry-mode 15m_to_1h` 和 `1h_to_4h`。
- 新增早退研究参数：`--early-exit-no-profit-candles N`。含义是开仓后的第
  `N` 根 15m K 线收盘价仍未高于开仓价，则用该根 K 线收盘价平多仓；不包含
  开仓那根 K 线。该参数只用于 `market_velocity_event_backtest`，不属于
  `market_velocity_paper_observation` preset。

实现边界：

- `simulate_trade` 和 framework equity replay 都支持早退。
- framework 明细只在真实反向信号平仓时把 `close_type` 映射为
  `early_exit_no_profit`；如果同一根 K 线已经由止损/止盈先平仓，保留原风控
  close_type，例如 `Signal_Kline_Stop_Loss`。
- `market_velocity_paper_observation` 禁止该参数，避免 forward observation
  混入临时研究口径。

测试命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source episodes \
  --paper-outcome-entry-rule-version rank_radar_4h_trend_15m_episode_research_03sl_24r_rank5_30_v1 \
  --stop-loss-pct 0.03 \
  --target-rs 2.4 \
  --entry-max-distance-pct 7.0 \
  --entry-min-volume-ratio 0.8 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 5 \
  --max-new-rank 30 \
  --chase-top-rank 5 \
  --chase-price-change-pct 80 \
  --entry-trigger-allowlist all \
  --fvg-entry-mode off \
  --early-exit-no-profit-candles 2 \
  --equity-report \
  --equity-trade-report \
  --min-trades 1
```

结果矩阵：

| FVG 模式 | 早退 | 交易数 | 胜率 | Sharpe | 最大回撤 | 总收益 | early-exit 平仓 |
|---|---:|---:|---:|---:|---:|---:|---:|
| off | 关闭 | 20 | 70.0% | 3.7954 | 6.045751% | 81.33033807U | 0 |
| off | 1 根 | 20 | 42.1053% | 3.0637 | 1.418650% | 51.98018016U | 12 |
| off | 2 根 | 20 | 50.0% | 3.8482 | 1.673744% | 66.86798181U | 10 |
| 15m_to_1h | 关闭 | 10 | 50.0% | 1.1941 | 3.070000% | 20.30000000U | 0 |
| 15m_to_1h | 1 根 | 10 | 37.5% | 1.2372 | 3.070000% | 15.50346853U | 6 |
| 15m_to_1h | 2 根 | 10 | 37.5% | 1.1961 | 3.070000% | 15.09025361U | 6 |
| 1h_to_4h | 关闭 | 5 | 40.0% | 0.4042 | 3.070000% | 5.05000000U | 0 |
| 1h_to_4h | 1 根 | 5 | 0.0% | -2.8833 | 1.632714% | -3.61118471U | 5 |
| 1h_to_4h | 2 根 | 5 | 0.0% | -3.0877 | 1.632714% | -4.03888891U | 5 |

结论：

- 现有 FVG 模式过于收窄样本，收益和 Sharpe 均明显弱于 FVG 关闭；当前不建议把
  FVG 开进 episode forward paper observation。
- 早退对 XAG 这类长期拖到止损的交易有效。例如 event `4280` 从原始
  `-3.07U` 止损，变为 1 根早退 `-0.47784107U` 或 2 根早退
  `-0.54362189U`。
- 早退也会砍掉部分后续反弹盈利单。`2` 根早退比 `1` 根更平衡，但样本内总收益
  仍低于不开早退。因此它只能作为风控研究备选，不进入当前 forward paper
  observation 默认 preset。

## Episode Runner Detail Backtest - 2026-06-22

本轮补齐了 framework 回测明细中的分批退出记录：

- 默认全平口径仍保持 1 条 `long` + 1 条 `close`。
- 开启 runner 后，命中基础止盈时先写一条部分平仓 close：
  `close_type=runner_base_target_hit`，`full_close=false`。
- 尾仓触发 runner 目标或 runner stop 时再写最终 close：
  `full_close=true`，`close_type=runner_target_hit` 或 `runner_stop_hit`。
- 每条 close 明细记录自己的 `close_position_time`、`close_price`、`quantity`、
  `profit_loss`，并在 `signal_value` 中写入 `exit_reason`、`leg_result_r`、
  `runner_target_r`、`runner_fraction`、`runner_stop_r`。

写库回测：

| back_test_log_id | runner | 交易数 | 明细行 | 部分平仓行 | 胜率 | Sharpe | 最大回撤 | 总收益 |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 19 | 关闭 | 20 | 40 | 0 | 70.0% | 3.7954 | 6.045751% | 81.33033807U |
| 20 | 8R / 30% / 0R stop | 20 | 54 | 14 | 70.0% | 3.7576 | 6.140000% | 108.88600000U |

`back_test_log_id=20` 的退出分布：

| close_type | full_close | 行数 | 合计收益 |
|---|---|---:|---:|
| runner_base_target_hit | false | 14 | 69.8740U |
| runner_target_hit | true | 8 | 57.4320U |
| runner_stop_hit | true | 6 | 0.0000U |
| stop_hit | true | 6 | -18.4200U |

典型样例 `BICO-USDT-SWAP`：

| 阶段 | 时间 | 价格 | 数量 | full_close | 收益 | R |
|---|---|---:|---:|---|---:|---:|
| 开仓 | 2026-06-19 18:15:00 | 0.02304 | 4340.27777778 | false | 0 | - |
| 基础止盈 | 2026-06-19 18:30:00 | 0.02469888 | 3038.19444444 | false | 4.9910U | 2.4 |
| 尾仓止盈 | 2026-06-20 00:00:00 | 0.02856960 | 1302.08333333 | true | 7.1790U | 8.0 |

runner 目标扫描，均为基础 `2.4R` 止盈、尾仓 `30%`、runner stop `0R`：

| runner_target_r | 48h 胜/亏/超时/不完整 | 48h avg R | framework Sharpe | framework 最大回撤 | framework 总收益 |
|---:|---|---:|---:|---:|---:|
| 6R | 12 / 3 / 5 / 0 | 2.032859 | 3.9450 | 6.140000% | 105.24400000U |
| 8R | 12 / 3 / 5 / 0 | 2.092859 | 3.7576 | 6.140000% | 108.88600000U |
| 10R | 11 / 3 / 6 / 0 | 2.305281 | 3.7862 | 6.140000% | 123.28600000U |
| 12R | 8 / 3 / 8 / 1 | 2.210719 | 3.4257 | 6.140000% | 105.85397357U |

结论：

- runner 8R 明细口径已能解释 BICO 这类强趋势单：先落袋 70%，再让 30% 尾仓继续吃趋势。
- 样本内 `10R` 账面总收益最高，但 48h 胜单减少、超时增加，更依赖后续行情延续。
- 当前更适合作为 forward paper observation 候选的是 `8R / 30% / 0R stop`；比原全平口径多
  `27.55566193U`，回撤只从 `6.045751%` 增至 `6.140000%`。

Forward observation 候选运行方式：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
RUST_QUAN_WEB_BASE_URL=http://127.0.0.1:8000 \
EXECUTION_EVENT_SECRET=<internal-secret> \
cargo run -q -p rust-quant-cli --bin market_velocity_paper_observation -- \
  --paper-strategy-preset research_episode_runner_03sl_24r_8r30_v1
```

持续观察时增加 loop 参数：

```bash
cargo run -q -p rust-quant-cli --bin market_velocity_paper_observation -- \
  --paper-strategy-preset research_episode_runner_03sl_24r_8r30_v1 \
  --loop-interval-seconds 21600
```

该 preset 会写入独立口径：

```text
entry_rule_version=rank_radar_4h_trend_15m_episode_runner_03sl_24r_8r30_v1
event_source=episodes
target_r=2.4
runner_target_r=8.0
runner_fraction=0.3
runner_stop_r=0.0
```

## Result Table Queries

Paper outcomes are written to Web table `market_velocity_paper_outcomes`.

Aggregate the active preset:

```sql
select target_r,
       horizon_hours,
       count(*) as trades,
       count(*) filter (where outcome_status = 'win') as win,
       count(*) filter (where outcome_status = 'loss') as loss,
       count(*) filter (where outcome_status = 'timeout') as timeout,
       count(*) filter (where outcome_status = 'incomplete') as incomplete,
       round((count(*) filter (where outcome_status = 'win')::numeric
         / nullif(count(*) filter (where outcome_status in ('win','loss','flat')), 0)) * 100, 4)
         as resolved_win_rate_pct,
       round(avg(result_r) filter (where outcome_status <> 'incomplete')::numeric, 6)
         as avg_r_complete
from market_velocity_paper_outcomes
where entry_rule_version = 'rank_radar_4h_trend_15m_momentum_03sl_20r_v5'
group by target_r, horizon_hours
order by target_r, horizon_hours;
```

Inspect recent losses:

```sql
select id,
       symbol,
       rank_event_id,
       entry_at,
       entry_trigger,
       target_r,
       horizon_hours,
       outcome_status,
       exit_reason,
       round(result_r::numeric, 6) as result_r
from market_velocity_paper_outcomes
where entry_rule_version = 'rank_radar_4h_trend_15m_momentum_03sl_20r_v5'
  and outcome_status = 'loss'
order by entry_at desc
limit 20;
```

## Signal Retest Low-Frequency Iteration - 2026-06-26

本轮目标是把 Market Velocity 动量策略从“信号后直接追”收敛到更低频、更高胜率的
结构回踩入场，并移除 `new_rank` 作为策略筛选参数。`new_rank` 仍可作为诊断字段输出，
但不再参与 candidate SQL、preset 参数、manifest filters、服务层信号准入或 live handoff
候选池过滤。

最终候选 preset：

```text
research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1
entry_rule_version=rank_radar_4h15m_r0375_15r_sigrt2_d24_34_p5_10_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.0375
target_r=1.5
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
entry_retest_after_signal=true
entry_retest_max_wait_candles=2
entry_retest_tolerance_pct=0.3
entry_retest_min_entry_open_gap_pct=0.0
min_delta_rank=24
max_delta_rank=34
min_price_change_pct=5.0
max_price_change_pct=10.0
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
ignore_entry_signal_updates_while_open=true
```

本地只读复核命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source raw_state \
  --trade-direction long \
  --paper-outcome-entry-rule-version rank_radar_4h15m_r0375_15r_sigrt2_d24_34_p5_10_v1 \
  --stop-loss-pct 0.0375 \
  --target-rs 1.5 \
  --entry-max-distance-pct 5.0 \
  --entry-min-volume-ratio 1.0 \
  --entry-retest-after-signal \
  --entry-retest-max-wait-candles 2 \
  --entry-retest-tolerance-pct 0.3 \
  --entry-retest-min-entry-open-gap-pct 0.0 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 24 \
  --max-delta-rank 34 \
  --min-price-change-pct 5.0 \
  --max-price-change-pct 10.0 \
  --entry-trigger-allowlist breakout_previous_high,reclaim_ema \
  --ignore-entry-signal-updates-while-open \
  --equity-report \
  --equity-split-report \
  --equity-concentration-report \
  --min-trades 10
```

2026-06-26 本地样本结果：

```text
raw_candidate_events=1633
stage_counts: raw=1633, trend_pass=1045, entry_pass=18
24h fixed outcome: trades=14, win=12, loss=1, timeout=1, resolved_win_rate=92.3077%
48h fixed outcome: trades=14, win=12, loss=1, incomplete=1, resolved_win_rate=92.3077%
framework equity: trades=14, win_rate=92.8571%, trade_sharpe=7.0323,
  max_drawdown_pct=3.82000000, total_profit=66.36922179
early split: trades=7, win_rate=100.0%, total_profit=38.88500000
late split: trades=7, win_rate=85.7143%, total_profit=27.48422179
trigger split: breakout_previous_high=11 trades / 81.8182% / 40.32922179,
  reclaim_ema=4 trades / 100.0% / 22.22000000
remove top3 positive symbols: remaining_trades=11, remaining_win_rate=90.9091%,
  remaining_total_profit=49.70422179
remove top5 positive symbols: remaining_trades=9, remaining_win_rate=88.8889%,
  remaining_total_profit=38.59422179
```

对照结论：

- `delta25-50 / 1.5R`：53 trades，framework win_rate `67.9245%`，
  total_profit `134.64027438`，remove top3 后 win_rate `62.2222%`。
- `delta25-39 / 1.5R`：43 trades，framework win_rate `69.7674%`，
  total_profit `115.95597225`，remove top3 后 win_rate `63.8889%`。
- `delta25-34 / 1.5R`：34 trades，framework win_rate `76.4706%`，
  total_profit `112.80006479`，remove top3 后 win_rate `71.4286%`。
- `delta25-34 / 2.0R`：33 trades，framework win_rate `69.6970%`，
  total_profit `127.47825501`，remove top3 后 win_rate `62.9630%`。
- `delta25-34 / 1.5R + retest4 + entry_open_gap>=0`：27 trades，framework win_rate
  `81.4815%`，total_profit `101.79776154`，remove top3 后 win_rate `76.1905%`。
- `delta25-34 / 1.5R + retest1 + entry_open_gap>=0`：16 trades，framework win_rate
  `87.5%`，total_profit `68.41280204`，remove top3 后 win_rate `83.3333%`。
- `delta25-34 / 1.5R + retest2 + entry_open_gap>=0`：20 trades，framework win_rate
  `85.0%`，total_profit `81.56638229`，remove top3 后 win_rate `80.0%`。
- `delta24-34 / 1.5R + retest2 + entry_open_gap>=0`：22 trades，framework win_rate
  `86.3636%`，total_profit `92.67638229`，remove top3 后 win_rate `82.3529%`，
  remove top5 后 win_rate `80.0%`。
- `delta24-34 / 1.5R + retest2 + entry_open_gap>=0 + price_change_pct<=10`：
  14 trades，framework win_rate `92.8571%`，total_profit `66.36922179`，
  early split `100.0%`，late split `85.7143%`，remove top3 后仍有 11 trades、
  win_rate `90.9091%`，是本轮低频高胜率主候选。
- `delta24-34 / 1.5R + retest2 + entry_open_gap>=0 + price_change_pct<=15`：
  19 trades，framework win_rate `84.2105%`，early split 降到 `77.7778%`，
  remove top3 后 win_rate `80.0%`；真实 SQL cap 扫描劣于 cap10，不采用。
- `delta23-34 / 1.5R + retest2 + entry_open_gap>=0`：23 trades，framework win_rate
  `86.9565%`，total_profit `98.23138229`，remove top3 后 win_rate `83.3333%`，
  remove top5 后 win_rate `81.25%`；作为 challenger，不作为主 preset。
- `delta22-34 / 1.5R + retest2 + entry_open_gap>=0`：25 trades，framework win_rate
  `84.0%`，total_profit `99.96638229`，remove top3 后 win_rate `80.0%`。
- `delta24-35/36 / 1.5R + retest2 + entry_open_gap>=0`：23 trades，framework win_rate
  `82.6087%`，early split 降到 `75.0%/76.9231%`，remove top3 后 win_rate
  `77.7778%`，不采用。
- `delta25-34 / 1.8R + retest2 + entry_open_gap>=0`：20 trades，framework win_rate
  `75.0%`，early split win_rate `60.0%`，remove top3 后 win_rate `66.6667%`。
- `delta25-34 / 2.0R + retest2 + entry_open_gap>=0`：20 trades，framework win_rate
  `75.0%`，early split win_rate `60.0%`，remove top3 后 win_rate `66.6667%`。
- `delta25-34 / 1.8R + retest4 + entry_open_gap>=0`：27 trades，framework win_rate
  `74.0741%`，early split win_rate `61.5385%`，remove top3 后 win_rate `68.1818%`。
- `delta25-34 / 2.0R + retest4 + entry_open_gap>=0`：27 trades，framework win_rate
  `74.0741%`，early split win_rate `61.5385%`，remove top3 后 win_rate `68.1818%`。

当前策略定位是低频观察，因此优先采用 `delta24-34 / 1.5R + retest2 +
entry_open_gap>=0 + price_change_pct<=10`：相对无 cap 的 22 笔版本，交易数降到 14 笔，
但胜率从 `86.3636%` 提升到 `92.8571%`，TOP3 去除后仍有 11 笔且 win_rate
`90.9091%`。`price_change_pct<=12` 与 `<=10` 得到相同交易集合，最终用整数 10
作为更清晰的“不追高”上限。`price_change_pct<=15` 虽然看似更宽，但真实 SQL 扫描会
重新选择部分同 15m bucket 内事件，胜率反而降到 `84.2105%`，因此不固化。`delta23-34`
样本内指标略高，但只比无 cap 候选多 1 笔，且 lower-bound 进一步放宽更容易吸收样本
拟合噪声，因此保留为 challenger，不固化为主 preset。`1.8R` / `2.0R` 在 `retest2`
下会把 early split 和 TOP3 去除后的胜率压到约 `60%/66.7%`，不符合本轮“低频高胜率
优先”的目标。

`entry_open_gap>=0` 的含义是回踩确认后，下一根 15m 开盘不能低于确认 K 收盘价，用来
过滤“确认后立刻走弱”的进场，不依赖 `new_rank`。`retest2` 的含义是信号后最多等待
2 根 15m K 线完成回踩确认；逐笔逆向分析中，`retest4` 的剩余亏损更集中在较晚完成
确认的样本上，因此本轮把最大等待从 4 收紧到 2。UTC `12-17` 与周末样本表现偏弱，
但这类时间过滤样本量不足、容易过拟合，本轮不固化为策略参数。

### Retest2 loss attribution follow-up - 2026-06-26

补跑逐笔报告：

```text
/tmp/mv_retest2_delta_24_34_trade_report.txt
```

`delta24-34 / retest2 / 1.5R` 无 cap 版本的 3 笔 framework 亏损均来自
`breakout_previous_high+retest_after_signal`，其中两笔属于 `price_change_pct > 15`
的追高样本：

```text
BILL-USDT-SWAP, event=1781287, delta=33, price_change_pct=15.37260468, latency=29.7m
AI-USDT-SWAP, event=2175842, delta=30, price_change_pct=15.01925546, latency=29.8m
IP-USDT-SWAP, event=2638659, delta=24, price_change_pct=5.64404432, latency=4.0m
```

加入 `price_change_pct<=10` 后，BILL 和 AI 被过滤；当前 cap10 候选只剩
`IP-USDT-SWAP` 一笔 framework 亏损，且回测报告时间已带 `+08:00` 时区后缀，避免
把上海时间 open_time 误读成 UTC 后认为回踩入场延迟 8 小时。

用当前代码的 15m `SMA/EMA/previous_volume_avg` 逻辑重建 signal / confirmation /
entry 后，没有找到足够干净的下一条入场过滤：

- `retest1`：16 trades，framework win_rate `87.5%`，但仍亏 BILL/IP，且 early/late
  各只有 8 笔，不满足当前主 preset 的样本稳健性要求。
- 更低目标 `1.0R/1.2R/1.3R`：framework win_rate 没有超过 `1.5R`，但总利润从
  `81.56638229` 降到 `55.32667960/64.24249800/70.68676050`，因此不替换 `1.5R`。
- 提高 `entry_min_volume_ratio`：`1.2` 直接降到 14 trades、win_rate `71.4286%`；
  `1.5/2.0` 只剩 5/4 笔，样本量不足，不固化。
- 15m self FVG 入场不替代 retest2：`min_delta=20 / wait2` 只有 10 trades、
  win_rate `60.0%`，remove top3 后降到 `42.8571%`；wait4/8/12/24 都在
  `45%-49%` 附近，不符合低频高胜率目标。
- reclaim-only 不替代：`min_delta=5 / max_delta=34/39` 只有 15 trades、
  win_rate `60.0%`，remove top3 后约 `50.0%`，会破坏高胜率属性。
- `breakout_previous_high` 的胜率低于 `reclaim_ema`，但 reclaim 当前只有 4 笔；
  直接只保留 reclaim 会把策略压成极低样本观察，不适合替代主 preset。

当前结论：固化
`research_momentum_0375sl_15r_signal_retest2_delta24_34_pchg5_10_v1`，不要再把
`new_rank` 当作参数，也不要在单个剩余亏损样本上继续追加时间、成交量或中段涨幅过滤。
下一轮优先等待 forward observation 或扩展历史样本，再判断是否需要做 trigger-specific
的 breakout / reclaim 入场分流。

### Removed live `new_rank` gate - 2026-06-26

`new_rank` 参数已从实时信号链路移除：

- `MarketVelocityStrategySignalConfig` 不再读取 `MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK`，
  也不再解析策略配置 JSON 中的 `max_new_rank` / `chasing_risk_top_rank` /
  `chasing_risk_price_change_pct`。
- 服务层不再用 `new_rank <= max_new_rank` 或 top-rank chasing bucket 阻断信号。
  `new_rank` 仍保留在 payload 顶层，作为后续复盘的原始事实字段。
- 不追高改由 `max_price_change_pct` 表达；服务层可从策略配置 JSON 或
  `MARKET_VELOCITY_SIGNAL_MAX_PRICE_CHANGE_PCT` 读取该上限，并在入场前阻断超过上限的
  信号。
- `market_velocity_live_handoff` 候选 SQL 不再按 `new_rank` 过滤，只保留
  `delta_rank`、方向、价格、交易所和去重/limit 边界。
- `docker-compose.deploy.yml` 不再暴露 `MARKET_VELOCITY_SIGNAL_MAX_NEW_RANK`。

### Reclaim passive FVG wait5 preset - 2026-06-26

在继续做 15m passive impulse FVG 回踩研究时，逐笔检查显示真正需要收紧的不是
`fill_pct` 或 `min_wait`，而是过晚才回填成交的 stale fill。因此把
`fvg_max_wait_candles` 从 `24` 收到更紧的区间后，再围绕更简单的 round-number 区间做只读重放。

本轮结论：

- `delta12-24 + wait8` 会引入新的 `H-USDT-SWAP` 亏损，不采用。
- `delta20-40 + wait8 + pchg5-12` 与 `pchg5-10` 在当前样本上 trade set 完全一致，
  但边界更自然，可替代 `cap10`。
- `cap15` 和 no-cap 都会额外引入 `TAO` 亏损，把 `2R` 从
  `4 trades / 100% / 29.72` 降回 `5 trades / 80% / 25.9`，不采用。
- `fvg_max_wait_candles=1/2` 只保留 `AMD`、`CHIP` 两笔；
  `3/4` 会补回 `BASED` 但仍丢掉 `ARM`；`5-11` 保持同一组
  `BASED/AMD/CHIP/ARM`；`12` 开始重新放进 `HMSTR` 亏损单。
  因此最终选择最小稳定值 `wait5`，而不是把 `wait8` 当成神奇参数。

最终固化的 research preset：

```text
research_momentum_0375sl_20r_reclaim_fvgwait5_delta20_40_pchg5_12_v1
entry_rule_version=rank_radar_4h15m_r0375_20r_rcm_fvg5_d20_40_p5_12_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.0375
target_r=2.0
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=20
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=12.0
entry_trigger_allowlist=reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=5
ignore_entry_signal_updates_while_open=true
```

本地只读复核命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source raw_state \
  --trade-direction long \
  --paper-outcome-entry-rule-version rank_radar_4h15m_r0375_20r_rcm_fvg5_d20_40_p5_12_v1 \
  --stop-loss-pct 0.0375 \
  --target-rs 2.0,3.0 \
  --entry-max-distance-pct 5.0 \
  --entry-min-volume-ratio 1.0 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 20 \
  --max-delta-rank 40 \
  --min-price-change-pct 5.0 \
  --max-price-change-pct 12.0 \
  --entry-trigger-allowlist reclaim_ema \
  --ignore-entry-signal-updates-while-open \
  --fvg-entry-mode 15m_impulse_retrace \
  --fvg-max-wait-candles 5 \
  --equity-report \
  --equity-concentration-report \
  --equity-trade-report \
  --min-trades 1
```

当前口径下，`2R` 的 framework result 为 `4 trades / win_rate 100% / total_profit 29.72`；
remove top1 后 `22.29`，remove top3 后仍有 `7.43`。`3R` 虽然也有
`4 trades / 75% / 29.72`，但对低频策略目标，当前更推荐先用 `2R` 观察 forward sample。

### Breakout + reclaim delayed FVG preset - 2026-06-26

继续沿着“第一次短时再突破是假，真正机会在回踩进 15m FVG 后”的思路做结构归因后，
先对比了两条现成路径：

- `15m_self_after_signal`：交易数会显著增多，但在 `delta20-40 + pchg5-12`
  口径下，`reclaim_ema` 单独时只有 `6-12` 笔且胜率 `33%-50%`，
  `breakout_previous_high` 单独时虽然可到 `35-48` 笔，但胜率也只有
  `34%-44%`，不适合当前“低频高胜率”的目标。
- `15m_impulse_retrace`：`breakout_previous_high` 单独仍然偏弱，但只要和
  `reclaim_ema` 合并，再要求延迟回踩进 FVG，结果会明显改善。

对 `breakout_previous_high,reclaim_ema + m15_impulse_retrace` 再扫
`fvg_max_wait_candles=5..12` 后，结论是：

- `wait5`：`2R = 6 trades / 66.67% / 22.08`
- `wait6-7`：补进 `INJ` 盈利单，`2R = 7 trades / 71.43% / 29.51`
- `wait8-9`：再补进 `USELESS` 盈利单，`2R = 8 trades / 75% / 36.94`
- `wait10-11`：再加入 `APR` 胜单和 `ORDI` 亏单，`2R = 10 trades / 70% / 40.55`
- `wait12`：重新放进 `HMSTR`、`EDEN` 等亏损样本，退化成
  `2R = 12 trades / 58.33% / 32.91`

在 `wait8` 和 `wait10` 之间，虽然 `wait8` 胜率更高，但 `wait10` 的
trade set 更分散：`remove top5 positive symbols` 后仍保留 `3.40` 利润，
而 `wait8` 在同口径下已经回到 `-0.21`。因此这里优先保留 `wait10` 作为
更稳健、也更 round-number 的候选。

最终固化的 research preset：

```text
research_momentum_0375sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1
entry_rule_version=rank_radar_4h15m_r0375_20r_brk_rcm_fvg10_d20_40_p5_12_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.0375
target_r=2.0
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=20
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=12.0
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=10
ignore_entry_signal_updates_while_open=true
```

本地只读复核命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source raw_state \
  --trade-direction long \
  --paper-outcome-entry-rule-version rank_radar_4h15m_r0375_20r_brk_rcm_fvg10_d20_40_p5_12_v1 \
  --stop-loss-pct 0.0375 \
  --target-rs 2.0,3.0 \
  --entry-max-distance-pct 5.0 \
  --entry-min-volume-ratio 1.0 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 20 \
  --max-delta-rank 40 \
  --min-price-change-pct 5.0 \
  --max-price-change-pct 12.0 \
  --entry-trigger-allowlist breakout_previous_high,reclaim_ema \
  --ignore-entry-signal-updates-while-open \
  --fvg-entry-mode 15m_impulse_retrace \
  --fvg-max-wait-candles 10 \
  --equity-report \
  --equity-concentration-report \
  --equity-trade-report \
  --min-trades 1
```

当前口径下，`2R` 为 `10 trades / win_rate 70% / total_profit 40.55`；
remove top1 后 `33.12`，remove top3 后 `18.26`，remove top5 后仍有 `3.40`。
这说明“突破信号本身不是问题，问题在于必须配合更晚的 FVG 回踩入场”。

### Breakout + reclaim delayed FVG 0.04SL preset - 2026-06-26

在上面这条 `breakout_previous_high,reclaim_ema + m15_impulse_retrace + wait10`
结构确认后，又继续对 `stop_loss × target_r` 做了邻域扫描。目标不是继续找更复杂的小数，
而是确认哪一段是稳定平台，避免把偶然点位误当最优。

粗网格结果：

- `SL=0.0325/0.035`：无论 `target_r=1.8..2.3`，胜率基本都卡在 `60%`
- `SL=0.0375`：`target_r=1.8..2.1` 升到 `70%`，其中 `2.1R = 43.175`
- `SL=0.04`：`target_r=1.8/1.9/2.0` 都稳定在 `80%`，其中 `2.0R = 55.30`
- `SL=0.04` 之后继续把 `target_r` 提到 `2.1+`，胜率反而掉回 `60%`

随后把 `target_r=2.0` 固定，只扫 stop 边界：

- `SL=0.038`：`10 trades / 80% / 52.50`
- `SL=0.039`：`10 trades / 80% / 53.90`
- `SL=0.040`：`10 trades / 80% / 55.30`
- `SL=0.041`：`10 trades / 80% / 56.70`
- `SL=0.0425`：直接退化成 `10 trades / 60% / 33.30`
- `SL=0.045`：`10 trades / 60% / 35.30`
- `SL=0.050`：`10 trades / 60% / 39.30`

这说明 `0.038-0.041` 是同一段稳定平台，而 `0.0425` 开始跨过了结构阈值。
因此这里不追 `0.041` 这种更细小数，而是选 round-number、同时仍处于稳定平台内的 `0.04`。

最终固化的更强 research preset：

```text
research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta20_40_pchg5_12_v1
entry_rule_version=rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d20_40_p5_12_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.04
target_r=2.0
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=20
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=12.0
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=10
ignore_entry_signal_updates_while_open=true
```

本地只读复核命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source raw_state \
  --trade-direction long \
  --paper-outcome-entry-rule-version rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d20_40_p5_12_v1 \
  --stop-loss-pct 0.04 \
  --target-rs 2.0 \
  --entry-max-distance-pct 5.0 \
  --entry-min-volume-ratio 1.0 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 20 \
  --max-delta-rank 40 \
  --min-price-change-pct 5.0 \
  --max-price-change-pct 12.0 \
  --entry-trigger-allowlist breakout_previous_high,reclaim_ema \
  --ignore-entry-signal-updates-while-open \
  --fvg-entry-mode 15m_impulse_retrace \
  --fvg-max-wait-candles 10 \
  --equity-report \
  --equity-concentration-report \
  --equity-trade-report \
  --min-trades 1
```

当前口径下，`2R` 为 `10 trades / win_rate 80% / total_profit 55.30`；
remove top1 后 `47.37`，remove top3 后 `31.51`，remove top5 后仍有 `15.65`。
相对 `0.0375SL` 版本，最大的变化不是样本数增加，而是 `ORDI` 从亏损翻成盈利，
同时整体 trade set 的集中度明显改善。

### Breakout + reclaim delayed FVG 0.04SL delta15-40 preset - 2026-06-26

在 `0.04SL + 2.0R + breakout_previous_high,reclaim_ema + wait10` 稳定后，
又回头扫了 `delta` 与 `price_change` 的边界，目标是确认当前最强解是不是还能在
不破坏高胜率的前提下继续扩样。

`delta` 邻域结果：

- `delta15-35`：`12 trades / 83.33% / 71.16`
- `delta15-40`：`13 trades / 84.62% / 79.09`
- `delta15-45`：与 `delta15-40` 完全一致
- `delta15-50`：退化成 `15 trades / 73.33% / 70.95`
- `delta20-40`：旧基线，`10 trades / 80% / 55.30`

这说明 `delta15-40` 已经把有价值的更低 delta 样本接进来了，而继续放到 `50`
会开始引入更差的样本；`15-45` 既然和 `15-40` 没区别，就没有必要额外放宽。

`price_change` 邻域结果（固定 `delta15-40`）：

- `pchg5-10`：`12 trades / 83.33% / 71.16`
- `pchg5-12`：`13 trades / 84.62% / 79.09`
- `pchg5-15`：`14 trades / 78.57% / 75.02`
- `pchg4-12`：`16 trades / 68.75% / 66.88`
- `pchg6-12`：`11 trades / 81.82% / 63.23`
- `pchg5-no-cap`：`17 trades / 64.71% / 62.16`

因此 `pchg5-12` 仍然是当前最优边界：既没有像 `5-10` 那样错过额外盈利样本，
也没有像 `5-15` / no-cap 那样开始明显牺牲胜率。

最终固化的当前最强 research preset：

```text
research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1
entry_rule_version=rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.04
target_r=2.0
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=15
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=12.0
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=10
ignore_entry_signal_updates_while_open=true
```

本地只读复核命令：

```bash
QUANT_CORE_DATABASE_URL=postgres://.../quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source raw_state \
  --trade-direction long \
  --paper-outcome-entry-rule-version rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_v1 \
  --stop-loss-pct 0.04 \
  --target-rs 2.0 \
  --entry-max-distance-pct 5.0 \
  --entry-min-volume-ratio 1.0 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 15 \
  --max-delta-rank 40 \
  --min-price-change-pct 5.0 \
  --max-price-change-pct 12.0 \
  --entry-trigger-allowlist breakout_previous_high,reclaim_ema \
  --ignore-entry-signal-updates-while-open \
  --fvg-entry-mode 15m_impulse_retrace \
  --fvg-max-wait-candles 10 \
  --equity-report \
  --equity-concentration-report \
  --equity-trade-report \
  --min-trades 1
```

当前口径下，`2R` 为 `13 trades / win_rate 84.62% / total_profit 79.09`；
remove top1 后 `71.16`，remove top3 后 `55.30`，remove top5 后仍有 `39.44`。
相对 `delta20-40` 版本，新增的主要是 `SAHARA`、`HOME`、`UNI` 这类更低 delta 但仍然
符合 delayed FVG 回踩结构的盈利样本。

同一外壳下把 FVG 入场模式做 A/B 后，结论也比较明确：

- `fvg_entry_mode=off`：`133 trades / 40.60% / 99.43`
- `fvg_entry_mode=15m_self_after_signal wait5`：`52 trades / 36.54% / 13.20`
- `fvg_entry_mode=15m_self_after_signal wait10`：`81 trades / 37.04% / 23.38`
- `fvg_entry_mode=15m_impulse_retrace wait5`：`7 trades / 71.43% / 31.51`
- `fvg_entry_mode=15m_impulse_retrace wait10`：`13 trades / 84.62% / 79.09`

所以“15 分钟自己的 FVG”在当前实现口径下不是更优入口，反而会明显放宽样本，
把胜率压到 `40%` 以下。相反，先允许第一次突破失败，再等更晚出现的
`15m_impulse_retrace` 回踩，才更符合“假突破后回撤到 FVG 底部附近再拉升”的交易
结构。结合现有 FVG 单测全部通过，这一轮没有发现 FVG 回测逻辑明显错配的证据，
当前更像是入口模式本身的优劣差异。

继续把“更靠近 FVG 底部”和“不要太快回踩成交”拆开扫后，结论更细：

- `fvg_impulse_retrace_fill_pct=35/50/65` 三档结果完全一致，都是
  `12 trades / 83.33% / 71.16`
- 基线 `fill_pct=20` 仍然最好：`13 trades / 84.62% / 79.09`
- `fvg_impulse_retrace_min_wait_candles=1`：`15 trades / 80% / 82.95`
- `fvg_impulse_retrace_min_wait_candles=2`：`20 trades / 65% / 73.95`
- `fvg_impulse_retrace_min_wait_candles=3`：`20 trades / 65% / 74.58`

这说明更深的 FVG 填充并没有带来更好结果，反而是“至少等 1 根 15m K”
这个时间约束有效过滤掉了第一次过快的回踩噪音；一旦继续等到 `2-3` 根，
又会开始把后续更宽的追入样本放回来。

因此当前更新后的最强 research preset 为：

```text
research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1
entry_rule_version=rank_radar_4h15m_r04_20r_brk_rcm_fvg10_mw1_d15_40_p5_12_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.04
target_r=2.0
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=15
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=12.0
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=10
fvg_impulse_retrace_fill_pct=20
fvg_impulse_retrace_min_wait_candles=1
ignore_entry_signal_updates_while_open=true
```

本地只读复核结果：

- `2R`：`15 trades / win_rate 80% / total_profit 82.95`
- remove top1：`75.02`
- remove top3：`59.16`
- remove top5：`43.30`

在这条 `minwait1` 主线上继续扫 `target_r` 后，结论也比较直接：

- `1.8R`：`15 trades / 80% / 73.35`
- `1.9R`：`15 trades / 80% / 78.15`
- `2.0R`：`15 trades / 80% / 82.95`
- `2.1R`：`15 trades / 66.67% / 62.95`
- `2.2R`：`15 trades / 66.67% / 66.95`
- `2.3R`：`15 trades / 66.67% / 70.95`
- `2.4R`：`15 trades / 66.67% / 74.95`
- `2.5R`：`15 trades / 66.67% / 78.95`
- `2.6R`：`15 trades / 53.33% / 54.15`

因此它并不是适合继续推高 RR 的壳子。`2.0R` 依然是当前最好的平衡点；
一旦上到 `2.1R+`，胜率会立刻从 `80%` 掉到 `66.67%`。

进一步补扫 `profit_protect` 后，也没有找到比当前基线更强的折中：

- `after=1.0R stop=0.0R`：`15 trades / 83.33% / 71.16`
- `after=1.0R stop=0.5R`：`15 trades / 86.67% / 58.95`
- `after=1.2R stop=0.0R`：`15 trades / 83.33% / 71.16`
- `after=1.2R stop=0.3R`：`15 trades / 86.67% / 67.75`
- `after=1.2R stop=0.5R`：`15 trades / 86.67% / 64.95`
- `after=1.4R stop=0.5R`：与基线一致，`15 trades / 80% / 82.95`
- `after=1.5R stop=0.5R`：与基线一致，`15 trades / 80% / 82.95`
- `after=1.5R stop=1.0R`：`15 trades / 80% / 66.95`

集中度也支持同样结论：

- 基线 `2.0R` remove top5：`43.30`
- `after=1.0R stop=0.0R` remove top5：`31.51`
- `after=1.0R stop=0.5R` remove top5：`19.30`

也就是说，保护止盈确实可以继续抬胜率，但它拿掉的主要是当前样本中最有价值的
尾部盈利腿，并没有改善稳健性。按“低频时优先高胜率，但 RR 不必被过度牺牲”的
口径，当前仍保留 `2.0R + no profit protect` 作为最强主候选。

把 `minwait1` 与上一版 `minwait0` 做逐笔审计后，差异也更清楚了：

- 新增胜单：`SLX +7.93`
- 新增亏单：`BILL -4.07`
- `AMD`、`CHIP` 只是顺延到更晚一根 15m K 线入场，结果没变

也就是说，`minwait1` 的净提升并不是来自大规模结构重塑，而是
`+1 win / +1 loss / 两笔顺延` 的组合，净收益约 `+3.86`。时间切分也支持这个判断：

- `minwait0` early：`7 trades / 85.71% / 43.51`
- `minwait0` late：`6 trades / 83.33% / 35.58`
- `minwait1` early：`8 trades / 87.5% / 51.44`
- `minwait1` late：`7 trades / 71.43% / 31.51`

因此 `minwait1` 是“净改善但仍需继续控制等待窗口”的候选，而不是无条件更稳的版本。

顺着这条线继续扫 `minwait1 × fvg_max_wait_candles` 后，结果很明确：

- `wait5`：`8 trades / 75% / 39.44`
- `wait6`：`9 trades / 77.78% / 47.37`
- `wait7`：`10 trades / 80% / 55.30`
- `wait8`：`11 trades / 81.82% / 63.23`
- `wait9`：`13 trades / 76.92% / 67.09`
- `wait10`：`15 trades / 80% / 82.95`
- `wait11`：`16 trades / 75% / 78.88`
- `wait12`：`18 trades / 66.67% / 70.74`

这说明 `wait10` 在 `minwait1` 结构下不是偶然，而是很清晰的顶点：
更短会错过有效 delayed retrace，更长又会重新放进噪音回踩。

最后再把 `fill_pct` 在 `20` 周围补完邻域扫描：

- `fill_pct=10`：`14 trades / 78.57% / 75.02`
- `fill_pct=15`：`14 trades / 78.57% / 75.02`
- `fill_pct=20`：`15 trades / 80% / 82.95`
- `fill_pct=25`：`15 trades / 80% / 82.95`
- `fill_pct=30`：`15 trades / 80% / 82.95`

因此当前没有证据支持继续把 entry 压到更深 FVG 底部。相反，`20-30`
已经形成稳定平台，说明这部分参数不敏感，不需要继续围绕它做细粒度耦合优化。

补上本地 `quant_web.market_velocity_paper_outcomes` 真实落库验证后，结论又进一步收敛了一层：
replay 最强的 `minwait1`，并不等于 paper outcome 最强。

三条 `0.04SL + 2.0R + delta15-40 + pchg5-12 + 15m impulse retrace` 候选的本地 paper outcome
对比如下：

- `breakout+reclaim + wait10`
  - `24h`：`13 trades / resolved_win_rate 87.50% / avg_r_complete 1.2939`
  - `48h`：`13 trades / resolved_win_rate 83.33% / avg_r_complete 1.5158`
- `breakout+reclaim + wait10 + minwait1`
  - `24h`：`15 trades / resolved_win_rate 80.00% / avg_r_complete 1.1866`
  - `48h`：`15 trades / resolved_win_rate 78.57% / avg_r_complete 1.3798`
- `reclaim-only + wait10`
  - `24h`：`4 trades / resolved_win_rate 100% / avg_r_complete 1.6787`
  - `48h`：`4 trades / resolved_win_rate 100% / avg_r_complete 1.9262`

其中 `reclaim-only` 的 4 笔信号分别是 `BASED`、`AMD`、`CHIP`、`ARM`：

- `BASED`：`24h/48h` 都 hit `2R`
- `CHIP`：`24h/48h` 都 hit `2R`
- `ARM`：`24h timeout`，但 `48h` hit `2R`
- `AMD`：`24h/48h` 都是高 R timeout，`1.51R / 1.70R`

也就是说，`reclaim_ema + 15m FVG` 这条分支本地没有出现任何 `stop_hit`，亏损全来自
`breakout_previous_high` 分支。`minwait1` 在 replay 里多拿到了一笔 `SLX` 胜单，但在
paper outcome 里同时多带入 `BILL` 亏单，最终整体质量反而低于 `minwait0`，更明显低于
`reclaim-only`。

从实时筛选过程看，这个 `reclaim-only` 候选也符合“低频高胜率”的目标函数：
在 `17` 个 entry-pass 候选里，`entry_trigger_allowlist=reclaim_ema` 最终只保留了 `4`
个信号，占比约 `23.5%`。如果按“开仓占比低时优先高胜率、不过度追 RR”的口径，这条线
比 `breakout+reclaim` 更一致。

继续只在 `reclaim-only` 主线上扫 `target_r` 后，结构也比较明确：

- `1.6R`：`24h 3 win + 1 timeout`，`48h 4/4 win`
- `1.7R`：`24h 3 win + 1 timeout`，`48h 4/4 win`
- `1.8R`：`24h 3 win + 1 timeout`，`48h 4/4 win`
- `1.9R`：`24h 2 win + 2 timeout`，`48h 3 win + 1 timeout`
- `2.0R`：`24h 2 win + 2 timeout`，`48h 3 win + 1 timeout`
- `2.1R`：`48h` 已经退化成 `2 win + 1 loss + 1 timeout`

也就是说，`1.8R` 是当前这条低频 reclaim 分支里，仍然保留相对更高 RR、同时又能把
`48h` 的 `AMD` 从 timeout 释放成完成赢单的最高 target。更低的 `1.6/1.7R` 只是继续让
RR 变低，并没有带来更多样本或额外完成度；更高的 `1.9/2.0R` 则会重新回到 `AMD` timeout。

继续沿用户“去掉过耦合数字”的要求，把这条 `1.8R reclaim-only` 主线做 round-number
去耦合后，也得到了一致结果：

- `delta20-40 + pchg5-10` 的 owner-side 结果与当前 `delta15-40 + pchg5-12` 完全相同
- trade set 仍是 `AMD / ARM / BASED / CHIP`
- `24h` 仍是 `3 win + 1 timeout`
- `48h` 仍是 `4/4 win`

这说明 `15-40` 和 `5-12` 在当前主线上并不是必要条件，只是更宽的等价边界。既然
`20-40` 与 `5-10` 可以得到相同结果，就没有必要继续把更细的参数留在 preset 里。

但在这条已经简化过的 `1.8R + delta20-40 + pchg5-10` 主线上，`fvg_max_wait_candles`
重新变得有信息量：

- `wait10`：`24h 4 trades / 3 win / 0 loss / 1 timeout`，`48h 4/4 win`
- `wait12`：`24h 5 trades / 4 win / 0 loss / 1 timeout`，`48h 5/5 win`
- `wait14+`：会继续放进 `ORDI` 胜单，但同时把 `USELESS` 亏损也带回来

因此 `wait12` 是当前这条 reclaim-only 主线上新的甜点位。它不是靠放松风险去换交易数，
而是只新增了一笔 `HMSTR` 的完成赢单，并且没有引入任何 `stop_hit`。

继续把这三个边界样本和 `delta15` 带回来的 `EDGE` 逐笔对齐后，`wait12` 的结论更稳：

- `HMSTR`：`detected_at=2026-06-07 05:30:24+00`，`entry_ts=2026-06-07 08:15:00+00`，
  延迟 `164.59m = 10.97` 根 `15m`，刚好落在 `wait12` 内，是这一轮新增的干净赢单。
- `ORDI`：`detected_at=2026-06-11 09:04:05+00`，`entry_ts=2026-06-11 12:15:00+00`，
  延迟 `190.91m = 12.73` 根 `15m`，只有把窗口放到 `14` 才会进来。
- `USELESS`：`detected_at=2026-06-13 22:30:12+00`，`entry_ts=2026-06-14 01:45:00+00`，
  延迟 `194.79m = 12.99` 根 `15m`，和 `ORDI` 几乎是同一档晚填充样本，但结果是 `stop_hit`。
- `EDGE`：`detected_at=2026-06-13 08:01:12+00`，`entry_ts=2026-06-13 10:30:00+00`，
  延迟 `148.79m = 9.92` 根 `15m`，问题不在等待窗口，而在它的 `delta_rank=16`；这正是
  `min_delta_rank` 从 `20` 放到 `15` 后会立刻引入的亏损样本。

也就是说，`wait12 -> wait14` 的边际扩张并不是“再多等两根就会多拿一个稳定赢单”，而是会
把 `ORDI` 和 `USELESS` 这一对几乎同延迟的样本一起带进来；单靠时间窗口本身没有干净分界。
因此当前最小、最稳的结论仍然是保留 `wait12 + delta20-40`，不要为了多拿 `ORDI` 再次放宽。

顺手也把这版实现的回测逻辑重新核了一遍。聚焦 `market_velocity_event_backtest` 的
`raw_state -> evaluate_events -> m15_impulse_retrace` 主链路后，没有发现新的明显逻辑错误：

- `raw_state` 仍然是按 `symbol + 15m bucket` 去重，避免同一根扫描 candle 重复开仓。
- `reclaim_ema + m15_impulse_retrace` 仍然在原始 `15m` K 线上找 signal 前最近未回补的 impulse gap，
  然后等后续 candle 回填到 lower-band 附近再成交。
- 当前 entry price 仍然是“回补 candle 触达 lower-band 的限价成交”，不是下一根开盘追单；
  这是一种偏保守的 `FVG` 挂单建模假设，不是这轮结果变化的 bug 来源。

所以这轮的新信息不是“FVG 回测逻辑有错”，而是 `stale fill` 的边界比上一轮更清楚了：超过
`12` 根 `15m` 以后，确实会开始混入质量不足的回补样本。

因此当前 paper observation 主推候选再次更新为：

```text
research_momentum_04sl_18r_reclaim_fvgwait12_delta20_40_pchg5_10_v1
entry_rule_version=rank_radar_4h15m_r04_18r_rcm_fvg12_d20_40_p5_10_v1
event_source=raw_state
trade_direction=long
stop_loss_pct=0.04
target_r=1.8
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=20
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=10.0
entry_trigger_allowlist=reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=12
ignore_entry_signal_updates_while_open=true
```

本地 owner-side paper outcome 结果：

- `24h`：`5 trades / 4 win / 0 loss / 1 timeout / resolved_win_rate 100% / avg_r_complete 1.6802`
- `48h`：`5 trades / 5 win / 0 loss / 0 timeout / resolved_win_rate 100% / avg_r_complete 1.8`

而 `research_momentum_04sl_20r_breakout_reclaim_fvgwait10_minwait1_delta15_40_pchg5_12_v1`
继续保留为 replay 侧的高收益对照组；`research_momentum_04sl_20r_reclaim_fvgwait10_delta15_40_pchg5_12_v1`
、`research_momentum_04sl_18r_reclaim_fvgwait10_delta15_40_pchg5_12_v1` 与
`research_momentum_04sl_18r_reclaim_fvgwait10_delta20_40_pchg5_10_v1` 则都降级为旧口径备选，
不再作为当前 paper outcome 主线。

## Immediate Reclaim vs FVG Entry - 2026-06-27

在当前最新 owner-side 主线
`rank_radar_4h15m_r04_18r_rcm_fvg14_d3_pb3_vol11_fp10_d20_40_p5_10_v1`
上，又补了一轮专门针对“是不是第一次突破触发本身就该降权”的对照。

先用完全相同的壳，只关闭 `FVG` 等待，直接跑 immediate reclaim：

```text
compare_reclaim_immediate_r04_18r_d3_vol11_pb3_d20_40_p5_10_v1
event_source=raw_state
stop_loss_pct=0.04
target_r=1.8
entry_max_distance_pct=3.0
entry_min_volume_ratio=1.1
entry_max_signal_pullback_pct=3.0
min_delta_rank=20
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=10.0
entry_trigger_allowlist=reclaim_ema
fvg_entry_mode=off
ignore_entry_signal_updates_while_open=true
```

结果直接退化成：

- `entry_trigger_filter before=109 after=28`
- `24h`: `23 trades / 12 win / 7 loss / 3 timeout / 1 incomplete / avg_r_complete 0.7268`
- `48h`: `23 trades / 12 win / 8 loss / 1 timeout / 2 incomplete / avg_r_complete 0.6639`

而当前 owner-side 主线仍然是：

- `entry_trigger_filter before=10 after=5`
- `24h/48h`: `5 trades / 5 win / 0 loss / 0 timeout / avg_r_complete 1.8`

所以这里的结论不是简单的“FVG 让频率更低”，而是它确实在修正 entry quality。

把这 5 个共享 event 逐笔对齐后，差异更具体：

- `HMSTR`：immediate reclaim `2026-06-07 05:45` 入场，`stop_hit=-1R`；当前 `FVG` 延后到
  `2026-06-07 08:15`，结果变成 `target_hit=1.8R`
- `BASED`：immediate reclaim `2026-06-12 04:45` 入场，`stop_hit=-1R`；当前 `FVG`
  延后到 `2026-06-12 05:00`，结果变成 `target_hit=1.8R`
- `ORDI`：两边都赢，但 immediate 只延后 `10.91m`，当前 `FVG` 延后 `190.91m`
- `AMD / CHIP`：两边都还是赢单

也就是说，当前这条 `reclaim_ema + m15_impulse_retrace` 主线不是只靠“少做单”显得漂亮，
而是真的把 `HMSTR / BASED` 这类 immediate 会被打掉的样本，改造成了可完成的赢单。

顺着这个假设，又补扫了当前主线的
`fvg_impulse_retrace_min_wait_candles=0/1/2`：

- `minwait0`：`entry_pass=10`，`after=5`，`24h/48h: 5/5 win`
- `minwait1`：`entry_pass=12`，`after=5`，`24h/48h: 5/5 win`
- `minwait2`：`entry_pass=14`，`after=5`，`24h/48h: 5/5 win`

所以“强制多等 1-2 根 15m”在当前 realized sample 上没有带来任何额外收益，只是把更多后来
仍会被锁跳过的候选重新放回池子。当前继续保留 `minwait0`，不要为了语义上的“更像二次回踩”
就把候选边界放宽。

最后又把用户之前提过的 `15m_self_after_signal` 用同样 tight shell 重新测了一遍：

- `entry_pass=72`
- `entry_trigger_filter before=72 after=18`
- `24h`: `16 trades / 7 win / 5 loss / 3 timeout / 1 incomplete / avg_r_complete 0.5956`
- `48h`: `16 trades / 7 win / 6 loss / 2 timeout / 1 incomplete / avg_r_complete 0.5139`

这比当前 `m15_impulse_retrace` 主线明显更差，所以在最新
`0.04SL + 1.8R + d3 + vol11 + pb3 + delta20-40 + pchg5-10`
外壳下，`15m self FVG` 仍然不是更优替代。

在此基础上，又把当前主线的 `fvg_impulse_retrace_fill_pct` 扫成整数邻域
`5 / 10 / 20 / 30 / 50`：

- 五档 owner-side realized trade set 完全一致，`24h/48h` 都还是 `5 trades / 5 win / avg_r 1.8`
- `5 / 10` 两档的 `entry_pass=11`，`20 / 30 / 50` 三档的 `entry_pass=10`
- 但 `5 / 10` 的共享 5 笔赢单 entry price 会更贴近 FVG 底部，例如
  `HMSTR 0.00017758 < 0.00017766`、`ORDI 3.1088 < 3.1096`

随后用独立 rule version
`rank_radar_4h15m_r04_18r_rcm_fvg14_d3_pb3_vol11_fp10_d20_40_p5_10_v1`
做了 owner-side Web 写回验证，结果仍然是：

- `paper_outcomes_submitted=10`
- Web summary 回读 `total_count=10`、`generated_execution_task_count=0`
- `24h/48h` 仍然都是 `5 trades / 5 win / avg_r 1.8`

因此当前同名 research preset 再次做了一个不改变 trade set 的小收紧：
显式把 `fvg_impulse_retrace_fill_pct` 从默认 `20` 调到 `10`。这一步不是为了增加样本，
而是让当前已验证有效的 5 笔 reclaim FVG 入场，尽量更靠近用户想要的 “FVG 底部左右”
成交位置。

随后把前一轮分析确认过的 reclaim 次级进场分支固化成一个独立 preset：

- preset:
  `research_momentum_04sl_18r_reclaim_retest1_pullback3_delta20_40_pchg5_10_v1`
- rule version:
  `rank_radar_4h15m_r04_18r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1`

它锁定的外壳和当前 FVG 主线一致：

- `event_source=raw_state`
- `stop_loss_pct=0.04`
- `target_r=1.8`
- `entry_max_distance_pct=3.0`
- `entry_min_volume_ratio=1.1`
- `entry_max_signal_pullback_pct=3.0`
- `min_delta_rank=20`
- `max_delta_rank=40`
- `min_price_change_pct=5.0`
- `max_price_change_pct=10.0`
- `entry_trigger_allowlist=reclaim_ema`
- `ignore_entry_signal_updates_while_open=true`

唯一变化是把入场方式改成：

- `fvg_entry_mode=off`
- `entry_retest_after_signal=true`
- `entry_retest_max_wait_candles=1`
- `entry_retest_tolerance_pct=0.3`

本地重新验证：

```bash
CARGO_TARGET_DIR=/tmp/rust_quant_target_tdd \
QUANT_CORE_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_core \
cargo run -q -p rust-quant-cli --bin market_velocity_event_backtest -- \
  --event-source raw_state \
  --stop-loss-pct 0.04 \
  --target-rs 1.8 \
  --entry-max-distance-pct 3.0 \
  --entry-min-volume-ratio 1.1 \
  --entry-max-signal-pullback-pct 3.0 \
  --entry-retest-after-signal \
  --entry-retest-max-wait-candles 1 \
  --entry-retest-tolerance-pct 0.3 \
  --trend-min-average-distance-pct 0.0 \
  --min-delta-rank 20 \
  --max-delta-rank 40 \
  --min-price-change-pct 5.0 \
  --max-price-change-pct 10.0 \
  --entry-trigger-allowlist reclaim_ema \
  --ignore-entry-signal-updates-while-open \
  --fvg-entry-mode off \
  --paper-outcome-sink jsonl \
  --paper-outcome-entry-rule-version \
    rank_radar_4h15m_r04_18r_rcm_rt1_d3_pb3_vol11_d20_40_p5_10_v1
```

结果仍然稳定：

- `24h`: `3 trades / 3 win / 0 loss / avg_r_complete 1.8`
- `48h`: `3 trades / 3 win / 0 loss / avg_r_complete 1.8`
- 48h realized set:
  - `EIGEN-USDT-SWAP`
  - `SLX-USDT-SWAP`
  - `INJ-USDT-SWAP`

这 3 笔与当前 `fill10` FVG 主线的 5 笔
`HMSTR / ORDI / BASED / AMD / CHIP`
完全不重叠，所以当前更合理的定位不是“用 retest 替换 FVG”，而是把它当成 reclaim-only 的高质量补充分支。

## TP/SL Simplification Recheck - 2026-06-28

针对当前最强 hybrid 外壳，又补了一轮只看简单整数/整档参数的止盈止损复核，目标是回答：

- 当前默认止损是不是仍该保留 `4%`
- 低频样本下，基础止盈是该压到 `1.5R`，还是保留 `2R`
- 分批止盈是否值得升成默认
- 结构止损是否真的能和分批止盈形成正增益

本轮固定的外壳不变：

```text
event_source=raw_state
trade_direction=long
entry_rule_version=rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_v1
entry_max_distance_pct=5.0
entry_min_volume_ratio=1.0
trend_min_average_distance_pct=0.0
min_delta_rank=15
max_delta_rank=40
min_price_change_pct=5.0
max_price_change_pct=12.0
entry_trigger_allowlist=breakout_previous_high,reclaim_ema
fvg_entry_mode=m15_impulse_retrace
fvg_max_wait_candles=10
ignore_entry_signal_updates_while_open=true
```

样本仍然是同一组 `13` 笔交易，入场时间覆盖 `2026-06-01` 到 `2026-06-16`。

### 固定止损宽度对照

基础止盈固定 `2R`，只扫简单止损宽度：

- `3% + 2R`：`profit 23.09U`，`win_rate 53.85%`，`Sharpe 1.3713`，`max_dd 3.07%`
- `4% + 2R`：`profit 79.09U`，`win_rate 84.62%`，`Sharpe 4.8677`，`max_dd 4.07%`
- `5% + 2R`：`profit 69.09U`，`win_rate 69.23%`，`Sharpe 2.6593`，`max_dd 5.07%`

结论很直接：当前壳里 `4%` 明显优于 `3%` 和 `5%`。`3%` 太紧，会把大量原始赢单提前打成止损；`5%`
虽然还能盈利，但已经开始用更宽回撤去换结果，收益质量不如 `4%`。

### 基础止盈对照

固定止损锁定 `4%` 后，再看简单目标位：

- `4% + 1.5R`：`profit 57.09U`，`win_rate 84.62%`，`Sharpe 4.2164`
- `4% + 2R`：`profit 79.09U`，`win_rate 84.62%`，`Sharpe 4.8677`
- `4% + 3R`：`profit 43.09U`，`win_rate 46.15%`，`Sharpe 1.4395`

这说明当前低频样本里，没必要为了“更高胜率”把基础止盈压到 `1.5R`，因为它并没有提升 framework
胜率，只是直接少赚；而 `3R` 又明显过高，会把大量本来能完成的样本重新拖回超时或止损。

因此当前基础止盈最优点仍然是 `2R`。

### 分批止盈对照

继续锁定当前最优固定止损 `4%`，对比：

- `4% + 2R 全平`：`profit 79.09U`，`win_rate 84.62%`，`Sharpe 4.8677`
- `4% + 2R base + 6R/20% runner + 1R runner stop`：
  `profit 80.68666560U`，`win_rate 84.62%`，`Sharpe 4.6761`

所以 runner 版本不是无效，而是一个很明确的 trade-off：

- 总利润多 `1.59666560U`
- 胜率没有提高
- Sharpe 下降

如果只看总利润，`runner6R20 stop1` 是当前最好的低耦合增强版；但如果看风险调整后的默认口径，
`4% + 2R 全平` 仍然更干净。

### 集中度复核

同一轮 fresh concentration 结果也支持上面的判断。

`4% + 2R 全平`：

- remove top1 后剩余 `71.16U`
- remove top3 后剩余 `55.30U`
- remove top5 后剩余 `39.44U`

`4% + 2R base + 6R/20% runner + 1R runner stop`：

- remove top1 后剩余 `69.55666560U`
- remove top3 后剩余 `49.80651195U`
- remove top5 后剩余 `34.64U`

runner 版本的额外利润更依赖头部样本，尤其是 `UNI / USELESS / AMD` 这类尾部放大利润的交易。
因此它适合继续做 paper/research 候选，但暂时不适合替代 `2R` 成为默认版本。

### 结构止损 + 分批止盈复核

现有实现里，`structure_or_fixed` 只会把初始止损收紧，不会放宽。因此它和 runner 组合以后，
本质是在拿更窄的 breathing room 去换更早的 base TP/runner 触发。

实际回测结果很差：

- `structure + runner6`，`structure_stop_min_pct=0`：
  `profit -0.95393177U`，`win_rate 15.38%`，`Sharpe -2.5443`
- `structure + runner6`，`min_pct_floor=1%`：
  `profit 6.69U`，`win_rate 53.85%`，`Sharpe 1.1969`
- `structure + runner6`，`min_pct_floor=2%`：
  `profit 18.29U`，`win_rate 53.85%`，`Sharpe 1.4769`
- `structure + runner6`，`min_pct_floor=3%`：
  `profit 27.68666560U`，`win_rate 53.85%`，`Sharpe 1.5052`

即便取这组里最不差的 `3% floor`，它也仍然远弱于：

- `4% + 2R 全平`
- `4% + 2R base + 6R/20% runner + 1R runner stop`

逐笔对照能看到，`SAHARA / ORDI / BASED / CHIP` 这类原本能完成 base TP 的赢单，
在 `structure + floor 3%` 下被更早打成 `stop_hit`。因此这个方向先停止推进，不作为当前主壳的可用止损优化。

### 当前结论

截至这轮复核，当前 hybrid 壳上最优、且不过度耦合的止盈止损结论可以收敛为两档：

1. 默认低耦合版本：
   `固定 4% 止损 + 2R 全平`
2. 继续 forward paper observation 的利润增强候选：
   `固定 4% 止损 + 2R base + 6R/20% runner + 1R runner stop`

而 `3%`、`5%`、`1.5R`、`3R`、`结构止损 + runner` 这几个方向，在当前主壳上都已经有了明确负证据，
后续不再优先重复扫描。

当前库里 `raw_state` 可用历史只覆盖 `2026-05-27` 到 `2026-06-28`。在这个边界内，再按时间切成三段：

- `w1`: `2026-05-27 ~ 2026-06-05`
- `w2`: `2026-06-06 ~ 2026-06-15`
- `w3`: `2026-06-16 ~ 2026-06-28`

对当前 hybrid 主壳继续做窗口化复核：

`4% + 2R 全平`

- `w1`: `2 trades / 100% / 15.86U`
- `w2`: `8 trades / 75% / 39.44U`
- `w3`: `3 trades / 100% / 23.79U`

`4% + 2R base + 6R/20% runner + 1R stop`

- `w1`: `2 trades / 100% / 18.26U`
- `w2`: `8 trades / 75% / 36.13015365U`
- `w3`: `3 trades / 100% / 26.29651195U`

这说明 runner6 不是“每个窗口都更优”的版本：

- 在 `w1 / w3` 它比 `2R` 多赚
- 在 `w2` 它反而少赚

所以当前对 hybrid 壳更稳的表述应该是：

1. `4% + 2R` 是当前主壳的默认低耦合基线
2. `runner6R20 stop1` 是当前主壳的弱增强候选，存在轻微 regime 依赖
3. 它可以继续 forward paper observation，但还不应替代 `2R` 成为默认

再往上游事实源追了一层，当前“不能继续扩长历史”的原因也基本明确了：

- `market_rank_events`：`2026-05-27` 到 `2026-06-28`
- `market_rank_snapshots`：`2026-06-27` 到 `2026-06-28`
- `signal_snapshot_log`：`2026-05-29` 到 `2026-06-02`
- `market_snapshots` / `indicator_snapshots` / 通用 `market_candles`：当前为空

代码侧也说明，这不是 `market_velocity_event_backtest` 自己的人为截断：

- scanner 恢复排名历史只需要 `24h / 4h / 15m / now` 四个目标帧
- `delete_rank_snapshots_before(...)` 目前只存在于 repository 实现里，当前代码路径里没有实际调用证据

因此当前约束应表述为：

1. 现有 TP/SL 结论已经在“当前可用的 ranking/signal fact history”里做到尽量充分
2. 如果要继续提升置信度，下一步不是继续抠 TP/SL 参数，而是先扩充上游可审计事实历史
3. 在没有更长 `market_rank_events` / `market_rank_snapshots` / 等价 rank history 之前，无法对当前
   raw_state 壳做更长周期的同口径验证

### Current Selection Matrix - 2026-06-28

基于当前可验证证据，TP/SL 先按“策略壳”分别收敛，不再追求统一默认：

| 壳 | preset / 组合 | 当前定位 | 原因 |
|---|---|---|---|
| hybrid raw_state | `research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_v1` | 默认基线 | `4% + 2R` 在当前壳里 Sharpe 最高，集中度更低，窗口化结果也最稳 |
| hybrid raw_state | `research_momentum_04sl_20r_breakout_reclaim_fvgwait10_delta15_40_pchg5_12_runner6r20_stop1_v1` | paper challenger | 总利润略高，但 `w2` 窗口弱于 `2R`，存在轻微 regime 依赖 |
| episode | `research_episode_momentum_05sl_20r_rank5_v1` | 低耦合 baseline | 比 `3%/4%` 更稳，但 win rate 仍低，不作为当前 production default |
| episode | `research_episode_momentum_05sl_30r_rank5_v1` | profit-seeking research | Sharpe / profit 更强，但收益更偏后半段 regime，不外推到其他壳 |

因此当前推进策略应该是：

1. 生产 / 默认只保留当前 hybrid 壳的 `4% + 2R`
2. hybrid 的 `runner6R20 stop1` 继续做 forward paper observation
3. episode 两档 preset 保留为独立研究入口，不参与当前 hybrid 默认升级

### Cross-Regime Recheck: Episode Shell

为了避免把当前 `13` 笔 hybrid 样本上的结论误判成通用规律，又把同样的“简化 TP/SL”
思路放到旧的 `episodes` 壳上重跑了一轮。

这一轮固定的是 episode research 壳：

```text
event_source=episodes
trade_direction=long
entry_max_distance_pct=7.0
entry_min_volume_ratio=0.8
trend_min_average_distance_pct=0.0
min_delta_rank=5
entry_trigger_allowlist=all
```

48h 框架样本大约是 `53-54` 笔，明显比当前 hybrid 壳更宽、波动也更大。

先固定 `target=2R`，扫描止损：

- `3% + 2R`：`profit 40.72240394U`，`win_rate 42.59%`，`Sharpe 1.2628`
- `4% + 2R`：`profit 68.04949295U`，`win_rate 43.40%`，`Sharpe 1.5325`
- `5% + 2R`：`profit 96.59975405U`，`win_rate 45.28%`，`Sharpe 1.7585`

在这个更宽的旧壳里，`5%` 反而优于 `4%`。因此“固定 4% 止损”不是跨所有壳的普适最优，
而是当前 hybrid 主壳下的最优点。

再固定 `5%` 止损，扫描基础目标位：

- `5% + 1.5R`：`profit 53.51197074U`，`win_rate 47.17%`，`Sharpe 1.1617`
- `5% + 2R`：`profit 96.59975405U`，`win_rate 45.28%`，`Sharpe 1.7585`
- `5% + 2.4R`：`profit 100.75475005U`，`win_rate 41.51%`，`Sharpe 1.6914`
- `5% + 3R`：`profit 154.40596384U`，`win_rate 41.51%`，`Sharpe 2.2339`

这说明旧 `episodes` 壳的尾部收益更重要，目标位越高，收益反而越强；这和当前 hybrid 壳里
`2R` 最优的结论明显不同。

继续把 `5% + 2R` 和 `5% + 3R` 做 early/late 与 concentration 对照后，这个差异更清楚：

`5% + 2R`：

- early half：`29 trades / win_rate 34.48% / profit 3.23769600U / Sharpe 0.0760`
- late half：`24 trades / win_rate 58.33% / profit 92.10052790U / Sharpe 2.5756`
- remove top1 后剩余 `69.46586011U`
- remove top3 后剩余 `49.60586011U`
- remove top5 后剩余 `29.74586011U`

`5% + 3R`：

- early half：`29 trades / win_rate 27.59% / profit 12.98419600U / Sharpe 0.2647`
- late half：`24 trades / win_rate 58.33% / profit 142.08152119U / Sharpe 3.0826`
- remove top1 后剩余 `139.47596384U`
- remove top3 后剩余 `109.61596384U`
- remove top5 后剩余 `79.75596384U`

因此 `3R` 不是靠单一头部交易撑起来的，它在 episode 壳里的 tail capture 确实更强；但它的收益
主要仍集中在后半段 regime，`q1` 甚至是 `-26.19U`。所以它更适合作为 episode 壳的
profit-seeking 研究候选，而不是可以无脑推广到其他 momentum 壳的默认止盈。

同样的 `runner6R20 stop1` 也补跑了一次：

- `5% + 2R base + 6R/20% runner + 1R stop`：
  `profit 94.00392495U`，`win_rate 45.28%`，`Sharpe 1.7113`

它不但没有超过 `5% + 2R 全平`，而且 concentration 更高：

- remove top1 后剩余 `76.14392495U`
- remove top3 后剩余 `48.28392495U`
- remove top5 后剩余 `24.27411632U`

因此 cross-regime 证据可以再收敛一层：

1. `4% + 2R` 是当前 hybrid 主壳的最优默认，不应轻易改。
2. `runner6R20 stop1` 只能算当前主壳上的利润增强候选，不是跨壳通用增强器。
3. 旧 `episodes` 壳更像“更宽止损 + 更高目标位”的结构，说明当前 repo 里不存在一个对所有
   momentum 壳都同样最优的单一 TP/SL 组合。
4. 因此后续如果要做生产默认推进，应该按策略壳分别固化，而不是继续追求一个全家族统一止盈止损。

为避免后续继续手工拼参数，当前把 episode 壳的两档研究入口也单独固化成 preset：

- 低耦合 baseline：
  `research_episode_momentum_05sl_20r_rank5_v1`
  - `entry_rule_version=rank_radar_4h_trend_15m_episode_research_05sl_20r_rank5_v1`
  - 用途：旧壳上更稳的简单 baseline，对照当前 hybrid 主壳的 `4% + 2R`
- 利润优先候选：
  `research_episode_momentum_05sl_30r_rank5_v1`
  - `entry_rule_version=rank_radar_4h_trend_15m_episode_research_05sl_30r_rank5_v1`
  - 用途：旧壳上更强的 profit-seeking 研究入口，但不作为当前 hybrid 默认的替代

### Jackknife Robustness Check

为了进一步减少“当前结论只是靠少数交易对撑起来”的风险，又补了一轮按 `symbol` 的
leave-one-out / jackknife 检查。

#### hybrid: `4% + 2R` vs `4% + 2R base + 6R/20% runner + 1R stop`

`4% + 2R`：

- total `79.09U`
- leave-one-out 最差剩余 `71.16U`
- leave-one-out 中位数 `71.16U`
- `13/13` 个 symbol 被移除后，剩余收益仍为正

`runner6R20 stop1`：

- total `80.68666560U`
- leave-one-out 最差剩余 `69.55666560U`
- leave-one-out 中位数 `73.55666560U`
- `13/13` 个 symbol 被移除后，剩余收益仍为正

两者逐 symbol 对比后：

- `runner6` 相对 `2R` 的总优势只有 `+1.59666560U`
- 在 `13` 个 leave-one-out 场景里，`runner6` 仅在 `11` 个场景更优，`2` 个场景更差
- 这两个反转点正好是 `UNI` 和 `USELESS`，去掉任一后，`runner6` 都会比 `2R` 少
  `1.60333440U`

因此 `runner6` 的增强是存在的，但它对少数尾部赢家仍有明显依赖，不适合作为默认基线。

#### episode: `5% + 2R` vs `5% + 3R`

`5% + 2R`：

- total `96.59975404U`
- leave-one-out 最差剩余 `69.46586010U`
- leave-one-out 中位数 `93.65745613U`
- `43/43` 个 symbol 被移除后，剩余收益仍为正

`5% + 3R`：

- total `154.40596383U`
- leave-one-out 最差剩余 `139.47596383U`
- leave-one-out 中位数 `151.46366592U`
- `43/43` 个 symbol 被移除后，剩余收益仍为正

更关键的是，两者逐 symbol leave-one-out 对比里：

- `5% + 3R` 相对 `5% + 2R` 的优势是 `+57.80620979U`
- 在 `43/43` 个 leave-one-out 场景里，`3R` 都仍然优于 `2R`

这说明 `episodes` 壳里的 `3R` 不是靠单一头部交易撑出来的，而是整个壳的 profit structure
本身更偏向高目标位。

### 2026-06-28 Hybrid 1.8R TP/SL Follow-up

针对当前最强的 hybrid `raw_state` 主壳，又补了一轮只围绕出场侧的 fresh 复核。壳保持不变：

- `event_source=raw_state`
- `stop_loss_pct=0.04`
- `entry_trigger_allowlist=reclaim_ema`
- `entry_retest_after_signal=true`
- `entry_retest_max_wait_candles=1`
- `entry_retest_tolerance_pct=0.3`
- `fvg_entry_mode=m15_impulse_retrace`
- `ignore_entry_signal_updates_while_open=true`

基线仍然是：

- `4% fixed SL + 1.8R full exit`
  - `48h`: `8 trades / 8 win / 0 loss / 0 timeout`
  - `avg_r_complete=1.8`
  - `framework total_profit=57.04U`

#### structure stop + staged TP

继续复核了 `structure_or_fixed + runner 4R/10%/0R stop`：

- `floor=0%`: `2W / 6L / 0T`, `avg_r_complete=-0.345`, `3.05215088U`
- `floor=1%`: `4W / 4L / 0T`, `avg_r_complete=0.41`, `4.58169353U`
- `floor=2%`: `4W / 3L / 1T`, `avg_r_complete=0.7801192505311001`, `12.79775968U`
- `floor=3%`: `4W / 3L / 1T`, `avg_r_complete=0.7159128336874001`, `17.161U`

这说明当前壳上“结构止损 + 分批止盈”不是弱一点，而是方向性错误。`structure_or_fixed`
只会把初始止损收紧，而这套 FVG/retest 入场正好需要给 pullback 留 breathing room。

#### structure stop only

又把 `runner` 完全关掉，只保留 `structure_or_fixed`，补扫了最简单的一组
`target = 1.5R / 1.8R / 2.0R` 与 `structure_stop_min_pct = 0 / 1% / 2% / 3%`。

先确认语义边界：当前实现里，`structure_or_fixed` 只有当结构止损比固定 `4%` 更靠近入场时才会被采用；
如果结构止损更宽，则直接回退到固定 `4%`。因此这个模式本质上是“只会收紧，不会放宽”。

回测结果也和这个语义一致：

- `floor=0%`
  - `1.5R`: `2W / 6L / 0T`, `2.86258867U`
  - `1.8R`: `2W / 6L / 0T`, `3.89138499U`
  - `2.0R`: `2W / 6L / 0T`, `4.57724920U`
- `floor=1%`
  - `1.5R`: `5W / 3L / 0T`, `5.61260022U`
  - `1.8R`: `4W / 4L / 0T`, `4.75033349U`
  - `2.0R`: `3W / 5L / 0T`, `2.84215568U`
- `floor=2%`
  - `1.5R`: `5W / 3L / 0T`, `9.12866637U`
  - `1.8R`: `5W / 3L / 0T`, `12.26639964U`
  - `2.0R`: `5W / 3L / 0T`, `14.35822182U`
- `floor=3%`
  - `1.5R`: `5W / 3L / 0T`, `12.94000000U`
  - `1.8R`: `5W / 3L / 0T`, `17.44000000U`
  - `2.0R`: `5W / 3L / 0T`, `20.44000000U`

即便取 structure-only 里最好的 `floor=3% + 2.0R`，它也仍然显著弱于当前基线
`4% fixed SL + 1.8R full exit = 57.04U`。因此当前 hybrid 主壳上，`structure_or_fixed`
不只是“不适合和 runner 组合”，而是连单独作为默认止损模式也不成立。

#### profit protect

同壳补扫 `profit_protect` 后，结论也很直接：

- `after=1.0 / stop=0.0`: 与基线完全一致，`57.04U`
- `after=1.2 / stop=0.3`: 与基线完全一致，`57.04U`
- `after=1.4 / stop=0.5`: `46.64U`
- `after=1.6 / stop=0.8`: `53.04U`

也就是说，这 8 笔里保护止盈要么没有触发实质差异，要么只是提前把盈利压掉，没有超过
`1.8R 全平`。

#### target_r neighborhood

继续只扫低耦合的 `target_r` 邻域：

- `1.7R`: `8W / 0L / 0T`, `53.84U`
- `1.8R`: `8W / 0L / 0T`, `57.04U`
- `1.9R`: `6W / 1L / 1T`, `48.64U`
- `2.0R`: `6W / 1L / 1T`, `51.44U`

因此 whole-shell 的局部峰值仍然在 `1.8R`，不是 `1.9R/2.0R`。

#### branch split

`1.9R` 和 `2.0R` 之所以输给 `1.8R`，仍然主要输在 FVG 分支，而不是 fallback retest：

- `target=1.9R`, `48h`
  - `reclaim_ema+fvg_15m_impulse_retrace`: `5 trades / 3W / 1L / 1T`, `avg_r_complete=1.280954004248801`
  - `reclaim_ema+retest_after_signal+fvg_fallback`: `3 trades / 3W`, `avg_r_complete=1.9`
- `target=2.0R`, `48h`
  - `reclaim_ema+fvg_15m_impulse_retrace`: `5 trades / 3W / 1L / 1T`, `avg_r_complete=1.3409540042488008`
  - `reclaim_ema+retest_after_signal+fvg_fallback`: `3 trades / 3W`, `avg_r_complete=2.0`

因此当前唯一还值得继续观察的轻耦合 challenger，仍然不是新的 stop/runner 组合，而是：

- whole-shell 默认继续保持 `4% + 1.8R`
- retest fallback 单独看 `2.0R`

但把 exit target 按 entry path 分裂，已经属于新增耦合；在样本只有 `3` 笔 fallback 时，还不应直接升成默认。

#### regime split

同壳又按 `market_rank_events` 的自然周窗口做了一次 fresh 重跑，仍只比较低耦合候选：

- `t17 = 4% fixed SL + 1.7R full exit`
- `t18 = 4% fixed SL + 1.8R full exit`
- `t20 = 4% fixed SL + 2.0R full exit`
- `t18pp = 4% fixed SL + 1.8R + profit_protect_after=1.6R / stop=0.8R`

按周结果：

- `w1 = 2026-05-25 ~ 2026-05-31`
  - 四档都 `0 trades`
- `w2 = 2026-06-01 ~ 2026-06-07`
  - `t17`: `1 trade / 1W / 0L / 0T`, `6.73U`
  - `t18`: `1 trade / 1W / 0L / 0T`, `7.13U`
  - `t20`: `1 trade / 0W / 1L / 0T`, `-4.07U`
  - `t18pp`: `1 trade / 1W / 0L / 0T`, `3.13U`
- `w3 = 2026-06-08 ~ 2026-06-14`
  - `t17`: `6 trades / 6W / 0L / 0T`, `40.38U`
  - `t18`: `6 trades / 6W / 0L / 0T`, `42.78U`
  - `t20`: `6 trades / 5W / 0L / 1T`, `47.58U`
  - `t18pp`: `6 trades / 6W / 0L / 0T`, `42.78U`
- `w4 = 2026-06-15 ~ 2026-06-21`
  - `t17`: `1 trade / 1W / 0L / 0T`, `6.73U`
  - `t18`: `1 trade / 1W / 0L / 0T`, `7.13U`
  - `t20`: `1 trade / 1W / 0L / 0T`, `7.93U`
  - `t18pp`: `1 trade / 1W / 0L / 0T`, `7.13U`
- `w5 = 2026-06-22 ~ 2026-06-28`
  - 四档都 `0 trades`

所以当前结论可以明确为：

1. 在现有可用历史内，最优且不过度耦合的默认 TP/SL 仍然是 `4% fixed SL + 1.8R full exit`
2. `structure stop + staged TP` 在当前 hybrid 主壳上已有明确负证据
3. `profit_protect` 在当前主壳上仍没有带来更优折中；它从未在 active week 里超过 `1.8R 全平`
4. `2.0R` 只在更强的一周 `w3` 和单笔样本的 `w4` 上更高，但在 `w2` 直接退化成完整亏损；
   如果不引入额外 regime 分类器，这个 trade-off 不适合升为默认
5. 因此当前 weekly rerun 已经把“只靠单一窗口支撑”的风险降下来，但样本仍只有 `8` 笔 active trades；
   后续若要继续验证默认稳健性，优先级仍然是补更早的 event history，而不是继续在这 8 笔上微调

#### data-source boundary

本地 `quant_core` 里又补查了一次数据源边界：

- `market_rank_events`:
  - `rank_velocity + top_entry`
  - `min(detected_at)=2026-05-27 03:29:52.854632+00`
  - `max(detected_at)=2026-06-28 16:35:21.980053+00`
  - `count(*)=4584585`
  - weekly row counts:
    - `2026-05-25`: `918049`
    - `2026-06-01`: `886781`
    - `2026-06-08`: `1034739`
    - `2026-06-15`: `865540`
    - `2026-06-22`: `879458`
- `market_velocity_episodes`:
  - `min(started_at)=2026-05-27 03:29:52.854632+00`
  - `max(started_at)=2026-06-22 03:53:27.945416+00`
  - `count(*)=4521`
- schema 中没有额外的 `market_rank_events_*` 或 `market_velocity_episodes_*` archive / partition 表可继续向前扩

因此当前 `raw_state/raw_events` 已经能覆盖到 `2026-06-28`；问题不再是“6 月后半段没有 event history”，
而是同一壳在这段历史里真正满足全部过滤并最终开仓的样本，仍集中在很少几个自然周。
如果要继续做更强的跨 regime 复核，真正缺的是 `2026-05-27` 之前的 event history，而不是更长的 candles。

同一时期 candles 明显更早，说明瓶颈也不在 K 线分表。例如：

- `aave-usdt-swap_candles_15m`: `2025-12-29 07:15:00+00 ~ 2026-06-27 07:00:00+00`
- `based-usdt-swap_candles_15m`: `2026-04-16 08:30:00+00 ~ 2026-06-21 05:45:00+00`
- `aave-usdt-swap_candles_4h`: `2026-04-16 12:00:00+00 ~ 2026-06-28 12:00:00+00`

所以如果要继续做同壳跨 regime 验证，真正缺的仍是更早的 event history，而不是更长的 candles。

#### raw_events proxy stability check

由于 `raw_state` 无法再向前扩历史，又补跑了一次同参数 `raw_events` proxy。目的不是替代生产壳，
而是验证 `1.8R` 的局部峰值，是否只是 `raw_state` 15m 去重的人为现象。

结果：

- `raw_events`, `1.8R`
  - `effective_entry`: `raw=51983`, `entry_pass_before_filters=794`, `trigger_after=97`
  - `48h`: `8 trades / skipped_lock=89 / 8W / 0L / 0T`
  - `total_profit=57.04U`
- `raw_events`, `1.9R`
  - `48h`: `8 trades / skipped_lock=89 / 6W / 1L / 1T`
  - `total_profit=48.64U`
- `raw_events`, `2.0R`
  - `48h`: `8 trades / skipped_lock=89 / 6W / 1L / 1T`
  - `total_profit=51.44U`

也就是说，虽然 `raw_events` 原始 event 数被放大了很多，但在
`ignore_entry_signal_updates_while_open=true` 的当前壳语义下，重复 scanner hit 最后都会被
`skipped_lock` 吸收掉，最终成交的仍然是和 `raw_state` 相同的 `8` 笔。

这让当前结论更稳一层：

1. `1.8R` 的 whole-shell 峰值不是 `raw_state` 15m 去重带来的假象
2. 当前 source history 里，whole-shell 默认依旧保持 `4% + 1.8R`
3. 如果后续要进一步推进，不该继续在 `1.7/1.8/1.9/2.0` 附近反复微调，而应优先补更长的
   `market_rank_events` 历史，再用同一壳重跑

#### early_exit_no_profit_candles

最后又补了一组之前还没扫过的低耦合 exit：`early_exit_no_profit_candles`。它不改入场、
不拆 target，也不引入 structure stop，因此是当前主壳上最后一个值得补证据的简单出场族。

同壳 `4% + 1.8R` 下的结果：

- `early_exit=1`:
  - `48h`: `2W / 6L / 0T`
  - `avg_r_complete=0.3974095089242428`
  - `total_profit=12.15710429U`
- `early_exit=2`:
  - `48h`: `3W / 5L / 0T`
  - `avg_r_complete=0.6302833787923322`
  - `total_profit=19.60906812U`
- `early_exit=3`:
  - `48h`: `3W / 5L / 0T`
  - `avg_r_complete=0.5909786035048251`
  - `total_profit=18.35131531U`
- `early_exit=4`:
  - `48h`: `4W / 4L / 0T`
  - `avg_r_complete=0.7800677449527064`
  - `total_profit=24.40216784U`

即便是这组里相对最不差的 `4` 根 15m 无利润早退，也仍然远弱于基线：

- baseline `4% + 1.8R full exit`: `8W / 0L / 0T`, `57.04U`

这说明当前主壳的有效单子并不是“短时间不盈利就说明错了”的结构；相反，它们经常需要给
`1h` 左右的 pullback / rebuild 时间，才能走完后面的 `1.8R`。因此 `early_exit_no_profit_candles`
在当前壳上也属于明确负证据，不进入默认候选。

#### local alternative history audit

由于当前 whole-shell 结论仍然缺跨 regime 的更长 event history，又继续盘点了一轮本地
`quant_core` 里所有可能相关的表，看看是否存在不用引入新合成逻辑、就能直接扩样本的候选源。

本地可见的 rank / velocity / signal 相关表包括：

- `market_rank_events`
- `market_velocity_episodes`
- `market_rank_snapshots`
- `signal_snapshot_log`
- `strategy_signals`
- `strategy_job_signal_log`
- `filtered_signal_log`

逐个核对后：

- `market_rank_events`
  - 当前 market velocity 回测主源
  - 当前本地覆盖 `2026-05-27 03:29:52+00 ~ 2026-06-28 16:35:21+00`
  - `rank_velocity + top_entry` 共 `4584585` 行
- `market_velocity_episodes`
  - 当前去重 episode 源
  - 当前本地覆盖 `2026-05-27 03:29:52+00 ~ 2026-06-22 03:53:27+00`
  - 共 `4521` 行
- `market_rank_snapshots`
  - 结构上只含 `exchange/symbol/rank/price/volume/captured_at`
  - 当前本地数据仅 `2026-06-27 15:37:48+00 ~ 2026-06-28 16:37:25+00`
  - 共 `449120` 行
  - 不足以恢复更长 rank velocity 历史
- `signal_snapshot_log`
  - 只有 `run_id/kline_ts/filtered/filter_reasons/signal_json`
  - 当前本地 `created_at` 仅 `2026-05-29 ~ 2026-06-02`
  - 样本 `filter_reasons` 已出现 `MACD_FALLING_KNIFE_LONG`，不属于当前 market velocity / rank radar 合同
- `strategy_signals`
  - 当前本地为空表
- `strategy_job_signal_log`
  - 当前本地只有 `2026-06-15` 的 `vegas` smoke 记录，共 `6` 行
  - 与当前 market velocity 壳无关
- `filtered_signal_log`
  - 当前本地 `created_at` 为 `2026-05-21 ~ 2026-06-02`
  - 字段与样本都来自旧 `4H` Vegas 过滤日志；抽样记录为 `ETH-USDT-SWAP / period=4H`
  - 不能作为 current market velocity shell 的同构历史样本

另外还补查了 retention 侧代码：

- `market_rank_snapshot_prune_job` 在本地默认保留 `90` 天，生产保留 `7` 天
- 当前本地 `market_rank_snapshots` 只有两天，并不是被本地 prune 成这样；更可能是该表本身就刚开始写

所以到这一步，本地可以确定：

1. 当前 repo/本地 DB 中不存在更长、且与 current market velocity shell 同构的历史事件源
2. `filtered_signal_log` 虽然长，但属于旧 Vegas 体系，不能拿来证明当前壳的 TP/SL
3. 在不引入新合成数据逻辑、也不读取其他环境数据的前提下，当前 whole-shell 结论已经到达本地证据上限

#### local quant_web paper outcome cross-check

虽然 `quant_core` 里没有更早、同构的 event history，但同一套本地 PostgreSQL 还挂着 `quant_web`，
并且存在 `market_velocity_paper_outcomes`。这张表是 forward paper observation 的 owner-side 后验结果，
因此可以作为“当前默认壳是否至少和已写入的 paper 证据一致”的补充校验。

先看当前默认版本：

- `entry_rule_version=rank_radar_4h15m_r04_18r_rcm_fvg_rt1_pb3_vol11_d20_40_p5_10_v2`
- `target_r=1.8`
- `horizon_hours=48`

本地 `quant_web.market_velocity_paper_outcomes` 中对应结果：

- `entry_at`: `2026-06-07 08:15:00 ~ 2026-06-16 03:30:00`
- 共 `8` 条
- `outcome_status`: `8 win / 0 loss / 0 timeout / 0 incomplete`
- `entry_trigger` 分布：
  - `reclaim_ema+fvg_15m_impulse_retrace`: `5 win`
  - `reclaim_ema+retest_after_signal+fvg_fallback`: `3 win`

逐笔也与本地回测一致：

- `HMSTR-USDT-SWAP`
- `ORDI-USDT-SWAP`
- `BASED-USDT-SWAP`
- `AMD-USDT-SWAP`
- `EIGEN-USDT-SWAP`
- `CHIP-USDT-SWAP`
- `SLX-USDT-SWAP`
- `INJ-USDT-SWAP`

这说明当前默认 `1.8R` 不是只停留在 CLI 回测里“看起来正确”；本地 `quant_web` 已持久化的
forward paper 结果也与 `5 笔 FVG + 3 笔 fallback retest` 的回测 realized set 完全对齐。

再看最接近的 `2.0R` paper 邻域。注意这里没有与当前 `v2` 完全同壳的 `2.0R` paper 版本，
所以以下只能作为近邻参考，不能直接当成严格 A/B：

- `rank_radar_4h15m_r04_20r_rcm_fvg10_d15_40_p5_12_v1`
  - `48h`: `3 win / 1 timeout`
- `rank_radar_4h15m_r04_20r_brk_rcm_fvg10_d15_40_p5_12_v1`
  - `48h`: `10 win / 2 loss / 1 timeout`

其中第二条还包含 `breakout_previous_high` 分支，壳比当前默认更宽，因此不能据此宣称 `2.0R`
优于 `1.8R`；但它至少说明：即便把壳放宽，`2.0R` 在 owner-side paper 后验里也不是一个干净的
“全 win / 零 timeout” 默认候选。

所以到这一步，本地可得出的最强结论可以再收敛一层：

1. 当前默认 `4% fixed SL + 1.8R full exit` 同时被本地回测、cross-week rerun、以及
   `quant_web.market_velocity_paper_outcomes` 的 owner-side 结果支持
2. 当前并不存在同壳、同阶段、同口径且更强的 `2.0R` paper 证据可以推翻它
3. 若要继续挑战 `1.8R` 默认，下一步不该继续抠本地小样本，而应补更早的 event history，
   或积累同壳 `2.0R` 的真实 forward paper observation
