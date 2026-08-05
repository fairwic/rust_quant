# Vegas 压缩突破弱扩张空头门禁 4H v1 评估清单

## 1. 研究身份与边界

- 独立研究策略键：`vegas_breakout_weak_expansion_guard_4h_research`。
- 入场/风控版本：`compressed_breakout_weak_expansion_guard_v1_20260720`。
- 候选配置版本：`xasset_4h_top100_v42_breakout_weak_expansion_guard_20260720`。
- 冻结来源：v41 `12582..12681`；诊断对象为 v40 新增的 90 笔压缩突破空头。
- 仅运行本地 `ResearchBar`；100 个配置保持 `enabled=false`，不覆盖 v27/v36/v40/v41，不进入 Paper、Live 或生产默认消费路径。

## 2. 冻结亏损归因

在不新增数值分割点的信号时点诊断中，压缩突破空头的相对量 `>=2.5x` 在 discovery/validation 分别为 `0.875R / 0.756R` EV；低于 `2.5x` 时降至 `0.266R / -0.134R`。进一步与既有 EMA 距离状态交叉后，仅 `relative_volume<2.5x && EMA state=Normal` 在两个时段同时为负：

- discovery：3 笔、1 胜、EV `-0.461R`；
- validation：4 笔、1 胜、EV `-0.572R`；
- 合计 7 笔、净 `-3.673R`。

其含义是：价格虽然跌破窄幅整理区，但既没有达到项目已存在的 `2.5x` 冲击量标准，EMA 距离也没有进入 TooFar 扩张或 Ranging/Tangled 压缩状态，容易成为缺乏持续性的普通波动。`2.5x` 直接复用流动性扫单规则的冻结阈值，EMA 状态直接复用现有分类；本轮不扫描新阈值。

## 3. 冻结规则

1. 完整保留 v41：v27 原始信号、v40 空头压缩突破结构止损、v36 低 ATR 扫高确认空均不改变。
2. 只在准备补充 `COMPRESSED_RANGE_BREAKOUT_SHORT` 时检查：若信号时点 `relative_volume_ratio < 2.5` 且 `ema_distance_filter.state == Normal`，不生成该新增信号。
3. 原 v27 已有方向时不运行本门禁；低 ATR 扫高确认空优先级不变。
4. 下一根 K 线仍由完整策略使用当时可见信息重新判断；不保存未来承诺、不用后续 K 线补造入场。
5. 不改变结构止损、止盈、风险、容量和成本，不叠加 v35，不筛选币种、月份或方向子集。

## 4. 门禁与停止条件

- 组合固定：`100U`、单笔风险 `0.75%`、容量 `12`、单边额外滑点 `5 bps`、资金费 `1 bps/8h`。
- 目标：实际成交频率 `>=8/月`；净 EV `>=0.6R`、PF `>=2.2`、Recovery `>=4`、Sharpe `>=1.5`、盘中回撤 `<=15%`。
- 同时报告 effective events/month、discovery/validation、walk-forward 与收益集中度。
- 频率和联合质量必须同时通过；不得因质量改善而把不足 8 笔/月四舍五入为达标。
- 标准成本任一门禁失败即拒绝，不运行双倍成本，不缩小 7 笔子集、不调整 `2.5x` 或 EMA 状态。
- 本门禁来自已查看历史，即使全部通过也只能进入 `development_candidate_forward_shadow_required`。

## 5. 实测结果与决策

- 完整历史回测：`back_test_log.id=12682..12781`，100 个配置完整返回。
- 门禁精确移除预注册的 7 笔压缩突破空头，未改变 v27、流动性扫高确认或其他信号；238 笔全部被组合接受。
- 实际成交频率约 `7.97` 笔/月，低于 `8`；174 个有效市场事件，约 `5.83/月`。
- 标准成本组合：收益 `+168.617%`、净 EV `0.64118R`、PF `2.16301`、Sharpe `1.65708`、Recovery `5.04357`、盘中最大回撤 `14.7882%`。
- discovery/IS：155 笔，PF `1.75150`、EV `0.51614R`、Sharpe `1.21083`；validation：83 笔，PF `2.71069`、EV `0.87469R`、Sharpe `2.90153`。
- walk-forward test-2 仍亏损 `-2.293%`、PF `0.606`、EV `-0.472R`；门禁改善了该弱窗，但没有消除结构性失效。

**决策：`rejected_standard_gate`。** 亏损归因与门禁方向成立，EV、Sharpe、Recovery、回撤进一步改善；但 PF 仍比 `2.2` 少 `0.03699`，实际频率也降到 `7.97/月`，两项同时失败。按停止条件不运行双倍成本、不缩小亏损子集、不把频率四舍五入为通过。
