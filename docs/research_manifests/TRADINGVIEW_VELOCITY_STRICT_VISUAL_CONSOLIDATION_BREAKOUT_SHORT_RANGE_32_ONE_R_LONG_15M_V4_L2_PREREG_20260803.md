# TradingView Velocity 严格视觉横盘短区间 1R 目标 V4 L2 修订预注册

## 修订原因与审计边界

- 原 L1 预注册 SHA-256：
  `d9c7c0d96476127d73cfc93e75128d79199b7e4544ce442bc639bcbd2c1edccc`，保持原文件不变。
- 原门禁要求 `range_length_bars <= 32` 占 V3 候选的 20%～80%；无 outcome 扫描得到
  502/627，即 `80.0637958533%`，因 1 笔候选超过比例上限而按原合同暂停。
- 同一次无标签扫描同时确认：短分支 502 笔、43 币、13 月、405 个事件簇；长分支 125 笔、
  39 币、13 月、106 个事件簇。两个分支均不存在单币、单月或近乎空分支退化。
- 用户在未读取任何 V4 outcome、未实现 V4 退出、未运行 V4 L2 的前提下，明确确认保留
  `<=32根 -> 1R` 交易规则，并把研究可行性改为绝对覆盖门禁。
- 本文件开启一个新的、可审计的 L2 批次；不修改或伪装原 L1 门禁为已通过。

## 冻结版本与唯一变量

```text
研究等级：L2
唯一变量：V3 严格横盘 Fixed 退出按冻结 range_length_bars 切换目标
短分支：range_length_bars <= 32 时 target_ticks = initial_stop_ticks
长分支：range_length_bars > 32 时继续使用源棒量能 2.7/3.6/4.5 ATR 目标
基线版本：volume_strict_visual_consolidation_breakout_body_midpoint_hold_long_15m_research_v3
候选版本：volume_strict_visual_consolidation_breakout_body_midpoint_hold_short_range_32_one_r_long_15m_research_v4
保持不变：横盘识别、三棒回踩、实体中点、信号身份、下一根开盘、1.5 ATR 初始止损、
          成本、退出优先级、冲突、反手、币种池和评价窗口
L2停止：家族成本后EV<=0或PF<=1；未优于V3；改善集中于1～2笔、单一币种或单一事件簇
```

V4 仍是 Candidate V20 上的独立 Research-only 身份；不得覆盖 V3，不创建 Pine，不接 Paper、
ReadOnly、Live、生产注册、调度或实盘执行。

## 修订后的 L1 绝对覆盖门禁

基线账本固定为
`target/research/strict_visual_breakout_body_midpoint_hold_v3_l1.json`，SHA-256
`e6b432952e134c4abe5c62b8836d30bf7797886a98536689695b5b334143e924`，标签边界为
`NO_MFE_NO_MAE_NO_EXIT_NO_WIN_LOSS_NO_R_NO_PNL_NO_PROFIT_FACTOR`。

| 分支 | 候选 | 币种 | 上海月份 | 60 分钟事件簇 | 门禁 |
|---|---:|---:|---:|---:|---|
| `<=32根 -> 1R` | 502 | 43 | 13 | 405 | 通过 |
| `>32根 -> 量能ATR` | 125 | 39 | 13 | 106 | 通过 |

修订门禁要求两个分支分别至少 15 笔、8 币、3 月、10 个事件簇，不再对比例设置人为上限。
ADA-USDT-SWAP `2026-07-18 08:15` 以冻结长度 32 正确归入短分支。该覆盖结论不读取退出、
MFE、MAE、胜负、R、PnL 或后续 K 线，因此允许进入 L2。

## V4 因果与退出优先级合同

1. `range_length_bars` 在合格突破源完成时由已确认父横盘冻结；等待第 1～3 根接受确认时不增长。
2. V4 完整复用 V3 的状态机；不得新增、删除、提前或延后任何原始信号。
3. `1R` 使用入场意图已经冻结的 `stop_ticks`；短分支进入既有 `ExitPolicy::Fixed` 时令
   `target_ticks = stop_ticks`，实际目标价仍以真实下一根开盘和 tick size 构造。
4. 长分支继续使用突破源棒的量能 ATR 目标，不能在确认棒重算量比、ATR 或目标档位。
5. Bollinger、EMA596、三棒吞没、逆势结构等更高优先级特殊退出保持不变；V4 不抢占它们。
6. 不加入部分止盈、1R 后保本、移动止损、时间退出或任何新的入场过滤。

## L2 执行与审计

1. 用同一当前 Release 可执行文件分别回放 V3 与 V4；行情、可执行文件、universe manifest、
   评价窗口、成本和其他 Research 轴必须逐项同一。
2. 成本固定为每边 5 bps 手续费加 3 bps 滑点；只使用本地 43 个完整成员，17 个缺失成员跳过
   并保留证据，不同步或补造 K 线。
3. 完整恢复下一根开盘、tick 对齐止损/目标、同棒止损优先、同币种持仓冲突与反手；提前退出
   释放的后续交易必须由 broker replay 自然产生，不能事后拼接。
4. 机器账本必须新增信号时可见的冻结横盘长度与 V4 短分支标识；不得把 outcome 混入特征。
5. 审计项目：
   - V3/V4 原始信号身份完全相同；
   - 短分支且进入 Fixed 退出的意图满足 `target_ticks == stop_ticks`；
   - 长分支与特殊退出的冻结目标合同不因 V4 漂移；
   - 单独报告目标被改变的已执行交易，以及持仓释放后新增/阻塞/改变的交易。

## L2 采用条件

- 严格视觉横盘家族成本后 `average_net_r > 0` 且 `profit_factor_r > 1`；
- V4 家族成本后 EV 与 PF 均优于同身份 V3；
- 正收益不依赖 1～2 笔、单一币种、单一月份或单一 60 分钟事件簇；
- 只有满足上述探索门禁才允许进入 L3。即使通过，也不等于达到生产职业指标或允许上线。

## 预期身份与产物

- universe：`top60_v36_direct_kline_20260721_frozen_20260723`
- manifest SHA-256：`3fd267ca5cf1ecee8199232729da0e6db917803f6e7a1b363fa84e0ba75d5a4f`
- L1 行情指纹：`4919b364fb4737b0da7921cd53e401adb97013597ba3049ca79c6e1e9890577f`
- V4 L2 机器结果：
  `docs/backtest_reports/tradingview_velocity_strict_visual_breakout_short_range_32_one_r_v4_l2_20260803.json`
- L2 行情与可执行文件指纹由本批次同一只读运行输出后核对。
