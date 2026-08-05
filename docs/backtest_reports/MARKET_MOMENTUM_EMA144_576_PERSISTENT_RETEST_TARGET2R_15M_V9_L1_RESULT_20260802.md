# 15 分钟动量 EMA144/576 永久资格回踩 V9：2.0R 固定止盈 L1 结论

## 结论

V9 通过 L1 几何与身份门禁，可以在另行冻结 L2 清单后读取成交后结果。此次通过只证明“V6 入场 + 4% 止损 + 2.0R 目标”可被完整、因果地构造，不证明它具有盈利能力。

## 冻结证据

- V9 机器报告：`docs/backtest_reports/market_momentum_ema144_576_persistent_retest_target2r_15m_v9_l1_20260802.json`。
- V9 L1 报告 SHA-256：`31dbd99af9d9a0cc42b659eb99ef3038a22596dd15712adfa3671b57b2128769`。
- V6 源账本 SHA-256：`a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da`。
- 行情指纹：`67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997`。
- 唯一变量：固定目标从 `0.52R` 改为 `2.0R`；入场、4% 止损、24h 持仓、成本、冲突和币种范围均未改变。

## L1 结果

- 源候选：54,837；有效 2R 几何：54,837；无效：0。
- 多头 22,569，空头 32,268；44 个完整本地成员，13 个 UTC 月份，3,747 个一小时去簇信号事件。
- 用户目标图：NMR 2026-07-01、BTC 2026-07-02、BTC 2026-07-12，全部保留，3/3。
- 多头保护因子：止损 `0.96*entry`，目标 `1.08*entry`；空头镜像：止损 `1.04*entry`，目标 `0.92*entry`。
- `outcome_evaluation_performed=false`；没有读取后续 K、退出、MFE、MAE、R、胜负或 PnL。

## 边界

V9 仍是 Research-only。此结论不授权创建 Pine、注册 Paper/ReadOnly/Live、修改生产 preset 或真实下单；只有下一阶段成本后 L2 门禁通过，才允许准备 L3。
