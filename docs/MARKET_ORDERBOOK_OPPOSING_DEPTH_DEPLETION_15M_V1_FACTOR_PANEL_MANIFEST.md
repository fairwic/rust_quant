# 对手盘深度消耗 15m V1 因子面板清单

## 1. 新研究身份与假设来源

- `research_key=market_orderbook_opposing_depth_depletion_panel`
- `version=15m_v1_20260721`
- `factor_rule_version=top2_impulse_opposed_1pct_depth_depletion_15m_8h_v1`
- 状态：`factor_research_only`，不生成策略信号，不具备 Promotion、Paper 或 Live 权限；
- 冻结时间：`2026-07-21`，在下载本窗口 bookDepth 和读取对应前瞻收益前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

假设来源：静态深度同向 V1 已淘汰；其 opposed 组 4h 平均方向收益为 `0.1927%`，但命中率仅
`50.68%`，不足以交易。新假设不是简单把符号反转，而是加入可解释的动态条件：价格冲量面对
可见对手盘深度时，如果该 1% 对手盘在决策前 15m 内持续减少，可能代表挂单被主动成交吸收或撤走，
价格延续应强于“对手盘未减少”的 control。

## 2. 独立窗口与数据边界

- OKX 可交易价格：已确认 15m；Binance USD-M `bookDepth` 只作为外部因子；
- 全窗口：`2025-07-01T00:00:00Z` 至 `2026-07-01T00:00:00Z`；
- Discovery：前 6 个完整月；Validation：后 6 个完整月；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202507_202606`；
- 币池 SHA-256：`39ea2f2cd13befdac456f25f0da052ed5fa04b0cf2692ec8b857e2f57750386a`；
- 每月 60 个、合计 155 个当前仍为 live 且 `instCategory=1` 的加密货币 USDT 永续；按用户要求
  排除退市币，并明确接受幸存者偏差；
- Binance 当前映射只接受 `TRADING + PERPETUAL + USDT + underlyingType=COIN`；官方日包必须有
  checksum；404、CSV 异常或窗口不完整只阻塞对应观测，不补值；
- 官方来源：`https://data.binance.vision/data/futures/um/daily/bookDepth/`；
- 本窗口虽用于其他价格/OI 假设，但从未读取本规则的订单簿变化与对应 outcome；本面板只打开一次。

## 3. 冻结价格候选与 outcome

- 每个 UTC `00:00 / 08:00 / 16:00`，计算已完成 15m 的 6h 与 24h 收益；两段必须连续、有限、
  严格同向；完整价格因子覆盖至少 80%；
- 多头按 6h 收益降序取前二，空头按 6h 收益升序取前二，并列按 symbol 字典序；
- 入场基准为下一根 OKX 15m 开盘；1h/4h outcome 分别为第 4/16 根收盘的方向收益；
- 候选形成不读取订单簿或 outcome；面板不做容量、止盈或止损回放。

## 4. 冻结动态订单簿因子

读取决策前 `[T-15m,T)` 的 Binance 1% 深度：

1. 每个 snapshot 必须同时存在 `percentage=-1` 的 bid notional 与 `percentage=1` 的 ask notional；
2. 全窗口至少 20 个 snapshot，最后一个距决策不超过 90 秒，相邻间隔不超过 90 秒；
3. 前段 `[T-15m,T-10m)` 与后段 `[T-5m,T)` 各至少 6 个有效 snapshot；
4. `imbalance=(bid-ask)/(bid+ask)`，全窗口取中位数；
5. Long 的对手盘是 ask，Short 的对手盘是 bid；对手盘变化为
   `median(last_5m)/median(first_5m)-1`；
6. `confirmed`：Long 的全窗口 imbalance `<0`、Short 的 imbalance `>0`，且对应对手盘变化严格 `<0`；
7. 其余非 neutral 完整观测全部进入 `control`；不扫描变化幅度阈值、深度档位或窗口长度。

## 5. 预注册因子门槛

分别报告 overall、Discovery、Validation 的 confirmed/control，以及 confirmed Long/Short：样本数、
平均 1h/4h 方向收益和正收益率。仅在以下全部满足时允许进入策略设计：

- 全窗口至少 600 个完整观测；Discovery 与 Validation 的 confirmed/control 各至少 100 个；
- confirmed 的 4h 平均方向收益在 Discovery 和 Validation 均 `>=0.25%`，正收益率均 `>=55%`；
- 两段中 confirmed 相对 control 的 4h 平均收益增量均 `>=0.15%`；
- confirmed Long 与 Short 全窗口都至少 100 个，且 4h 平均方向收益都为正。

任一不满足即淘汰，不在当前窗口改成 first/last 3m、10m、2%/3% 深度、变化分位或其他 outcome。

## 6. 一次性结果与决策

- 冻结源码指纹：`868dabbc62bd2b6cecd4c77a9fe0effe4e40758939365c6bd24f56f1418d2e8d`；
- 无效试跑审计：Binance 自 2026 年部分日包把 `percentage` 从整数文本 `-1/1` 改为等价小数
  `-1.00/1.00`；旧解析器错误拒绝 1,513 个文件，使 Validation 仅剩 122 个观测。该试跑不计作
  因子结果；修复只把来源字段解析为有限浮点并仍精确接受 `±1.0`，未改变档位、窗口、候选、
  confirmed 条件或 outcome，并由小数格式测试固定；
- bookDepth 数据审计：148/155 个 OKX 币存在当前 Binance 映射；请求 3,264 个候选日文件，
  3,262 个完整有效、2 个 404、0 个 CSV 无效；796 个候选窗口未通过完整性门禁；
- 漏斗：1,095 个决策时点、1 个价格覆盖阻塞；42,324 个 6h/24h 同向观测、4,062 个多空
  各前二候选、0 个 outcome 不完整、3,190 个完整订单簿观测、0 个 neutral；
- confirmed overall：911 个，1h 平均 `0.2009%`、4h 平均 `0.3374%`，4h 正收益率 `50.05%`；
- control overall：2,279 个，1h 平均 `-0.0136%`、4h 平均 `0.0579%`，4h 正收益率 `46.99%`；
- confirmed Discovery：472 个，4h 平均 `0.1409%`、正收益率 `49.79%`；control 为 `0.1080%`；
- confirmed Validation：439 个，4h 平均 `0.5486%`、正收益率 `50.34%`；control 为 `0.0049%`；
- confirmed Long：259 个，4h 平均 `0.0276%`、正收益率 `44.02%`；
- confirmed Short：652 个，4h 平均 `0.4604%`、正收益率 `52.45%`；
- 决策：`factor_gate_passed=false`。全窗口均值有边际，但 Discovery、命中率和 Long 明确失败，
  效果集中在后半段和 Short，不能宣称跨时间/双方向稳定。本因子定义永久淘汰，不在该窗口调整
  前后段、档位、变化阈值或 outcome，也不把 Short 子组直接晋级为策略。
