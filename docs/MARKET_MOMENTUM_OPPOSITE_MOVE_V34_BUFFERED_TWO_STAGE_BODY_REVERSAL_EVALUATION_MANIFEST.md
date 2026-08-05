# Market Momentum Opposite Move v34 缓冲历史与两阶段实体反转评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_buffered_two_stage_body_v34`
- 状态：`research_only`，`promotion_eligible=false`

v34 从 v33 分叉，完整保留 192 根/8%、96 根/R² 0.60、两个连续 24 根恢复窗口、
历史极值量能、BTC 96 根震荡、3% 初始止损、Volume-ATR 1.8R～3R、两根无浮盈早退
与显式成本。唯一变化是删除当前 K 线“涨幅至少 3%”的绝对幅度门槛。

这不是允许任意收阳。当前 K 线仍必须：

- 收阳；
- 实体占整根振幅至少 45%；
- 收盘位于整根振幅上方至少 65%；
- 收盘突破上一根最高价；
- 收盘高于 EMA20 与 SMA20；
- 成交量与历史极值量能门禁继续通过。

假设是：长期下跌后若两个连续 6 小时窗口已恢复，方向结构比单根必须上涨 3% 更重要；
保留实体/突破/均线确认可以增加样本而不退回 v21 的任意弱阳线。

## 2. 停止规则

- 原始成交至少 30 笔，30 分钟同方向聚类后至少 20 个有效事件；不足即样本淘汰；
- 净 EV `>=0.6R`、PF `>=2.2`、后半段 EV `>0R`、Sharpe `>=1.5`；
- 少于半数币种亏损，移除前三个盈利币种后仍为正；
- 不扫描 1%/2% 当前涨幅，也不放宽 45% 实体、65% 收盘位置或前高突破；
- 开发通过后仍需全新未见时间/universe 与主项目完整组合、成本、资金费、回撤、Recovery、
  Tier、月份、市场状态和 point-in-time universe 门禁。

## 3. 预注册命令

与 v33 完全相同，但删除 `--min-price-change-pct 3`。不带 `--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v34_buffered_two_stage_body_20260720 \
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

## 4. 待填写结果

执行前保持为空；不得根据结果修改第 1～3 节。

## 5. 开发池结果（2026-07-20）

- 30,657 个原始候选中 35 个通过信号门禁，BTC 过滤后 29 个；同币种持仓锁跳过
  3 个，最终成交 26 笔，覆盖 23 个币种。
- 交易级净收益 23.0055R、净 EV 0.8848R、PF 4.5739、胜率 38.46%、Sharpe
  2.6863、逐币隔离最大回撤 4.7383R，表面上越过 EV/PF/Sharpe/回撤门槛。
- 26 笔按同方向、相邻不超过 30 分钟归并后约 15 个有效事件，低于 20 个下限；
  10 笔盈利全部来自 2025-10-12 15:00～15:15 UTC 的同一个市场事件。
- 2025-10-12 16:15 的后续簇与其余约 14 个独立事件均未盈利。最后四分位 8 笔
  胜率 0、EV -0.4156R、PF 0、Sharpe -4.0138，时间稳定性明确失败。
- 移除前三个盈利币种后交易级资金仍为正，但这些头部盈利属于同一相关市场冲击，
  不能替代有效事件集中度审计。

## 6. 决策

**v34 淘汰，不冻结未见池，不进入 paper/read-only shadow。**

删除 3% 幅度门槛确实增加了样本并改善交易级汇总，但没有增加独立盈利事件。当前问题
不再是“收阳是否太严格”，而是绝大多数非 2025-10-12 信号入场后没有形成可持续跟随。
后续只允许审计这些独立事件的信号后 MFE/退出路径，再预注册独立退出假设；不得按日期、
币种或市场事件增加入场过滤。
