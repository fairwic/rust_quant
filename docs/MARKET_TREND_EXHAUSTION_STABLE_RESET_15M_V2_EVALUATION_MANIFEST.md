# Market Trend Exhaustion Stable Reset 15m V2 评估清单

## 1. 策略身份与本轮唯一变化

- 产品 slug：`market-trend-exhaustion-one-shot-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_trend_exhaustion_extreme_volume_stable_reset_v2`
- 状态：`research_only`，`promotion_eligible=false`

V2 保留 V1 的趋势背景、极端量方向、入场、退出、成本、币池和样本窗口。本轮唯一变化
是趋势重置确认：V1 观察到一根中性 15m K 线就允许重置；V2 必须连续 8 根已完成
15m K 线都处于中性，累计 2 小时后才允许重新 armed。

状态规则冻结如下：

1. 中性首次进入互斥上涨/下跌背景时 armed；
2. 只消费该背景第一个满足量比和振幅条件的极端量 setup；
3. consumed 后，同方向背景继续保持冷却；
4. 一至七根中性 K 线只累计重置确认，不解除冷却；期间原趋势恢复时中性计数归零；
5. 连续第八根中性 K 线完成后才回到 neutral；
6. 趋势直接翻转进入等待中性状态，仍必须满足连续八根中性，不能直接重新 armed。

8 根是结果读取前按生产状态防抖语义固定的 2 小时确认窗口。本轮不扫描 4/8/12 等
邻域，也不增加第二根确认、MACD、FVG、BOS 或其他入场条件。

## 2. 数据、时序与冻结参数

- 研究窗口：`2025-07-01 08:00:00` 至 `2026-07-17 17:00:00`，Asia/Shanghai；
- 只读取本地 `quant_core` 的已完成 15m K 线，不读取 1m、`market_rank_events` 或 episode；
- 只加载 `exchange_symbols` 当前 `live`、线性、USDT 永续且存在 15m 表的 OKX 交易对；
  按当前研究口径排除已标记退市币，同时承认 current-live 幸存者偏差；
- 历史背景保持 V1：前 192 根净上涨/下跌至少 8%，或前 96 根同向线性趋势且
  R² 至少 0.60；同时满足多空时按中性；
- 极端量保持 V1：量比至少 2.0、振幅扩张至少 1.4 倍；实体占振幅至少 20% 时
  反向实体，小实体按更长影线对应的拒绝方向；
- 3% 初始止损，Volume-ATR 目标限制在 1.8R～3R，最长持有 48 小时；
- 单边手续费 5 bps、等价滑点 3 bps；同一 symbol 持仓时忽略重复信号更新；
- 窗口开始前的有效 setup 仍会消费当时状态，禁止在研究窗口边界伪造重新 armed。

## 3. 预注册判定

V1 冻结对照为：167,461 次 armed、167,364 次中性重置、20,644 个状态后 setup、
18,360 笔实际交易、约 1,465.31 笔/月、胜率 35.6427%、净 EV -0.072563R、
PF 0.888247、Sharpe -7.194071、最大单币隔离回撤 82.4487%。

- 首先验证 V2 的 armed、重置、状态后 setup 和实际交易都低于 V1，确认稳定重置生效；
- 组合频率目标仍为 50～120 笔/月；不能把原始 setup 当作交易频率；
- 胜率必须高于 V1，但不能以降低交易数单独宣称提升；
- 研究继续条件至少要求净 EV `>0` 且净 PF `>1`；
- 职业级晋级仍要求净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`，并满足风险、
  分段、有效事件和统一组合门禁；
- 同时报告前后半段、四分位和移除前三盈利币结果；
- 若净 EV 或 PF 仍为负向，本轮立即淘汰，不在已查看结果上调整 8 根确认窗口。

## 4. 冻结命令

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-current-live-only \
  --sample-limit 1000 \
  --sample-seed current_live_stable_reset_v2_20260721 \
  --trade-direction both \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 0 \
  --entry-min-volume-ratio 2 \
  --entry-min-range-expansion-ratio 1.4 \
  --entry-extreme-volume-contrarian \
  --entry-once-per-opposite-trend-state \
  --entry-opposite-trend-reset-confirm-candles 8 \
  --entry-opposite-move-lookback-candles 192 \
  --entry-min-opposite-net-move-pct 8 \
  --entry-min-opposite-duration-candles 96 \
  --entry-opposite-duration-min-r-squared 0.60 \
  --volume-atr-take-profit \
  --volume-atr-target-scale 4 \
  --volume-atr-min-target-r 1.8 \
  --volume-atr-max-target-r 3 \
  --backtest-fee-bps-per-side 5 \
  --backtest-slippage-bps-per-side 3 \
  --trend-timeframe off \
  --event-start-ms 1751328000000 \
  --event-end-ms 1784278800000 \
  --ignore-entry-signal-updates-while-open \
  --equity-max-holding-hours 48 \
  --equity-report \
  --equity-split-report \
  --equity-quartile-report \
  --equity-trigger-report \
  --equity-concentration-report \
  --equity-symbol-window-report \
  --min-trades 1 \
  --paper-outcome-entry-rule-version \
  kline15m_trend_exhaustion_extreme_volume_stable_reset_v2
```

## 5. 冻结回放结果

回放严格使用第 4 节命令，未修改第 1～4 节参数。扫描 216 个候选 15m 表，结果如下：

| 指标 | V1 对照 | V2 稳定重置 | 变化 |
|---|---:|---:|---:|
| 有效 setup（状态去重前） | 66,902 | 66,902 | 0 |
| armed 次数 | 167,461 | 97,037 | -42.05% |
| 中性重置次数 | 167,364 | 96,930 | -42.08% |
| 状态去重后 setup | 20,644 | 17,566 | -14.91% |
| 实际交易 | 18,360 | 16,288 | -11.29% |
| 月均实际交易 | 约 1,465.31 | 约 1,299.94 | -11.29% |
| 胜率 | 35.6427% | 36.2782% | +0.6355 个百分点 |
| 净 EV | -0.072563R | -0.070694R | +0.001869R |
| 净 PF | 0.888247 | 0.889728 | +0.001481 |
| 交易级 Sharpe | -7.194071 | -6.688597 | +0.505474 |
| 最大单币隔离回撤 | 82.4487% | 82.3868% | -0.0618 个百分点 |

V2 前半段为 8,201 笔、EV `-0.072460R`、PF `0.889494`；后半段为
8,087 笔、EV `-0.068573R`、PF `0.890504`。四分位均为负：

| 分段 | 交易数 | 净 EV | 净 PF | 胜率 | Sharpe |
|---|---:|---:|---:|---:|---:|
| Q1 | 4,103 | -0.003735R | 0.993967 | 38.2891% | -0.174364 |
| Q2 | 4,103 | -0.140128R | 0.797544 | 32.3666% | -6.537980 |
| Q3 | 4,060 | -0.079322R | 0.877178 | 35.8621% | -3.764324 |
| Q4 | 4,029 | -0.060228R | 0.901036 | 38.4711% | -2.928033 |

移除盈利最高的 `TRUTH-USDT-SWAP`、`MASK-USDT-SWAP`、`TIA-USDT-SWAP`
后，剩余 16,030 笔交易，胜率 36.1884%，总利润由 `-3,344.70U` 降为
`-3,487.77U`；结果不是由单个头部盈利币掩盖，底层总体本身为负。

## 6. 判定与后续边界

V2 证明连续八根中性确认能够减少状态抖动，但只减少 11.29% 的实际交易，胜率仅增加
0.64 个百分点，EV/PF 仍为负向，四个时间分段全部未形成优势，月频仍远高于 50～120
笔目标。按第 3 节停止规则，本版本状态为
`rejected_stable_reset_without_edge`，保持 `research_only`、
`promotion_eligible=false`。

本轮没有把确认窗口改成 4/12 根，也没有追加 MACD、FVG、BOS、影线或其他后验参数。
后续不得继续扫描重置长度；若继续研究，应先在新版本中预登记并分别诊断多空方向，
再验证极端量出现后、生产可见短窗口内的价格拒绝/回收确认是否能提供独立优势。当前
结果未写入回测业务表，未进入 Paper/Live，也未触发任何交易 mutation。
