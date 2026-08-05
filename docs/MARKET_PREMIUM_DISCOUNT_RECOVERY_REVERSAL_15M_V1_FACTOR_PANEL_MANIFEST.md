# 永续溢价折价修复反转 15m V1 因子面板清单

## 1. 因子身份与假设

- `factor_key=market_premium_discount_recovery_reversal`
- `version=15m_v1_20260721`
- `rule_version=top2_down_impulse_premium_discount_5bps_1h_recovery_4h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在下载本窗口 premium index 和读取对应多头 outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

独立假设：结算 funding 是滞后且离散的持仓成本代理；Binance premium index 直接反映永续合约
相对现货指数的即时溢折价。当流动性 Top60 中 6h/24h 同时下跌的最弱币出现至少 `5bps` 永续折价，
且最近 1h 折价已经修复但尚未转正时，说明衍生品卖压开始衰竭；下一根 OKX 15m 开盘做多的
1h/4h outcome 应显著优于没有该确认的同类下跌候选。

该因子不同于已证伪的 funding 拥挤、OI/taker 翻转、订单簿和跨交易所可执行基差，不使用
BOS、CHoCH、FVG、影线或当前阳线。

## 2. 封存窗口、币池与来源

- OKX 价格：本地 `quant_core` 已确认 15m；
- Binance 因子：官方 USD-M `monthly/premiumIndexKlines/<symbol>/15m` ZIP 与 `.CHECKSUM`；
- 官方数据根：`https://data.binance.vision/data/futures/um/monthly/premiumIndexKlines/`；
- 窗口：`2022-07-01T00:00:00Z` 至 `2023-07-01T00:00:00Z`，12 个完整月；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202207_202306`；
- manifest SHA-256：`b610840ca9272e6db3bd3b9363bd5b6e84fac1e5b02e09a74ffd03db3c3fbdfd`；
- 每月 60 个当前仍为 live、`instCategory=1` 的 OKX 加密货币 USDT 永续；Binance 映射也必须
  当前 `TRADING + PERPETUAL + USDT + underlyingType=COIN`，排除退市币且不补币；
- 明确接受双交易所幸存者偏差；该窗口虽已用于其他价格/基差诊断，但 premium-index 确认与本规则
  outcome 从未打开，只作为新家族 Discovery；
- 全部候选、因子、入场和 outcome 均为 15m；1m 不参与。

## 3. 冻结价格候选与 premium 确认

每个 UTC `00:00 / 04:00 / 08:00 / 12:00 / 16:00 / 20:00`，对应 OKX 15m K 线完成后：

1. 当月成员必须具有连续 24h 和 6h OKX 收盘收益；当时完整价格覆盖至少 80%，否则整点阻塞；
2. 只保留 `return_24h < 0` 且 `return_6h < 0` 的成员；
3. 按 `return_6h` 从最负到较弱、symbol 字典序打破并列，冻结前两个价格候选；
4. premium 因子必须有连续 5 个 15m close，最后一个对应决策 K 线；
5. 确认条件同时为：`premium_current <= -0.0005` 且
   `premium_current > premium_1h_ago`；
6. 若前两个中存在确认候选，按第 3 条价格顺序只选第一个确认候选；若都未确认，则把第一个
   premium 完整候选记为 control；不因 outcome 缺失递补；
7. 方向固定做多；不要求阳线、下影线、BOS、CHoCH、FVG、OI、taker、funding 或订单簿。

所有输入只读取决策时点及以前的已完成数据；premium 的最后值不得来自决策后的文件行。

## 4. 冻结 outcome 与事件口径

- 下一根 OKX 15m 开盘为入场价；第 4、16 根完成 K 线收盘分别计算 1h、4h 多头毛收益；
- 面板不使用 high/low、止损、止盈、成本、容量或资金曲线；
- 报告 confirmed/control、前 6 月 Discovery、后 6 月 Validation、月份和 4h 触发聚类事件；
- 单腿真实策略标准往返成本约 `16bps`，因此因子晋级要求明显高于成本，而非只看命中率。

## 5. 预注册晋级与停止门槛

只有以下全部成立，才允许另立完整交易策略版本；否则永久淘汰，不扫描 5bps、1h、候选数、
价格窗口、持有期、月份或币种：

- confirmed 总观察数 `600～1,440`，对应 `50～120 笔/月`原始机会带；
- 4h 聚类后有效事件不少于 300；
- Discovery/Validation confirmed 各不少于 250；
- 两个时间段的 confirmed 4h 平均多头收益均 `>=0.25%`、正收益率均 `>=55%`；
- 两个时间段 confirmed 相对 control 的 4h 平均收益增量均 `>=0.15%`；
- confirmed 1h 总体平均收益为正。

后续策略仍须计入标准/双倍成本并满足净 EV `>=0.6R`、PF `>=2.2`、最大回撤 `<=10%`、
Recovery `>=4`、Sharpe `>=1.5`，以及月份、币种、事件集中度和统一资金容量审计。

## 6. 边界

- current-live-only 口径按用户要求排除退市币，但存在幸存者偏差；
- Binance premium index 是外部因子，不等于 OKX 可成交盘口；
- ResearchBar 不证明真实成交、保护单、资金费结算或恢复链路；
- 本面板不授权 Paper、Live、部署或任何真实交易 mutation。

## 7. 一次性结果与决策

- 冻结源码与 manifest 指纹：`638cc935ce24961537e8be5748078066278437f51b6b0887770cd4df7cb1848d`；
- 数据审计：81 个唯一 OKX 合约、79 个当前 Binance live crypto perpetual 映射、2 个映射阻塞；
  请求 1,106 个 premium 月包，795 个校验和/ZIP/CSV 连续有效、146 个历史未上市 404、165 个
  内容完整性无效，共解析 2,298,347 根；无效文件只阻塞对应 premium 因子，不补值；
- 漏斗：2,190 个 4h 决策时点、20 个价格覆盖阻塞、130,152 个完整价格观察、3,546 个前二
  下跌候选；2,537 个 premium 完整，237 个时点确认、1,306 个对照，1 个 outcome 不完整；
- confirmed 仅 237 个，月均 `19.75`，4h 聚类后 199 个有效事件，频率和独立样本均失败；
- confirmed 总体 1h/4h 毛收益 `0.0711% / 0.0938%`，命中率 `55.70% / 49.37%`；4h
  幅度低于约 16bps 单腿往返成本，命中也低于 55%；
- control 总体 1h/4h 为 `-0.0561% / -0.0098%`，说明 premium 确认相对 control 有一定诊断
  增量，但不足以形成可执行优势；
- Discovery 162 个，4h `+0.1706%`、命中 `51.85%`；Validation 仅 75 个，4h
  `-0.0722%`、命中 `44.00%`，方向与时间稳定性翻转；
- 月度 confirmed 只有 `1～38` 个，最后两个月 4h 分别 `-0.2888% / -0.5405%`；
- 决策：`factor_gate_passed=false`。premium 折价修复在频率、有效事件、成本幅度、4h 命中和
  Validation 同时失败，永久淘汰；不扫描 5bps、1h、候选数或持有期，不生成交易策略。
