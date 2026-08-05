# TradingView Velocity 严格视觉横盘强突破棒 V5 L1 预注册

## 当前判断与研究边界

- 当前等级：`L1_OUTCOME_BLIND_COVERAGE`。
- 基线版本：
  `volume_strict_visual_consolidation_breakout_body_midpoint_hold_long_15m_research_v3`。
- 候选版本：
  `volume_strict_visual_consolidation_breakout_body_strength_60pct_25bps_body_midpoint_hold_long_15m_research_v5`。
- V4 的短横盘 1R 退出已经在 L2 停止，V5 不继承 V4；V5 直接继承 V3 的入场、止损和目标合同。
- 本轮只验证突破源完成棒的强度门禁，不修改弱离区后的横盘生命周期。弱离区仍按既有行为立即消费活动横盘，但不能形成突破锚点。
- L1 不读取或输出退出时间、MFE、MAE、胜负、R、PnL 或 Profit Factor，也不修改 Pine。

## 四层认知框架

### 已知的已知

1. 用户已冻结强突破棒定义：实体占整根振幅至少 `60%`，且方向实体涨跌幅至少 `0.25%`；两项必须同时满足，向下镜像。
2. 2026-08-01 16:45 BTC 样本实体占比约 `35.79%`、方向实体涨幅约 `0.0361%`，两项均不合格。
3. 2026-07-29 05:30 BTC 样本实体占比约 `74.10%`、方向实体涨幅约 `0.1826%`，只通过实体占比，仍应被 `0.25%` 门禁拒绝。
4. V3 只有合格放量阳线上破才冻结来源，随后继续使用三棒回踩接受与突破实体中点守稳。

### 已知的未知

1. 冻结样本内有多少结构离区棒和 V3 合格放量上破来源会被强度门禁拒绝。
2. 强门禁保留的候选是否仍覆盖足够币种、月份和独立事件簇。
3. 本地冻结数据窗口截至 2026-07-19，无法在本轮数据库扫描中直接复核两笔 2026-07-29/08-01 样本；只能核对截图中已经完成的 OHLC 计算。

### 未知的已知

1. 只检查 `close > frozen_upper` 或 `close < frozen_lower` 不足以定义有效突破；它只能证明价格离开边界。
2. 单独使用实体占比会错误保留 2026-07-29 样本，因此必须同时使用方向实体涨跌幅。
3. 弱离区是否保留横盘属于第二变量，本轮必须冻结为立即消费，不能在看到覆盖率后改成观察一根。

### 未知的未知

1. `0.25%` 对 BTC 与低价高波动币的实际覆盖可能不同，L1 只能报告分布，不能按结果修改阈值。
2. 冻结 Top60 当前本地成员不完整，L1 只能称为本地覆盖诊断，不能宣称正式全市场结论。
3. 后续若弱离区进入观察期，必须防止第二根趋势延续棒补算旧横盘突破；该状态合同留给独立 V6。

## 单变量假设卡片

```text
研究等级：L1
唯一变量：突破源完成棒增加冻结的“实体占比 >= 60% 且方向实体涨跌幅 >= 0.25%”强度门禁
因果定义：只使用突破源完成棒的 open/high/low/close 与突破前已冻结边界
基线版本：volume_strict_visual_consolidation_breakout_body_midpoint_hold_long_15m_research_v3
候选版本：volume_strict_visual_consolidation_breakout_body_strength_60pct_25bps_body_midpoint_hold_long_15m_research_v5
目标币种/周期/窗口：冻结 Top60 本地完整成员；15m；沿用 V3 同一 manifest 评价窗口
预计影响：预计拒绝 V3 合格突破来源的 5%～60%，且至少影响 3 个来源
停止条件：强来源少于15；保留候选少于15笔、8币、3月或10个60分钟事件簇；
          只影响1～2个来源；产生任何 V3 之外的新候选；两笔 BTC 截图样本未被拒绝
保持不变：横盘定义、弱离区立即消费、放量门禁、V3三棒接受、实体中点守稳、币种池、窗口、
          下一根开盘成交、1.5 ATR止损、量能目标、成本、冲突、反手和资金路径
```

## 冻结的 V5 强度合同

设完成棒：

```text
body = abs(close - open)
range = high - low
body_ratio = body / range
```

上破来源必须同时满足：

```text
close > frozen_upper
close > open
body_ratio >= 0.60
(close - open) / open >= 0.0025
```

向下视觉离区镜像为：

```text
close < frozen_lower
close < open
body_ratio >= 0.60
(open - close) / open >= 0.0025
```

`range <= 0` 或 `open <= 0` 一律不构成强突破。V5 当前交易家族仍只做多；向下公式只用于冻结未来 TradingView/Rust 视觉 parity，不能在本轮新增空单家族。

## L1 无标签门禁

1. 输出上下离区总数、强离区数、弱离区数，以及弱离区失败原因的交集分布。
2. 输出 V3 合格放量上破来源数、V5 强来源数、来源拒绝数和保留比例。
3. 输出 V5 最终候选数、覆盖币种、月份和 60 分钟事件簇。
4. V5 只能删除 V3 来源或候选，不得新增；保留信号的冻结区间、确认时点和风险字段不得漂移。
5. 达到假设卡片的最低覆盖且至少拒绝 3 个来源，才允许进入 L2；否则停止。

## 预期数据身份

- universe：`top60_v36_direct_kline_20260721_frozen_20260723`
- manifest SHA256：`3fd267ca5cf1ecee8199232729da0e6db917803f6e7a1b363fa84e0ba75d5a4f`
- 实际评价窗口、完整成员、行情指纹与机器结果 SHA-256 由本轮只读扫描冻结。
