# 15 分钟动量 EMA144/576 回踩收盘守线确认 V7：L1 预注册

## 当前等级与晋级来源

- 当前等级：`L1 快速研究`；本清单先于 V7 候选筛选和任何 V7 结果读取冻结。
- 基线：V6 `market_momentum_ema144_576_persistent_qualification_order_retest_15m_v6`。
- V6 L1 报告：`docs/backtest_reports/market_momentum_ema144_576_persistent_qualification_order_retest_15m_v6_l1_20260802.json`。
- V6 L1 报告 SHA-256：`a69b9cafb83ea55601bc35eaf13a821c0a5fb5080f4d256632457ab3e6f974da`。
- 行情指纹：`67516c927ce30323f38f34e6c87fd7bac7720bae8084209cc44b86cce6efe997`。
- V6 L2 已因 8 bps/side 成本后负期望停止；V7 是独立新版本，不覆盖或改写 V6 结论。

## 单变量假设卡片

- 候选键：`market_momentum_ema144_576_close_hold_confirmation_15m_v7`。
- L1 规则版本：`l1_v6_touch_close_hold_current_ema144_v7`。
- 唯一变量：在 V6 首次触及动态 EMA144 回踩区后，要求同一根已完成触碰 K 的收盘重新位于当前 EMA144 的趋势侧。多头为 `close >= current EMA144`，空头为 `close <= current EMA144`。
- 因果性：触碰 K 收盘后才生成确认信号；成交最早只能发生在下一根 15m K 开盘。L1 只读取 V6 账本已经记录的 `close_holds_current_ema144`、信号时间、方向、币种和月份，不读取退出、MFE、MAE、R、胜负或 PnL。
- 预计影响：保留 V6 候选的 60%～85%；该范围只用于防止过滤器失效或退化，不依据任何成交后结果选择。
- 停止条件：三张用户目标图任一不再命中；保留率不在 10%～90%；只影响极少样本；覆盖或分散性门禁失败；或必须增加第二个变量才能成立。
- 保持不变：144 根永久历史资格、两收盘 EMA576 转换、24 根窗口、0.75 ATR 离开与重扩张、0.30 ATR 动态回踩区、资格/订单生命周期、Top60 快照、评价窗口和一小时事件归并。

## L1 目标与门禁

三张定义目标保持不变：

1. NMR long：`1782835200000..=1782878400000`；
2. BTC long：`1782943200000..=1782964800000`；
3. BTC long：`1783828800000..=1783850400000`。

V7 必须同时满足：

1. V6 L1 文件 SHA、候选键、规则版本、候选数 54,837、未读取结果标签和行情指纹全部匹配；
2. 三张用户目标图 3/3 仍有至少一个 V7 候选；
3. 保留候选至少 30，且占 V6 候选的 10%～90%；
4. 多空各至少 10 个候选；
5. 至少覆盖 8 个币种、6 个 UTC 月份和 15 个按方向与连续 60 分钟归并的有效事件；
6. 报告只包含信号时字段，`outcome_evaluation_performed=false`。

全部通过只能标记为 `coverage_pass_ready_for_l2_prereg`。任一失败立即停止 V7，不读取该过滤版本的成交后结果，不创建 Pine，不修改 Paper、ReadOnly、Live 或生产 preset。
