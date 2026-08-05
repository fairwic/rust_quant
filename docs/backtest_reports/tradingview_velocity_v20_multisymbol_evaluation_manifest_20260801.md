# TradingView Velocity V20 多币种评估清单

## 研究身份

- 对照版本：`tradingview_velocity_v18_plus_false_breakout_lower_wick_guard_15m_research_v19@406cde87`
- 候选版本：`volume_anchor_upthrust_failed_acceptance_short_15m_research_v20@a755168d`
- 唯一变量：V20 新增 `volume_anchor_upthrust_failed_acceptance_short_15m_v1`；V19 其余入场、退出与优先级保持冻结。
- 状态：`Research-only`，本轮不得接入 Paper、ReadOnly、Live 或生产调度。

## 数据与回放口径

- 数据源：本地 `quant_core` 已校准的 OKX 永续 15 分钟完成 K 线，量能字段固定为 `vol_ccy`。
- 币种池：`top60_v36_direct_kline_20260721_frozen_20260723`，manifest SHA256 固定为 `3fd267ca5cf1ecee8199232729da0e6db917803f6e7a1b363fa84e0ba75d5a4f`。
- 评价窗口：`1751328000000..=1784470500000`；预热固定 60 天。
- 数据处理：不补造、不联网回填；缺少完整评价窗口或 60 天预热的成员跳过，并把结果标记为 `partial_data_diagnostic`。
- 成交时序：信号在完成棒收盘确认，最早下一根开盘成交；不得读取信号后的 K 线决定入场。
- 成本压力：手续费每侧 5 bps、滑点每侧 3 bps；同时保留零成本路径用于定位成本侵蚀。

## 预注册评价指标

1. 首要指标：新增家族成本后的成交数、币种覆盖、胜率、净 R、平均净 R 与 PF。
2. 配对影响：同一币种池、窗口和成本下，比较 V20 与 V19 的交易数、净 PnL、平均净 R、PF、最大回撤和盈利币种数差值。
3. 独立性：同方向且相隔不超过 60 分钟的信号只计为同一个粗略市场事件；原始交易数不能替代独立样本数。
4. 集中度：统计新增家族按币种、月份的净 R；若收益依赖单一币种、单一月份或一笔头部交易，只保留为窄域研究线索。
5. 淘汰规则：新增家族成本后 `平均净 R <= 0` 或 `PF <= 1`，直接判定当前定义无正向边际；即使为正，样本不足、币种覆盖不足或整体组合恶化时也不得晋级。
6. 职业级晋级仍要求成本后 EV `>= 0.6R`、PF `>= 2.2`、最大回撤 `<= 10%`、Recovery `>= 4`、Sharpe `>= 1.5`，并另行完成 point-in-time 币种池、容量约束、正式事件聚类与样本外验证。

本清单在查看 V20 多币种结果前冻结；本轮不根据结果修改阈值。
