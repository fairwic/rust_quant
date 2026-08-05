# 横截面 Funding Carry 15m V1 因子面板清单

## 1. 因子身份与独立假设

- `factor_key=market_cross_sectional_funding_carry`
- `version=15m_v1_20260721`
- `rule_version=post_settlement_bottom1_top1_hold_next_funding_8h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在读取本规则的下一期 funding 与价格 outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

独立假设：BOS/FVG、单币 OHLC 反转、订单簿、跨交易所基差、premium 和无条件横截面动量
都没有稳定成本后优势。永续合约 funding 则是可实际结算的现金流：若刚结算的横截面极端费率在下一
个 8h 周期具有持续性，做多最低费率合约、做空最高费率合约应同时收取两侧 funding；价格相对收益
不应系统性吞掉 carry，标准成本后仍应为正。

本面板不使用 BOS、CHoCH、FVG、K 线颜色、影线、趋势、OI、taker、premium、基差或订单簿。

## 2. 数据、窗口与币池

- 因子和实际 funding：Binance 官方 USD-M monthly `fundingRate` 月包及官方 SHA-256；
- 入场与退出：同一 Binance 合约官方 regular 15m K 线月包及官方 SHA-256；
- 窗口：`2024-07-01T00:00:00Z` 至 `2025-07-01T00:00:00Z`，12 个完整月；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202407_202506`，
  manifest SHA-256 `c57182f2b52f7d62d13fb5968ee6e2b864a68b67b05c1ebe1b6519a332a9c137`；
- 每月 OKX current-live crypto-only Top60，再映射到当前 Binance `TRADING / PERPETUAL /
  USDT / COIN` 合约；按用户要求排除退市币，不补币并明确接受幸存者偏差；
- 全部价格执行与 outcome 都是 15m；不使用 1m。

## 3. 冻结信号与执行时序

对每个 Binance 标准 8h funding 时点 `T`：

1. funding 行必须声明 `funding_interval_hours=8`；允许官方时间戳相对 15m 边界最多偏移 1 秒，
   归一到该边界，其他行拒绝；
2. 使用 `T` 刚公布且刚结算的 `last_funding_rate` 作为下一周期的持久性信号；不得使用 `T+8h`
   实际费率做排序；
3. 当月 Top60 至少 80% 同时有 `T` 的有效费率，否则整个时点阻塞；
4. 按当前费率升序、symbol 字典序打破并列；做多最低费率、做空最高费率；
5. 当前 `highest_rate - lowest_rate >= 0.0032`（32bps）才是经济可执行候选；`0.0016～0.0032`
   作为预注册近成本对照，低于 16bps 不产生观察；
6. 信号在 `T` 发布，统一到 `T+15m` 开盘入场，避免把结算瞬间与同一根开盘当成可同步成交；
7. 持仓跨越下一结算时点 `T+8h`，在 `T+8h+15m` 开盘退出；两腿必须都有连续价格，且下一
   funding 必须恰为 `T+8h`、间隔仍为 8h；缺失不递补；
8. 等 USDT 名义：价格毛 PnL 为 `long_return - short_return`；实际 funding PnL 为
   `next_high_rate - next_low_rate`；总毛 PnL 为两者之和；
9. 两腿进出四次成交，每次手续费加滑点 `8bps`，标准成本 `32bps`；同时报告零成本和双倍成本；
10. 每个结算周期互不重叠，按一组独立 carry 事件计数；不把同一时点多个币种伪装成多个事件。

## 4. 预注册晋级与停止门槛

仅当以下全部成立，才允许建立独立完整策略版本；否则永久淘汰，不扫描费率阈值、rank、持有期或
币种：

- 可执行组不少于 600，Discovery/Validation 各不少于 250，对应总体至少 50 组/月；
- 下一期实际 funding spread 在两个半年平均都 `>=32bps`，证明 signal 的 carry 持续性本身覆盖成本；
- 标准成本后总 PnL 在两个半年平均都 `>=0.50%`、正收益率都 `>=55%`；
- 两个半年相对近成本对照的标准成本后平均增量都 `>=0.25%`；
- 至少 8/12 月标准成本后平均为正；
- 任一合约参与极端多空腿次数不超过全部可执行观察的 20%；
- 双倍成本总体平均仍为正。

若通过，后续完整策略仍须固定初始风险、保护单、统一资金与容量，并满足净 EV `>=0.6R`、PF
`>=2.2`、最大回撤 `<=10%`、Recovery `>=4`、Sharpe `>=1.5`。本面板不授权 Paper、Live、
部署或任何交易 mutation。

## 5. 一次性结果与决策

官方文件审计：regular 15m 请求 `1,610` 个合约月包、有效 `1,420`、明确缺失 `190`、无效
`0`，共 `4,094,598` 行；funding 请求 `1,495` 个合约月包、有效 `1,335`、明确缺失
`160`、无效 `0`，共 `179,631` 行。两类数据均有 `115` 个当前 Binance live 映射。

研究窗口共有 `1,095` 个标准 8h 结算时点，但全部被 Top60 的 80% 覆盖门槛阻塞：单时点
最多只有 `41` 个当时已上市且使用 8h funding 的成员，只有 `186` 个时点达到 40 个，`931`
个时点达到 30 个，永远不可能达到要求的 48 个。因此因子观察、可执行候选与 outcome 均为 0，
`factor_gate_passed=false`。

结论：V1 因覆盖母集合同不可实现而无效，不能解释为 carry 盈利或亏损。由于没有读取任何下一期
funding 或价格 outcome，允许另立 V2，只把覆盖母集改为“信号时点至少 30 个共同可交易的 8h
funding 合约”；V1 规则和零样本证据保留，不原地降低 80% 门槛。
