# 严格视觉横盘突破：1.0H 成本后保本退出 V4 预注册

## 研究身份

- 研究日期：2026-08-05。
- 当前等级：L1；只有无标签覆盖闸门通过后才进入 L2 本地多币种诊断。
- 研究版本：`strict_visual_breakout_range_height_1_0_net_be_15m_research_v4`。
- 冻结入场基线：`volume_strict_visual_consolidation_stronger_acceptance_margin_0_40_atr_long_15m_research_v8`。
- V8 L1 来源：`target/research/strict_visual_acceptance_margin_v8_l1_impl_20260804.json`，SHA-256 `7e609ee39a29d4c04b41e0a4333251db6eb15e1b124e0584dd9acb3ad24115e3`。
- V8 L2 来源：`docs/backtest_reports/tradingview_strict_visual_breakout_long_acceptance_margin_stronger_grid_v8_l2_20260804.json`，SHA-256 `9eb41ba7a10c9763157144303932871bd15b48589b3f925cc4b97100cfc94220`。
- L2 行情指纹：`eda87a30667f040cd74048e659def7453a5d49f6c81e062d012bebcb1c2ad5c4`。
- 币种池：冻结 Top60，本地可用 43/60；缺失成员只记录，不自动补数据。
- 周期与窗口：15 分钟；Unix 毫秒 `[1751328000000, 1784470500000]`。

## 假设卡片

- 唯一变量：把严格视觉横盘突破仓位的动态保护激活距离定义为冻结横盘高度的 `1.0` 倍。
- 因果定义：`H = visual_upper - visual_lower`，上下沿均来自信号形成时已经冻结的已完成横盘；不得使用入场后的 K 线反算或修订 `H`。
- 多单激活价：`max(actual_entry + H, long_net_break_even)`。
- 空单镜像激活价：`min(actual_entry - H, short_net_break_even)`；当前 V8 没有该家族空单，只验证公式与时序，不报告经验收益。
- 净保本价：设单边手续费与滑点合计比例为 `c`，多单为 `ceil_tick(entry * (1+c)/(1-c))`，空单为 `floor_tick(entry * (1-c)/(1+c))`。
- 激活证据：仅使用一根已经完成的 K 线；多单看 `high >= activation_price`，空单看 `low <= activation_price`。
- 生效顺序：激活 K 线收盘后冻结，新的保护止损从下一根 K 线开始生效；不允许同棒追溯保护。
- 跳空成交：保护生效后若下一根开盘越过止损，按实际开盘价退出；否则按净保本止损价退出。
- 预计影响：预计 `20%～80%` 的合格交易会在原固定退出前激活保护。

## 保持不变

- V8 的信号、下一根成交、原始止损、原始止盈、冲突与持仓路径保持不变。
- 原止盈继续有效，动态保护只能收紧止损，不能放宽止损或延长持仓。
- 成本基准固定为单边 `5bps` 手续费加 `3bps` 滑点；另报告 `10bps`、`12bps` 单边压力，但不据此选择阈值。
- 仅研究 `families == [strict_visual_consolidation_break_long] && exit_policy == Fixed` 的纯严格视觉横盘交易；混合家族和其他退出政策保持不变。
- 60 分钟链式同方向聚类仅用于集中度诊断，不冒充独立相关性模型。
- 不修改 V8、Pine、Paper、ReadOnly、Live 或生产默认版本，不触发任何真实下单。

## L1 无标签闸门

L1 只读取：交易身份、信号时间、实际入场、冻结目标、tick size，以及同一信号冻结的 `upper/lower/H`。禁止读取退出时间、退出价、MFE、MAE、最终 R 和胜负来选择规则。

通过条件：

1. 每个合格交易都能按 `symbol + signal_time_ms` 唯一匹配 V8 L1 的冻结横盘，且 `H` 有限并大于零；
2. 合格交易不少于 30 笔、覆盖不少于 10 个币种、6 个上海月份和 20 个 60 分钟事件簇；
3. `activation_price` 不高于冻结目标的交易不少于 30 笔且不少于合格交易的 20%；
4. 不需要引入第二个阈值、确认棒或目标改动才能计算。

任一条件失败即停止，不进入 L2。

## L2 本地诊断闸门

L1 通过后，只运行 `1.0H` 这一个主候选。L2 必须同时满足：

1. 交易身份、入场、原始止损和原始目标与 V8 完全一致；
2. 8bps 单边成本下净 R、平均净 R 与 Profit Factor 均优于同交易集合基线；
3. 8bps 单边成本下候选净 R 大于零且 Profit Factor 大于 1；
4. 实际激活不少于 30 笔、20 个事件簇；
5. 改善至少分布于 3 笔交易、3 个币种、3 个上海月份和 3 个事件簇，而非由 1～2 笔或单一事件簇贡献。

任一条件失败则停在 L2 并淘汰该退出变体；全部通过也只允许进入 L3，不能直接合并主策略。

## 已知风险

- L1 与 L2 产物的行情指纹字段并不相同，因此本轮用文件 SHA、窗口、成员和逐信号唯一键共同校验，不能只比较一个指纹字符串。
- OHLC 只能按冻结的 TradingView broker path 近似同棒先后；没有 tick 数据时，不宣称能还原真实盘口成交顺序。
- 当前只有多单经验样本；空单镜像仅是合同与单元测试证据，不能被表述为已完成空单跨币种验证。
