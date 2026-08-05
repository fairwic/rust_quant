# BTC—ETH 强平耗竭价差 15m V1 因子面板清单

## 1. 身份与唯一假设

- `factor_key=market_btc_eth_liquidation_exhaustion_spread`
- `version=15m_v1_20260721`
- `rule_version=coinm_liquidation_6h_vs_prior30d_zscore_rank_6h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在计算任何 forward outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

此前 OI/taker 只能间接推断去杠杆。本轮首次使用 Binance 官方 COIN-M
`liquidationSnapshot` 的真实强平事件：若某资产过去 6 小时的强制卖出相对自身前 30 天
显著高于另一资产，卖方强平耗竭可能使其下一段相对 BTC/ETH 另一腿反弹。

## 2. 冻结数据、去重和时序规则

- 因子窗口 `[2024-07-01 00:00:00 UTC, 2025-07-01 00:00:00 UTC)`；
- 只使用当前仍 live 的 `BTCUSD_PERP/ETHUSD_PERP` 强平事实和
  `BTCUSDT/ETHUSDT` USD-M 15m 执行价格，不涉及退市币；
- 强平请求额外覆盖 2024-05-31～2025-06-30，以支持首个决策的前 30 天基线；
- 官方 ZIP 与 SHA-256 必须有效；缺失文件不能当成零强平；
- 同一文件内完全相同的重复 snapshot 只记一次；同一订单语义键若出现多条更新，只保留
  最大 `accumulated_fill_quantity`，避免累积成交重复相加；
- 只接受 `FILLED/PARTIALLY_FILLED` 且累计成交严格正的事件；`SELL` 记为多头强平卖出，
  `BUY` 记为空头强平买回；
- 按官方合约面值换算美元：BTC 每张 `100 USD`，ETH 每张 `10 USD`；
- 所有事件按其真实 `time` 落入已完成 15m 桶，不使用 1m；
- 每个 UTC `00:00/06:00/12:00/18:00` 决策 `T`，当前值为 `[T-6h,T)` 的
  `SELL_USD-BUY_USD`；
- 对每个资产分别用紧邻此前 120 个不重叠 6h 窗口，即 `[T-30d-6h,T-6h)`，计算
  point-in-time 均值和样本标准差；标准差为零或窗口经过无效日则阻塞；
- score 为当前 6h 净强平卖压相对前 30 天的 z-score；等名义做多 score 较高资产、做空
  score 较低资产，symbol 打破并列；不设置绝对阈值；
- `T` 15m 开盘入场，`T+6h` 开盘为唯一主 outcome，`T+24h` 仅作持续性诊断；内部
  15m 缺口阻塞；收益为 `long_return-short_return`；
- 不使用价格跌幅、蜡烛、ATR、EMA、MACD、BOS/FVG/CHoCH、funding、OI、taker、
  positioning、BVOL 或订单簿门禁。

Discovery 固定 2024-07～12，Validation 固定 2025-01～06。不得根据结果反向、调整
30 天、6 小时、决策频率、合约面值或持有期。

## 3. 覆盖、晋级和停止门槛

覆盖门禁先于 outcome：BTC/ETH 15m 月包完整；强平日包有效率各至少 95%；形成至少
1,300 个可计算 score 决策点。失败则不计算收益。

覆盖通过后，V1 同时满足：

- 完整交易不少于 1,300，Discovery/Validation 各不少于 600；
- 12 个完整月每月 `100～124` 组，至少 8/12 月 6h 毛价差为正；
- 总体、Discovery、Validation 的 6h 毛价差均大于 32bps，扣除标准成本后均至少
  `+20bps`，6h 命中率均至少 55%；
- score spread 按五分位分桶后，6h 毛收益随 score spread 至少四次非下降；
- 双倍成本 64bps 后总体仍为正；24h 不得替代失败的 6h 主门槛。

任一核心门槛失败则 V1 永久淘汰，不扫描方向、窗口、z-score 阈值、事件金额、BTC/ETH
权重或持有期。全部通过后才允许建立带实际 funding、固定初始止损/R、统一资金与保护单的
策略版本，并进一步满足净 EV `>=0.6R`、PF `>=2.2`、最大回撤 `<=10%`、Recovery
`>=4`、Sharpe `>=1.5`。本面板不授权 Paper、Live、部署或交易 mutation。

## 4. 一次性结果与决策

严格解析、完全重复 snapshot 去重和测试通过后执行覆盖审计。USD-M regular 15m 月包
`28/28` 有效，共 `81,792` 根；COIN-M 强平日包请求 `792`，仅 `271` 个存在且有效，
官方明确缺失 `521`，无格式无效文件。BTC/ETH 分别只有 `134/137` 个有效日，远低于各
`396` 个请求日的 95% 门槛。原始 `43,528` 行经语义去重后为 `21,764` 个订单，确认官方
文件内存在成对完全重复 snapshot。

`1,460` 个 6h 决策点中，因 30 天窗口经过缺失日而阻塞 `1,084` 个，只能计算 `376`
个 score，远低于预注册 `1,300`。`coverage_gate_passed=false`，程序没有读取任何
`T+6h/T+24h` outcome，收益字段全部为空。

结论：V1 按预注册规则停止。公共档案的 404 无法证明是零强平还是缺档，因此不允许把
`521` 个缺失日擅自补零，也不缩短 30 天窗口或调高决策频率。本面板未进入 Paper/Live、
部署或交易 mutation。
