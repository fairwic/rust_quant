# Vegas bearish FVG 收复 + bullish CHoCH + MACD 多头 4H v1 评估清单

## 1. 研究身份与边界

- 独立研究策略键：`vegas_fvg_reclaim_bullish_choch_macd_long_4h_research`。
- 入场规则版本：`fvg_reclaim_bullish_choch_macd_long_v1_20260720`。
- 候选配置版本：`xasset_4h_top100_v50_fvg_reclaim_bull_choch_20260720`。
- 冻结来源：v42 `12682..12781`；不携带已判退的 v46—v49 补充规则。
- 仅运行本地 `ResearchBar`；配置保持 `enabled=false`，不覆盖 v27/v42，不进入 Paper、Live 或生产默认消费路径。

## 2. 预注册市场机理

v49 证明，bearish FVG 被收复且 MACD 柱改善在全币池中过于常见：高胜率由大量约 `0.3R` 的短促反弹组成，无法覆盖约 `1.06R` 的完整止损。FVG 和 MACD 只描述缺口被回补与短期动能改善，没有证明此前 bearish internal structure 已被破坏。

本轮不扫描 v49 的年龄、零轴、EMA、RSI 或量能桶，只增加一个离散的结构事件：收复棒当根必须产生新鲜 internal bullish CHoCH。它表示价格首次突破内部空头结构，而不是沿用 active BOS 这类成熟趋势状态。bullish BOS 继续作为后续延续事件，不提前用于本次反转入场。

## 3. 冻结规则

1. bearish FVG、`>=0.10 ATR` 缺口、`>=0.80 ATR` bearish 位移实体、形成后 1—4 根与首次完整收复定义，全部沿用 v49，不改变数值。
2. 收复棒必须收阳、收盘严格高于 FVG 上沿，且 MACD 柱体较上一根增加。
3. 同一根已完成收复棒必须出现 `internal_bullish_choch=true`；仅有历史 bullish BOS active、bearish BOS、没有新结构事件或下一根才出现 CHoCH，均不入场。
4. 内部结构指标固定使用 `internal_length=2`、`internal_threshold=1.5%`、只启用 internal signal；不扫描长度或阈值。
5. FVG 形成当根不得入场；此前已经收盘站上 FVG 上沿、超过 4 根或 CHoCH/MACD 缺任一条件时立即过期，不允许后续补造。
6. 保护止损继续位于 FVG 下沿与收复棒最低价二者较低值外 `0.6%`；原 ATR/最大损失风控继续保留。
7. 只新增多头 setup，且仅在原 Vegas、v36 与 v42 压缩突破均未产生方向时补充；v27/v42 既有入场和退出语义不变。
8. v48 bearish BOS + FVG 空头与 v49 无 CHoCH 收复多头均关闭；本轮只验证结构转向后的交集，不做参数邻域搜索。

## 4. 统一回放与停止门禁

- 固定组合：`100U`、单笔风险 `0.75%`、容量 `12`、单边额外滑点 `5 bps`、资金费 `1 bps/8h`。
- 联合目标：实际成交频率 `8—40/月`；净 EV `>=0.6R`、PF `>=2.2`、Recovery `>=4`、Sharpe `>=1.5`、盘中回撤 `<=15%`。
- 必须单列 v50 新增交易的 discovery/validation EV、PF、FVG 年龄、退出类型、有效事件数以及 current CHoCH/BOS 审计，并复核 XRP 指定时点没有被未来 K 线重写。
- 标准成本任一联合门禁失败即拒绝，不扫描 FVG、MACD 或结构参数；标准成本全部通过后才运行双倍成本。

## 5. 冻结后结果与决策

- 本地禁用配置 100 个，回测 `13484..13583`；标准成本组合 245 笔且全部接受，约 `8.14` 笔/月，有效事件 183 个、约 `6.08` 个/月。
- 组合净收益 `+158.58%`、PF `2.087`、EV `0.599R`、Sharpe `1.629`、Recovery `5.032`、保守盘中回撤 `13.87%`。频率、Sharpe、Recovery、回撤通过，但 PF 与 EV 失败；walk-forward test-2 为负。
- v50 新增 setup 原始成交 9 笔、5 胜，总 EV `-0.085R`、PF `0.818`；discovery 8 笔 EV `-0.145R`、PF `0.724`，validation 只有 1 笔小赢 `0.395R`，不能证明跨期优势。
- 为排除旧动态止盈压缩赢家的解释，固定做了一个不改变入场的 `2.5R` 入场后路径反事实：3 笔先到目标、6 笔先止损，毛 EV 仅 `0.167R`，计入成本后仍远低于目标。
- XRP 未触发 v50 反手多头：只保留 `2026-07-05 00:00` 的确认空单并在 `2026-07-08 20:00` 约 `+4.124R` 平仓，没有回到 `2026-07-06 20:00` 长下影位置重复追空。
- fresh bullish CHoCH 能把 v49 的过度交易压回目标频率，但没有把 FVG 收复变成正期望 setup；v50 拒绝，不运行双倍成本或参数扫描，配置保持 `enabled=false`。
