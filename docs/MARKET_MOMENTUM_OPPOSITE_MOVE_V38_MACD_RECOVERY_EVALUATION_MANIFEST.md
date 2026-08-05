# Market Momentum Opposite Move v38 MACD 动量衰减评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_v36_macd_recovery_v38`
- 状态：`research_only`，`promotion_eligible=false`

v38 从 v36 分叉，完整保留 v36 的币池、15m 信号、历史下跌双分支、两阶段恢复、
反转阳线、前高突破、均线收复、BTC 震荡门禁、成交量与成交额门禁、止损、止盈、
成本、最长持仓和同币持仓锁。唯一变化是新增标准收盘价 MACD(12,26,9) 门禁：

- 信号前一根已完成 15m K 线的 MACD 柱体必须小于 0；
- 当前信号 K 线的 MACD 柱体必须高于前一根，允许当前柱仍为负，也允许刚越过 0；
- 不扫描周期、金叉、零轴阈值、柱体改善幅度或连续改善根数。

假设是：v36 的价格与量能反转确认中仍混入下行动量继续加速的样本；负柱缩短可以只
删除这类样本，并改善扩窗期的净 EV、PF 和时间稳定性。MACD 使用信号时已经完成的
15m 收盘价顺序计算，不读取信号后的 K 线，也不改变下一根可执行价格和退出逻辑。

## 2. 数据边界

- 开发窗口：与 v36 相同，当前仍为 live 的 Top60，`2025-07-01` 起；
- 扩展窗口：`2024-07-01` 至 `2025-06-30`，使用已冻结的逐月 Top60 manifest；
- 按用户要求不纳入退市币；因此扩展窗口仍有“当前 live”幸存者偏差，只能评价当前
  幸存币的历史表现；
- 本轮不查询或回放新的 1m K 线；开仓、MACD 和结果回放均来自本地 15m 数据表；
- 扩窗 manifest 内记录的官方 1m 仅是此前构建 15m 历史表时的原始材料，不参与本轮
  信号计算。

## 3. 预注册停止规则

开发窗口先与同一二进制重放的 v36 基线核对，再执行 v38。扩窗是决定性结果：

- v38 原始成交至少 30 笔、30 分钟同方向聚类后至少 20 个有效事件；
- 净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`、逐币隔离最大回撤 `<=10%`；
- 后半段 EV `>0R` 且 PF `>=1.2`，最后四分位 EV `>0R`；
- 扩窗至少 8/12 个月为正，最后连续 3 个月合计为正；
- 移除前三个盈利币和前三个盈利事件后仍为正；
- 同时报告实际成交与有效市场事件，频率不足不能由重复相关币信号补足；
- 若扩窗的 EV、PF、时间稳定性或样本量任一失败，MACD 增量假设淘汰，不继续扫描
  MACD 参数、金叉、零轴或柱体阈值。

## 4. 冻结命令

开发窗口 v38 在 v36 命令末端仅新增 MACD 门禁和独立规则版本：

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 --sample-seed top60_v36_volume_gate_dedup_20260720 \
  --trade-direction long --stop-loss-pct 0.03 --target-rs 1 \
  --entry-period 20 --entry-max-distance-pct 14 --entry-min-volume-ratio 1.5 \
  --entry-opposite-move-lookback-candles 192 --entry-min-opposite-net-move-pct 8 \
  --entry-min-opposite-duration-candles 96 \
  --entry-opposite-duration-min-r-squared 0.60 \
  --entry-btc-96-max-abs-net-move-pct 2 --volume-atr-take-profit \
  --volume-atr-target-scale 4 --volume-atr-min-target-r 1.8 \
  --volume-atr-max-target-r 3 --backtest-fee-bps-per-side 5 \
  --backtest-slippage-bps-per-side 3 --entry-require-two-stage-recovery \
  --entry-require-macd-negative-histogram-improving \
  --entry-require-opposite-reversal-confirmation \
  --entry-require-reversal-average-reclaim --trend-timeframe off \
  --min-delta-rank 1 --max-price-change-pct 8 --event-start-ms 1751328000000 \
  --entry-trigger-allowlist all --ignore-entry-signal-updates-while-open \
  --equity-max-holding-hours 48 --equity-report --equity-split-report \
  --equity-quartile-report --equity-trigger-report --equity-concentration-report \
  --equity-feature-report --equity-symbol-window-report --equity-trade-report \
  --min-trades 1 --paper-outcome-entry-rule-version \
  kline15m_market_momentum_opposite_reversal_v36_macd_recovery_v38
```

扩窗命令把开发池参数替换为已冻结 manifest 与完整时间边界，其余参数不变：

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --historical-universe-manifest /private/tmp/market_momentum_current_live_202407_202506.json \
  --event-start-ms 1719792000000 --event-end-ms 1751327999999 \
  --trade-direction long --stop-loss-pct 0.03 --target-rs 1 \
  --entry-period 20 --entry-max-distance-pct 14 --entry-min-volume-ratio 1.5 \
  --entry-opposite-move-lookback-candles 192 --entry-min-opposite-net-move-pct 8 \
  --entry-min-opposite-duration-candles 96 \
  --entry-opposite-duration-min-r-squared 0.60 \
  --entry-btc-96-max-abs-net-move-pct 2 --volume-atr-take-profit \
  --volume-atr-target-scale 4 --volume-atr-min-target-r 1.8 \
  --volume-atr-max-target-r 3 --backtest-fee-bps-per-side 5 \
  --backtest-slippage-bps-per-side 3 --entry-require-two-stage-recovery \
  --entry-require-macd-negative-histogram-improving \
  --entry-require-opposite-reversal-confirmation \
  --entry-require-reversal-average-reclaim --trend-timeframe off \
  --min-delta-rank 1 --max-price-change-pct 8 \
  --entry-trigger-allowlist all --ignore-entry-signal-updates-while-open \
  --equity-max-holding-hours 48 --equity-report --equity-split-report \
  --equity-quartile-report --equity-trigger-report --equity-concentration-report \
  --equity-feature-report --equity-symbol-window-report --equity-trade-report \
  --min-trades 1 --paper-outcome-entry-rule-version \
  kline15m_market_momentum_opposite_reversal_v36_macd_recovery_v38
```

## 5. 回放结果（2026-07-21）

### 5.1 开发池可复现性

同一 seed 在当前日期重新按 live 币抽样时，Top60 已混入大量 2026 年上市合约，
`2025-07-01` 起窗口的原始候选为 0。该动态币池已不等于 v36 当时的开发池，因此没有
用 0 笔结果评价 v38，也没有用当前币表补选旧币。决定性对比改用冻结历史 manifest。

### 5.2 扩窗 v36 基线复核

同一 release 二进制、同一 manifest 和同一成本口径复现 v36：

- 原始候选 44,799 个，信号门禁后 129 个，BTC 门禁后成交 84 笔；
- 净 EV `-0.0194R`、PF `0.9719`、胜率 `29.76%`、Sharpe `-0.1122`；
- 逐币隔离最大回撤 `9.1768%`；
- 前半段 `0.0343R / PF 1.0503`，后半段 `-0.0731R / PF 0.8954`；
- Q1/Q2/Q3/Q4 EV 分别为 `-0.7868R / 0.8555R / 0.1075R / -0.2537R`。

结果与 v36 历史扩窗清单完全一致，说明新增 MACD 字段在门禁关闭时没有改变基线。

### 5.3 v38 MACD 增量结果

- MACD 将信号门禁后的候选从 129 个降到 23 个，BTC 门禁后只剩 17 笔；相对 v36
  删除 `79.76%` 的实际成交，组合频率约 `1.42 笔/月`，有效事件不可能达到 20 个；
- 净 EV 改善到 `0.1870R`，PF 改善到 `1.2927`，胜率 `35.29%`，Sharpe
  `0.4431`，最大回撤降到 `3.1576%`；
- 前半段仍为 `-0.3724R / PF 0.5283`，后半段为 `0.6841R / PF 2.3550`；
- Q1/Q2/Q3/Q4 EV 分别为 `-0.6914R / -0.0533R / 1.0395R / 0.3999R`；
- 12 个完整月中只有 2024-12 和 2025-06 为正，即 `2/12`；最后三个月合计为正，
  但 2025-04 无成交、2025-05 亏损，不能弥补全年覆盖失败；
- 移除 TURBO、ANIME、POPCAT 三个盈利币后，剩余 14 笔总收益为
  `-16.9635U`，收益集中度门禁失败。

## 6. 决策

**v38 淘汰，不进入 paper/read-only shadow，不继续扫描 MACD 参数。**

MACD 负柱缩短对扩窗样本有方向性筛选价值：它把略亏结果变成小幅盈利，并降低回撤；
但增益来自删掉约八成交易，最终只剩 17 笔，EV、PF、Sharpe、样本量、频率、月份覆盖
和去头部集中度全部未达标。该条件可保留为研究诊断标签，不能作为 v36 的可晋级开仓
门禁。后续不得在同一已见窗口继续扫描金叉、零轴、周期或柱体幅度来救活该分支。
