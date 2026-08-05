# 15m 历史趋势极端量 RVAT10 延续 V2 评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-trend-extreme-volume-continuation`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_historical_trend_extreme_volume_rvat10_continuation_v2`
- 状态：`research_only`，`promotion_eligible=false`

V2 只修正 V1 的量比定义。V1 用当前成交量除以前置连续 K 线均量，无法区分正常日内
活跃时段与真正异常量。TradingView Relative Volume at Time 的 Regular 模式按日锚定，
用当前时点成交量除以过去若干日相同时间偏移的平均量；其 Screener 说明使用过去 10 天。

V2 固定以 UTC 自然日为锚，每个已完成 15m setup 的成交量除以前 10 个 UTC 日同一
15m 时点的平均成交量，仍要求比值至少 2.0。十个对应时点任一缺失、成交量非正或时间戳
不精确匹配时不产生信号。除这一项外，趋势、实体、振幅、一次性状态、退出、成本和币池
全部保持 V1。

## 2. 数据、时序与冻结参数

- Discovery：`2025-07-01 08:00:00`～`2026-07-17 17:00:00`，Asia/Shanghai；
- 独立历史：`2024-07-01 08:00:00`～`2025-07-01 07:59:59`，Asia/Shanghai；
- 只读取本地 `quant_core` 的 `confirm=1` 已完成 15m K 线；不读取 1m、事件归档、
  episode、未来 K 线或未完成 K 线；
- 只加载当前 `live` OKX 线性 USDT 永续，不纳入当前退市币，承认幸存者偏差；
- 历史趋势：前 192 根净幅至少 8%，或前 96 根同向线性趋势且 R² 至少 0.60；
- setup 与历史趋势同向，实体比例至少 20%，RVAT10 至少 2.0，振幅扩张至少 1.4 倍；
- 每个趋势状态只消费第一个 setup，连续八根中性 15m K 线后才重置；
- 3% 固定初始止损，Volume-ATR 目标 1.8R～3R，最长持有 48 小时；
- 单边手续费 5 bps、等价滑点 3 bps，同币持仓期间忽略重复信号。

不扫描 5/10/20 天、日锚时区、累计模式、RVAT 阈值或其他参数；不追加方向、MACD、
FVG、BOS、CHoCH、RSI 或布林带过滤。

## 3. 预注册门禁

- 必须验证 RVAT10 只读取 setup 之前十个相同 UTC 日内时点，不包含当前或未来 K 线；
- Discovery 与独立历史窗口都必须净 EV `>0` 且 PF `>1`，任一失败即淘汰；
- 两个窗口都报告整体、多空、前后半段、四分位和盈利币集中度；
- 职业晋级仍要求净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`、最大回撤
  `<=15%`、Recovery `>=4`，并通过统一资金与有效事件门禁；
- 频率目标 50～120 笔/月；不以频率救援负收益，也不在结果后调整 RVAT 长度或阈值。

## 4. 冻结命令

Discovery 使用以下命令；独立历史只替换窗口毫秒为
`1719792000000` / `1751327999999`：

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-current-live-only \
  --sample-limit 1000 \
  --sample-seed current_live_extreme_volume_rvat10_continuation_v2_20260721 \
  --trade-direction both \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 0 \
  --entry-min-volume-ratio 2 \
  --entry-min-body-ratio-pct 20 \
  --entry-min-range-expansion-ratio 1.4 \
  --entry-extreme-volume-continuation \
  --entry-relative-volume-at-time-10d \
  --entry-once-per-historical-trend-state \
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
  --equity-price-volume-diagnostic-report \
  --equity-symbol-window-report \
  --min-trades 1 \
  --paper-outcome-entry-rule-version \
  kline15m_historical_trend_extreme_volume_rvat10_continuation_v2
```

## 5. 冻结后结果

第 1～4 节在读取结果前已冻结；以下结果没有触发参数、阈值或方向调整。

### 5.1 Discovery

- 扫描 216 个当前 live 交易对；状态机 armed `97,037` 次、稳定重置 `96,930` 次；
- `48,699` 个 setup 通过价格结构，`12,788` 个一次性状态发出信号，最终成交
  `11,092` 笔、覆盖 166 个币，约 `885` 笔/月；
- 胜率 `32.0141%`，成本后 EV `-0.093252R`，PF `0.861429`，Sharpe
  `-6.938883`，最大单币隔离回撤 `76.2928%`；
- 前半段/后半段 EV 为 `-0.091180R/-0.095540R`，四分位 EV 分别为
  `-0.112428R/-0.068766R/-0.076950R/-0.113478R`，全部为负；
- 多头 `4,902` 笔，EV/PF 为 `-0.083857R/0.876193`；空头 `6,190` 笔，
  EV/PF 为 `-0.100692R/0.849600`；
- RVAT10 `2～3x`、`3～5x`、`5x+` 三档 EV/PF 分别为
  `-0.155408R/0.773513`、`-0.081393R/0.877011`、
  `-0.011689R/0.982442`。三档均未转正，因此不允许事后上调阈值。

### 5.2 独立历史

- 扫描同一当前 live 币池；状态机 armed `65,657` 次、稳定重置 `65,589` 次；
- `42,014` 个 setup 通过价格结构，`9,455` 个状态发出信号，最终成交
  `8,257` 笔、覆盖 127 个币，约 `688` 笔/月；
- 胜率 `35.4366%`，成本后 EV `+0.090063R`，PF `1.137263`，Sharpe
  `5.167080`，最大单币隔离回撤 `59.0823%`；
- 前半段/后半段 EV 为 `+0.077419R/+0.106321R`，四分位均为微弱正值；
- 多头 `3,975` 笔，EV/PF 为 `-0.032018R/0.953657`；空头 `4,282` 笔，
  EV/PF 为 `+0.203392R/1.326020`；旧窗口优势主要来自空头；
- 去除前五个盈利币后仍盈利 `1,403.87U`，但最大回撤、PF、EV 和频率均未达到
  职业级门禁。

## 6. 结论

状态：`rejected_temporal_decay_not_volume_seasonality`。

RVAT10 修正了滚动均量没有处理日内活跃时段的问题，但没有修复最新 Discovery 的负收益。
旧窗口空头为正、最新窗口多空皆负，说明主要问题不是成交量季节性，而是总成交量缺少主动
买卖方向，以及该延续关系随市场阶段失效。该版本不落库、不进入 Paper/Live，也不继续扫描
RVAT 天数、阈值、方向或退出参数。

若继续研究，需要新建策略家族并先获得方向性 15m 成交量证据，例如把逐笔主动买卖量聚合为
一根已完成 15m K 线的 volume delta/taker imbalance；它不是用 1m K 线触发交易。若只允许
现有 15m OHLCV，则停止本策略家族，不再从已见历史追加指标。
