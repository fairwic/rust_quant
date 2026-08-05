# Market Momentum Opposite Move v37 因果结构突破评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_structure_break_v37`
- 状态：`research_only`，`promotion_eligible=false`

v37 从已淘汰的 v36 分叉，完整保留其 192 根/8%、96 根/R² 0.60、两个连续
24 根恢复窗口、当前收阳、实体与收盘位置、EMA20/SMA20 收复、BTC 96 根震荡、
当前量比 1.5、成交额增长、3% 初始止损、Volume-ATR 1.8R～3R 动态目标、48h
最大持仓与双边成本。

唯一变化：做多信号 K 线收盘必须从下向上突破最近一个已确认的 15m 摆动高点。
摆动点固定使用左右各 5 根 K 线确认，只在最近 192 根已完成 K 线内查找；前一根
收盘必须不高于该结构位，当前收盘必须高于。确认只读取信号时已经完成的 K 线，
不使用信号后的 1m、15m 或盘口数据。

本轮不把 FVG、BOS 或 CHoCH 加入门禁。预分析中 FVG 有无两组净 EV 基本相同；
精确 BOS 的 PF 与 Sharpe 未达门槛；CHoCH 只有 2 笔。泛化结构突破是唯一进入
本轮的单变量假设，不扫描 pivot、lookback 或 gap 参数。

## 2. 停止规则

- 原始成交至少 30 笔，30 分钟同方向聚类后至少 20 个有效事件；
- 净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`；
- 后半段净 EV `>0R` 且 PF `>=1.2`；
- Q3、Q4 均必须有盈利交易，且各自净 EV `>0R`；
- 移除前三个盈利币种后总收益仍为正，最大回撤不高于 10%；
- 组合频率仍按 15m 目标 `50～120` 笔/月评估，不以提高单笔质量掩盖频率不足；
- 任一停止线失败即淘汰 v37，不调整 5+5 pivot 或与 FVG/BOS 组合救参。

## 3. 预注册命令

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v36_volume_gate_dedup_20260720 \
  --trade-direction long \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 14 \
  --entry-min-volume-ratio 1.5 \
  --entry-opposite-move-lookback-candles 192 \
  --entry-min-opposite-net-move-pct 8 \
  --entry-min-opposite-duration-candles 96 \
  --entry-opposite-duration-min-r-squared 0.60 \
  --entry-btc-96-max-abs-net-move-pct 2 \
  --volume-atr-take-profit \
  --volume-atr-target-scale 4 \
  --volume-atr-min-target-r 1.8 \
  --volume-atr-max-target-r 3 \
  --backtest-fee-bps-per-side 5 \
  --backtest-slippage-bps-per-side 3 \
  --entry-require-two-stage-recovery \
  --entry-require-opposite-reversal-confirmation \
  --entry-require-reversal-average-reclaim \
  --entry-require-bullish-structure-break \
  --trend-timeframe off \
  --min-delta-rank 1 \
  --max-price-change-pct 8 \
  --event-start-ms 1751328000000 \
  --entry-trigger-allowlist all \
  --ignore-entry-signal-updates-while-open \
  --equity-max-holding-hours 48 \
  --equity-report \
  --equity-split-report \
  --equity-quartile-report \
  --equity-trigger-report \
  --equity-concentration-report \
  --equity-symbol-window-report \
  --equity-trade-report \
  --min-trades 1
```

## 4. 待填写结果

执行前保持为空；不得根据结果修改第 1～3 节。

## 5. 开发池结果（2026-07-20）

- v37 实际成交 27 笔，按同方向且相邻不超过 30 分钟粗聚类为 20 个时间事件；
  聚类事件刚到下限，但原始成交少于 30 笔，组合频率约 2.1 笔/月。
- 扣除双边手续费与滑点后，总体净 EV 0.9649R、PF 3.3729、胜率 55.56%、
  交易级 Sharpe 2.7644、逐币隔离最大回撤 9.1768%。总体质量高于 v36。
- 前半段 13 笔净 EV 1.5547R、PF 8.8984；后半段 14 笔净 EV 0.4173R、
  PF 1.6938、Sharpe 0.8657。后半段仍未达到职业级联合目标。
- Q1、Q2、Q3 为正；Q4 只有 7 笔，净 EV -0.6529R、PF 0.2764、胜率
  14.29%、Sharpe -1.6334。结构突破没有消除 2026 年后段的状态失效。
- 移除前三个盈利币种后总收益仍为正，但 2025-10-12 同一市场冲击仍贡献多笔
  盈利；过滤后独立样本和月频进一步下降，不能只用总体 EV/PF 宣称改进成功。

## 6. 决策

**v37 淘汰，不进入 paper/read-only shadow。**

泛化结构突破可以保留为研究报告中的质量标签或候选排序特征，但不得作为当前版本的
硬开仓门禁：它把 v36 已不足的约 4.5 笔/月进一步压到约 2.1 笔/月，同时 Q4 和
后半段稳定性仍失败。FVG、精确 BOS、CHoCH 不与 v37 组合救参；若继续追求 15m
组合目标，应验证独立的新信号来源或新策略家族，而不是继续叠加过滤器。
