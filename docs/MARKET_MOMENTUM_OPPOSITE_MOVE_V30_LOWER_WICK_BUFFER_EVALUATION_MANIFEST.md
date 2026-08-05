# Market Momentum Opposite Move v30 长下引线灰区缓冲评估清单

## 1. 策略身份与可证伪假设

v30 从 v28 分叉为独立研究规则版本，不覆盖 v28/v29：

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_lower_wick_buffer_v30`
- 运行范围：仅本地 ResearchBar，`promotion_eligible=false`

本轮只检验一个假设：保留 v28 的 A 级强反转分支，同时增加 B 级长下引线分支。B 级
允许触发 K 线为小阳线或小阴线，但必须形成下影扫低，并由下一根完整 15m K 线确认后，
再以下一根开盘价成交。B 级不得在触发 K 线收盘立即成交。

历史窗口不缩短：固定净跌幅仍看前 192 根，线性下降趋势仍看前 96 根。只给 B 级增加
灰区：前 192 根净跌幅达到 `8%`，或前 96 根下降回归 `R² >= 0.60`。A 级仍要求
`10%` 或 `R² >= 0.70`，且当前阳线实体涨幅至少 `3%`。

## 2. 冻结的形态与时序规则

B 级 setup 在信号时点必须同时满足：

- 下影线占整根振幅至少 `45%`；
- 收盘位置位于整根振幅上方 `65%` 区域；
- 最低价严格低于上一根最低价，证明发生向下扫流动性；
- 整根振幅至少为信号时点 ATR14 的 `1.2` 倍；
- 成交量至少为前 20 根均量的 `1.5` 倍；
- 仍通过 v28 的历史极值量能比例 `1.0` 与 BTC 96 根净幅不超过 `2%` 门禁。

确认只允许使用紧随 setup 的下一根完整 15m K 线：它必须是强阳线，实体占振幅至少
`45%`、收盘位于振幅上方至少 `65%`、收盘突破 setup 最高价，并重新站上 EMA20 与
SMA20。确认后只允许在再下一根 K 线开盘成交。缺少下一根确认、确认失败、无下一根
成交 K 线或任一数据不足均失败关闭。

## 3. 开发池、有效事件与停止规则

Top60 已反复查看，整池都是开发样本。本轮结果不能用于晋级，只用于决定该分支是否值得
冻结到新的未见池。查看结果前固定以下规则：

- 原始成交与 30 分钟同方向时间聚类后的有效市场事件同时报告；同一时段多币共振不能
  当作多个独立统计样本；
- A、B 两个 trigger 分开报告；B 级至少需要 20 个有效事件，否则只判样本不足；
- B 级净 EV `<= 0R`、PF `<= 1`、后半段 EV `<= 0R`、半数及以上成交品种亏损，
  或移除前三个盈利品种后转负，任一成立即淘汰 B 级；
- 合并后若净 EV `< 0.6R`、PF `< 2.2`、后半段净 EV `<= 0R`，不冻结未见样本；
- 本轮不扫描 7%/9%、`R² 0.55/0.65`、影线 40%/50% 或确认等待 2～3 根等邻域，
  避免在已见样本追优；
- 即使开发结果通过，也必须先冻结新的 point-in-time universe 与未见时间窗，再补资金费、
  费用/滑点压力、参数邻域、统一组合容量、相关簇、日权益 Sharpe、逐时盯市回撤与
  Recovery；未完成前不得进入 Paper/Live。

有效事件的预注册下界聚类为：按交易方向相同、相邻触发时间不超过 30 分钟归并。由于
当前研究数据没有 point-in-time 板块与相关性快照，这只是最宽松时间聚类下界，不冒充
完整相关簇样本。

## 4. 预注册执行命令

本轮不带 `--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v30_lower_wick_buffer_20260720 \
  --trade-direction long \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 14 \
  --entry-min-volume-ratio 1.5 \
  --entry-opposite-move-lookback-candles 192 \
  --entry-min-opposite-net-move-pct 10 \
  --entry-min-opposite-duration-candles 96 \
  --entry-min-exhaustion-volume-dominance-ratio 1 \
  --entry-btc-96-max-abs-net-move-pct 2 \
  --volume-atr-take-profit \
  --volume-atr-target-scale 4 \
  --volume-atr-min-target-r 1.8 \
  --volume-atr-max-target-r 3 \
  --backtest-fee-bps-per-side 5 \
  --backtest-slippage-bps-per-side 3 \
  --entry-defer-long-lower-wick-reversal \
  --entry-defer-max-wait-candles 1 \
  --trend-timeframe off \
  --min-delta-rank 1 \
  --max-price-change-pct 8 \
  --event-start-ms 1751328000000 \
  --entry-trigger-allowlist all \
  --early-exit-no-profit-candles 2 \
  --ignore-entry-signal-updates-while-open \
  --equity-report \
  --equity-split-report \
  --equity-quartile-report \
  --equity-trigger-report \
  --equity-concentration-report \
  --equity-feature-report \
  --equity-symbol-window-report \
  --equity-trade-report \
  --min-trades 1
```

## 5. 执行结果与结论

- 移除 3% 事件源下限后，排名源产生 62,252 个原始事件；241 个通过信号层，其中
  200 个在“下一根确认”阶段失败，41 个形成实际入场，BTC 震荡门禁后剩 29 笔。
- 29 笔包含 27 笔 A 级强反转与 2 笔 B 级长下引线。按预注册 30 分钟同方向时间
  归并，A 级沿用 v28 的 21 个有效事件下界，2 笔 B 级分别发生在不同日期，合并下界
  为 23 个；B 级自身只有 2 个有效事件，远低于 20 个停止线。
- 合并后累计 `+14.2595R`、净 EV `+0.4917R`、PF `2.5342`、胜率 `27.59%`、
  trade Sharpe `1.6898`、symbol-isolated 最大回撤 `3.36%`。PF 与 trade Sharpe 达线，
  但 EV 仍低于 `0.6R`，且这不是统一组合日权益 Sharpe/逐时盯市回撤。
- B 级 2 笔为 1 胜 1 近乎持平，净 EV `+1.4455R`、PF `55.205`；样本极小，
  只能说明规则可产生交易，不能说明其统计有效。
- 前半段 14 笔 EV `+1.4584R`、PF `7.5095`；后半段 15 笔全部没有盈利，
  EV `-0.4105R`、PF `0`。Q3/Q4 也均无盈利，时间失效与 v28 相同。
- 23 个成交品种只有 8 个盈利；移除前五个盈利品种后剩余组合转为 `-1.4980U`。
  A 级的 7 个主要盈利仍集中于 2025-10-12 同一次市场反转事件。

v30 同时触发 B 级有效事件不足、合并 EV 不达标、后半段 EV 不达标、亏损品种过半和
移除头部盈利后转负五条停止线，按预注册规则淘汰，不进入未见样本、Paper 或 Live。
下一版不得通过把确认等待延长到 2～3 根来追样本；若继续研究长下引线，必须提出新的
形态/成交语义假设并建立独立版本。
