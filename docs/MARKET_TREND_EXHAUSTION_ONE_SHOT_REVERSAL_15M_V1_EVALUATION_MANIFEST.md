# Market Trend Exhaustion One-Shot Reversal 15m V1 评估清单

## 1. 策略身份与本轮唯一变化

- 产品 slug：`market-trend-exhaustion-one-shot-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_trend_exhaustion_extreme_volume_one_shot_v1`
- 状态：`research_only`，`promotion_eligible=false`

本策略是独立的趋势耗竭反转家族，不覆盖 v36。冻结的历史背景、极端量、出场和成本
保持上一轮口径；本轮唯一变化是把连续历史背景改成一次性状态：

1. 信号 K 线之前 192 根净上涨/下跌至少 8%，或前 96 根同向线性趋势且 R² 至少 0.60；
2. 同时满足多空背景时按中性处理，不选择事后更有利的方向；
3. 趋势从中性首次成立后进入 armed，只消费第一个量比至少 2.0、振幅扩张至少 1.4 倍
   的极端量 K 线；
4. 实体占振幅至少 20% 时反向当前实体，低于 20% 时按更长影线对应的拒绝方向入场；
5. 第一个有效 setup 一旦出现就进入 consumed，即使后续未成交也不允许同一趋势重复信号；
6. 只有历史背景至少一根完整 15m K 线回到中性后才能重新 armed；方向直接翻转不能绕过重置。

## 2. 数据、时序与币池

- 研究窗口：`2025-07-01 08:00:00` 至 `2026-07-17 17:00:00`，Asia/Shanghai；
- 只读取本地 `quant_core` 的已完成 15m K 线，不读取 1m、`market_rank_events` 或 episode；
- 只加载 `exchange_symbols` 中当前状态为 `live`、线性、USDT 永续且存在 15m 表的 OKX
  交易对；按用户要求不纳入退市币；
- 当前 live 过滤属于幸存币口径，仍存在幸存者偏差，不能宣称 point-in-time 全市场无偏；
- 历史背景排除当前信号 K 线；信号在 K 线完成时才可见；后续 K 线只用于出场；
- 3% 初始止损，Volume-ATR 目标限制在 1.8R～3R，最长持有 48 小时；
- 单边手续费 5 bps、等价滑点 3 bps；同一 symbol 持仓时忽略重复信号更新。

## 3. 预注册判定

- 首先比较去重前有效 setup、一次性状态 setup 与最终实际成交，确认频率下降来自状态消费；
- 胜率必须相对未去重基线有实质改善，但不单独作为晋级条件；
- 净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`；
- 同时报告月频、前后半段、四分位、币种集中度和移除前三盈利币结果；
- 15m 全市场组合频率目标仍为 50～120 笔/月；去重不能通过保留少数头部赢家伪造提升；
- 任一核心收益门槛失败时，V1 保持研究淘汰，不调整趋势、量比、振幅或退出参数救参。

## 4. 冻结命令

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-current-live-only \
  --sample-limit 1000 \
  --sample-seed current_live_one_shot_20260721 \
  --trade-direction both \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 0 \
  --entry-min-volume-ratio 2 \
  --entry-min-range-expansion-ratio 1.4 \
  --entry-extreme-volume-contrarian \
  --entry-once-per-opposite-trend-state \
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
  --equity-trade-report \
  --min-trades 1 \
  --paper-outcome-entry-rule-version \
  kline15m_trend_exhaustion_extreme_volume_one_shot_v1
```

## 5. 冻结结果（2026-07-21）

第 1～4 节保持预登记原文，按冻结命令完成只读复算：

- 当前 live OKX USDT 线性永续候选表：216 个；框架实际成交覆盖：166 个；
- 趋势 armed episode：167,461 次；中性重置：167,364 次；
- 去重前有效极端量 setup：66,902 个；一次性状态发出：20,644 个，减少 69.14%；
- 最终同币持仓锁实际成交：18,360 笔，约 1,465.31 笔/月；相对上一轮同口径
  31,236 笔基线减少 41.22%，仍远高于预登记 50～120 笔/月目标；
- 交易胜率：35.6427%；净 EV：-0.072563R；净 PF：0.888247；
  交易级 Sharpe：-7.194071；最大单币隔离回撤：82.4487%；
- 前半段：9,260 笔，EV -0.073803R，PF 0.888539；
  后半段：9,102 笔，EV -0.070859R，PF 0.888627；
- Q1～Q4 EV 依次为 -0.000893R、-0.145743R、-0.104120R、-0.044889R，
  PF 依次为 0.998565、0.792405、0.842390、0.927193；四段均未形成正期望；
- 组合总利润为负，移除前三个正贡献币后总利润进一步从 -3,748.6763U 降到
  -3,894.1140U，不存在少数头部赢家掩盖总体亏损的问题。

## 6. 判定与后验诊断

V1 判定为 `rejected_frequency_and_edge`，保持 `research_only`，不得进入 Paper、Live
或覆盖 v36。一次性消费机制本身按预期删除了同背景重复 setup，胜率相对上一轮已报告
的 31.37% 基线上升约 4.27 个百分点，但净 EV、PF、Sharpe、回撤和频率全部失败，
不能把“胜率上升”解释为策略优势。

主要后验问题是重置定义不足：只要求一根 15m K 线的历史背景回到中性，使 192 根净变化
和 96 根回归趋势的 OR 状态在阈值附近频繁 neutral/armed 抖动；167,461 次 armed episode
证明当前实现并未把长期趋势压缩成少量稳定状态。该观察只用于定义下一份预登记假设，
不得回头修改 V1。后续若继续，应独立验证“稳定中性持续 + 趋势真正失效/反向确认”的
重置规则，并继续只使用当时已完成的 15m K 线。
