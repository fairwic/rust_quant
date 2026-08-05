# Vegas 压缩突破 1ATR 最小结构止损 4H v1 评估清单

## 1. 研究身份与边界

- 独立研究策略键：`vegas_breakout_one_atr_invalidation_stop_4h_research`。
- 入场/风控版本：`compressed_breakout_one_atr_invalidation_stop_v1_20260720`。
- 候选配置版本：`xasset_4h_top100_v43_breakout_one_atr_stop_20260720`。
- 冻结来源：v41 `12582..12681`；不继承 v42 的删单门禁，确保成交频率不因过滤下降。
- 仅运行本地 `ResearchBar`；100 个配置保持 `enabled=false`，不覆盖 v27/v40/v41/v42，不进入 Paper、Live 或生产默认消费路径。

## 2. 冻结亏损归因

v40 新增的 90 笔压缩突破空头中，初始结构止损距离不超过 `1ATR` 的交易有 46 笔：

- discovery：24 笔，EV `0.520R`、PF `1.670`；
- validation：22 笔，EV `0.021R`、PF `1.027`。

同一 setup 中止损距离为 `1—1.5ATR` 的两段 EV 为 `0.639R/0.695R`，距离大于 `1.5ATR` 的两段 EV 为 `1.653R/0.567R`。这说明区间失效位如果离突破收盘过近，容易落在正常 4H 波动噪声内；问题可能是止损尺度，而不是入场方向或币种。

`1ATR` 是自然波动单位，不来自本轮阈值扫描。本轮只验证一个改动，不运行 `0.8/1.2/1.5ATR` 邻域。

## 3. 冻结规则

1. 完整保留 v41：v27 原始信号、v40 空头压缩突破、v36 低 ATR 扫高确认空均不改变；v42 弱扩张删单门禁关闭。
2. 只修改 `COMPRESSED_RANGE_BREAKOUT_SHORT` 的信号结构止损：

```text
prior_range_stop = 突破前 5 根 K 线最低价
one_atr_stop = 入场收盘价 + 信号时点 ATR(14)
signal_stop = max(prior_range_stop, one_atr_stop)
```

3. 风险引擎仍在 `signal_stop` 与既有最大亏损止损中选择更紧者，所以本规则不会绕过 `max_loss_percent=3.5%`。
4. 不删除或延迟信号，不改变止盈、仓位、容量、成本和其他 setup；所有输入均来自信号时点已完成 K 线。
5. ATR 未就绪或为零时退化为原整理区边界，不读取未来 K 线补值。

## 4. 统一回放与停止门禁

- 固定组合：`100U`、单笔风险 `0.75%`、容量 `12`、单边额外滑点 `5 bps`、资金费 `1 bps/8h`。
- 目标：实际成交频率 `>=8/月`；净 EV `>=0.6R`、PF `>=2.2`、Recovery `>=4`、Sharpe `>=1.5`、盘中回撤 `<=15%`。
- 同时报告 effective events/month、discovery/validation、walk-forward、月份和收益集中度。
- 必须保持与 v41 相同的入场身份集合；若成交身份变化，先判定实现合同错误，不解释收益。
- 标准成本任一联合门禁失败即拒绝，不运行双倍成本、不扫描 ATR 倍数、不叠加 v42 门禁救参。
- 标准成本全部通过后，才运行双倍滑点/资金费；即使通过，也只能标记为 `development_candidate_forward_shadow_required`。

## 5. 实测结果与决策

- 本地回测：`back_test_log.id=12782..12881`，100 个配置，全部保持 `enabled=false`。
- 与 v41 的 245 个入场身份逐笔比对：缺失 `0`、新增 `0`；因此差异只来自本候选预注册的止损合同。
- 标准成本组合：245 笔、约 `8.20` 笔/月，178 个有效事件、约 `5.96` 个/月；净收益 `+160.986%`、EV `0.584R`、PF `2.032`、Sharpe `1.600`、Recovery `4.952`、盘中最大回撤 `14.79%`。
- discovery 158 笔：EV `0.471R`、PF `1.737`；validation 87 笔：EV `0.790R`、PF `2.396`。
- walk-forward test-2 为 `-4.142%`、PF `0.460`、EV `-0.555R`，弱窗没有修复，反而比 v41 更差。

固定决策：`rejected_standard_gate`。放宽止损虽然略提高胜率，但按冻结初始风险换算后，EV 从 v41 的 `0.612R` 降至 `0.584R`、PF 从 `2.120` 降至 `2.032`；这说明亏损根因不是结构止损普遍落在 1ATR 噪声内，而是部分突破入场本身缺少质量。按停止门禁不运行双倍成本，也不扫描 ATR 倍数。
