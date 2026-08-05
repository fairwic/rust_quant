# Market Momentum Opposite Move v31 阳线锤头灰区评估清单

## 1. 独立假设与版本边界

v31 是从 v28 分叉的独立研究规则，不覆盖 v28/v29，也不延长 v30 的确认窗口：

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_bullish_hammer_buffer_v31`
- 运行范围：仅本地 ResearchBar，`promotion_eligible=false`

唯一新假设是：长下引线本身可以替代“大实体阳线”，但必须已经收阳并在当前完成 K 线
收盘重新站上 EMA20 与 SMA20。满足这些条件后，信号在当前 K 线完成时成立，以该时点
生产可见价格入场；收阴、十字或仍在均线下方的长下引线全部拒绝，不进入延迟等待。

A 级强反转完整保留 v28：前 192 根净跌至少 10% 或前 96 根下降回归
`R² >= 0.70`，当前阳线实体涨幅至少 3%、实体/收盘位置/前高突破与均线收复不变。
B 级锤头使用 v30 已冻结的历史灰区：前 192 根净跌至少 8%，或前 96 根下降回归
`R² >= 0.60`。192/96 窗口均不缩短。

## 2. B 级锤头固定条件

- 当前 K 线收阳，`close > open`；
- 下影线占整根振幅至少 45%；
- 收盘位于整根振幅上方至少 65%；
- 最低价严格扫过上一根最低价；
- 整根振幅至少为信号时点 ATR14 的 1.2 倍；
- 收盘高于 EMA20 与 SMA20；
- 成交量至少为前 20 根均量的 1.5 倍；
- 仍通过历史极值量能比例 1.0 与 BTC 前 96 根绝对净幅不超过 2% 门禁。

B 级不要求实体涨幅 3%、实体占振幅 45% 或收盘突破上一根最高价；这些正是本轮要检验
的替代语义。除形态差异外，固定 3% 初始止损、Volume-ATR 目标、两根无浮盈早退、
手续费与滑点均与 v28 一致。

## 3. 开发池与停止规则

Top60 全部属于已见开发池，结果不能晋级。查看结果前冻结：

- 同时报告原始交易与按同方向、相邻触发不超过 30 分钟归并的有效事件下界；
- B 级至少 20 个有效事件，否则仅判样本不足；
- B 级净 EV `<=0R`、PF `<=1`、后半段 EV `<=0R`、半数及以上成交品种亏损，
  或移除前三个盈利品种后转负，任一成立即淘汰；
- 合并后要求净 EV `>=0.6R`、PF `>=2.2`、后半段 EV `>0R`，否则不冻结未见池；
- 不扫描影线 40%/50%、收盘位置 60%/70%、灰区 7%/9% 或 `R² 0.55/0.65`；
- 开发池通过也仅能冻结新未见 universe/time window。正式晋级仍须补 point-in-time
  universe、资金费、费用压力、参数邻域、统一资金/容量/相关簇、日权益 Sharpe、逐时
  盯市回撤、Recovery 与 BTC/ETH/其他 Tier 覆盖。

## 4. 预注册命令

不带 `--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v31_bullish_hammer_buffer_20260720 \
  --trade-direction long \
  --stop-loss-pct 0.03 \
  --target-rs 1 \
  --entry-period 20 \
  --entry-max-distance-pct 14 \
  --entry-min-volume-ratio 1.5 \
  --entry-opposite-move-lookback-candles 192 \
  --entry-min-opposite-net-move-pct 10 \
  --entry-min-opposite-duration-candles 96 \
  --entry-min-exhaustion-volume-dominance-ratio 1 \
  --entry-btc-96-max-abs-net-move-pct 2 \
  --volume-atr-take-profit \
  --volume-atr-target-scale 4 \
  --volume-atr-min-target-r 1.8 \
  --volume-atr-max-target-r 3 \
  --backtest-fee-bps-per-side 5 \
  --backtest-slippage-bps-per-side 3 \
  --entry-long-bullish-hammer-reversal \
  --trend-timeframe off \
  --min-delta-rank 1 \
  --max-price-change-pct 8 \
  --event-start-ms 1751328000000 \
  --entry-trigger-allowlist all \
  --early-exit-no-profit-candles 2 \
  --ignore-entry-signal-updates-while-open \
  --equity-report \
  --equity-split-report \
  --equity-quartile-report \
  --equity-trigger-report \
  --equity-concentration-report \
  --equity-feature-report \
  --equity-symbol-window-report \
  --equity-trade-report \
  --min-trades 1
```

## 5. 待填写结果

执行前保持为空；不得根据结果修改第 1～4 节。

## 6. 开发池结果（2026-07-20）

使用第 4 节预注册命令完成 Top60 开发池回放，未写入回测明细：

- 原始 15m 候选 30,657 个；信号与执行门禁通过 52 个；BTC 门禁后 38 笔成交。
- A 级强反转 27 笔；B 级阳线锤头缓冲 11 笔，涉及 9 个币种。
- 合并口径：净收益 12.7448R、净 EV 0.3354R、PF 2.1790、胜率 21.05%、
  交易级 Sharpe 1.4844、逐币隔离最大回撤 3.8309R。
- B 级单独口径：净收益 1.3762R、净 EV 0.1251R、PF 1.8776、胜率 9.09%、
  交易级 Sharpe 0.4427；11 笔中仅 GRASS 一笔盈利。
- B 级按同方向、30 分钟聚类后最多 10 个有效事件，低于预注册的 20 个下限。
- 时间切分：前半段 19 笔 EV 1.0237R、PF 5.7407；后半段 19 笔 EV
  -0.3530R、PF 0，后半段没有胜单。
- 季度切分：Q1 与 Q2 为正，Q3 与 Q4 没有胜单；移除头部 5 笔盈利后组合净收益
  降为 -6.1504R，收益集中度不可接受。

## 7. 决策

**v31 淘汰，不冻结未见池，不进入 paper/read-only shadow。**

B 级同时触发“有效事件不足”“后半段 EV 不大于 0”“多数币种亏损”等停止规则；
合并结果也未达到 EV 0.6R、PF 2.2 与后半段为正的门槛。当前收阳并不是主要矛盾：
放宽为阳线锤头确实增加了交易，但新增样本仍主要是亏损。后续不得继续扫描影线、灰区、
R² 或收盘位置阈值，应切换到与蜡烛形态独立的结构/退出假设。
