# Market Momentum Opposite Move v32 两阶段筑底恢复评估清单

## 1. 策略身份与证据边界

v32 仍属于 `market_momentum_opposite_move_reversal`，但使用独立、可审计的研究版本：

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_two_stage_recovery_v32`
- 状态：`research_only`，`promotion_eligible=false`

V28 的 27 笔强反转开发交易显示，盈利集中在一个市场级事件；V30/V31 又证明长下影与
8%/R² 灰区不能修复后半段。随后进行的只读特征核对发现：7 笔盈利交易在信号 K 线前
的两个连续 24 根窗口均为正收益，而多数亏损交易仍是“前段下跌、后段反抽”。这个发现
使用了已知结果，因此 v32 只能验证结构是否有区分度，不能作为未见或晋级证据。

## 2. 本轮唯一变化

完整保留 v28 的 Top60、long-only、192 根下跌 10%或96 根下降趋势、历史极值量能
比例 1.0、BTC 96 根震荡、当前反转阳线至少 3%、实体突破、EMA20/SMA20 收回、3%
初始止损、Volume-ATR 1.8R～3R、两根无浮盈早退和显式成本。

只新增“两个连续 6 小时段均已恢复”的信号时点门禁：

1. 排除当前信号 K 线，只读取它之前已经完成的 48 根 15m K 线；
2. 按时间顺序拆成较早 24 根和最近 24 根；
3. 两段各自的末根收盘都必须严格高于首根收盘；
4. 任一窗口历史不足、价格无效或净幅不大于 0 都失败关闭。

24 根是既有 96 根持续趋势窗口的四分之一，对应 6 小时；0% 是方向边界。本轮不扫描
12/18/32 根，也不扫描最小涨幅。该门禁不修改 192/96 的长期下跌事实，而是区分长期
下跌后的多阶段恢复与单根反抽。

## 3. 开发停止规则

查看回放结果前冻结：

- 原始成交至少 25 笔；按同方向、相邻触发不超过 30 分钟归并后的有效事件至少 20 个；
  任一不足即判样本不足，不因表面高 EV/PF 晋级；
- 净 EV `>=0.6R`、PF `>=2.2`、后半段 EV `>0R`、交易级 Sharpe `>=1.5`；
- 少于半数成交币种亏损，且移除前三个盈利币种后组合仍为正；
- 不运行 12/18/32 根邻域，不按日期、币种或结果改门槛；
- 即使开发门槛全部通过，也必须冻结新的未见 universe/time window，并补 point-in-time
  universe、资金费、费用压力、统一资金/容量/相关簇、日权益 Sharpe、逐时盯市回撤、
  Recovery、BTC/ETH/其他 Tier 与月度市场状态覆盖后才可晋级。

15m 的 50～120 笔/月仍作为组合频率目标；频率不足不单独否定盈利性，但必须将策略
降级为低频事件候选，不能表述为常规 15m 交易策略。

## 4. 预注册命令

不带 `--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v32_two_stage_recovery_20260720 \
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

使用第 4 节预注册命令完成 Top60 回放，未写入回测明细：

- 650 个原始强反转候选中 10 个通过完整信号门禁，BTC 震荡过滤后成交 9 笔，覆盖
  9 个币种。
- 9 笔中 7 笔盈利、2 笔亏损；净收益 19.4499R、净 EV 2.1611R、PF 17.7676、
  胜率 77.78%、交易级 Sharpe 4.1696、逐币隔离最大回撤 2.0495R。
- 7 笔盈利全部发生在 2025-10-12 15:00～15:15 UTC，同方向 30 分钟聚类后只算
  1 个有效市场事件；另有 2026-02-25 与 2026-06-13 两个亏损事件。因此全样本仅
  3 个有效事件，远低于预注册的 20 个下限。
- 时间切分的“后半段”仍含 2025-10-12 的 3 笔相关盈利，不能当成独立时间稳定性；
  真正最后四分位仅 3 笔，EV 0.5948R、Sharpe 0.5056，仍低于职业门槛。
- 组合频率约 0.69 笔/月，有效事件频率约 0.23 个/月；既不满足 15m 频率目标，也
  无法估计跨市场状态的置信区间。

## 7. 决策

**v32 按样本量停止规则淘汰，不冻结未见池，不进入 paper/read-only shadow。**

高 EV/PF 来自同一次市场冲击下的七个相关币种，不能视为七个独立成功样本。v32 只保留
一条研究结论：两个连续 6 小时段均恢复，比单根阳线或长下影更有区分度。下一轮如继续，
必须作为独立版本检验预先提出的 8% / R² 0.60 历史缓冲能否增加有效事件；不得改选
12/18/32 根窗口或按日期、币种保留盈利样本。
