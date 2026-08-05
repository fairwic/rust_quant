# TradingView Velocity RSI 反向长影门禁 V7 评估清单

## 1. 研究身份与边界

| 项目 | 冻结值 |
|---|---|
| 对照组 | `tradingview_velocity_parity_15m_research_v5@a36f0e19` |
| 候选组 | `tradingview_velocity_parity_15m_research_v7` |
| 周期 | 15m，仅使用已完成 K 线 |
| 成交量 | OKX `vol_ccy`，不乘收盘价 |
| 成交时点 | 信号棒收盘确认，最早下一根开盘 |
| 研究边界 | Research-only，不注册 Paper、ReadOnly、Live |
| 当前状态 | `evaluated / rejected / Research-only / not promoted` |

本清单在运行 V7 回放前冻结。V7 来源于 ATOM
`2026-07-28 07:45 Asia/Shanghai` 的已知失败个例，因此本轮 60/60
只用于同源诊断和回归验证，不构成未见样本或晋级证据。

用户口述的 `05:45` 与截图不一致：`05:45` 为
`O=1.335 / H=1.337 / L=1.334 / C=1.334` 的阴线，不可能构成看涨吞没；
截图 tooltip 的价格 `1.298` 对应 `07:45` 信号棒。

## 2. 已知事实与待验证问题

### 已知的已知

- ATOM 信号棒为 `O=1.295 / H=1.306 / L=1.295 / C=1.298`；
- 前一棒为阴线，现有实体吞没条件成立；
- 信号棒上影占整根振幅 `72.73%`，符合既有“长上影”定义；
- 当前 RSI 超卖形态和 RSI 背离确认都没有排除方向相反的长影线。

### 已知的未知

- 同一门禁在冻结 60 品种中会删除多少 RSI 交易；
- 被删除交易在零成本和压力成本下是否确实劣于保留交易；
- 对称应用到顶背离/超买空单后，是否会误删有价值的下方拒绝样本。

### 未知的已知

- 当前代码已经统一定义长影线为：影线占振幅至少 `60%`，且长于另一侧影线；
- 三棒强势反包已有收盘位于振幅顶部 `90%` 的独立门禁，不属于本次改动；
- 布林下轨收回、EMA、箱体突破等独立家族不应被 RSI 门禁污染。

### 未知的未知

- 15m OHLC 无法证明影线形成过程中的成交先后；
- 同棒若还存在独立有效家族，删除 RSI 家族未必删除整笔交易；
- 已见样本上的改善可能只是删除交易造成，不能替代 forward shadow。

## 3. V7 唯一规则

V7 继承 V5 的全部入场、冲突、止损、止盈与退出合同，只增加以下对称门禁：

```text
accepted_bullish_engulfing =
    bullish_engulfing
    && !long_upper_shadow

accepted_bearish_engulfing =
    bearish_engulfing
    && !long_lower_shadow

rsi_bullish_divergence =
    v5_rsi_bullish_divergence
    && !long_upper_shadow

rsi_bearish_divergence =
    v5_rsi_bearish_divergence
    && !long_lower_shadow
```

其中直接复用 V5 已经计算的 `long_upper_shadow / long_lower_shadow`，包括
`candle_valid && !is_doji` 门禁；不得另写一套缺少有效 K 线或非十字星条件的定义。
其核心比例边界为：

```text
long_upper_shadow =
    upper_shadow / candle_range >= 60%
    && upper_shadow > lower_shadow

long_lower_shadow =
    lower_shadow / candle_range >= 60%
    && lower_shadow > upper_shadow
```

边界 `60%` 包含。不得改成 25%、50% 或其他阈值，也不得同时增加实体、
RSI、MACD、EMA、布林或量能条件。

## 4. 影响范围

只允许改变以下 RSI 家族：

- `rsi_oversold_pattern`
- `rsi_overbought_pattern`
- `rsi_bullish_divergence`
- `rsi_bearish_divergence`

不得改变三棒反包、EMA 趋势、EMA 压缩扩张、箱体突破、假突破、布林收回、
止损尺度、结构目标、移动保护或同棒 OHLC 冲突顺序。

## 5. 验证与判定

必须同时完成：

1. ATOM 固定样本仍满足原始实体吞没，但 V7 不再生成 RSI 多单；
2. 镜像长下影看跌吞没不再生成 RSI 空单；
3. 恰好 `60%` 的反向长影被阻断，小于 `60%` 的合格样本不受影响；
4. Pine 与 Rust 规则、边界和版本身份一致；
5. sealed Top60 仍为 `60/60`，60 天预热、tick size、时间窗口和成本不变；
6. V5/V7 共有交易的方向、入场、止损、目标和退出结果不得漂移；
7. 被删除交易及四个 RSI 家族必须单独报告笔数、净 R、平均 R 与 PF_R。

本轮若 V7 压力成本净 R、平均 R、PF_R 或最大回撤任一劣于 V5，则保持
V5 主 Pine，不接受 V7。即使全部改善，V7 仍只能是 Research 候选，因为
规则由已见 ATOM 个例提出且全策略尚未达到职业级联合门槛；后续必须冻结后
积累未见 forward shadow，才能讨论晋级。

## 6. 7 月 17 日三棒反包退出的隔离边界

ATOM `2026-07-17 21:45 Asia/Shanghai` 三棒反包交易只做阻力与退出事实审计，
不纳入 V7：

- 不修改 `three_bar_bullish_engulfing_long` 的入场；
- 不修改其 `1R` 保护或 `1.5R` 目标；
- 不在本轮增加结构阻力封顶；
- 若后续研究“`min(1.5R, 冻结最近上方阻力)`”，必须另立独立退出消融，
  在读取回放结果前冻结结构识别、最低预期 R 和同棒触价顺序。

## 7. 冻结 60/60 评估结果

同源数据、60 天预热、tick size、评估窗口和成本假设均保持不变：

| 指标 | V5 对照 | V7 候选 | 差异 |
|---|---:|---:|---:|
| 覆盖 | 60/60 | 60/60 | 0 |
| 零成本交易 | 6,662 | 6,642 | -20 |
| 零成本净 R | 100.7094 | 93.1303 | **-7.5791** |
| 零成本平均 R | 0.015117 | 0.014021 | -0.001096 |
| 零成本 PF | 1.127049 | 1.127632 | +0.000583 |
| 压力成本净 R | -968.0088 | -970.9416 | **-2.9329** |
| 压力成本平均 R | -0.145303 | -0.146182 | **-0.000879** |
| 压力成本 PF | 0.809696 | 0.810191 | +0.000494 |
| 压力成本最大回撤 | 21,954.31 | 21,925.27 | -29.04 |

V7 删除的 20 笔在 V5 零成本口径合计为 `+7.5791R`，其中 12 胜、8 负，
平均 `+0.3790R`；在压力成本口径下仍约为 `+2.9329R`。按家族拆分：

| 被删除家族 | 笔数 | 零成本净 R | 平均 R |
|---|---:|---:|---:|
| RSI 顶背离空 | 3 | +5.0356 | +1.6785 |
| RSI 底背离多 | 7 | +2.0642 | +0.2949 |
| RSI 超买形态空 | 7 | -2.5682 | -0.3669 |
| RSI 超卖形态多 | 3 | +3.0475 | +1.0158 |

6,642 笔共有交易的方向、信号时间、入场、止损、退出策略、退出时间和结果
全部一致，漂移数为 0；因此差异只来自预注册门禁删除交易。

### 判定

V7 能正确过滤 ATOM 目标个例，但同时删除了更多正期望 RSI 背离和超卖形态交易。
它违反了本清单“压力成本净 R 或平均 R 不得劣于 V5”的预注册门槛，因此：

- V7 状态为 `research_rejected_negative_ev_delta`；
- 当前主 Pine 恢复为冻结 V5 `a36f0e19`；
- V7 源码、Rust 候选路径和 60/60 报告只作为可审计的 Research 证据保留；
- 不注册 Paper、ReadOnly、Live，不进行默认生产消费。
