# Market Momentum Opposite Move v36 量能门禁去重评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_volume_gate_dedup_v36`
- 状态：`research_only`，`promotion_eligible=false`

v36 从 v35 分叉，完整保留 192 根/8%、96 根/R² 0.60、两个连续 24 根恢复窗口、
当前收阳、45% 实体、65% 收盘位置、突破前高、EMA20/SMA20 收复、BTC 96 根震荡、
3% 初始止损、Volume-ATR 1.8R～3R 动态目标、48h 最大持仓与显式成本。

唯一变化是删除“当前反转量必须不弱于此前下跌极值量簇”的门禁。当前 K 线仍必须满足
自身成交量至少为历史均量 1.5 倍，且事件必须来自成交额排名改善并要求成交额增长。

假设是：反转量超过下跌极值量、当前量比和横截面成交额改善表达了重复的量能信息；
极端下跌阶段的单根爆量会把后续有效恢复永久压低。删除重复比较应增加独立事件，
但不能牺牲时间稳定性和净期望。

## 2. 停止规则

- 原始成交至少 30 笔，30 分钟同方向聚类后至少 20 个有效事件；
- 净 EV `>=0.6R`、PF `>=2.2`、后半段 EV `>0R`、Sharpe `>=1.5`；
- 最后四分位必须有盈利交易且净 EV `>0R`；
- 少于半数币种亏损，移除前三个盈利币种后仍为正；
- 组合频率报告实际成交和有效事件两个口径，不把同一市场冲击当独立样本；
- 不放宽当前量比 1.5、成交额增长、实体、收盘位置、前高突破、均线收复或历史缓冲；
- 若有效事件、时间稳定性或收益门槛任一失败，v36 直接淘汰。

## 3. 预注册命令

与 v35 相同，但删除 `--entry-min-exhaustion-volume-dominance-ratio 1`，并使用已修复的
严格 48h 权益窗口。不带 `--save-backtest-detail`：

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
  --equity-feature-report \
  --equity-symbol-window-report \
  --equity-trade-report \
  --min-trades 1
```

## 4. 待填写结果

执行前保持为空；不得根据结果修改第 1～3 节。

## 5. 开发池结果（2026-07-20）

- 删除重复极值量门禁后，信号层从 v35 的 35 个增加到 81 个，BTC 过滤后 63 个；
  48h 同币种持仓锁跳过 6 个，实际成交 57 笔，覆盖 41 个币种。
- 57 笔按同方向、相邻不超过 30 分钟归并后约 44 个有效事件，样本量门槛通过；
  组合成交频率约 4.5 笔/月，有效事件约 3.5 个/月，仍远低于 15m 组合目标频率。
- 严格 48h、扣除双边手续费与滑点后，净 EV 0.7568R、PF 2.7544、胜率 52.63%、
  交易级 Sharpe 3.3214、逐币隔离最大回撤 9.1768%，总体交易级指标达到门槛。
- 前半段 27 笔净 EV 1.5304R、PF 7.5871；后半段 31 笔净 EV 仅 0.0247R、
  PF 1.0395、Sharpe 0.0934。前后段样本因分币回放可各自多计一笔边界信号，不能
  将两段笔数直接相加替代完整 57 笔口径。
- Q1/Q2 为正；Q3 净 EV 0.4641R、PF 1.9406，已低于职业门槛；Q4 16 笔净 EV
  -0.3872R、PF 0.4823、Sharpe -1.3312，时间稳定性失败。
- 移除前三个盈利币种后总收益仍为正，但 2025-10-12 相关簇仍贡献大量盈利；2026 年
  1～3 月连续独立亏损，不能用总体 Sharpe 掩盖明显的状态失效。

## 6. 决策

**v36 淘汰，不冻结未见池，不进入 paper/read-only shadow。**

极值量比较确实是重复且过严的门禁：删除后样本和总体指标显著改善，因此后续研究不应
默认恢复它。但 v36 仍触发后半段、Q3、Q4、频率和市场状态稳定性停止线。当前开发池
已经证明继续组合影线、BTC 短窗、回踩或早退等已失败条件会形成结果驱动过拟合；下一步
必须补更长历史与可重建的 point-in-time universe，或将市场状态适配声明为新的策略
能力，而不能继续覆盖当前反转策略版本。
