# TradingView 严格视觉横盘突破多单确认收盘强接受余量 V8 预注册

## 研究身份

- 日期：`2026-08-04`
- 当前等级：`L1_OUTCOME_BLIND_COVERAGE`
- 基线版本：
  `volume_strict_visual_consolidation_weak_departure_one_bar_probation_body_strength_long_15m_research_v6`
- 前序停止版本：
  `volume_strict_visual_consolidation_acceptance_margin_atr_long_15m_research_v7`
- 新研究身份：
  `volume_strict_visual_consolidation_stronger_acceptance_margin_atr_long_15m_research_v8`
- 周期：`15m`
- L1 universe：`top60_v36_direct_kline_20260721_frozen_20260723`
- L1 行情指纹：`4919b364fb4737b0da7921cd53e401adb97013597ba3049ca79c6e1e9890577f`
- L2 行情指纹：`eda87a30667f040cd74048e659def7453a5d49f6c81e062d012bebcb1c2ad5c4`
- 本地成员：`43/60`，只能作为部分成员诊断。

V7 在不读取 outcome 的前提下确认，421 笔 V6 候选的接受余量 P25 为 `0.3049 ATR`、中位数为
`0.5197 ATR`；原 `0.05～0.20 ATR` 网格最多只影响 14.4893%，按事前门禁停止。本轮阈值仅由
该无标签分布重新预注册，不使用 V6/V7 的成交、退出、R、PnL 或胜负。

## 唯一变量

保持定义不变：

```text
acceptance_margin_atr
  = (confirmation_close - frozen_range_upper) / frozen_breakout_source_atr
```

V8 只要求首次合法确认棒满足：

```text
acceptance_margin_atr >= selected_min_margin_atr
```

不足阈值时，在同一完成棒记录 `acceptance_margin_rejected` 并消费突破来源；不能等待后续确认补开。

## L1 事前网格与选择规则

只扫描：

```text
0.30 ATR
0.40 ATR
0.50 ATR
```

每个阈值只读取 V6 L1 候选中的 `signal_close`、`upper`、`source_atr`、symbol、上海月份和冻结
60 分钟事件簇。禁止读取入场、退出、MFE、MAE、最终 R、PnL、PF、胜负或信号后的 K 线。

合格阈值必须同时满足：

1. 拒绝 V6 候选的 `15%～50%`；
2. 保留至少 150 个候选、25 个币、8 个上海月份和 100 个事件；
3. 拒绝至少 30 个候选、10 个币、6 个上海月份和 20 个事件；
4. 421 笔候选的余量均有限，且 `source_atr > 0`；
5. 不依赖任何事后指定的赢家或亏家。

若多个阈值合格，选择拒绝比例最接近 30% 的阈值；距离相同时选择较低阈值。没有合格阈值则停止
在 L1，不继续扩展网格。

## 预计影响

根据已冻结的无标签 P25/中位数，预计三档分别拒绝约 25%、35%～45%、接近 50%。这只是覆盖率
预期，不代表收益方向。选中阈值应影响至少 60 个候选并覆盖多个币、月和事件。

## 保持不变

- V6 横盘定义、弱离区一根观察期、突破棒 `60% + 0.25%` 强度门禁；
- 三棒有限接受窗口、首次确认、实体中点门禁和来源消费顺序；
- 下一根真实开盘入场、1.5 source ATR 初始止损、量能目标与特殊退出；
- Candidate V20 其他信号、优先级、冲突、反手、币种池、窗口和每边 8 bps 成本；
- 不改变确认根数、横盘长度、突破强度、止损、目标、保本、分批或时间退出。

## L2 停止门禁

只有 L1 合法选出一个阈值后，才实现该唯一阈值并运行一次完整 broker 回放。每边 8 bps 下必须
同时满足：

1. V8 严格视觉家族净 R、平均 R、PF 均优于 V6；
2. V8 严格视觉家族净 R 与平均 R 为正，PF 大于 1；
3. 实际删除或重排至少 20 笔严格家族成交，覆盖 10 币、6 月和 15 个事件；
4. 改善覆盖至少 3 币、3 月和 3 个事件，移除头部两笔改善后仍为正；
5. Candidate V20 全家族总成本后净 PnL 与 PF 不得同时恶化；
6. 共同严格信号的冻结来源、上沿、source ATR、风险和下一开盘合同零漂移；
7. L1/L2 数据、可执行文件、Pine、成本和 universe identity 可审计。

任一条件失败即停止在 L2，不搜索新的余量阈值，不创建或修改 Pine，不接 Paper、ReadOnly、Live
或生产。即使全部通过，本轮也是根据旧窗口归因提出的自适应样本内研究；43 个成员目前只有约
8.47 天前向数据，仍不得进入 L3。

## 因果测试

1. 确认余量低于阈值时当棒消费来源，之后更高确认不得补开。
2. 等于阈值时以 `>=` 保留。
3. 分母固定使用突破棒 `source_atr`，确认棒 ATR 或未来 ATR 不得参与。
4. V6 原有实体中点拒绝必须先执行，V8 不能把原拒绝棒重新分类为候选。
5. V8 过滤不得改变弱离区观察期、上下离区镜像和其他家族信号。
