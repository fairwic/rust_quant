# TradingView Velocity D0 EMA 空头交易解剖与市场状态研究清单 V1

## 1. 研究目的

本轮不增加任何入场过滤器，也不改 Pine、Paper、Live 或现有退出规则。唯一目标是把
`candidate-v19 + structure-break` 中已经实际成交的 `ema_trend_short` 交易拆成四类问题：

1. 信号方向错误或破位没有延续；
2. 方向最终正确，但直接入场或初始止损时机不合理；
3. 持仓曾取得足够浮盈，但退出规则把利润还回；
4. 多币种同时触发造成同一市场事件的风险重复计算。

研究结论只用于确定下一轮应该研究哪个单变量，不能直接晋级策略。

## 2. 冻结基线

- 策略：`candidate-v19`
- EMA 空头研究分支：`structure-break`
- 周期：15 分钟已完成 K 线
- 行情：本地 `quant_core` 同源 OKX 永续 K 线
- 量能字段：`vol_ccy`
- 正式评价起点：2025-07-01 00:00:00 UTC
- 本地共同快照终点：以 runner 已冻结的 modal common end 为准
- 指标预热：60 天
- 成本压力：单边手续费 5 bps + 单边滑点 3 bps
- 币池：当前冻结 Top60 manifest；本轮允许只报告已有完整数据的成员，必须明确标记
  `partial_data_diagnostic`

## 3. 样本边界

主样本只包含已经平仓且 `families` 含 `ema_trend_short` 的实际成交交易。它不是全部原始
D0 候选信号，因为单持仓、反向信号和其他执行门禁可能阻断候选。

每笔交易必须同时满足：

- 能在同源 K 线中精确定位信号、入场和退出时间；
- 方向为做空；
- 初始风险 `initial_risk > 0`；
- 信号前至少存在 20 根完整 K 线；
- 信号收盘严格低于前 20 根最低价，保持 D0 定义不变；
- 零成本与成本后回放的信号、入场和退出身份完全一致。

不满足条件的记录只能计入 `invalid_trade_records`，不能按零值参加比例计算。

## 4. 交易路径定义

### 4.1 冻结破位线

信号 K 线为 `t`，冻结破位线为：

```text
break_line = min(low[t-20], ..., low[t-1])
```

信号后第 1 根是实际下一开盘入场棒。入场棒或其后完成棒收盘
`close >= break_line`，定义为破位失去接受。分别统计 1、2、4 根内是否收回。

### 4.2 固定前向路径

前向结果只作为后验标签，不得参与信号生成或补造入场：

- 1 根：入场棒；
- 2 根：入场棒及其后 1 根；
- 4 根：入场棒及其后 3 根；
- 8 根：入场棒及其后 7 根；
- 16 根：入场棒及其后 15 根。

做空方向的前向指标为：

```text
MFE_R(N) = (entry_price - min(low over N bars)) / initial_risk
MAE_R(N) = (max(high over N bars) - entry_price) / initial_risk
close_R(N) = (entry_price - close at the Nth bar) / initial_risk
```

末端数据不足 N 根时，该 N 根指标记为缺失，禁止缩短窗口后混入完整样本。

### 4.3 实际持仓路径

为避免 15 分钟 OHLC 无法确定退出棒内先后顺序，实际持仓 MFE/MAE 只使用：

- 入场后、退出棒之前的完整 K 线；
- 实际退出价格作为退出棒唯一可确认价格。

因此：

```text
pre_exit_MFE_R = max(entry_price - completed_bar_low, entry_price - exit_price) / initial_risk
pre_exit_MAE_R = max(completed_bar_high - entry_price, exit_price - entry_price) / initial_risk
```

这是一种保守口径，不把退出后同一根 K 线的极值冒充为持仓可得路径。

### 4.4 止损后恢复

只针对 `exit_reason = stop_loss`：

- 从退出棒的下一根开始；
- 截止原入场后的第 16 根；
- 若期间价格相对原入场价重新达到至少 `+1R` 的有利波动，则记为
  `initial_stop_then_recovered_1r_within_16`。

退出棒本身不参与，避免未知的棒内顺序污染结论。

## 5. 预注册诊断标签

以下标签可以重叠，不强行把交易塞入互斥分类：

- `no_follow_through_4bar`：`MFE_R(4) < 0.5`
- `immediate_wrong_direction_4bar`：`MFE_R(4) < 0.5` 且 `MAE_R(4) >= 1.0`
- `initial_stop_then_recovered_1r_within_16`：满足 4.4
- `profit_giveback_after_1r`：`pre_exit_MFE_R >= 1.0` 且成本后 `net_r <= 0`
- `healthy_capture_2r`：`pre_exit_MFE_R >= 2.0` 且成本后 `net_r > 0`

## 6. 信号时市场状态描述

本轮只描述，不生成门禁：

### 6.1 48 根方向效率

```text
short_efficiency_48 =
    (close[t-48] - close[t])
    / sum(abs(close[i] - close[i-1]), i=t-47..t)
```

值越接近 `1`，代表过去 48 根越稳定地下跌；接近 `0` 代表震荡；负值代表总体仍上涨。

### 6.2 波动阶段

先计算每根 K 线的 True Range，再计算信号时 14 根简单均值。波动阶段指标为：

```text
tr14_ratio_to_prior96_median =
    current_TR14_SMA / median(TR14_SMA over the 96 completed observations ending at t)
```

该指标不是 Pine 的 Wilder ATR，不得与策略 ATR 字段混用；它只用于描述波动是否扩张。

## 7. 有效市场事件

把实际成交的 EMA 空头交易按信号时间升序排列。相邻信号间隔不超过 60 分钟时采用链式
归并为同一个事件。每个事件统计：

- 交易数；
- 不同币种数；
- 成本后净 R；
- 负 R 绝对值。

`multi_symbol_cluster` 定义为不同币种数至少 2。正式报告同时给出：

- 原始交易数与事件数；
- 单币事件与多币事件的平均成本后 R；
- 多币事件贡献的亏损绝对值占全部亏损绝对值比例；
- 最大事件的交易数和不同币种数。

这仍不是相关系数或板块模型，只能作为市场共同冲击的近似诊断。

## 8. 固定分组

所有指标同时报告：

- 全部有效交易；
- 2025 年 8 月和 9 月；
- 除 2025 年 8 月和 9 月外；
- BTC；
- 非 BTC。

时间分组一律使用信号时间的 UTC 月份。

## 9. 下一轮单变量选择顺序

按以下顺序只选择第一个满足条件的方向：

1. 若成本后非正收益交易中，`profit_giveback_after_1r` 占比至少 30%，下一轮只研究退出保护；
2. 否则，若初始止损交易中，`initial_stop_then_recovered_1r_within_16` 占比至少 40%，
   下一轮只研究入场时机或初始止损；
3. 否则，若亏损交易中 2 根内收回破位线的占比至少 50%，下一轮只研究破位接受确认；
4. 否则，若多币事件贡献至少 60% 的亏损绝对值，且多币事件平均 R 低于单币事件，
   下一轮只研究事件级容量与相关风险；
5. 否则，下一轮只研究信号时市场状态，优先使用 48 根方向效率，不再回到 EMA 距离或
   96 根价格位置。

本轮查看结果后不得修改上述阈值。

## 10. 成功标准与限制

本轮成功不是提高 PF，而是：

- 每笔有效交易能生成可审计路径记录；
- 分组和事件聚类的分母明确；
- 能按第 9 节唯一选出下一轮研究方向；
- 不修改任何交易行为。

限制：

- 当前冻结币池不是 point-in-time 历史成员，正式晋级仍需消除幸存者偏差；
- 独立单币固定 1 单位结果不是共享资金组合；
- 15 分钟 OHLC 无法恢复退出棒内的真实价格先后；
- 实际成交样本不能代表被执行门禁阻断的全部原始候选。
