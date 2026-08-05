# 15 分钟动量 EMA144/576 回踩入场壳 V10（多头）：L1 结论

## 结论

V10 在 L1 因定义不匹配而停止，不读取 outcome。把 EMA144 回踩仅作为现有 `breakout + 量能 + RSI + 布林 + 回撤` 信号之后 24 根内的 FVG 替代成交壳，虽然形成了分散候选，但用户三张目标图为 0/3；因此这不是该形态在现有主链路中的正确职责。

## 冻结证据

- 机器报告：`docs/backtest_reports/market_momentum_15m_ema144_576_retest_entry_shell_long_v10_l1_20260802.json`。
- 报告 SHA-256：`f63b39b46748361773208165281cb943f0489a97c6c6576e2f87ebdeeee3a140`。
- V6 源报告 SHA-256：`a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da`。
- 行情指纹：`67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997`。
- 上游信号账本 SHA-256：`e87763909dcdc6e5da255d4c72f5ccf8ad7e8c873a3cc405d9c6a8e838b4b127`。
- 最终候选账本 SHA-256：`ccd8631a1cb1baaa70cf11188bb27f785e9105f79bfebf901474a29b53e114d0`。

## 无标签覆盖结果

- 44 个完整成员中有 763,679 个多头 raw 15m 排名事件。
- 现有动量入场信号门禁通过 8,591 个，`breakout_previous_high` 触发保留 8,378 个。
- 1,185 个上游事件在 24 根内映射到 V6 EMA144 回踩；704 个重复映射同一次回踩，6,489 个没有等待窗内回踩。
- 4 根同币种冷却再阻塞 1 个，最终候选 1,184 个，覆盖 44 个币种、13 个 UTC 月份、794 个一小时有效事件。
- NMR 2026-07-01、BTC 2026-07-02、BTC 2026-07-12 均未命中，目标为 0/3。
- `outcome_evaluation_performed=false`；没有读取后续 K、退出、MFE、MAE、R、胜负或 PnL。

## 停止边界

V10 不进入 L2，不放宽 24 根窗口，不删除上游过滤器来追逐目标图，不创建 Pine，不注册 Paper/ReadOnly/Live，不修改现有 FVG preset。
