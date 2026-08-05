# OKX—Binance 可执行基差首次越界 15m V1 因子面板清单

## 1. 新因子身份与独立假设

- `factor_key=market_cross_exchange_executable_basis_dislocation`
- `version=15m_v1_20260721`
- `rule_version=okx_binance_7d_basis_first_cross_50bps_15m_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在下载本窗口 Binance 15m 月包和读取配对 outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

独立假设：z-score 面板已经证明跨交易所微小基差高概率收敛，但 4h 毛收益只有约 7bps，无法覆盖
四次成交成本。本面板不提高 z-score，而是只观察相对自身 7 日均值首次跨越 `50bps` 的绝对基差
事件；该阈值由 `32bps` 标准往返成本加执行余量推导，未从本窗口 outcome 搜索。若此类新发生的
经济幅度偏离仍不能产生足够毛收益和频率，则整个跨交易所基差分支停止。

## 2. 封存窗口、币池与来源

- OKX：本地 `quant_core` 已确认 15m；Binance：官方 USD-M monthly 15m klines 和 `.CHECKSUM`；
- 窗口：`2024-07-01T00:00:00Z` 至 `2025-07-01T00:00:00Z`，12 个完整月；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202407_202506`；
- manifest SHA-256：`c57182f2b52f7d62d13fb5968ee6e2b864a68b67b05c1ebe1b6519a332a9c137`；
- 每月 60 个当前仍为 live、`instCategory=1` 的 OKX 加密货币 USDT 永续；Binance 腿也必须当前
  `TRADING + PERPETUAL + USDT + underlyingType=COIN`，不使用退市币、不事后补币；
- 明确接受双交易所幸存者偏差；本窗口虽然用于其他因子，但从未读取本 50bps 首次越界规则的
  双腿 outcome；
- 全部因子、入场和 outcome 都是 15m；1m 不参与。

## 3. 冻结首次越界事件

每根两交易所共同完成的 15m K 线后：

1. 使用连续同步的 673 个 basis level，`basis = ln(okx_close / binance_close)`；
2. `current_deviation = current_basis - mean(current trailing 672 basis)`；
3. `previous_deviation = previous_basis - mean(previous trailing 672 basis)`；
4. 可执行越界：`abs(current_deviation) >= 0.0050` 且
   `abs(previous_deviation) < 0.0050`；
5. 近成本对照：`0.0032 <= abs(current_deviation) < 0.0050` 且
   `abs(previous_deviation) < 0.0032`；
6. 当时币池至少 80% 成员具有完整同步因子，否则整个 15m 时点阻塞；
7. 只在新越界成员中按 `abs(current_deviation)` 降序、symbol 字典序打破并列，最多选择一个；
8. 若同时存在可执行和对照越界，绝对偏离更大的候选自然优先；不因 outcome 缺失递补；
9. deviation 为正：short OKX / long Binance；为负：long OKX / short Binance；
10. 不使用 z-score、BOS、CHoCH、FVG、影线、OI/taker、funding 或订单簿。

## 4. 冻结 outcome 与事件口径

- 下一根共同 15m 开盘为两腿入场价；
- 第 4、16、96 根共同完成 K 线收盘分别结算 1h、4h、24h 毛配对收益；
- 不使用 high/low，不在面板阶段添加止损、止盈、容量或仓位；
- 按选中决策时间 4h 内归并为一个有效跨市场事件，用于防止一次系统性价差冲击虚增样本；
- 报告可执行/对照、Discovery/Validation、正/负 deviation、原始观察数和有效事件数；
- 标准成本可行性仍以四次成交约 `32bps` 为基准。

## 5. 预注册晋级与停止门槛

只有以下全部成立，才允许另立完整交易策略；否则永久淘汰，不扫描 50bps、32bps、lookback、
首次越界语义、持有期、币种或方向：

- 可执行越界总观察数 `600～1,440`，对应组合 `50～120 笔/月`的原始机会带；
- 4h 聚类后有效事件不少于 300；
- Discovery/Validation 可执行样本各不少于 250，正/负 deviation 各不少于 100；
- Discovery/Validation 的 4h 平均毛配对收益均 `>=0.50%`、正收益率均 `>=55%`；
- 两个时间段相对近成本对照的 4h 平均收益增量均 `>=0.20%`；
- 可执行组 1h 与 24h 总体平均毛收益均为正。

后续策略仍须计入真实四次成交与双倍成本，满足净 EV `>=0.6R`、PF `>=2.2`、最大回撤
`<=10%`、Recovery `>=4`、Sharpe `>=1.5`，并通过统一资金、并发、月份、方向、币种和事件集中度。

## 6. 边界

- 当前 live-only 口径排除退市币但存在幸存者偏差；
- 15m 共同开盘仍不证明真实腿间延迟、盘口深度、部分成交、资金划转或交易所信用风险；
- 本面板不授权 Paper、Live、部署或任何真实交易 mutation。

## 7. 一次性结果与决策

- 冻结源码与 manifest 指纹：`240164e809137ee4594ee35cf146304cc3368a721dfd3e4b1fc08aa2bc03c48e`；
- 数据审计：119 个唯一 OKX 合约、115 个当前 Binance live crypto perpetual 映射、4 个映射
  阻塞；请求 1,610 个官方月包，1,420 个校验和/ZIP/CSV 有效、190 个历史未上市 404、0 个
  无效文件，共 4,094,598 根 Binance 15m；
- 因果漏斗：35,040 个 15m 决策时点、0 个覆盖阻塞、2,040,341 个完整同步因子；横截面出现
  565 个 50bps 首次越界和 2,571 个 32～50bps 对照越界；确定性 Top1 后为 501 个可执行、
  2,519 个对照，0 个 outcome 不完整；
- 501 个可执行事件仅月均 `41.75` 个，低于 50～120 机会带；4h 全市场聚类后只有 107 个
  有效事件，远低于 300；
- 可执行总体 1h/4h/24h 平均毛配对收益为 `0.1056% / 0.1809% / 0.2449%`，正收益率
  `59.68% / 65.67% / 71.46%`；方向正确但 4h 毛收益仍低于约 32bps 四次成交成本；
- Discovery 只有 21 个可执行样本，4h 均值 `0.5121%`、命中 `90.48%`；Validation 480 个，
  4h 均值降至 `0.1664%`、命中 `64.58%`，样本数量和经济幅度都严重时间漂移；
- 正偏离 227 个，4h 均值 `0.2029%`；负偏离 274 个，`0.1627%`，两边都低于成本；
- 对照总体 4h 均值 `0.0939%`，可执行组虽有增量，但不足以满足成本与职业目标；
- 决策：`factor_gate_passed=false`。50bps 规则在频率、有效事件、Discovery/Validation 覆盖和
  成本后可行性同时失败。整个跨交易所基差分支永久停止，不扫描阈值、节奏或持有期，不生成策略。
