# Market Momentum Opposite Move v33 缓冲历史与两阶段恢复评估清单

## 1. 策略身份与唯一假设

v33 使用独立研究版本，不覆盖 v28、v30～v32：

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_buffered_two_stage_recovery_v33`
- 状态：`research_only`，`promotion_eligible=false`

v32 的两阶段恢复只产生 3 个有效事件。v33 只检验用户在查看 v30/v31 结果前已经提出
的历史缓冲：保留 192 根与 96 根窗口长度，把累计下跌门槛从 10% 改为 8%，并把 96
根下降回归的 R² 从 0.70 改为 0.60。两者仍为“任意一个成立”，不是同时要求。

本轮不启用长下影或锤头分支。当前 K 线仍必须是至少上涨 3% 的强反转阳线、突破前高、
收回 EMA20/SMA20；信号前两个连续 24 根窗口仍必须各自上涨。历史缓冲只能增加长期
反向走势候选，不能替代当前价格确认与两阶段恢复。

## 2. 固定边界

- 8% 与 R² 0.60 是预先提出的灰区值，本轮不扫描 7%/9% 或 0.55/0.65；
- 192/96、两个 24 根恢复窗口、3% 强反转、量能、BTC 震荡、止损、止盈、两根早退、
  手续费和滑点全部冻结；
- 所有历史窗口截止于当前信号 K 线之前，当前信号后数据只用于退出；
- Top60 已见开发池仅用于证伪，任何结果都不能直接晋级。

## 3. 开发停止规则

- 原始成交至少 25 笔，30 分钟同方向聚类后有效事件至少 20 个；不足即样本淘汰；
- 净 EV `>=0.6R`、PF `>=2.2`、后半段 EV `>0R`、Sharpe `>=1.5`；
- 少于半数成交币种亏损，移除前三个盈利币种后仍为正；
- 不追加门槛邻域，不按盈利日期、币种或市场事件修改规则；
- 即使开发门槛通过，也必须冻结全新未见时间窗/universe，并完成 point-in-time universe、
  资金费、费用压力、统一资金和容量、相关簇风险、日权益 Sharpe、逐时盯市回撤、
  Recovery 与 Tier/月份/市场状态覆盖后才能晋级。

## 4. 预注册命令

不带 `--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v33_buffered_two_stage_recovery_20260720 \
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
  --entry-min-exhaustion-volume-dominance-ratio 1 \
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
  --trend-timeframe off \
  --min-delta-rank 1 \
  --min-price-change-pct 3 \
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

## 5. 待填写结果

执行前保持为空；不得根据结果修改第 1～4 节。

## 6. 开发池结果（2026-07-20）

- 650 个原始强反转候选中 11 个通过信号门禁，BTC 震荡过滤后仍为 9 笔成交；v32
  对应为 10 个与 9 笔。
- 缓冲只在 BTC 过滤前增加 1 个候选，最终 9 笔的币种、时间、方向与结果和 v32
  完全相同。
- 因成交集合相同，净 EV 2.1611R、PF 17.7676、Sharpe 4.1696 等表面指标不变；
  7 笔盈利仍聚集为 2025-10-12 单一市场事件，全样本仍只有 3 个有效事件。
- 原始成交 9 笔与有效事件 3 个，分别低于预注册的 25 笔与 20 个下限。

## 7. 决策

**v33 淘汰，不进入未见、Paper 或 Live。**

8% / R² 0.60 在当前两阶段恢复结构下没有增加任何实际成交，说明 192/96 的历史严格度
不是主要瓶颈。后续不再扫描 7%/9% 或 R² 邻域；若继续增加样本，只能作为独立版本
检验当前 3% 强阳线幅度是否可由已经存在的实体突破、均线收回和两阶段恢复替代。
