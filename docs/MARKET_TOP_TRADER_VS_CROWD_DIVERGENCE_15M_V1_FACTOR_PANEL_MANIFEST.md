# Top Trader 对全市场人群分歧 15m V1 因子面板清单

## 1. 因子身份与最后一项 positioning 假设

- `factor_key=market_top_trader_vs_crowd_divergence_spread`
- `version=15m_v1_20260721`
- `rule_version=top_position_over_global_account_ratio_rank1_rankN_8h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在计算本第三字段组合的任何 outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

上一因子比较的是头部交易者内部“持仓金额方向 ÷ 账户数量方向”，已经失败。本轮不是改方向，
而是加入此前未计算的全市场 `count_long_short_ratio`：若头部交易者持仓金额偏多、全市场账户数量
却偏空，smart money 与 crowd 的分歧可能预测相对强势；反之预测相对弱势。

## 2. 冻结数据和因果规则

- 数据、窗口、current-live-only 币池、Binance 映射、官方 daily metrics/regular 15m、SHA-256、
  最多 60 秒 5m 发布时间归一、至少 30 个共同成员均与 size-conviction V1 相同；
- 不使用 1m；外生 metrics 为原生 5m，执行与 outcome 为 15m；
- 每个 UTC 8h 决策 `T` 只读精确 `T-5m` 已发布点；
- score 固定为 `ln(sum_toptrader_long_short_ratio / count_long_short_ratio)`；两字段必须有限且正；
- 等名义做多 score rank1、做空 rankN；中间 25%/75% 为对照；symbol 打破并列；
- `T` 15m 开盘入场，`T+8h/T+24h` 开盘计算固定价差；任意内部缺口阻塞；
- 不使用 top-trader account ratio、OI、taker、funding、premium、基差、BOS/FVG 或价格阈值。

## 3. 预注册晋级与停止门槛

完全沿用上一因子：总组数 `>=1,000`、前后半年各 `>=500`；两个半年 24h 毛价差都
`>=0.50%`、命中率都 `>=55%`、相对对照增量都 `>=0.25%`；总体 8h 为正，至少 8/12 月
24h 为正，最大单币参与 `<=20%`，24h 毛幅度高于 32bps 成本。

任一失败则 positioning 家族停止，不再排列 top/global account、top position、taker 或 OI 字段，
也不扫描方向、rank、频率和持有期。通过才允许在全新时间窗验证，并进一步满足职业级 EV/PF/
回撤/Recovery/Sharpe 与执行风控。本面板不授权 Paper、Live、部署或交易 mutation。

## 4. 一次性结果与决策

严格解析与因果测试通过后只运行一次冻结面板。官方定位文件请求 `21,560` 个，有效
`20,720`，明确缺失 `155`，不满足完整日合同 `685`，最终保留 `62,160` 个决策前指标点；
regular 15m K 线请求 `1,610` 个月包，有效 `1,420`，明确缺失 `190`，共 `4,094,598` 根。

`1,095` 个决策点仅 `6` 个因共同成员不足 30 而阻塞；形成 `61,736` 个因子观察与
`1,089` 组完整价差，outcome 无缺失。总体 8h/24h 毛价差分别为 `-0.0757%/-0.4572%`，
24h 正收益率 `47.38%`；中间分位对照 24h 为 `+0.0251%`，因子相对对照反而落后
`-0.4823%`。

Discovery `549` 组，8h/24h 为 `-0.1863%/-0.7200%`，24h 正收益率 `44.99%`；
Validation `540` 组，8h/24h 为 `+0.0367%/-0.1900%`，24h 正收益率 `49.81%`。
只有 4/12 月 24h 均值为正；最大参与币 `BIGTIME-USDT-SWAP` 占 `17.36%`，集中度不是失败
主因。`factor_gate_passed=false`。

结论：V1 永久淘汰并按预注册停止规则结束 positioning 家族。精确反向的总体 24h 毛收益约
`+45.72bps`，但 Validation 只有约 `+19.00bps`，低于四腿标准成本 `32bps`，且方向选择已经
看过 outcome，因此不建立反向版本，也不继续排列 top/global account、top position、taker、
OI、rank、频率或持有期。

本次源码指纹：定位文件解析器
`bffe355d38233caa713eab60ec381476ad9443736e503c748ff5e300e64773c5`，面板实现
`fbabcdeaf0f37c8692249642649da68f120bd23f2c60943aef824d53068e740b`，运行入口
`fba781104d1c3e6b3b3b0d5706800e7b793da73a443c8603d2528a4cbea25530`。本面板始终为只读研究，
没有进入组合、Paper/Live、部署或任何交易 mutation。
