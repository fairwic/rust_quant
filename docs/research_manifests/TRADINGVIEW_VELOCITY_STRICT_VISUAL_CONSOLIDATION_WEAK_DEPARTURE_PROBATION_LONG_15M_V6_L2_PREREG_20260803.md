# TradingView Velocity 严格视觉横盘弱离区一根观察期 V6 L2 预注册

## 冻结版本与唯一变量

```text
研究等级：L2_LOCAL_MULTI_SYMBOL_DIAGNOSTIC
唯一变量：V5 弱离区立即消费，改为 V6 紧邻一根完成棒观察期
基线版本：volume_strict_visual_consolidation_breakout_body_strength_60pct_25bps_body_midpoint_hold_long_15m_research_v5
候选版本：volume_strict_visual_consolidation_weak_departure_one_bar_probation_body_strength_long_15m_research_v6
保持不变：60%+0.25%强突破门禁、横盘定义、放量门禁、V3三棒接受、实体中点守稳、
          下一根开盘、1.5 ATR止损、量能2.7/3.6/4.5 ATR目标、成本、冲突、反手和资金路径
L2停止：V6严格横盘家族成本后EV<=0或PF<=1；EV/PF未同时优于V5；
          改善依赖1～2笔、单一币种、单一月份或单一事件簇
```

V6 是 Candidate V20 上的 Research-only 身份，不覆盖 V3/V5，不继承已经停止的 V4 1R 退出，不创建 Pine，不接运行态。

## 已通过的 L1 证据

- 行情指纹：`4919b364fb4737b0da7921cd53e401adb97013597ba3049ca79c6e1e9890577f`。
- 43/60 个本地完整成员，15m，北京时间 `2025-07-01 08:00` 至 `2026-07-19 22:15`。
- 18,301 次弱离区中，6,729 次下一根回区；最大等待 1 根、未决 0、区外确认补锚点 0。
- V5 351 个候选；V6 421 个候选，43 币、13 月、338 个事件簇。
- L1 只读取信号时与紧邻结构决策时可见字段，未使用任何成交后标签。

## L2 执行合同

1. 同一当前 Release 可执行文件分别运行 V5 基线与 V6 候选；行情、universe、评价窗口和其他 Research 轴必须一致。
2. 成本固定为每边 5 bps 手续费加 3 bps 滑点；只使用本地 43 个完整成员，17 个缺失成员跳过且不补数据。
3. 恢复真实下一根开盘、tick 对齐止损/目标、同棒止损优先、同币种持仓冲突、反手和提前释放后的后续路径。
4. V5/V6 都继续使用 V3 量能 ATR 目标；禁止混入 V4 的短区间 1R、保本、部分止盈或移动止损。
5. 报告严格横盘家族与全部 Candidate V20 的成本前后 EV、PF、胜率、净 R、止损比例和执行数。
6. 报告 V6 相对 V5 的新增、删除、共同交易与路径释放变化，并检查币种、月份和 60 分钟事件簇集中度。
7. V6 机器账本必须保留信号时冻结的区间、止损、目标和 outcome 分栏；不得再修改生命周期、阈值或退出。

## L2 采用条件

- V6 严格横盘家族成本后 `average_net_r > 0` 且 `profit_factor_r > 1`；
- V6 成本后 EV 与 PF 均高于同身份 V5；
- 正边际不依赖 1～2 笔、单一币种、单一月份或单一 60 分钟事件簇；
- 满足后才允许进入 L3 的 OOS、压力、集中度和 Pine/Rust parity；否则停止在 L2。

## 预期身份与产物

- universe：`top60_v36_direct_kline_20260721_frozen_20260723`
- manifest SHA-256：`3fd267ca5cf1ecee8199232729da0e6db917803f6e7a1b363fa84e0ba75d5a4f`
- V6 L2 机器结果：
  `docs/backtest_reports/tradingview_velocity_strict_visual_weak_departure_probation_v6_l2_20260803.json`
- L2 行情指纹、可执行文件指纹和 V5 基线指标由同一批当前二进制运行后核对。
