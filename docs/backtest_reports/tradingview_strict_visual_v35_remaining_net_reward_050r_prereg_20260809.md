# 严格视觉横盘 V35：剩余净收益至少 0.50R 预注册

## 研究等级与唯一变量

- 当前等级：L1 无结果标签几何覆盖；通过后才允许一次 L2 多币种成本回放。
- 基线：`V31InternalImpulseBodyGuard`。
- 候选：`V35RemainingNetReward50`，独立 Research-only 身份。
- 唯一变量：下一根实际开盘时，若冻结目标剩余净收益不足 0.50 个初始风险，则拒绝成交。

## 因果定义

```text
gross_remaining = direction_gross(entry, target)
expected_cost = entry * 0.0003 + target * 0.0006
initial_stop_distance = abs(entry - final_initial_stop)
remaining_net_reward_r = (gross_remaining - expected_cost) / initial_stop_distance
```

1. `entry` 只使用信号后下一根实际开盘；`target`、结构止损和最小 1 ATR 风险均来自已冻结意图。
2. 门槛固定为 `remaining_net_reward_r >= 0.50`；没有方向正确目标、风险非正或目标已越过时一律拒绝。
3. 在拒绝当前意图前不得先平掉既有反向仓位；其他入场、退出和冲突合同不变。

## 目标样本与停止条件

- `BTC-USDT-SWAP` `BTC-L-1785400200000` 与 `ETH-USDT-SWAP` 北京时间 `2026-08-04 06:15` 的小剩余目标交易必须被拒绝。
- L1 至少拒绝 3 个意图，拒绝比例不超过 V31 可成交意图的 60%，且候选仍覆盖多空及至少 8 个币种。
- 任一目标未命中即停止，不读取最终盈亏。
- L2 候选须同时改善净 EV 与 PF，并控制最大回撤；改善不得集中于 1～2 笔、单币或单一事件簇。
