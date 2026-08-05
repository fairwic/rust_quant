# TradingView Velocity V22 确认棒结构目标消耗评估清单

## 研究问题

V21 要求 V20 扫高拒绝 setup 后的下一根完成 K 线收盘跌破 setup 低点，再于下一根
开盘做空。该右侧确认把家族成本后表现从 `-10.04R / PF_R 0.942` 改善为
`+13.27R / PF_R 1.383`，但等待确认也可能让价格提前走完过多的冻结结构目标，降低
实际成交后的剩余盈亏比。

本轮只验证一个变量：**确认棒收盘相对 V20 拒绝 setup 收盘，已经消耗了多少原始冻结
结构奖励距离**。

## 因果口径

对空单定义：

- `setup_close`：V20 扫高拒绝 setup 的收盘价；
- `confirmation_close`：紧邻下一根右侧确认棒的收盘价；
- `frozen_target`：突破时冻结的锚区下沿；
- `original_reward = setup_close - frozen_target`；
- `consumed_reward = setup_close - confirmation_close`；
- `target_consumption_ratio = consumed_reward / original_reward`。

所有字段在确认棒收盘时均已完成且生产可见。分母不大于零、比率非有限数或确认收盘已
到达/跌破冻结目标时直接视为无效 setup，不允许用后续 K 线补造信号。

## 冻结版本与参数邻域

- 基线：`volume_anchor_upthrust_failed_acceptance_right_side_short_15m_research_v21`。
- 25% 上限：
  `volume_anchor_upthrust_failed_acceptance_target_consumption_cap_25_15m_research_v22a`。
- 主候选 33% 上限：
  `volume_anchor_upthrust_failed_acceptance_target_consumption_cap_33_15m_research_v22b`。
- 50% 上限：
  `volume_anchor_upthrust_failed_acceptance_target_consumption_cap_50_15m_research_v22c`。

25% / 33% / 50% 是结果前冻结的同一变量参数邻域，不得在查看结果后追加中间阈值或把
表现最好的档位事后改称主候选。33% 是主候选，表达“确认最多消耗原结构空间三分之一”；
25% 用于测试更保守门禁，50% 用于检查放宽后的方向一致性。

## 保持不变的规则

- V20 放量突破、冻结锚区、扫高拒绝、阴线、收盘位置与拒绝量门槛不变；
- V21 只允许紧邻下一根完成棒确认、确认棒不得触及冻结止损、确认收盘最低 1.5R 不变；
- 确认后仍在下一根开盘成交，继续使用 V20 冻结止损与锚区下沿目标；
- 不调整 RSI、量比、ATR、止损、止盈、持仓冲突顺序或其他信号家族；
- 不按币种、月份或已知盈亏筛选样本。

## 数据与成本

- 数据源：本地 `quant_core` 的 OKX 已确认 15m K 线，成交量固定使用 `vol_ccy`；
- 币种池：与 V21 相同的 current-live 冻结 Top60；本地数据或 60 天预热不完整成员按
  既有规则跳过；
- 评价窗口、tick size、60 天预热与 V21 同范围；
- 成本：每边 5 bps 手续费 + 3 bps 滑点等价压力；
- 资金口径：逐币固定 1 单位，只比较统一初始风险 R，不把跨币原始价格 PnL 当组合收益。

## 查看结果前冻结的判断标准

目标消耗假设只有同时满足以下条件才算获得支持：

1. 25% → 33% → 50% → 无上限的成本后平均 R 或 PF_R 至少呈总体递减关系；若明显
   非单调，则不能宣称“消耗越少越好”。
2. 主候选 33% 的成本后净 R 为正，平均 R 与 PF_R 均高于 V21；不能只靠提高胜率。
3. 主候选至少保留 30 笔平仓交易、25 个 60 分钟事件簇，并覆盖至少 15 个币种；低于
   此样本只记为探索线索。
4. 移除最大 3 笔盈利后主候选仍为正，且全策略成本后总 R 不得比 V21 恶化。
5. 即使通过，本轮也只保留为 Research 候选；未达到净 EV `0.6R`、PF_R `2.2` 及完整
   稳健性闸门前，不进入 Pine 主版本、Paper、ReadOnly、Live 或生产路径。

## 解释边界

- 本变量只测量确认棒收盘的路径消耗；确认后到下一根实际开盘之间的跳空/滑移另作审计，
  不在本轮新增第二个门禁。
- 过滤会改变仓位占用和其他家族的实际成交路径，因此同时报告独立家族与全策略 R，不能
  用简单删单算术代替整段回放。
- current-live Top60 存在幸存者偏差，且当前只能使用本地完整成员，结果仍属于部分多币种
  诊断而非正式 60/60 point-in-time 结论。
