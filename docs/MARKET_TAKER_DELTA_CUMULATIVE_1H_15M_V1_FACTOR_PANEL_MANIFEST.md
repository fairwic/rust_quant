# 15m 多棒累计 Taker Delta 增量因子 V1 评估清单

## 1. 因子身份与唯一问题

- 研究标识：`market-taker-delta-cumulative-1h-factor-panel`
- 规则版本：`okx15m_binance15m_cumulative_taker_delta_1h_incremental_factor_v1`
- 状态：`factor_research_only`，`strategy_candidate=false`，`promotion_eligible=false`

上一版单根 Taker Delta 背离策略在双年度合计后接近零优势，说明“单根主动成交方向直接
开仓”不能成立。本轮不再构造交易规则，只回答一个更窄的问题：连续四根原生 15m 主动
成交量合成的 1h 累计 Delta，在已经知道价格方向、价格幅度和总成交量状态之后，是否仍
能为未来 1h/4h 反转收益提供稳定增量信息。

冻结计算如下：

```text
signed_delta_quote_i = 2 * taker_buy_quote_volume_i - quote_volume_i
flow_1h = sum(signed_delta_quote_i, 4 bars) / sum(quote_volume_i, 4 bars)
price_1h = okx_close(t-15m) / okx_open(t-60m) - 1
relative_volume_1h = current_4bar_quote_volume / mean(previous_20_nonoverlap_4bar_quote_volume)
```

`flow_1h` 只使用 Binance USD-M 官方原生 15m K 线字段，不通过 1m、K 线涨跌或
TradingView 下级周期估算买卖量。本轮不扫描 2 根、8 根、阈值或分位点。

## 2. 数据、币池与严格时序

- 开发窗口：`2025-07-01 00:00:00` 至 `2026-07-01 00:00:00`，UTC；
- 独立旧窗口：`2024-07-01 00:00:00` 至 `2025-07-01 00:00:00`，UTC；
- OKX 价格与 outcome 只读取本地 `quant_core` 中 `confirm='1'` 的原生 15m K 线；
- Binance flow 与总量只读取官方 USD-M 原生 15m 月包，继续校验官方 `.CHECKSUM`；
- 不读取 1m、`market_rank_events`、episode、BOS、FVG、MACD、盘口或资金费；
- 每月使用冻结的 current-live crypto-only OKX USDT 永续 top60，并要求对应 Binance
  合约当前为 `TRADING + PERPETUAL + USDT + COIN`。按用户要求不纳入当前退市币，
  同时明确承认该口径存在幸存者偏差；
- 每个 UTC `00/04/08/12/16/20` 点计算一次。因子只读取决策点前已经完成的 84 根
  Binance 15m：前 80 根形成 20 个非重叠 1h 总量基线，最后 4 根形成当前累计 Delta；
- 未来收益从决策点 OKX 新 15m 棒开盘开始，1h 使用第 4 根收盘，4h 使用第 16 根
  收盘。未来 K 线只进入 outcome，不参与因子、分组、覆盖或中位数计算；
- 当前月份至少 80% 成员具有完整同步因子才保留该决策点；缺口不插值、不用未来补齐。

## 3. 预注册对照与增量比较

每个 observation 仅按零阈值进入四个互斥象限：

| 价格 1h | flow 1h | 解释 | 方向收益 |
|---|---|---|---|
| 下跌 | 正 | Delta 背离，检验反转多 | `+forward_return` |
| 下跌 | 负 | Delta 同向，价格对照 | `+forward_return` |
| 上涨 | 负 | Delta 背离，检验反转空 | `-forward_return` |
| 上涨 | 正 | Delta 同向，价格对照 | `-forward_return` |

为了避免把价格幅度或总量误认成 Delta 增量，每个决策点、每个价格方向内部，使用当时
横截面的 `abs(price_1h)` 中位数和 `relative_volume_1h` 中位数形成 2×2 四个可见
分层。在每个同时含背离组和同向组的分层中，计算两组平均方向收益之差；再对该时点的
有效分层等权平均，形成一个 time-level paired spread。主要统计单位是该配对时点，而
不是把同一市场时刻的数十个币当作完全独立样本。

中位数只来自当前决策时点的可见因子，不使用全年度分位数，因此不会把未来分布带入
历史判断。本轮不按结果挑选单币、阈值或流量强度。

## 4. 固定 outcome 与经济门槛

- 因子面板报告毛 forward return，不模拟止盈、止损、仓位、组合资金曲线或交易级 R；
- 16 bps 仅作为未来双边 5 bps 手续费 + 3 bps 滑点的最低经济幅度参考，不把因子面板
  冒充已经扣费的策略回测；
- 每个年度窗口分别报告四象限 observation 均值/正收益率，以及多头、空头 time-level
  paired spread 的整体、前半年、后半年；
- 单年度通过条件同时要求：
  1. 多头和空头各至少 100 个有效配对时点；
  2. 多头与空头配对 spread 的 1h 均值均 `> 0`；
  3. 多头与空头配对 spread 的 4h 均值均 `>= 0.16%`；
  4. 两个背离象限自身的 4h 平均方向收益均 `>= 0.16%`；
  5. 多头与空头配对 spread 在前、后半年 4h 均值都 `> 0`；
- 只有开发窗和独立旧窗均通过，才允许创建下一版可执行策略假设；任一失败则停止，不在
  已查看结果上修改累计窗口、分组、阈值或 forward horizon。

## 5. 公开证据边界

- [Binance Public Data](https://github.com/binance/binance-public-data) 明确 USD-M K 线包含
  quote asset volume 与 taker buy quote asset volume，并为月包提供 SHA-256 校验；
- [TradingView Cumulative Volume Delta](https://www.tradingview.com/support/solutions/43000725058-cumulative-volume-delta/)
  将 CVD 定义为一段时期内逐棒 Delta 的累计；TradingView 自身使用更低周期估算，而
  本轮使用 Binance 原生 taker 字段，避免重新引入 1m 信号；
- Cont、Kukanov、Stoikov 的
  [The Price Impact of Order Book Events](https://arxiv.org/abs/1011.6402) 说明短周期价格与
  order-flow imbalance 的关系可能比总成交量更稳健。但该论文使用股票限价簿事件，
  不是加密永续主动成交量，因此这里只提供研究动机，不能直接证明本因子有效或反转。

## 6. 架构落位与停止边界

- Market owner：OKX/Binance 已完成 15m 行情与数据完整性；
- Research owner：冻结数据选择、因子政策、配对统计和证据；
- Strategy owner：本轮没有 StrategySignal、入场、出场或参数版本变更；
- 入口：独立只读 `market_taker_delta_factor_panel` CLI；
- 事务与副作用：无数据库写入、无跨进程 contract、无 Paper/Live、无真实交易 mutation；
- 当前仅在 legacy `rust-quant-cli` 研究适配层完成最小切片。因子只有通过双窗口门禁后，
  才考虑迁入目标 Research Domain 或创建独立 Strategy 版本。

## 7. 冻结状态

本节以上规则在读取双窗口结果前冻结。后续只允许追加实现验证、覆盖、结果和最终判定，
不得回写修改本节以迎合结果。

## 8. 实现与验证

新增独立只读入口 `market_taker_delta_factor_panel`。实现严格区分三类时间：决策点前
84 根 Binance 15m 只形成相对总量基线与当前累计 Delta；决策前最后四根 OKX 只形成
价格方向和幅度；决策点开始的 OKX K 线只形成 1h/4h outcome。任何 15m 缺口、非确认
K 线、非有限数值、非法 taker volume 或不足 80% 横截面覆盖均失败关闭。

定向单元测试 5 项通过：四根累计 Delta 数值、原生 taker quote volume、可见窗口缺口
拒绝、决策开盘后的固定 outcome、以及 2×2 可见分层配对；研究二进制构建通过。运行
期间只读本地 `quant_core` 和官方 Binance 合约元数据/已校验缓存，没有写业务表。

## 9. 双窗口冻结结果

### 9.1 开发窗口：2025-07～2026-06

- 2,190 个 4h 决策点中仅 1 个因覆盖不足被阻塞；
- 126,126 个可见因子 observation，126,103 个具有完整 outcome；
- 下跌背景 1,949 个有效配对时点，上涨背景 1,884 个有效配对时点。

| 象限 | observation | 平均 1h 方向收益 | 平均 4h 方向收益 | 4h 正收益率 |
|---|---:|---:|---:|---:|
| 下跌 + 正 Delta，反转多背离 | 15,658 | -0.008382% | +0.006788% | 49.36% |
| 下跌 + 负 Delta，同向对照 | 49,517 | -0.018810% | -0.091586% | 47.89% |
| 上涨 + 负 Delta，反转空背离 | 21,277 | -0.017772% | +0.005784% | 52.02% |
| 上涨 + 正 Delta，同向对照 | 39,651 | -0.008452% | -0.013721% | 51.69% |

| 配对方向 | 时点 | 1h 背离减同向 | 4h 背离减同向 | 前半年 4h | 后半年 4h |
|---|---:|---:|---:|---:|---:|
| 下跌后反转多 | 1,949 | +0.003274% | +0.035890% | +0.005922% | +0.064800% |
| 上涨后反转空 | 1,884 | +0.039969% | -0.054704% | -0.099767% | -0.014527% |

多头 4h 增量只有 `3.589 bps`，远低于 16 bps 经济幅度；空头在控制价格幅度和总量后
反而为负。两个背离象限自身 4h 平均方向收益也都不到 1 bps。开发窗口门禁失败。

### 9.2 独立旧窗口：2024-07～2025-06

- 2,190 个 4h 决策点中 5 个因覆盖不足被阻塞；
- 126,034 个可见因子 observation，全部具有完整 outcome；
- 下跌背景 1,807 个有效配对时点，上涨背景 1,820 个有效配对时点。

| 象限 | observation | 平均 1h 方向收益 | 平均 4h 方向收益 | 4h 正收益率 |
|---|---:|---:|---:|---:|
| 下跌 + 正 Delta，反转多背离 | 13,220 | -0.025080% | -0.042774% | 48.12% |
| 下跌 + 负 Delta，同向对照 | 50,297 | -0.020908% | -0.033359% | 49.25% |
| 上涨 + 负 Delta，反转空背离 | 23,994 | +0.013361% | -0.005283% | 51.33% |
| 上涨 + 正 Delta，同向对照 | 38,523 | -0.002040% | -0.053813% | 50.65% |

| 配对方向 | 时点 | 1h 背离减同向 | 4h 背离减同向 | 前半年 4h | 后半年 4h |
|---|---:|---:|---:|---:|---:|
| 下跌后反转多 | 1,807 | -0.000940% | +0.004369% | +0.033754% | -0.025311% |
| 上涨后反转空 | 1,820 | +0.002902% | +0.055970% | +0.093448% | +0.018574% |

旧窗口多头增量降到 `0.437 bps` 且后半年转负；空头虽然有 `5.597 bps`，仍只有经济
门槛的约三分之一，而且该方向在开发窗口变成 `-5.470 bps`。两个背离象限自身的 4h
平均方向收益均为负。独立旧窗口门禁失败。

## 10. 最终判定

V1 状态为 `rejected_no_economic_increment_and_temporal_direction_flip`，继续保持
`factor_research_only`、`strategy_candidate=false`、`promotion_eligible=false`。

结果说明累计 Delta 不是完全无信息：在部分窗口、部分方向中，背离组相对价格同向组有
几个 bps 的差异。但差异远低于预注册 16 bps 经济幅度，方向在两个年度之间互换，部分
半年度翻负，背离象限自身也没有形成可交易的绝对 forward return。因此它不能作为
15m 动量反转的开仓门禁，也不能据此建立只做多或只做空版本。

按停止规则，本轮不扫描累计 2h/4h、Delta 强度阈值、只做单方向、forward horizon、
MACD、BOS/FVG 或总量分层。没有回测 ID、没有写入 Paper/Live、没有触发交易 mutation。
