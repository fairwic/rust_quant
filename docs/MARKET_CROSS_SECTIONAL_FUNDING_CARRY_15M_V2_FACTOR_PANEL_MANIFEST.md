# 横截面 Funding Carry 15m V2 因子面板清单

## 1. 版本身份与 V1 修正边界

- `factor_key=market_cross_sectional_funding_carry`
- `version=15m_v2_20260721`
- `rule_version=post_settlement_common_min30_bottom1_top1_hold_next_funding_8h_v2`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，V1 的 `1,095` 个时点全部在读取 outcome 前被覆盖阻塞；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

V1 错把 OKX Top60 的 80% 直接当成 Binance 历史 8h funding 必须覆盖的分母，然而单时点最大
共同覆盖只有 41，导致规则结构上不可能产生观察。V2 只修正数据母集：当时共同可交易且当前仍
live 的 8h funding 合约至少 30 个才允许排名。该阈值来自 V1 的输入覆盖审计：`931/1,095`
个时点达到 30 个；V1 没有读取任何下一期 funding 或价格 outcome。

## 2. 冻结数据与共同可交易母集

- 窗口、官方 Binance funding/regular 15m 月包、校验和、OKX current-live Top60 manifest、
  current Binance crypto perpetual 映射与 V1 完全相同；
- manifest SHA-256：`c57182f2b52f7d62d13fb5968ee6e2b864a68b67b05c1ebe1b6519a332a9c137`；
- 每个结算时点的母集只包含：当月 OKX current-live Top60 成员、当前 Binance 仍为
  `TRADING/PERPETUAL/USDT/COIN`、当时已经上市、`funding_interval_hours=8` 且当前费率有效；
- 该共同母集少于 30 时整个时点阻塞；达到 30 后对全部可用成员排序，不递补未来数据；
- 按用户要求排除退市币，明确接受 current-live-only 幸存者偏差；全部执行价格为 15m，不使用 1m。

## 3. 冻结信号、outcome 与成本

除覆盖母集外完全沿用 V1：

1. 在 funding 时点 `T` 只使用刚公布且刚结算的当前费率；最低者做多、最高者做空；
2. 当前费率差 `>=32bps` 为可执行候选，`16～32bps` 为近成本对照，低于 16bps 不观察；
3. `T+15m` 共同开盘入场，跨越 `T+8h` 下一结算，在 `T+8h+15m` 共同开盘退出；
4. 下一期 funding 必须精确存在且仍为 8h；它只用于 outcome，绝不用于信号排序或递补；
5. 总毛 PnL=`long_price_return-short_price_return+next_high_rate-next_low_rate`；
6. 标准成本为四次成交共 32bps，双倍成本 64bps；任意价格缺口或未来 funding 缺失则阻塞；
7. 不使用 BOS、CHoCH、FVG、影线、趋势、OI、taker、premium、基差或订单簿。

## 4. 预注册晋级与停止门槛

- 可执行组不少于 600，Discovery/Validation 各不少于 250，即总体至少 50 组/月；
- 下一期实际 funding PnL 在两个半年平均都 `>=32bps`；
- 标准成本后总 PnL 在两个半年平均都 `>=0.50%`、正收益率都 `>=55%`；
- 两个半年相对近成本对照的标准成本后平均增量都 `>=0.25%`；
- 至少 8/12 月标准成本后平均为正；
- 任一合约参与次数不超过全部可执行观察的 20%；双倍成本总体平均仍为正。

任一门槛失败则 V2 永久淘汰，不扫描 `20/25/35/40` 个成员、费率阈值、rank、持有期或币种。
只有全部通过，才允许另立完整策略并验证固定 R、PF、回撤、Recovery、Sharpe、容量和保护单。
本面板不授权 Paper、Live、部署或任何交易 mutation。

## 5. 一次性结果与决策

官方输入与 V1 相同且全部复用已校验缓存。`1,095` 个 8h 结算时点中，`164` 个低于 30 个
共同成员而阻塞；其余 `931` 个时点产生 `34,401` 个当前费率观察。当前费率差低于 16bps 的
时点 `911` 个，近成本对照 `8` 个，达到 32bps 的经济候选仅 `12` 个；其中 `1` 个缺少严格
共同 outcome，最终可执行观察 `11` 个，即约 `0.92` 组/月。

| 分组 | 观察 | 当前费率差 | 下一 funding PnL | 价格 PnL | 零成本总 PnL | 标准成本后 | 正收益率 | 双倍成本后 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 可执行总体 | 11 | +0.6539% | +0.4373% | -0.5925% | -0.1552% | -0.4752% | 27.27% | -0.7952% |
| Discovery | 7 | +0.7740% | +0.5560% | -0.9509% | -0.3950% | -0.7150% | 14.29% | -1.0350% |
| Validation | 4 | +0.4438% | +0.2296% | +0.0348% | +0.2644% | -0.0556% | 50.00% | -0.3756% |
| 近成本对照 | 8 | +0.2085% | +0.1599% | -0.9000% | -0.7401% | -1.0601% | 50.00% | -1.3801% |

12 个月只有 1 个月出现可执行组标准成本后正均值；`FLOKI-USDT-SWAP` 参与 5/11 次，集中度
`45.45%`。`factor_gate_passed=false`。

结论：V2 永久淘汰。费率极端确有短期持续性，但极端低 funding 多头与极端高 funding 空头的
相对价格继续沿拥挤方向运动，平均 `-59.25bps`，不仅吞掉下一期 `+43.73bps` carry，零成本总
收益已经为负。精确反向只会得到 `+15.52bps` 零成本毛收益，仍低于 32bps 成本，且仍只有 11
次机会；不另立反向版本，也不降低费率阈值扫描价格效应。
