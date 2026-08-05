# 订单簿深度失衡 15m V1 因子面板清单

## 1. 研究身份与目的

- `research_key=market_orderbook_depth_imbalance_panel`
- `version=15m_v1_20260721`
- `factor_rule_version=top2_impulse_binance_depth_1pct_median_15m_8h_v1`
- 状态：`factor_research_only`，不生成策略信号，不具备 Promotion、Paper 或 Live 权限；
- 冻结时间：`2026-07-21`，在下载本候选集合的 Binance bookDepth 和读取对应前瞻收益前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

目的：先证明订单簿深度相对 6h/24h 价格冲量具有稳定边际预测价值，再决定是否值得形成完整交易策略。
本面板不使用 BOS、CHoCH、FVG、K 线颜色、影线、OI、taker、funding 或残差条件。

## 2. 数据窗口与边界

- OKX 可交易价格：已确认 15m；Binance USD-M `bookDepth` 只作为外部因子；
- 全窗口：`2024-07-01T00:00:00Z` 至 `2025-07-01T00:00:00Z`；
- Discovery：前 6 个完整月；Validation：后 6 个完整月，二者分别报告；
- 币池：`okx_current_live_usdt_swap_prior_month_median_quote_volume_top60_202407_202506`；
- 币池 SHA-256：`c57182f2b52f7d62d13fb5968ee6e2b864a68b67b05c1ebe1b6519a332a9c137`；
- 每月 60 个、合计 119 个当前仍为 live 且 `instCategory=1` 的加密货币 USDT 永续；按用户要求
  排除退市币，并明确接受幸存者偏差；
- Binance 当前映射只接受 `TRADING + PERPETUAL + USDT + underlyingType=COIN`；官方日包必须有
  checksum，404、checksum 错误、CSV 格式异常或窗口快照不完整只阻塞对应观测，不补值；
- 官方来源：`https://data.binance.vision/data/futures/um/daily/bookDepth/`；字段只使用
  `timestamp / percentage / notional`；
- 1m、tick 和信号后的订单簿都不参与因子；最终候选与前瞻收益口径仍是 15m。

## 3. 冻结价格候选

每个 UTC `00:00 / 08:00 / 16:00`，对应 OKX 15m 完成后：

1. 对当月成员计算截至该已完成 K 线的 6h 与 24h 收益；两段必须连续、有限且严格同向；
2. 数据覆盖先于方向过滤计算；至少 80% 当月成员具备完整因子，否则整个时点阻塞；
3. 多头候选按 6h 收益降序取前二；空头候选按 6h 收益升序取前二；并列按 symbol 字典序；
4. 候选形成只读取决策时点及以前的 OKX 数据，不使用订单簿或前瞻收益调整候选集合；
5. 方向沿 6h 冲量；面板最多每个时点四个观测，不做容量、止盈或止损回放。

## 4. 冻结订单簿因子

对每个候选读取决策前 `[T-15m, T)` 的 Binance 1% 深度：

1. 每个 snapshot 必须同时存在 `percentage=-1` 的 bid notional 与 `percentage=1` 的 ask notional；
2. `imbalance=(bid_notional-ask_notional)/(bid_notional+ask_notional)`；
3. 窗口至少 20 个有效 snapshot，最后一个距决策不超过 90 秒，相邻 snapshot 间隔不超过 90 秒；
4. 因子值为窗口 imbalance 中位数，不读取 `T` 及之后 snapshot；
5. 多头 `imbalance>0`、空头 `imbalance<0` 记为 `aligned`；相反符号记为 `opposed`；等于零单独
   记为 neutral，不并入 aligned/opposed；
6. 不扫描 1% 档位、15m 窗口、20 个快照、90 秒新鲜度或零分界阈值。

## 5. 冻结前瞻收益与因子门槛

- 入场基准：决策后的下一根 OKX 15m 开盘；
- 1h outcome：入场后第 4 根 15m 收盘；4h outcome：入场后第 16 根 15m 收盘；
- 方向收益：Long 为 `exit/entry-1`，Short 为 `entry/exit-1`；只用于因子诊断，不等同于扣费策略 PnL；
- 分别报告 overall、Discovery、Validation 的 aligned/opposed，以及 aligned 的 Long/Short：样本数、
  平均 1h/4h 收益、1h/4h 正收益率；
- 因子仅在以下全部满足时允许进入策略设计：
  - 全窗口至少 600 个有效订单簿观测；Discovery 与 Validation 的 aligned/opposed 各至少 100 个；
  - aligned 的 4h 平均方向收益在 Discovery 和 Validation 均 `>=0.20%`，4h 正收益率均 `>=55%`；
  - 两段中 aligned 相对 opposed 的 4h 平均收益增量均 `>=0.15%`；
  - aligned Long 与 aligned Short 全窗口都至少 100 个，且 4h 平均方向收益都为正。

任一不满足即淘汰该因子定义，不在当前窗口改成 2%/3% 深度、5m/30m 窗口、分位阈值或换 outcome。

## 6. 一次性结果与决策

- 冻结源码指纹：`b84bd6a394c0aae95968a7389a0a34bae2c9c7b096f745e6bb533e69a8a28dcd`；
- 15m 数据审计：119 个币、1,013 个币月、2,957,946 根聚合 K 线；写入或更新 115,369 行；
  6 个部分月只保留自身连续 15m 桶，不填补；
- bookDepth 数据审计：115/119 个 OKX 币存在当前 Binance 映射；请求 3,171 个候选日文件，
  3,128 个完整有效、43 个 404、0 个 CSV 无效；123 个候选窗口未通过快照完整性；
- 漏斗：1,095 个决策时点、0 个价格覆盖阻塞；43,021 个 6h/24h 同向观测、3,776 个多空
  各前二候选、0 个 outcome 不完整、3,516 个完整订单簿观测、0 个 neutral；
- aligned overall：1,675 个，1h 平均 `-0.0714%`、4h 平均 `-0.0137%`，4h 正收益率 `46.33%`；
- aligned Discovery：901 个，4h 平均 `-0.1102%`、正收益率 `45.73%`；
- aligned Validation：774 个，4h 平均 `0.0986%`、正收益率 `47.03%`；
- aligned Long：1,405 个，4h 平均 `0.0113%`；aligned Short：270 个，4h 平均 `-0.1439%`；
- opposed overall：1,841 个，1h 平均 `0.0477%`、4h 平均 `0.1927%`，4h 正收益率 `50.68%`；
  Discovery/Validation 的 4h 平均分别为 `0.1464% / 0.2421%`，但正收益率均约 `50.68%`；
- 决策：`factor_gate_passed=false`。静态 1% 深度同向没有边际价值；opposed 组虽有小幅正均值，
  但命中率近随机，约 16bps 往返成本后只剩约 3bps，不能反向包装成可交易策略。本因子定义永久
  淘汰，不在该窗口改档位、窗口、快照数、阈值或 outcome。
