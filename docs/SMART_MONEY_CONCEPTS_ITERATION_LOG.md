# Smart Money Concepts v1 research 迭代记录

## 指标拆解

TradingView LuxAlgo Smart Money Concepts 指标的可交易核心不是单一买卖点，而是一组结构证据：

- `CHoCH / BOS`：确认后的前高、前低被收盘价突破，表示结构切换或延续。
- `Order Block`：突破前的结构区间，用于约束回踩和结构止损。
- `Premium / Discount`、均衡区、FVG 等属于位置过滤，不能用未来 K 线确认后再反推入场。

本项目 v1 research 只落地可用 OHLCV 严格回放的最小子集：确认 pivot、结构突破、order block 回踩、趋势一致性、ATR 波动率过滤、结构止损和 R 倍止盈。所有判断只使用信号时点已经完成的 K 线。

## 实现范围

- 策略 key：`smart_money_concepts_v1_research`
- 策略类型：`SmartMoneyConceptsV1Research`
- 当前定位：research/backtest only，不进入 live mutation。
- 风控边界：没有结构止损时返回 `Flat`；不允许用 UI 展示或 smoke 结果替代回测证据。

## 回测样本

命令口径：

```bash
QUANT_CORE_DATABASE_URL=... cargo run -p rust-quant-cli --bin btc_eth_strategy_family_okx_backtest -- --scan-smc --limit 3000 --risk-percent 2 --trade-fee-rate 0.0005
```

样本覆盖：

- BTC / ETH / SOL
- 5m / 15m
- 每个 case 最多 3000 根 K 线

## 迭代结果

| 轮次 | 关键过滤 | 最佳胜率 | PnL | 最大回撤 | 交易频率 | 结论 |
|---|---:|---:|---:|---:|---:|---|
| 原始 SMC | pivot + BOS/CHoCH + OB | 30.34% | -59.2384 | 19.34% | 18.57/day | 高频但亏损，不可用 |
| 顺势 + 回踩 | trend 20/96 + require retest | 33.68% | -5.8034 | 5.08% | 3.04/day | 回撤改善，胜率仍差 |
| long-only + 低 R | 禁空、强趋势、低目标 | 55.56% | 4.1201 | 1.34% | 1.15/day | 盈利但未达 60% |
| ATR 上限 | max ATR/price 0.60% | 57.14% | 5.5238 | 0.74% | 1.12/day | 仍未达标 |
| 强趋势局部 | min trend 1.00%、ATR 0.00-0.50% | 63.64% | 2.6032 | 0.79% | 0.35/day | 达到胜率/回撤/PnL，但频率太低 |
| 高频网格复查 | pivot 3/5、cooldown 0/2、低 R、可选回踩 | 60.00% | 2.2319 | 0.74% | 0.48/day | 达标候选仍低频 |
| 延迟 OB 回踩 | 突破后 pending OB，后续 K 线触碰再入场 | 57.14% | 0.8106 | 0.10% | 0.22/day | 更贴近 SMC，但未达胜率目标 |
| Liquidity sweep | 前高/前低 wick sweep 后收回 | 61.54% | 2.7536 | 0.79% | 0.42/day | `sweep=true` 未进入达标候选，最高频仍低 |
| FVG / fade | 同周期 FVG 与反向 trap 候选 | 26.66% | -56.9804 | 14.76% | 19.33/day | 高频但跨样本严重亏损 |
| Displacement + Premium/Discount | 实体位移与区间半区过滤 | 71.43% | 0.6991 | 0.10% | 0.22/day | 质量变好但只剩极低频局部样本 |
| 延迟 FVG/OB 回踩 | `retest_wait=4`，FVG/OB 后等待触碰 | 63.64% | 2.9140 | 0.10% | 1.06/day | ETH 5m 单样本改善，跨样本不达标 |
| 频率门槛复核 | 成功标准加入 `trades_per_day>=1` | - | - | - | - | 跨 BTC/ETH/SOL 的 5m/15m 无达标候选 |
| 6000 根长窗口复核 | 同一收缩网格，limit 6000 | 66.67% | 0.0846 | 0.53% | 0.14/day | 高频候选仍亏损，低频质量候选频率进一步不足 |

当前最佳低频 research 候选（不满足频率目标）：

- `pivot=5`
- `trend=20/96`
- `allow_short=false`
- `require_trend_alignment=true`
- `min_trend_strength_pct=1.00`
- `atr_pct=0.00-0.50`
- `cooldown=2`
- `require_retest=true`
- `max_entry_extension_atr=0.80`
- `stop_atr_buffer=0.75`
- `target_r=0.25/0.50/0.75`
- 聚合结果：`entries=11 wins=7 losses=4 win_rate=63.64% pnl=2.6032 max_dd=0.79% trades_per_day=0.35`

最新频率门槛复核把达标条件收紧为：

- `win_rate>=60`
- `max_dd<15`
- `pnl>0`
- `trades_per_day>=1`

在该条件下，完整扫描输出为：

```text
no_smc_candidates source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,trades_per_day>=1
```

6000 根长窗口复核命令：

```bash
QUANT_CORE_DATABASE_URL=... cargo run -p rust-quant-cli --bin btc_eth_strategy_family_okx_backtest -- --scan-smc --limit 6000 --risk-percent 2 --trade-fee-rate 0.0005
```

关键结果：

- 最佳质量候选：`entries=9 wins=6 losses=3 win_rate=66.67% pnl=0.0846 max_dd=0.53% trades_per_day=0.14`
- 原生结构最高频候选：`entries=501 win_rate=30.74% pnl=-44.9233 max_dd=16.81% trades_per_day=8.02`
- sweep 最高频候选：`entries=1028 win_rate=28.21% pnl=-92.2669 max_dd=23.88% trades_per_day=16.45`
- FVG 最高频候选：`entries=766 win_rate=26.76% pnl=-59.4178 max_dd=15.53% trades_per_day=12.26`
- fade FVG 最高频候选：`entries=1255 win_rate=25.18% pnl=-119.0345 max_dd=26.01% trades_per_day=20.08`
- 最终仍输出：`no_smc_candidates source=quant_core_sharded constraints=win_rate>=60,max_dd<15,pnl>0,trades_per_day>=1`

## 现有短周期基底复核

为判断 SMC 是否适合作为过滤/确认层，复核了当前 CLI 中已有的短周期研究扫描，命令统一使用 `--limit 3000 --risk-percent 2 --trade-fee-rate 0.0005`，均为只读回测。

| 基底策略 | 最佳/关键结果 | 结论 |
|---|---:|---|
| `micro_scalper_1m` | 最高频 raw top 为 `entries=9 win_rate=0.00% pnl=-1.5146 trades_per_day=4.32` | 1m 频率不足且质量极差，不适合作为 SMC 过滤基底 |
| `btc_eth_liquidity_scalper` 窄网格 | 裸 K 线扫描 0 交易，主要阻断 `MISSING_MARKET_SNAPSHOT` | 不能用裸 K 线评估 |
| `btc_eth_liquidity_scalper` + market context | 仍 0 交易，主要阻断 `MICROSTRUCTURE_CONFIRMATION_MISSING` 与部分 `MISSING_MARKET_SNAPSHOT` | 当前样本无有效信号，不适合作为基底 |
| `eth_volume_reversal_5m` | raw top 为 `entries=1 win_rate=100.00% pnl=21.0986 trades_per_day=0.10` | 质量高但极低频 |
| `btc_volume_reversal_dual_5m` 频率扫描 | raw top 为 `entries=1 win_rate=100.00% pnl=25.3743 trades_per_day=0.10` | 质量高但极低频 |
| `breakdown` | 0 交易 | 当前样本没有可迭代信号 |
| `exhaustion` | 无达标候选 | 当前样本没有可直接接 SMC 的候选 |

这轮基底复核没有发现“高频但略低质、适合被 SMC 过滤提升”的候选。现阶段把 SMC 强行叠到已有短周期策略上，缺少可验证收益入口。

## 当前判断

SMC v1 research 已经能做出低回撤、正收益、胜率超过 60% 的候选，但它靠强趋势和低波动过滤换来的是极低交易频率，不满足短周期策略“开仓频率高”的目标。

后续补充验证显示：把扫描候选按交易频率排序后，满足 `win_rate>=60,max_dd<15,pnl>0` 的最高频组合也只有 `0.48/day`；加入更贴近 SMC 的“突破后等待 OB 回踩”机制后，真实样本没有达标候选。因此当前 SMC 结构信号不能靠 pivot、冷却、低 R 或回踩等待参数直接变成高频策略。

最新 FVG / fade / displacement / Premium-Discount / 延迟回踩复查使用收缩后的 384 组日常扫描网格，覆盖 BTC/ETH/SOL 的 5m/15m、每个 case 3000 根 K 线。单独看 ETH 5m 时，延迟回踩能把一个结构候选推到 `entries=11 win_rate=63.64% pnl=2.9140 max_dd=0.10% trades_per_day=1.06`；FVG 加 Premium/Discount 也能出现 `entries=7 win_rate=71.43% pnl=0.2868 max_dd=0.37% trades_per_day=0.67`。但跨 6 个 case 聚合后，所有 `trades_per_day>=1` 的高频候选胜率只有约 26%-35%，且 PnL 显著为负；满足胜率与回撤的候选频率只有 0.16-0.35/day。因此新增子信号仍没有解决跨币种、跨周期后的频率问题。

这说明 TradingView SMC 裸结构不适合直接升级为当前产品的短周期高频策略，也不适合直接叠到现有短周期基底上。后续如果继续推进，应优先做两件事：

1. 把 v1 定位为低频结构确认策略，只用于 15m/1h 级别候选观察。
2. 若要做 1m/5m 高频版本，需要新增独立策略能力，而不是继续在 v1 上硬调参数；方向应改为 SMC 事件作为上下文标签，入场由 session VWAP、盘口/成交量确认或 Market Velocity 前置过滤提供。

## SMC 与 Market Velocity 组合回滚

后续复核确认，SMC 与 Market Velocity 的组合观察入口样本不足、近期没有新入场，继续迭代也没有稳定提升开仓频率和收益质量。因此已回滚该组合 preset 与轻量 sweep tag 实验，当前只保留基础 `smart_money_concepts_v1_research` 指标、扫描与研究测试。

当前结论不变：SMC v1 适合作为低频结构确认与上下文研究指标，不作为 Market Velocity 的默认过滤层、触发标签或 paper-observation preset。
