# Vegas Compressed Range Breakout Short Structural Stop 4H v1 评估清单

## 1. 研究身份与边界

- 独立研究策略键：`vegas_compressed_range_breakout_short_4h_research`。
- 入场/风控版本：`compressed_range_breakout_short_structural_stop_v1_20260720`。
- 候选配置版本：`xasset_4h_top100_v40_breakout_short_structural_stop_20260720`。
- 冻结基线：v27 `10976..11075`；前置诊断版本：v39 `12282..12381`。
- 仅运行本地 `ResearchBar`；100 个配置保持 `enabled=false`，不覆盖 v27/v36/v39，不进入 Paper、Live 或生产默认消费路径。

## 2. 冻结假设

v39 的压缩突破多头在 discovery `10/10` 全亏，validation 的正收益由 JUP 单笔 `+12.292R` 主导；压缩突破空头则在 discovery/validation 分别取得 `0.359R / 0.322R` EV，方向一致但止损质量不足。

亏损路径显示：空头突破后重新站回此前 5 根整理区下沿的 7 个样本全部最终亏损。把该下沿作为开仓时即可确定的结构失效位，静态路径诊断在两个时间段都改善新增空头 EV/PF。因此 v40 只验证一个变化：空头压缩突破使用此前 5 根最低价作为保护止损；v39 其他入场条件完全冻结。

## 3. 冻结规则

1. 只启用 v39 的 `COMPRESSED_RANGE_BREAKOUT_SHORT`，多头分支关闭。
2. 仍要求：前 5 根区间宽度 `<=3%`、当前阴线实体占比 `>=60%`、量比 `>=1.5`、向下有效跌破 `>=1%`、MACD 双线在零轴下且负柱增强、EMA 非多头、无 bullish BOS。
3. 信号仅在当前 4H 收盘确认，且只补原 v27 没有方向的时点。
4. 初始结构止损固定为信号时点此前 5 根 K 线的最低价；该位置天然高于有效跌破后的入场收盘，不增加事后缓冲或阈值。
5. 本轮不扫描任何入场、止损、退出或方向参数；不叠加 v35/v36/v38，不增加事后亏损子集过滤。

## 4. 门禁与解释限制

- 组合口径固定：`100U`、单笔风险 `0.75%`、容量 `12`、单边额外滑点 `5 bps`、资金费 `1 bps/8h`。
- 目标：成交频率 `>=8/月`；净 EV `>=0.6R`、PF `>=2.2`、Recovery `>=4`、Sharpe `>=1.5`、盘中回撤 `<=15%`；同时报告 effective events/month、时间分段、月份和集中度。
- 新增空头 setup 在 discovery 与 validation 都必须为正 EV/PF，且结构止损相对 v39 的同向改善不能只来自单一事件。
- 由于 v40 的方向选择与止损来自已查看的 v39 全样本，即使历史门禁全部通过，也只能标记为 `development_candidate_forward_shadow_required`；必须等待未见数据或预先冻结的 forward shadow 后才能讨论 promote。
- 标准成本失败即拒绝，不运行双倍成本，不做任何救参。

## 5. 实测结果

- 回测区间：`back_test_log.id=12382..12481`，100 个币种配置完整返回。
- 原始成交：244 笔；多头 32 笔保持 v27 不变，空头 212 笔。
- 压缩突破空头本身：discovery 43 笔，EV `0.691R`、PF `2.051`；validation 47 笔，EV `0.358R`、PF `1.552`。结构止损相对 v39 在两段都改善，但验证段质量仍明显偏弱。
- 标准成本组合：收益 `+161.440%`、244 笔、约 `8.15` 笔/月、177 个有效市场事件（约 `5.93/月`）。
- 联合指标：净 EV `0.59948R`、PF `2.09638`、Sharpe `1.61655`、Recovery `4.93459`、盘中最大回撤 `14.7882%`。
- discovery/IS：157 笔，PF `1.7306`、EV `0.49595R`、Sharpe `1.1899`；validation/OOS：87 笔，PF `2.5667`、EV `0.7863R`、Sharpe `2.806`。
- walk-forward 仍有负窗口，其中 test-2 为收益 `-3.25%`、PF `0.521`、EV `-0.575R`；时间稳定性门禁未通过。

## 6. 决策

`rejected_standard_gate`。v40 首次把成交频率推到长期周期最低目标附近，并通过 Sharpe、Recovery 与回撤门禁，但净 EV 比 `0.6R` 少 `0.00052R`，PF 比 `2.2` 少 `0.10362`；同时 discovery 与 walk-forward 证据不足。按预注册规则不四舍五入、不运行双倍成本，也不追加事后过滤条件。
