# 15m 历史趋势极端量延续 V1 评估清单

## 1. 策略身份与独立假设

- 产品 slug：`market-trend-extreme-volume-continuation`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_historical_trend_extreme_volume_continuation_v1`
- 状态：`research_only`，`promotion_eligible=false`

本策略不是反转 V1～V3 的参数补丁，而是独立入场语义：TradingView 官方量价说明把
价格沿趋势推进且成交量放大视为趋势确认；本地 V2 诊断中 94.22% 的极端量 setup 实体
也仍沿历史趋势方向推进。V1 因此验证“极端量是延续而非耗竭”：历史上涨背景只接受
同向阳线并做多，历史下跌背景只接受同向阴线并做空。

## 2. 数据、时序与冻结规则

- Discovery 窗口：`2025-07-01 08:00:00` 至 `2026-07-17 17:00:00`，
  Asia/Shanghai；已用于相关反转研究，只能作为开发窗口；
- 独立历史检查窗口：`2024-07-01 08:00:00` 至 `2025-07-01 07:59:59`，
  Asia/Shanghai；在本策略结果读取前冻结；
- 只读取本地 `quant_core` 的 `confirm=1` 已完成 15m K 线，不读取 1m、事件归档、
  episode、盘口、未来 K 线或信号后补偿条件；
- 只加载当前 `live`、线性、USDT 永续且存在 15m 表的 OKX 交易对，按用户要求不纳入
  当前退市币；明确承认 current-live 幸存者偏差；
- 历史背景保持原阈值：前 192 根净上涨/下跌至少 8%，或前 96 根同向线性趋势且
  R² 至少 0.60；同时满足多空时按中性；
- 当前 setup 必须与历史趋势同向、实体占振幅至少 20%、量比至少 2.0、振幅扩张至少
  1.4 倍；十字或小实体不按影线猜方向，直接跳过；
- 每个历史趋势状态只消费第一个有效 setup，之后必须连续八根中性 15m K 线才重置；
- setup 收盘完成时生成信号，以该收盘价作为信号入场基准并计入单边 5 bps 手续费与
  3 bps 等价滑点；不读取下一根 K 线决定是否开仓；
- 3% 固定初始止损，Volume-ATR 目标限制在 1.8R～3R，最长持有 48 小时；同一
  symbol 持仓期间忽略重复信号更新。

本轮不扫描量比、振幅、实体、趋势长度或重置长度，不追加 MACD、FVG、BOS、CHoCH、
RSI、布林带、只做多/只做空或其他后验过滤。

## 3. 预注册门禁

- 首先验证发出的事件方向与历史趋势、setup 实体方向一致，且一次性状态生效；
- Discovery 和独立历史窗口都必须报告整体、多空、前后半段、四分位和盈利币集中度；
- 继续研究的最低条件为独立历史窗口净 EV `>0` 且 PF `>1`，任一失败即淘汰；
- 职业级晋级要求成本后净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`、最大回撤
  `<=15%`、Recovery `>=4`，并通过统一资金、有效事件、相关簇和流动性门禁；
- 组合频率目标 50～120 笔/月；频率不足不能放宽信号，频率过高也不能替代正 EV；
- 若失败，不在已查看窗口改成只做空、提高实体比例或调整量比/振幅。

## 4. 冻结命令

Discovery 使用以下命令；独立历史检查只把开始/结束毫秒替换为
`1719792000000` / `1751327999999`：

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-current-live-only \
  --sample-limit 1000 \
  --sample-seed current_live_extreme_volume_continuation_v1_20260721 \
  --trade-direction both \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 0 \
  --entry-min-volume-ratio 2 \
  --entry-min-body-ratio-pct 20 \
  --entry-min-range-expansion-ratio 1.4 \
  --entry-extreme-volume-continuation \
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
  kline15m_historical_trend_extreme_volume_continuation_v1
```

## 5. 冻结回放结果

### 5.1 Discovery 窗口

216 个交易对产生 58,420 个去重前有效 setup、16,616 个一次性状态后事件；同币持仓
锁后实际成交 15,173 笔、覆盖 166 个交易对，约 1,211 笔/月。

| 指标 | 结果 |
|---|---:|
| 胜率 | 32.3403% |
| 净 EV | -0.090580R |
| PF | 0.864218 |
| 交易级 Sharpe | -7.961607 |
| 最大单币隔离回撤 | 81.9479% |

前后半段分别为 `-0.094594R/PF 0.860265` 与
`-0.086229R/PF 0.868713`；Q1～Q4 为 `-0.148521R`、`-0.041211R`、
`-0.044133R`、`-0.130636R`，全部为负。多头 6,634 笔为
`-0.082888R/PF 0.876840`，空头 8,539 笔为
`-0.096556R/PF 0.854255`。移除盈利最高三个币后总利润进一步降至
`-3,778.55U`，不存在集中盈利掩盖底层负优势。

### 5.2 独立历史窗口

216 个当前 live 交易对产生 50,581 个去重前 setup、12,504 个状态后事件；实际成交
11,433 笔、覆盖 128 个交易对，约 953 笔/月。净 EV `+0.055022R`、PF
`1.084146`、胜率 35.1351%、Sharpe `3.842262`，但最大单币隔离回撤高达
74.5192%。前后半段为 `+0.053281R/PF 1.081985` 与
`+0.060625R/PF 1.092382`；Q1～Q4 也均为微正。

该优势完全不具方向稳定性：多头 5,473 笔为 `-0.052919R/PF 0.922168`，空头
5,960 笔为 `+0.154142R/PF 1.244678`。盈利最高五个币贡献 46.71%，移除后仍为正，
但总体 EV 只有职业门槛的 9.17%，回撤约为门槛的 4.97 倍。

## 6. 判定与下一缺陷

V1 证明简单把极端量从反向改为顺向也不能形成跨时间稳定优势：旧窗口只有微弱空头优势，
更新窗口多空同时转负，且频率约为目标上限的 8～10 倍。状态为
`rejected_temporal_instability_and_low_edge`，保持 `research_only`、
`promotion_eligible=false`。

本结果没有被用于改成只做空或扫描实体/量比。外部平台规则指出一个可独立验证的数据定义
缺陷：当前连续均量没有校正 24 小时市场的日内时点季节性。下一版本只允许把量比替换为
TradingView Relative Volume at Time 同口径的“过去十天相同 15m 时点均量”，其他条件
不变；该版本必须另立 manifest 后运行。
