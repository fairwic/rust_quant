# Top Trader 仓位规模确信度价差 15m V1 因子面板清单

## 1. 因子身份与正交假设

- `factor_key=market_top_trader_size_conviction_spread`
- `version=15m_v1_20260721`
- `rule_version=top_position_over_account_ratio_rank1_rankN_8h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在下载完整全年指标档案与读取 outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

独立假设：Binance `count_toptrader_long_short_ratio` 衡量头部账户方向数量，
`sum_toptrader_long_short_ratio` 衡量头部账户持仓金额方向。两者之比高，表示平均头部多头仓位
相对平均头部空头仓位更大；两者之比低则相反。该规模确信度差异若含有信息，应在下一 8h/24h
形成横截面相对收益，而不是依赖 BOS/FVG、K 线反转或资金费 carry。

## 2. 数据、窗口与币池

- 外生因子：Binance 官方 USD-M daily `metrics` 5m 文件及官方 SHA-256；
- 价格 outcome：Binance 官方 regular 15m 月包及官方 SHA-256；
- 窗口：`2024-07-01T00:00:00Z` 至 `2025-07-01T00:00:00Z`，12 个完整月；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202407_202506`，
  manifest SHA-256 `c57182f2b52f7d62d13fb5968ee6e2b864a68b67b05c1ebe1b6519a332a9c137`；
- 每月成员还必须映射到当前 Binance `TRADING/PERPETUAL/USDT/COIN`；按用户要求排除退市币，
  不补币并明确接受幸存者偏差；
- 外生指标使用交易所原生 5m 发布频率，信号和执行时钟为 15m；不使用 1m。

## 3. 冻结信号、对照与 outcome

每个 UTC `00:00 / 08:00 / 16:00` 决策时点 `T`：

1. 每个成员只允许读取精确 `T-5m` 的已发布 metrics 点，不使用 `T` 或未来最近值；
   官方 `create_time` 若位于名义 5m 槽之后最多 60 秒，归一到该名义槽；超过 60 秒拒绝；
2. `count_toptrader_long_short_ratio` 与 `sum_toptrader_long_short_ratio` 都必须有限且严格大于 0；
3. 分数固定为 `ln(sum_toptrader_long_short_ratio / count_toptrader_long_short_ratio)`；
4. 当时可计算分数的 current-live 成员至少 30 个，否则整个时点阻塞；未来 outcome 完整性不参与覆盖；
5. 按分数降序、symbol 字典序打破并列；等名义做多 rank1、做空 rankN；
6. 对照固定为做多 index `floor((N-1)*0.25)`、做空 index `floor((N-1)*0.75)`；四腿不同；
7. `T` 的 Binance 15m 开盘入场，`T+8h` 与 `T+24h` 开盘计算固定简单收益；窗口内任意
   15m 缺口均阻塞，不递补其他币；
8. 因子价差收益为 `long_return-short_return`，对照同理；每个 8h 时点是一组原始观察；
9. 不设置 score、价格、OI、taker、funding、premium、基差、波动或 K 线阈值。

## 4. 预注册晋级与停止门槛

- 完整因子组不少于 1,000，Discovery/Validation 各不少于 500，对应约 83～91 组/月；
- 前后半年 24h 平均毛价差都 `>=0.50%`，正收益率都 `>=55%`；
- 前后半年相对中间分位对照的 24h 平均增量都 `>=0.25%`；
- 总体 8h 平均毛价差为正，至少 8/12 月 24h 平均为正；
- 任一合约参与极端多空腿次数不超过全部观察的 20%；
- 24h 平均毛价差必须显著高于两腿进出四次标准成本 32bps。

任一门槛失败则 V1 永久淘汰，不扫描 top-trader 两字段的方向、比值变换、rank、决策频率、
持有期或币种。只有全部通过才允许另立完整策略，计入 funding、标准/双倍成本、固定 R、容量、
保护单，并满足净 EV `>=0.6R`、PF `>=2.2`、最大回撤 `<=10%`、Recovery `>=4`、Sharpe
`>=1.5`。本面板不授权 Paper、Live、部署或任何交易 mutation。

## 5. 一次性结果与决策

首次运行发现 Binance 官方 `create_time` 存在 1～6 秒正常发布抖动，旧解析器错误要求原始相邻
时间精确等于 300 秒，导致 `7,554/21,560` 个文件被误标无效，2024-10/11 两个月零覆盖。
抽查 `BTCUSDT-metrics-2024-10-01.zip` 证实文件有完整 288 行、官方 checksum 有效，偏移行如
`08:00:01` 在下一 15m 决策前已发布。首次不完整面板的 814 组结果不得作为最终裁决。

结果可见后不改因子、方向、rank、窗口、币种或 outcome，只按第 3 节冻结的 60 秒发布容差修复
归一化，并新增回归测试。修复后官方文件审计为：请求 `21,560`，有效 `20,720`，明确缺失
`155`，仍不满足完整日合同 `685`，保留决策前指标点 `62,160`。

修复后的有效结果：`1,095` 个决策点仅 `6` 个因少于 30 个成员而阻塞；`61,736` 个因子观察，
完成 `1,089` 组且 outcome 无缺失。总体 8h/24h 毛价差 `-0.0959%/-0.2837%`，24h 命中率
`47.02%`；对照 24h `-0.2976%`，因子增量仅 `+0.0139%`。

Discovery `549` 组，8h/24h `-0.2245%/-0.6963%`；Validation `540` 组，
`+0.0349%/+0.1358%`。只有 5/12 月 24h 均值为正；最大参与币 `SATS-USDT-SWAP`
占 `15.70%`，集中度不是主因。`factor_gate_passed=false`。

结论：V1 永久淘汰。精确反向的总体 24h 毛收益只有 `+28.37bps`，仍低于 32bps 标准成本，
并会把 Validation 变为 `-13.58bps`，无需另立反向版本。不扫描 top-trader 账户/持仓两字段的
方向、变换、rank 或窗口。
