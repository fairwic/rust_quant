# 严格视觉横盘外部结构上沿门禁 V9 预注册

## 研究身份

- 研究等级：L1；只有 L1 通过后才允许进入 L2 本地多币种诊断。
- 基线版本：`volume_strict_visual_consolidation_stronger_acceptance_margin_0_40_atr_long_15m_research_v8`。
- 候选版本：`volume_strict_visual_consolidation_external_structure_clearance_long_15m_research_v9`。
- 周期：OKX 永续 15 分钟。
- 冻结币池：沿用 V8 的 Top60 manifest；L1/L2 允许显式 partial diagnostic，但不得表述为正式全市场结论。
- V8 L1 基线：`target/research/strict_visual_acceptance_margin_v8_l1_impl_20260804.json`，SHA-256 `7e609ee39a29d4c04b41e0a4333251db6eb15e1b124e0584dd9acb3ad24115e3`。
- V8 L2 基线：`docs/backtest_reports/tradingview_strict_visual_breakout_long_acceptance_margin_stronger_grid_v8_l2_20260804.json`，SHA-256 `9eb41ba7a10c9763157144303932871bd15b48589b3f925cc4b97100cfc94220`。

## 唯一变量

V9 只在 V8 原本会形成接受确认信号的同一根完成棒上，新增“突破来源必须收盘越过尚未解决的外部结构上沿”门禁。横盘识别、P90/P10 视觉边界、强突破实体、量能、回踩接受、0.40 ATR 接受余量、下一根开盘成交、止损、止盈、冲突和退出政策全部保持 V8 不变。

## 因果定义

对 V8 已冻结的父横盘：

```text
L = range_length_bars
N = min(L, 32)
pre_start = range_start_index - N
pre_end = range_start_index - 1
external_high = max(high[pre_start..pre_end])

resolved_before_breakout =
    any(close[range_start_index..breakout_index - 1] >= external_high + tick_size)

trade_breakout_upper =
    resolved_before_breakout
        ? visual_upper
        : max(visual_upper, external_high)

qualified = breakout_close >= trade_breakout_upper + tick_size
```

- 必须完整拥有 `N` 根前置完成 K 线；证据不足时 V9 门禁失败，不以较短窗口替代。
- 外部高点、解除状态和交易上沿均在突破棒完成时冻结；确认棒及其后的价格不得重算。
- 为保持 V9 是 V8 的严格子集，V9 仍推进 V8 原有三棒 pending，并在 V8 原本确认或拒绝来源的同一决策时点消费来源；被过滤后不得释放状态补造稍后的新信号。
- `visual_upper` 继续用于画区间与回踩接受；`trade_breakout_upper` 只用于判断突破来源是否真正越过外部结构。

## L1 无标签字段

机器结果必须记录：`external_lookback_bars`、`external_high_time_ms`、`external_high`、`resolved_before_breakout`、`trade_breakout_upper`、`required_breakout_close`、`breakout_high`、`breakout_close`、`clearance_ticks`、`clearance_atr`，以及接受或拒绝身份。不得包含或读取 MFE、MAE、退出时间、最终 R、胜负、PnL 或 Profit Factor 来选择范围。

## 目标样本与预计影响

- 目标样本：`BTC-USDT-SWAP`，突破时间 `2026-07-15 06:15:00 Asia/Shanghai`。冻结外部高点应为 `65,100.0`，突破高点 `65,097.1`、收盘 `64,907.0`，V9 必须拒绝。
- 预计影响：过滤 V8 合格候选的 5%～40%。这是预注册估计，不作为收益结论。

## L1 门禁与停止条件

L1 同时满足以下条件才进入 L2：

1. 目标 BTC 样本被拒绝，且外部高点时间和值与冻结证据一致；
2. V9 是 V8 严格子集，没有新增候选；
3. 至少拒绝 3 个候选，避免只解释 1～2 笔；
4. 保留候选不少于 15 笔、覆盖不少于 8 个币、3 个上海自然月和 10 个 60 分钟事件簇；
5. 外部证据全部来自突破时已经完成的 K 线，缺失证据数为 0。

任一条件失败立即停止，不通过查看后续盈亏调整 `N`、32 根上限、解除条件或 1 tick 门槛。

## L2 保持不变项与停止条件

- 成本：沿用 V8 的单边 5 bps 手续费加 3 bps 滑点压力。
- 本轮不启用横盘持久性止盈，也不启用 0.8R 净保本；它们属于后续独立研究变量。
- 只使用本地现有且预热完整的成员，不自动补 K 线。
- 若成本后净 R、平均净 R 或 Profit Factor 相对 V8 不改善，或改善只集中于 1～2 笔、单币、单月或单一事件簇，则停止在 L2，不创建 Pine、不进入生产入口。
