# TradingView Velocity 慢均线带收复空单门禁 V8 评估清单

## 1. 研究身份与边界

| 项目 | 冻结值 |
|---|---|
| 对照组 | `tradingview_velocity_parity_15m_research_v5@a36f0e19` |
| 候选组 | `tradingview_velocity_parity_15m_research_v8` |
| 规则标识 | `fresh_slow_ema_band_reclaim_rsi_overbought_upper_wick_short_guard_15m_v1` |
| 周期 | 15m，仅使用已完成 K 线 |
| 成交量 | OKX `vol_ccy`，不乘收盘价 |
| 成交时点 | 信号棒收盘确认，最早下一根开盘 |
| 研究边界 | Research-only，不注册 Paper、ReadOnly、Live |
| 当前状态 | `quick LTC evaluated / formal 60/60 deferred by user` |

本清单在读取 V8 跨币种回放结果前冻结。V8 直接继承已接受为当前研究主线的
V5，不继承已经否决的 V6、V7。规则来源于 LTC
`2026-07-26 05:30 Asia/Shanghai` 已见失败个例，因此 frozen 60/60 只用于
同源消融和回归验证，不构成未见样本或晋级证据。

## 2. 四层认知框架

### 2.1 已知的已知

- LTC 信号棒为 `O=46.46 / H=46.80 / L=46.46 / C=46.58`；
- 该棒 `filtered_volume_ratio=9.490995`、`RSI14=72.0490`、长上影占整根
  `64.706%`，满足 V5 普通 `rsi_overbought_pattern`；
- 图上紫色慢均线实际为 EMA596，不是 EMA696；
- 信号棒收盘同时高于 `EMA596=46.477085` 与 `EMA696=46.425704`，且该棒
  才重新收复两者的较高值；
- 下一根开盘 `46.57`，该空单最大有利波动约 `0.61R`，随后触及 `46.80`
  初始止损。

### 2.2 已知的未知

- 同一慢均线带门禁在 frozen 60 品种会删除多少交易；
- 被删除交易在零成本和压力成本下是否为负期望；
- 该形态在不同币种、月份和市场状态中是否仍代表突破初期而非顶部衰竭。

### 2.3 未知的已知

- 当前 `recentBullishVolumeEmaTransition` 只识别 EMA12 对 EMA144/596 的
  快均线交叉，不能识别价格刚收复慢均线带；
- LTC 信号棒虽有长上影，但同时是 `1.093 ATR` 的放量阳实体，不能把它和
  普通高位衰竭长上影等同；
- RSI 顶背离、二次扫高、假突破和高位努力无结果已有独立结构确认，不应被
  本次普通形态门禁污染。

### 2.4 未知的未知

- 15m OHLC 无法还原信号棒内先突破还是先形成上影；
- 慢均线近乎走平时的收复可能仍是假突破，五根保护只能避免立即逆势，不能
  证明多头趋势已经成立；
- 同棒若还有独立结构空单，删除普通 RSI 归因未必删除整笔交易。

## 3. V8 唯一规则

定义慢均线带上沿：

```text
slow_band_upper[t] = max(EMA596[t], EMA696[t])
```

定义放量多头冲量棒与收复：

```text
strong_bullish_volume_impulse[t] =
    volume_event[t]
    && filtered_volume_ratio[t] >= 6
    && close[t] > open[t]
    && body[t] >= 1.0 * ATR14[t]

slow_band_reclaim_up[q] =
    strong_bullish_volume_impulse[q]
    && close[q] > slow_band_upper[q]
    && close[q-1] <= slow_band_upper[q-1]
```

最近的 `q` 必须位于 `t-4 ... t`，并且从 `q` 到 `t` 的每根完成 K 线都满足
`close > slow_band_upper`。因此保护窗口包含收复棒，共五根；任一完成棒重新
收于当根慢均线带上沿或以下，保护立即失效，后续只有新的有效收复才能重启。

V8 只增加：

```text
v8_guard[t] =
    fresh_slow_band_reclaim[t]
    && raw_rsi_pattern_short[t]
    && long_upper_shadow[t]

rsi_pattern_short_v8 =
    rsi_pattern_short_v5
    && !v8_guard
```

边界一律严格按上式执行：

- 前一棒收盘恰好等于慢均线带上沿，当前严格收上，算有效上穿；
- 当前收盘恰好等于带上沿，不保护；
- 只有最高价刺穿、收盘仍在线下，不保护；
- `q+4` 仍保护，`q+5` 不保护；
- 不增加慢线斜率、额外 ATR 距离、RSI 上限或第二组形态条件。

阻断原因固定为：

```text
V8_RSI_OVERBOUGHT_UPPER_WICK_FRESH_SLOW_EMA_BAND_RECLAIM_5
```

## 4. 允许影响与隔离范围

只允许移除包含长上影的普通 `rsi_overbought_pattern` 归因。

不得改变：

- RSI 顶背离；
- 仅看跌吞没且没有长上影的普通超买形态；
- EMA 趋势空、EMA 压缩扩张空；
- 锚区假突破、流动性二次扫高；
- 高位放量努力无结果；
- 所有多单家族；
- 初始止损、止盈、移动保护、同棒 OHLC 冲突顺序。

同棒若存在独立空单家族，只删除普通 RSI 归因；独立家族仍可生成交易。

## 5. 固定验证

必须同时完成：

1. LTC `2026-07-26 05:30` 固定事实在 V5 开空，V8 以固定原因拒绝普通 RSI
   空单；
2. 收复当根与 `age=4` 被保护，`age=5` 不保护；
3. 前值等于带上沿时有效，当前值等于带上沿时无效；
4. 仅影线刺穿不保护；
5. 窗口内收回带下立即解除，再次完成收复后重新启动；
6. 看跌吞没-only 与所有独立空单家族不受影响；
7. Pine 与 Rust 的窗口、比较符号和信号家族边界完全一致；
8. V5/V8 共有交易的方向、入场、止损、目标、退出时间与结果不得漂移。

LTC 目标发生在 frozen 正式窗口之后，只能作为固定案例或延伸诊断；不得将其
混入 frozen 60/60，也不得称为样本外证据。

## 6. frozen 60/60 判定门槛

继续使用同一个 sealed Top60、60 天预热、tick size、`vol_ccy`、下一根开盘
成交和两种成本口径：

- 零成本；
- 每边 `5 bps` 手续费加 `3 bps` 滑点。

任一条件成立即否决 V8、主 Pine 保持 V5：

- 覆盖不足 `60/60`；
- LTC 固定样本未被拦截；
- Pine/Rust 边界不一致；
- 非目标家族或共有交易发生路径漂移；
- 全策略零成本净 R 或平均 R 下降；
- 压力成本净 R、平均 R、PF 任一下降；
- 压力成本最大回撤扩大；
- 被门禁删除的交易在压力成本下仍为正期望。

必须单独报告 exact-only `rsi_overbought_pattern`、同棒多家族交易和全部删除集。
若删除少于 20 笔或覆盖少于 10 个币种，只能判为样本不足，不得晋级。即使所有
同源指标改善，V8 仍只允许进入冻结后的 forward shadow，不能直接注册生产。

## 7. 快速验证结果

用户明确要求停止数据库复审，因此本轮没有生成新的正式 60/60 报告。改用同一
TradingView LTC 图表做 V5/V8 快速对照：

- 交易数 `18 → 17`；
- 净利润 `-0.06 → +0.17 USDT`；
- PF `0.9822 → 1.0541`；
- 亏损交易 `11 → 10`；
- 最大回撤均为 `1.45 USDT`。

Rust 固定 LTC 测试确认 V5 开空、V8 按预注册原因阻断；V8 focused `5/5`、
完整 parity `91/91`、strict CLI `4/4` 通过。Pine 编译 0 错误并已重新加载
到 LTC 图表。

该结果不满足本清单第6节的跨币种判定范围，因此 V8 只保持
`Research-only`，不进入 Paper、ReadOnly、Live 或默认生产消费。
