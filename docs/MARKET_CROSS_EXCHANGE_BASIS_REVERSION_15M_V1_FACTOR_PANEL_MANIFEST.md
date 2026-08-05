# OKX—Binance 永续基差回归 15m V1 因子面板清单

## 1. 因子身份与假设

- `factor_key=market_cross_exchange_basis_reversion`
- `version=15m_v1_20260721`
- `rule_version=okx_binance_7d_basis_zscore_top1_4h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在下载本窗口 Binance 15m 合约 K 线和读取配对 outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

独立假设：同一 USDT 永续在 OKX 与 Binance 的相对价格偏离自身 7 日均值达到统计极端后，交易所
特有的短期供需应回归。若在下一共同 15m 开盘做空相对昂贵交易所、做多相对便宜交易所，未来
1h/4h 的双腿配对收益应显著优于非极端时点，并足以覆盖两腿进出四次成交成本。

该机制直接研究跨交易所相对价格，不使用 BOS、CHoCH、FVG、影线、BTC 残差、OI/taker、funding
或订单簿方向确认。

## 2. 封存窗口、币池与来源

- OKX：本地 `quant_core` 已确认 15m K 线；
- Binance：官方 USD-M `monthly/klines/<symbol>/15m` ZIP 及对应 `.CHECKSUM`；
- 官方数据根：`https://data.binance.vision/data/futures/um/monthly/klines/`；
- 当前 Binance 映射只接受官方 `exchangeInfo` 的 `TRADING + PERPETUAL + USDT + underlyingType=COIN`；
- 窗口：`2022-07-01T00:00:00Z` 至 `2023-07-01T00:00:00Z`，12 个完整月；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202207_202306`；
- manifest SHA-256：`b610840ca9272e6db3bd3b9363bd5b6e84fac1e5b02e09a74ffd03db3c3fbdfd`；
- 每月 60 个当前仍为 live、`instCategory=1` 的 OKX 加密货币 USDT 永续；只保留当前 Binance
  仍可交易且存在历史文件的对应合约，不用退市币或事后补币，明确接受双交易所幸存者偏差；
- 该市场窗口曾用于其他价格/OI 假设，但从未计算本跨交易所 7 日基差 z-score 和双腿 outcome；
  仅作为新家族 Discovery，不宣称为全局未触碰的最终 OOS；
- 全部信号、入场与 outcome 均为 15m；1m 不参与。

## 3. 冻结因子、覆盖与选择

每个 UTC `00:00 / 04:00 / 08:00 / 12:00 / 16:00 / 20:00`，对应两交易所 15m K 线完成后：

1. 对当月成员读取截至该时点连续、同时间戳的 `7d = 672` 个 OKX 与 Binance 15m 收盘；
2. 每根 `basis = ln(okx_close / binance_close)`；合约面值的固定倍数只改变均值，不改变 z-score；
3. 使用 672 个 basis level 的样本均值与样本标准差，计算最新
   `z = (basis_last - mean_7d) / std_7d`；
4. 当时币池至少 80% 成员具有完整同步因子，否则整个决策时点阻塞；
5. 每个时点按 `abs(z)` 降序、OKX symbol 字典序打破并列，只选择一个；
6. `abs(z) >= 2.0` 为 `extreme`，否则为 `control`；阈值在 outcome 前冻结，不扫描；
7. `z > 0` 表示 OKX 相对昂贵，方向为 short OKX / long Binance；`z < 0` 反向；
8. 不因后续 outcome 缺失或方向而递补第二名。

## 4. 冻结因子 outcome

- 下一根共同 15m 开盘作为两腿入场价；
- 分别用第 4、16、96 根共同完成 K 线收盘计算 1h、4h、24h 双腿方向收益：
  `direction * (okx_return - binance_return)`，`direction=+1` 表示 long OKX / short Binance；
- 面板不使用 high/low、止损、止盈、容量或资金曲线，不把因子诊断伪装成策略回测；
- 同时报告 `extreme/control`、前 6 月 Discovery、后 6 月 Validation、正/负 z 方向；
- 成本可行性基准：两腿进出共四次成交，每次 `8bps`，标准往返约 `32bps`；面板先报告毛收益，
  但晋级门槛必须明显高于该成本。

## 5. 预注册因子晋级门槛

只有以下全部成立，才允许另立完整交易策略版本；否则永久淘汰，不扫描 lookback、z-score、节奏、
持有期、币种或方向：

- `extreme` 总样本不少于 300，Discovery/Validation 各不少于 100；
- 正 z 与负 z 的 extreme 各不少于 100；
- Discovery 和 Validation 的 extreme 4h 平均配对收益均 `>=0.50%`、正收益率均 `>=55%`；
- 两个时间段的 extreme 相对 control 4h 平均收益增量均 `>=0.25%`；
- extreme 正 z 与负 z 的 4h 平均收益均 `>0.32%`，即在毛口径上高于标准四次成交成本；
- extreme 的 1h 与 24h 总体平均收益均为正，避免只依赖单一结算点。

因子通过也不等于职业策略通过。后续独立交易版本仍须满足净 EV `>=0.6R`、PF `>=2.2`、
15m `50～120 笔/月`、最大回撤 `<=10%`、Recovery `>=4`、Sharpe `>=1.5`，并验证双倍成本、
月份、方向、币种和有效事件集中度。

## 6. 边界

- 当前 live-only 双交易所口径按用户要求排除退市币，但存在幸存者偏差；
- 共同 15m 开盘不证明真实腿间延迟、盘口深度、部分成交、资金划转或交易所风险；
- 本面板不授权 Paper、Live、部署或任何真实交易 mutation。

## 7. 一次性结果与决策

- 冻结源码与 manifest 指纹：`f6f9a464480be7659d8d8b5a61341413d3eefa40f15e1258ac4147ba9d97e7f1`；
- 传输审计：首次下载中一个 HTTP 200 响应体提前中断，未产生任何因子结果；只补充响应体错误的
  有界重试并复用已校验缓存，因子、阈值、候选和 outcome 规则均未修改；
- 数据审计：81 个唯一 OKX 合约、79 个当前 Binance live crypto perpetual 映射、2 个映射阻塞；
  请求 1,106 个官方月包，961 个校验和/ZIP/CSV 有效、145 个历史未上市月包 404、0 个无效文件，
  共 2,782,058 根 Binance 15m；
- 因果漏斗：2,190 个 4h 决策时点、128 个覆盖阻塞、115,230 个完整同步因子、2,062 个
  Top1 候选；1,874 个极端、188 个对照，18 个缺少完整 24h outcome；
- 极端总体 1,856 个：1h/4h/24h 平均毛配对收益分别 `0.0600% / 0.0724% / 0.0862%`，
  正收益率 `75.11% / 80.39% / 85.45%`；
- Discovery 极端 915 个：4h 平均 `0.0804%`、正收益率 `80.00%`；对照 `0.0691%`，增量仅
  `0.0113%`；
- Validation 极端 941 个：4h 平均 `0.0646%`、正收益率 `80.77%`；对照 `0.0402%`，增量仅
  `0.0244%`；
- 正 z 963 个，4h 平均 `0.0891%`；负 z 893 个，4h 平均 `0.0544%`；方向和时间均稳定为正，
  但所有毛收益都远低于约 `0.32%` 的四次成交成本，更未达到 `0.50%` 因子门槛；
- 决策：`factor_gate_passed=false`。该 z-score 因子方向预测有效，却只捕捉高命中、低幅度的微小
  收敛，经济上不可执行。永久淘汰，不扫描 z-score、lookback、节奏或持有期，不生成交易策略。
