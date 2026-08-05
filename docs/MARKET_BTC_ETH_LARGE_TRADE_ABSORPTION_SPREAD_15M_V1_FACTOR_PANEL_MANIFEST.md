# BTC—ETH 大单吸收价差 15m V1 因子面板清单

## 1. 身份与唯一假设

- `factor_key=market_btc_eth_large_trade_absorption_spread`
- `version=15m_v1_20260721`
- `rule_version=aggtrade_tail_pressure_price_residual_rank_6h_v1`
- 状态：`research_only_factor_panel`，`promotion_eligible=false`
- 冻结时间：`2026-07-21`，在计算任何 forward outcome 前完成；
- 基线 Git HEAD：`438b67af52e64ad4babdc310ddef36c5dddbcc09`

15m K 线的 taker buy/sell 只保留一阶成交额，无法区分大量小单与少数真正大单。本轮首次
使用 Binance 官方 USD-M `aggTrades`：若价格表现显著强于大额主动卖单尾部压力，说明
被动买方可能正在吸收；相反则可能是被动卖方吸收主动买盘。该残差可能预测 BTC/ETH 的
下一段相对强弱。

## 2. 冻结数据、聚合和因果规则

- 因子窗口 `[2024-07-01 00:00:00 UTC, 2025-07-01 00:00:00 UTC)`；
- 额外请求 2024-06 月支持首个决策的前 30 天基线；
- 只使用当前仍 live 的 `BTCUSDT/ETHUSDT` USD-M 永续，不涉及退市币；
- 使用 Binance 官方 monthly `aggTrades`、regular 15m 与各自 SHA-256；月包必须全部
  存在、时间不倒退、aggregate trade id 不重复；
- `is_buyer_maker=false` 记为主动买，`true` 记为主动卖；每条成交美元额为
  `price*quantity`，必须严格正且有限；
- 不生成或读取 1m；逐笔成交直接按真实 `transact_time` 聚合到已完成 15m，再汇总为
  UTC 对齐 6h 信号窗口；所有执行与 outcome 仍为 15m；
- 当前 6h 的大单尾部压力固定为
  `sum(sign*notional²)/sum(notional²)`，主动买为正、主动卖为负；平方权重只改变单笔
  大小贡献，不设置“大单金额”阈值；
- 当前价格表现为同一 `[T-6h,T)` 的 BTC/ETH 对数收益；
- 对每个资产分别用此前 120 个不重叠 6h 窗口，即 `[T-30d-6h,T-6h)`，计算价格收益
  与尾部压力各自的 point-in-time 均值和样本标准差；当前窗口不进入自身基线；
- `absorption_score=price_return_z-tail_pressure_z`：正值表示价格强于大额主动流压力，
  负值表示价格弱于主动流；
- 每个 UTC `00:00/06:00/12:00/18:00` 决策 `T`，等名义做多 score 较高资产、做空
  score 较低资产；不设置绝对阈值；
- `T` 15m 开盘入场，`T+6h` 开盘为唯一主 outcome，`T+24h` 仅作持续性诊断；内部
  K 线缺口阻塞；收益为 `long_return-short_return`；
- 不叠加普通一阶 taker flow、蜡烛、ATR、EMA、MACD、BOS/FVG/CHoCH、funding、OI、
  liquidation、positioning、BVOL、订单簿或币种筛选。

Discovery 固定 2024-07～12，Validation 固定 2025-01～06。不得根据结果反向、调整
平方权重、30天、6小时、方向、频率或持有期。

## 3. 覆盖、晋级和停止门槛

覆盖门禁先于 outcome：26 个 BTC/ETH `aggTrades` 月包与所需 15m 月包全部有效；形成
至少 1,400 个可计算 score 决策。失败则不计算收益。

覆盖通过后，V1 同时满足：

- 完整交易不少于 1,400，Discovery/Validation 各不少于 680；
- 12 个完整月每月 `100～124` 组，至少 8/12 月 6h 毛价差为正；
- 总体、Discovery、Validation 的 6h 毛价差均大于 32bps，标准成本后均至少
  `+20bps`，6h 命中率均至少 55%；
- score spread 五分位的 6h 毛收益至少四次非下降；
- 双倍成本 64bps 后总体仍为正；24h 不得替代失败的 6h 主门槛。

任一核心门槛失败则 V1 永久淘汰，不扫描方向、单笔幂次、窗口、阈值、币种权重或持有期。
全部通过后才建立包含实际 funding、固定初始止损/R、统一资金、容量和保护单的策略版本，
并进一步满足净 EV `>=0.6R`、PF `>=2.2`、最大回撤 `<=10%`、Recovery `>=4`、Sharpe
`>=1.5`。本面板不授权 Paper、Live、部署或交易 mutation。

## 4. 一次性结果与决策

等待严格解析、覆盖审计和唯一一次面板运行；不得根据结果修改第 1～3 节。
