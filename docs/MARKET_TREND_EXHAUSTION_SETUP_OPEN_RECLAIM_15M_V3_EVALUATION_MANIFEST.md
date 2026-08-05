# 15m 趋势耗竭开盘价收回确认 V3 评估清单

## 1. 策略身份与唯一变化

- 产品 slug：`market-trend-exhaustion-one-shot-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_trend_exhaustion_setup_open_reclaim_v3`
- 状态：`research_only`，`promotion_eligible=false`

V3 保留 V2 的趋势背景、一次性状态、连续八根中性重置、极端量 setup、止损、
Volume-ATR 止盈、成本、币池和持仓上限。唯一变化是入场时序：极端量 setup 只消费
当前趋势状态，不立即反向开仓；随后最多观察三根已完成 15m K 线。做多要求某根收盘价
高于 setup 开盘价，做空要求某根收盘价低于 setup 开盘价；确认后只在下一根 15m K 线
开盘入场。三根内没有确认，setup 永久过期，不在同一趋势状态内寻找第二次机会。

本轮不扫描一根、两根或其他等待长度，不使用确认 K 收盘价成交，不使用盘中高低点触碰，
也不增加 MACD、FVG、BOS、CHoCH、影线、量比、振幅、方向限定或其他救援条件。

## 2. 数据、时序与冻结参数

- Discovery 窗口：`2025-07-01 08:00:00` 至 `2026-07-17 17:00:00`，
  Asia/Shanghai；该窗口已经用于 V1/V2 和 V3 特征发现，不再视为样本外；
- 独立历史检查窗口：`2024-07-01 08:00:00` 至 `2025-07-01 07:59:59`，
  Asia/Shanghai；在 V3 结果读取前冻结，只用于方向一致性检查；
- 只读取本地 `quant_core` 的 `confirm=1` 已完成 15m K 线；不读取 1m、
  `market_rank_events`、episode 或信号时间后的非确认 K 线；
- 按用户当前要求，只加载 `exchange_symbols` 当前 `live`、线性、USDT 永续且存在 15m
  数据表的 OKX 交易对，不纳入当前已退市币；同时明确承认这会产生 current-live
  幸存者偏差，因此结果不能声明为历史全市场普适；
- 历史背景：前 192 根净上涨/下跌至少 8%，或前 96 根同向线性趋势且 R² 至少 0.60；
- 极端量 setup：量比至少 2.0、振幅扩张至少 1.4 倍；实体占振幅至少 20% 时反向实体，
  小实体按更长影线对应的拒绝方向；
- 一次性状态：只消费趋势首次成立后的第一个有效极端量 setup，消费后必须连续八根中性
  15m K 线才允许重置；
- 3% 固定初始止损，Volume-ATR 目标限制在 1.8R～3R，最长持有 48 小时；
- 单边手续费 5 bps、等价滑点 3 bps；同一 symbol 持仓时忽略重复信号更新；
- `R` 仍以实际延迟入场价到 3% 初始止损的风险固定，不沿用原 setup 价格计算风险。

## 3. 预注册判定与停止规则

V2 Discovery 对照为 16,288 笔、胜率 36.2782%、净 EV `-0.070694R`、PF
`0.889728`、Sharpe `-6.688597`。V3 必须报告实际延迟成交，不得把 V2 的 future-path
分组按旧入场价当成 V3 成绩。

- Discovery 只用于确认真实入场实现和变化方向，不用于宣称样本外优势；
- 独立历史窗口继续研究的最低条件为净 EV `>0` 且 PF `>1`；任一失败即淘汰 V3；
- 职业级晋级仍要求成本后净 EV `>=0.6R`、PF `>=2.2`、Sharpe `>=1.5`、最大回撤
  `<=15%`、Recovery `>=4`，并满足统一资金、有效事件与相关簇门禁；
- Discovery 与独立窗口都报告多空、前后半段、四分位和盈利币集中度；
- 组合频率目标仍为 50～120 笔/月，但频率不能救援负 EV/PF；
- 若独立历史窗口失败，不扫描等待长度、收回价位或方向，不在已查看结果上追加指标。

## 4. 冻结命令

Discovery 使用以下命令；独立历史检查仅把 `event-start-ms` / `event-end-ms` 替换为
`1719792000000` / `1751327999999`，其余参数完全相同：

```bash
target/release/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-current-live-only \
  --sample-limit 1000 \
  --sample-seed current_live_setup_open_reclaim_v3_20260721 \
  --trade-direction both \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 0 \
  --entry-min-volume-ratio 2 \
  --entry-min-range-expansion-ratio 1.4 \
  --entry-extreme-volume-contrarian \
  --entry-once-per-opposite-trend-state \
  --entry-wait-setup-open-reclaim \
  --entry-defer-max-wait-candles 3 \
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
  kline15m_trend_exhaustion_setup_open_reclaim_v3
```

## 5. 冻结回放结果

### 5.1 Discovery 窗口

状态扫描与 V2 完全一致：216 个交易对、97,037 次 armed、96,930 次中性重置、
66,902 个去重前有效 setup、17,566 个状态后 setup。三根内有收盘确认且存在下一根
可成交 K 线的信号为 5,202 个，占 29.61%；同币持仓锁后实际成交 5,068 笔。

| 指标 | V2 立即反向 | V3 收回后真实入场 |
|---|---:|---:|
| 实际交易 | 16,288 | 5,068 |
| 覆盖交易对 | 216 | 166 |
| 胜率 | 36.2782% | 32.0245% |
| 净 EV | -0.070694R | -0.160582R |
| PF | 0.889728 | 0.764387 |
| 交易级 Sharpe | -6.688597 | -8.569070 |
| 最大单币隔离回撤 | 82.3868% | 61.5172% |

前半段/后半段分别为 `-0.195726R/PF 0.723407` 与
`-0.123849R/PF 0.810887`；Q1～Q4 的净 EV 分别为 `-0.160138R`、
`-0.230793R`、`-0.148964R`、`-0.097286R`，四段全部为负。多头 2,741 笔的净
EV/PF 为 `-0.259505R/0.635228`，空头 2,327 笔为
`-0.044061R/0.931834`。移除盈利最高的三个币后，剩余 4,951 笔总利润进一步降至
`-2,306.56U`，不存在头部盈利掩盖整体优势。

### 5.2 独立历史窗口

2024-07-01 至 2025-07-01 窗口扫描 216 个当前 live 交易对，产生 13,168 个状态后
setup，4,051 个确认入场，实际成交 3,962 笔、覆盖 128 个交易对。净 EV
`-0.143475R`、PF `0.801138`、胜率 29.7325%、Sharpe `-6.239818`、最大单币
隔离回撤 66.5765%。前后半段为 `-0.159950R/PF 0.781005` 与
`-0.126505R/PF 0.822471`；四分位净 EV 为 `-0.099024R`、`-0.220753R`、
`-0.149880R`、`-0.102940R`，仍全部为负。多头/空头分别为
`-0.202532R/PF 0.728843` 与 `-0.080602R/PF 0.883926`。

## 6. 判定

V2 的 future-path 正分组不能转化为可执行策略：它使用了 setup 时的旧入场价来标注后来
是否收回；V3 等到收盘确认后再按下一根真实开盘成交，价格优势已经消失，且固定 3% 风险
下的多头结果明显恶化。Discovery 和独立历史窗口的 EV/PF、全部时间分段均失败，因此
V3 状态为 `rejected_confirmation_arrives_after_edge`，保持 `research_only`、
`promotion_eligible=false`。

本轮未扫描等待长度、收回价位或方向，未追加 MACD/FVG/BOS，未落库、未进入
Paper/Live，也未触发任何交易 mutation。下一研究假设不得继续救援反转入场；应独立验证
外部量价理论直接指出的另一方向：放量大实体沿既有趋势推进是否属于动量延续，而非耗竭。
