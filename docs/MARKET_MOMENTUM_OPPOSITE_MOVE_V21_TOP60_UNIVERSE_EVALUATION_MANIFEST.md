# Market Momentum Opposite Move v21 Top60 Universe 扩展评估清单

## 1. 策略身份与本轮唯一变化

v21 仍是同一个 `market_momentum_opposite_move_reversal` 策略，不创建研究策略身份：

- 产品 slug：`market-momentum-opposite-move-reversal`
- 数据库策略类型：`market_velocity_kline_15m`
- 规则版本：`kline15m_market_momentum_opposite_reversal_top60_btc_flat_direct_long_v21`

本轮不修改信号和退出参数，只扩大排名 universe。v20 的 BTC 震荡 long-only、当前
看涨 K 线、滚动成交额自身增长、`delta_rank >= 1`、历史整体下跌/长时间分支、历史
极值量能、反转/均线确认、3% 止损、Volume-ATR 止盈与显式成本全部保持不变。

## 2. 查看历史结果前冻结的 Universe 规则

以 2026-07-19 执行时的 OKX 公共 instruments 与 tickers 快照为事实源：

- `instType=SWAP`、`settleCcy=USDT`、`ctType=linear`、`state=live`；
- 上市时间不晚于 2024-12-31 23:59:59 UTC，确保 2025-07-01 回放开始前至少约半年
  公开交易历史；
- 当前近似 24h quote 成交额按 `volCcy24h * last` 从高到低排序，冻结前 60 名；
- 不根据任何成员历史回测结果纳入或剔除；成员、快照时间、成交额和上市时间必须在
  回填前写入本清单；
- 历史 15m 数据统一回填 400 天，缺口、重复和确认状态审计不合格的成员不静默删除，
  只允许整体失败关闭或在清单中声明 universe 版本失效。

这是当前存活名单回看历史，仍有幸存者偏差和退市币缺失，固定
`promotion_eligible=false`。Top60 只是比 33 币更接近生产横截面的研究近似，不冒充
完整 Top150 或历史 membership。

## 3. 未见执行与停止规则

前 33 个已有历史表的成员全部视为已见，只参与 Top60 排名；v21 只执行 Top60 中此前
未进入任何本策略回放的新成员。查看结果前冻结：

- 新成员少于 20 个有效事件仅判样本不足；
- 净 EV `<=0R`、PF `<=1`、后半段 EV `<=0R` 或半数及以上成交成员亏损则淘汰；
- 若通过最低门槛，再报告原始/有效事件、逐币、时间、BTC 状态、集中度、费用压力和
  `delta_rank 1/2` 邻域；不得依据结果删除亏损币种。

正式晋级仍须满足净 EV `>=0.6R`、PF `>=2.2`、统一组合最大回撤 `<=15%`、
Recovery `>=4`、Sharpe `>=1.5`，以及历史 universe、统一资金/容量/相关簇、资金费
和实盘状态机等全部要求。未完成前不保存正式回测、不进入 Paper/Live。

## 4. 冻结成员

快照时间：2026-07-19 13:00:57 UTC。近似 24h quote 成交额单位为 USDT，取整数展示。
`新池=是` 的 27 个成员此前没有进入本策略历史回放；其余 33 个只参与排名。

| 排名 | 成员 | 上市日期 UTC | 近似 24h quote 成交额 | 新池 |
|---:|---|---:|---:|:---:|
| 1 | ETH-USDT-SWAP | 2019-11-12 | 3,134,668,189 | 否 |
| 2 | BTC-USDT-SWAP | 2019-11-12 | 2,749,902,825 | 否 |
| 3 | SOL-USDT-SWAP | 2021-01-22 | 306,966,705 | 否 |
| 4 | XRP-USDT-SWAP | 2019-11-12 | 75,963,965 | 否 |
| 5 | DOGE-USDT-SWAP | 2020-04-21 | 73,154,852 | 否 |
| 6 | PEPE-USDT-SWAP | 2023-05-03 | 65,007,915 | 否 |
| 7 | LTC-USDT-SWAP | 2019-11-12 | 48,812,998 | 否 |
| 8 | ONDO-USDT-SWAP | 2024-08-06 | 46,992,851 | 是 |
| 9 | WLD-USDT-SWAP | 2023-07-24 | 46,195,205 | 否 |
| 10 | SUI-USDT-SWAP | 2023-05-05 | 29,373,552 | 否 |
| 11 | ADA-USDT-SWAP | 2020-02-28 | 21,865,494 | 否 |
| 12 | BNB-USDT-SWAP | 2022-12-23 | 19,593,592 | 否 |
| 13 | LRC-USDT-SWAP | 2020-09-09 | 19,309,947 | 否 |
| 14 | FIL-USDT-SWAP | 2019-08-10 | 18,507,002 | 否 |
| 15 | BONK-USDT-SWAP | 2024-01-08 | 17,207,728 | 是 |
| 16 | AAVE-USDT-SWAP | 2020-12-10 | 15,844,817 | 否 |
| 17 | UNI-USDT-SWAP | 2020-09-17 | 13,187,596 | 否 |
| 18 | XLM-USDT-SWAP | 2020-04-21 | 12,968,709 | 否 |
| 19 | NEAR-USDT-SWAP | 2020-12-21 | 12,752,068 | 否 |
| 20 | LINK-USDT-SWAP | 2020-02-28 | 11,641,866 | 否 |
| 21 | PENGU-USDT-SWAP | 2024-12-17 | 10,787,734 | 是 |
| 22 | BCH-USDT-SWAP | 2019-11-12 | 10,329,600 | 否 |
| 23 | TAO-USDT-SWAP | 2024-09-20 | 10,221,769 | 是 |
| 24 | ETHFI-USDT-SWAP | 2024-03-18 | 9,861,387 | 是 |
| 25 | AVAX-USDT-SWAP | 2020-09-23 | 8,848,477 | 否 |
| 26 | ORDI-USDT-SWAP | 2023-05-21 | 8,257,207 | 否 |
| 27 | ARB-USDT-SWAP | 2023-03-23 | 7,667,900 | 否 |
| 28 | DOT-USDT-SWAP | 2020-08-25 | 6,793,273 | 否 |
| 29 | TRX-USDT-SWAP | 2019-11-12 | 5,975,390 | 否 |
| 30 | STRK-USDT-SWAP | 2024-02-20 | 5,659,983 | 是 |
| 31 | LDO-USDT-SWAP | 2023-02-02 | 5,440,990 | 否 |
| 32 | BLUR-USDT-SWAP | 2023-02-15 | 4,436,525 | 否 |
| 33 | OP-USDT-SWAP | 2022-06-01 | 4,339,175 | 否 |
| 34 | INJ-USDT-SWAP | 2023-11-30 | 4,027,948 | 否 |
| 35 | ETC-USDT-SWAP | 2019-11-12 | 3,822,659 | 否 |
| 36 | JTO-USDT-SWAP | 2024-01-08 | 3,721,542 | 是 |
| 37 | VIRTUAL-USDT-SWAP | 2024-12-11 | 3,502,107 | 是 |
| 38 | SHIB-USDT-SWAP | 2021-05-09 | 3,229,719 | 否 |
| 39 | SUSHI-USDT-SWAP | 2020-09-02 | 3,165,025 | 是 |
| 40 | TIA-USDT-SWAP | 2023-10-31 | 3,050,946 | 否 |
| 41 | EIGEN-USDT-SWAP | 2024-10-01 | 3,040,744 | 是 |
| 42 | APT-USDT-SWAP | 2022-10-19 | 2,979,342 | 否 |
| 43 | HBAR-USDT-SWAP | 2023-08-16 | 2,932,550 | 否 |
| 44 | NEO-USDT-SWAP | 2020-02-28 | 2,884,112 | 是 |
| 45 | ENS-USDT-SWAP | 2021-11-11 | 2,873,024 | 是 |
| 46 | GALA-USDT-SWAP | 2021-09-24 | 2,786,468 | 是 |
| 47 | CHZ-USDT-SWAP | 2021-03-12 | 2,728,041 | 是 |
| 48 | QTUM-USDT-SWAP | 2020-04-21 | 2,565,522 | 是 |
| 49 | FARTCOIN-USDT-SWAP | 2024-12-20 | 2,392,496 | 是 |
| 50 | ZRO-USDT-SWAP | 2024-06-20 | 2,380,412 | 是 |
| 51 | ICP-USDT-SWAP | 2021-05-14 | 2,343,536 | 是 |
| 52 | TRB-USDT-SWAP | 2020-09-08 | 2,335,847 | 是 |
| 53 | ATH-USDT-SWAP | 2024-06-13 | 2,143,443 | 是 |
| 54 | CRV-USDT-SWAP | 2020-09-04 | 2,124,371 | 是 |
| 55 | WIF-USDT-SWAP | 2024-04-15 | 2,102,965 | 是 |
| 56 | EGLD-USDT-SWAP | 2020-12-22 | 2,082,641 | 是 |
| 57 | HMSTR-USDT-SWAP | 2024-09-26 | 2,032,147 | 是 |
| 58 | GRASS-USDT-SWAP | 2024-10-28 | 2,013,543 | 是 |
| 59 | CFX-USDT-SWAP | 2021-03-12 | 2,004,098 | 是 |
| 60 | VANA-USDT-SWAP | 2024-12-17 | 1,937,573 | 是 |

## 5. 预注册执行命令

`sample-limit=60` 只在确认数据库恰好包含上述 60 张目标 15m 表后执行。旧 33 个成员通过
`symbol-blocklist` 只参与排名，不产生入场；命令不带 `--save-backtest-detail`：

```bash
target/debug/market_velocity_event_backtest \
  --event-source kline_15m \
  --kline-volume-rank-velocity \
  --kline-volume-rank-require-turnover-growth \
  --sample-limit 60 \
  --sample-seed top60_v21_frozen_20260719 \
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
  --entry-require-opposite-reversal-confirmation \
  --entry-require-reversal-average-reclaim \
  --entry-defer-max-wait-candles 3 \
  --trend-timeframe off \
  --min-delta-rank 1 \
  --max-price-change-pct 8 \
  --event-start-ms 1751328000000 \
  --entry-trigger-allowlist all \
  --ignore-entry-signal-updates-while-open \
  --symbol-blocklist AAVE-USDT-SWAP,ARB-USDT-SWAP,AVAX-USDT-SWAP,BCH-USDT-SWAP,BNB-USDT-SWAP,BTC-USDT-SWAP,DOGE-USDT-SWAP,DOT-USDT-SWAP,ETH-USDT-SWAP,FIL-USDT-SWAP,LDO-USDT-SWAP,LINK-USDT-SWAP,LTC-USDT-SWAP,ORDI-USDT-SWAP,PEPE-USDT-SWAP,SOL-USDT-SWAP,SUI-USDT-SWAP,TRX-USDT-SWAP,UNI-USDT-SWAP,WLD-USDT-SWAP,XLM-USDT-SWAP,XRP-USDT-SWAP,ADA-USDT-SWAP,LRC-USDT-SWAP,NEAR-USDT-SWAP,BLUR-USDT-SWAP,INJ-USDT-SWAP,OP-USDT-SWAP,ETC-USDT-SWAP,APT-USDT-SWAP,SHIB-USDT-SWAP,TIA-USDT-SWAP,HBAR-USDT-SWAP \
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

## 6. 数据审计

- 新增 27 个成员首次回填均成功；随后修复旧 BCH 的 100 根内部缺口，并把
  BCH/LTC/SOL 从约 390 天统一补到 400 天。
- 最终 60 张表共 2,304,006 行，每表至少 38,400 根；重复时间戳 0、内部缺口 0，
  任一表最多只有最新 1 根未确认 K 线。
- 60 币共同窗口为 2025-06-14 14:30 至 2026-07-19 09:00 UTC；预注册回放从
  2025-07-01 00:00 UTC 开始，具备足够的 96/192 根预热历史。

## 7. 执行结果

- 60 币排名产生 30,657 个原始事件，149 个通过完整信号层；旧 33 币仅参与排名，
  新 27 币形成 66 个过滤后候选、61 笔实际成交，覆盖 25 个品种。
- 61 笔均有固定初始 R；扣除双边 5 bps 手续费和 3 bps 滑点后，累计
  `-4.6895R`、EV `-0.0769R`、PF `0.9010`、trade Sharpe `-0.362`、胜率
  `26.23%`，单币隔离最大回撤 `12.04%`。
- 前半段 EV `+0.1573R`、PF `1.2167`；后半段 EV `-0.2891R`、PF `0.6484`。
  Q1/Q3/Q4 均为负，只有 Q2 为正；Q2 的主要盈利集中在 2025-10-12 同步市场反弹，
  不能视为多个独立有效事件。
- 25 个成交品种中 15 个亏损。移除最佳 BONK 后剩余收益从 `-15.38U` 降到
  `-28.01U`；移除前三个盈利品种后为 `-45.68U`，没有跨品种稳定性。
- 事后特征诊断显示当前阳线涨幅低于 5% 的 58 笔累计 `-9.5255R`；5% 至 10% 只有
  3 笔、累计 `+4.8360R`，样本太小，不能把 5% 直接固化为策略阈值。

v21 同时触发净 EV、PF、后半段 EV 和亏损品种比例四条预注册淘汰线，不保存
`back_test_log` / `back_test_detail`，不进入 Paper/Live。Top60 扩展解决了样本不足，
但证明“任意收阳即反转确认”过弱；下一轮只允许研究当前反转阳线的最小涨幅，不按币种
或单一盈利时间簇筛选。
