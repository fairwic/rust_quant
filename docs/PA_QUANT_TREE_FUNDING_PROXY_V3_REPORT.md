# PA Quant Tree Hyperliquid 资金费率代理 v3 报告

## 协议与数据

- 训练协议：`pa-training-v3-hyperliquid-funding-proxy`。
- K 线来源：OKX，BTC、ETH、SOL、BCH 永续。
- 资金费率来源：Hyperliquid 公共 `fundingHistory`，只使用这一个代理来源。
- 资金费率用途：成本压力，不作为信号特征。
- 计费方式：交易持仓跨越的每个小时桶累计 `abs(funding_rate)`；负费率也按成本扣除，不计作收益。
- 原因：代理来源与 OKX 不同，使用绝对值可以避免跨交易所方向差异美化收益。
- 阶段：训练期，不是密封 OOS 或 Forward Paper。
- Promote：禁止；`promotion_blocker=funding_cost_proxy_and_sealed_oos_not_opened`。

## 覆盖证据

资金费率写入 `quant_core` 的分源分片表，未写入缺少 exchange 维度的旧 `funding_rates` 表。

| 周期 | 公共窗口 | 每市场 funding 点 | 小时桶缺口 |
| --- | --- | ---: | ---: |
| 15m | 2025-07-15 02:00 UTC 至 2026-07-15 01:30 UTC | 8,760 | 0 |
| 1h | 2025-07-15 03:00 UTC 至 2026-07-15 01:00 UTC | 8,759 | 0 |

指纹：

| 周期 | K 线指纹 | Funding 指纹 | 联合训练指纹 |
| --- | --- | --- | --- |
| 15m | `sha256:2104caaef43352418afb8d94cd658e2959cde75ecdcdee1d61b241bac193f533` | `sha256:54ba0abaac9f5772e7b3d9fbd4a9d4568906dde51c065a4f16a146e26a17069f` | `sha256:92697e7e815aafdb40783fd00c0e79c896d04faf30ae5414e3f4edac139feaa9` |
| 1h | `sha256:eba26a8bca1a1c409ce23f83b4fcfb67d21a50cca71e997f5b4350ef0233a474` | `sha256:05a3336f4cfac170cc104b1ea5768813a5dd518db2e1bad886b1424577707942` | `sha256:d24e093a64a5cde482d93963e8184c5ab43da8f76963f1db9fcc73a3e0470701` |

## 成本后结果

| 策略 | 样本 | 平均 R | 胜率 | PF | 两倍成本平均 R | 组合最终权益 | 最大回撤 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `pa_trend_15m` | 2,350 | -0.274 | 39.57% | 0.644 | -0.542 | 75.94U | 26.83% |
| `pa_range_15m` | 713 | -0.711 | 33.94% | 0.370 | -1.414 | 75.99U | 24.21% |
| `pa_trend_1h` | 879 | -0.109 | 41.07% | 0.836 | -0.245 | 86.29U | 17.74% |
| `pa_range_1h` | 193 | -0.393 | 31.61% | 0.562 | -0.717 | 90.62U | 11.63% |

7日共享市场块 bootstrap 单侧95%下界：

| 策略 | 观察均值 | 95%下界 |
| --- | ---: | ---: |
| `pa_trend_15m` | -0.274R | -0.349R |
| `pa_range_15m` | -0.711R | -0.830R |
| `pa_trend_1h` | -0.109R | -0.219R |
| `pa_range_1h` | -0.393R | -0.599R |

## 模型竞赛

- `pa_trend_15m` 仍选择 M2 逻辑回归：保留 719 / 940 个验证候选，覆盖率 76.49%，验证平均 -0.040R。
- `pa_range_15m`、`pa_trend_1h`、`pa_range_1h` 仍选择 M0。
- CART 与 Forest 继续因0覆盖率淘汰。
- 资金费率代理没有改变任何分支的晋级结论，也没有产生正期望 Challenger。

## 决策

1. 单一资金费率来源已经足够完成保守成本压力研究，不再扩展其他交易所 funding 数据。
2. `funding_cost_included=true` 只表示报告已扣代理费率；`funding_cost_is_proxy=true` 继续阻止 Promote。
3. 当前主要矛盾是候选结构与特征解释力，不是资金费率缺失。
4. 下一轮训练应预注册新的趋势入场质量特征或候选语义；不得在当前窗口继续无边界扫阈值。
5. 密封 OOS 保持关闭，Vegas 与生产执行路径保持不变。
