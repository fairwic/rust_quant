# Market Momentum Opposite Move v35 缓冲两阶段持仓评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_buffered_two_stage_hold_v35`
- 状态：`research_only`，`promotion_eligible=false`

v35 从 v34 分叉，完整保留 192 根/8%、96 根/R² 0.60、两个连续 24 根恢复窗口、
当前收阳、45% 实体、65% 收盘位置、突破前高、EMA20/SMA20 收复、历史极值量能、
BTC 96 根震荡、3% 初始止损、Volume-ATR 1.8R～3R 动态目标与显式成本。

唯一变化是取消“两根无浮盈早退”。交易仍只允许持有到预注册的 24 小时或 48 小时
结果窗口，期间固定止损与动态止盈继续生效。

## 2. 事前依据与停止规则

v34 淘汰后只读审计了非 2025-10-12 主盈利簇的 13 笔亏损/持平交易。30 分钟内多数
样本的 MFE 很低，但 VANA、WIF 等信号在未先触发 3% 止损的情况下，于 6～24 小时后
达到 1.8R～3R。该证据只用于提出“持仓窗口是否过短”的独立假设，不用于选择日期、
币种或新的入场阈值。

- 仍要求原始成交至少 30 笔，30 分钟同方向聚类后至少 20 个有效事件；不足即样本淘汰；
- 净 EV `>=0.6R`、PF `>=2.2`、后半段 EV `>0R`、Sharpe `>=1.5`；
- 24h 与 48h 必须分别报告，不得事后只挑更优窗口；
- 少于半数币种亏损，移除前三个盈利币种后仍为正；
- 最后四分位必须有盈利交易且 EV `>0R`；
- 不增加保护止损、分批止盈或其他退出参数；若取消早退仍失败，v35 直接淘汰。

## 3. 预注册命令

与 v34 完全相同，但删除 `--early-exit-no-profit-candles 2`。不带
`--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v35_buffered_two_stage_hold_20260720 \
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

- 30,657 个原始候选中 35 个通过信号门禁，BTC 过滤后 29 个；48h 同币种持仓锁
  跳过 4 个，最终成交 25 笔。
- 24h 口径为 7 胜、8 负、10 个超时，毛 EV 0.8851R；48h 口径为 12 胜、
  9 负、4 个超时，毛 EV 1.1234R。取消 30 分钟早退后，持仓路径明显改善。
- 回测工具原权益报告此前没有最大持仓窗口，会把 TRX 等仓位持有到 48h 之后，不能
  与 24h/48h 结果报告对照。本轮新增可选 `--equity-max-holding-hours 48`，默认不改变
  旧行为；修正同时阻断超时平仓信号反向开仓，并由单元测试固定该契约。
- 严格 48h、扣除双边手续费与滑点后，25 笔净 EV 1.0692R、PF 3.7729、胜率 60%、
  交易级 Sharpe 2.8269、逐币隔离最大回撤 3.1576%。后半段净 EV 0.6904R、
  PF 2.4211，但 Sharpe 只有 1.3004。
- 最后四分位 8 笔净 EV -0.7183R、PF 0.0900、Sharpe -3.2793。12 个 48h 止盈中
  11 个仍来自 2025-10-12 15:00～16:15 UTC 的同一相关市场事件；按 30 分钟同方向
  归并仍只有约 15 个有效事件，低于 20 个下限。
- 移除前三个盈利币种后总收益仍为正，但无法消除“绝大多数盈利来自同一市场冲击”
  的事件集中度。组合频率约 2.1 笔/月、有效事件约 1.3 个/月，也远低于 15m 组合
  50～120 笔/月的目标区间。

## 6. 决策

**v35 淘汰，不冻结未见池，不进入 paper/read-only shadow。**

30 分钟早退被证伪，后续研究不应恢复该退出规则；但延长到 48h 只修复了退出错配，
没有修复独立事件、时间稳定性和频率。不得继续围绕当前 25 笔样本扫描持仓小时数、
止盈或日期过滤；下一步必须扩展历史市场状态，或提出能显著增加独立事件的新入场假设。
