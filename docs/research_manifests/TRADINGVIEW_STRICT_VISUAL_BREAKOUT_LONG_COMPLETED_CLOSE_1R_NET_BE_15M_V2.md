# TradingView 严格视觉横盘突破多单完成收盘 1R 净保本 V2 预注册

## 研究身份

- 日期：`2026-08-04`
- 当前等级：`L1_QUICK_RESEARCH`
- 基线版本：`volume_strict_visual_consolidation_weak_departure_one_bar_probation_body_strength_long_15m_research_v6`
- 前序淘汰研究：`strict_visual_breakout_long_net_be_activation_15m_research_v1`
- 新研究身份：`strict_visual_breakout_long_completed_close_1r_net_be_15m_research_v2`
- 周期：`15m`
- 基线机器结果：`docs/backtest_reports/tradingview_strict_visual_breakout_long_net_be_activation_v1_l2_20260804.json`
- 行情指纹：`eda87a30667f040cd74048e659def7453a5d49f6c81e062d012bebcb1c2ad5c4`
- 本地成员：`43/60`，只能作为本地部分成员诊断。

## 唯一变量

只改变净保本保护的激活证据：

```text
V1：已完成 K 线最高价达到 1R
V2：已完成 K 线收盘价达到 1R
```

阈值固定为 `1.0R`，不同时搜索 `0.5R`、收盘缓冲、连续确认根数或其他门槛。净保本价、成本、
tick 取整、下一棒生效、跳空成交、入场、初始止损和目标全部保持不变。

本假设是在查看过 V1 退出结果后形成，属于自适应样本内后续研究。即使 L2 转正，也不能把同一窗口
结果当成 L3/OOS 晋级证据，必须在未参与本轮推导的新窗口重新验证。

## 因果定义

只处理同时满足以下条件的 V6 基线成交：

- `families` 包含 `strict_visual_consolidation_break_long`；
- 方向为多单；
- `exit_policy = Fixed`；
- 入场、初始止损和冻结目标有效。

定义：

```text
initial_risk = entry_price - initial_stop
cost_ratio = 8 / 10000
raw_net_be = entry_price * (1 + cost_ratio) / (1 - cost_ratio)
net_be_stop = round_up_to_tick(raw_net_be)
activation_price = max(entry_price + 1.0 * initial_risk, net_be_stop)
```

仅当一根已完成 15m K 线的 `close >= activation_price` 时才冻结保护；保护最早从下一根 K 线生效。
确认棒内部的 low 不能反向触发刚形成的止损。下一棒若开盘低于保护价，按实际开盘价退出；保护不能
放宽。`R` 始终使用入场时的初始风险，不因移动保护而重算。

## 保持不变

- V6 横盘识别、强突破源、1～3 根接受确认和下一根开盘入场；
- 初始止损、冻结目标、信号优先级和其他信号家族；
- `CounterTrendStructureV4` 等非固定退出保持原样；
- 币种池、评价窗口、双边成本、冲突、反手和资金路径；
- 不增加分批止盈、结构追踪、横盘上沿保护、时间退出或新的入场过滤。

本轮仍使用冻结交易清单上的隔离退出反事实。提前退出不得释放容量或制造后续交易；只有 L2 正向后，
后续 L3 才允许重算完整组合路径。

## L1 无标签门禁

L1 只核对交易身份、冻结风险/目标和字段可用性，不读取新的完成收盘触发次数、退出时间、MFE、MAE、
最终 R 或胜负来调整定义。

- 严格视觉横盘基线成交：`290`
- `Fixed` 多单且风险/目标完整：`283`，覆盖 `43` 个币
- 非固定退出、保持原样：`7`
- `1.0R` 激活价不高于冻结目标：`283/283`
- 已确认 15m `close`、tick size、入场价、初始风险和双边 8 bps 成本均可由现有回放输入因果获得

定义可计算且覆盖充分，L1 停止条件未触发，允许进入本地 L2 隔离退出诊断。

## L2 停止与升级条件

V2 只与同一 V6 无保本基线比较；V1 最高价 1R 仅作归因参考，不参与阈值选择。只有 V2 同时满足：

1. 每边 `8 bps` 下净 R、平均 R 和 PF 均高于 V6；
2. 成本后净 R 为正且 PF 大于 `1`；
3. 至少激活 `30` 笔、覆盖 `20` 个 60 分钟事件簇；正向改善至少分布于 `3` 笔、`3` 个币、
   `3` 个上海月份和 `3` 个事件簇；
4. 交易身份完全不变，只有退出时间、价格和原因允许改变；
5. 完成收盘激活、确认棒不得自触发、下一棒生效、tick 向上取整和跳空实际开盘测试全部通过。

任一条件不满足即停止在 L2：不创建或修改 Pine、不接 Paper/ReadOnly/Live、不覆盖 V6、V1 或主策略。
