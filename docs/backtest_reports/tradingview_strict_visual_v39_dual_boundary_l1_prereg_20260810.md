# V39 视觉父区间与交易核心边界 L1 预注册

## 假设卡片

- 研究等级：`L1_OUTCOME_BLIND_PARITY`。
- 基线：`V31InternalImpulseBodyGuard` 的固定窗口候选与 `High P90 / Low P10` 交易边界。
- 唯一变量：为每个已通过 V31 的固定窗口附加一个只读 `visual_parent` 外壳；`trade_core` 继续完整复用 V31。
- 因果定义：只读取同一确认时点已经完成的固定窗口 K 线，不使用确认后的任何价格。
- 预计影响：`trade_core` 有效身份和数值变化必须为 0；至少 10% 的有效窗口发生视觉外壳扩边，并覆盖至少 8 个币种、10 个小时事件簇。
- 停止条件：任一交易核心身份或字段漂移；任一实体未被视觉外壳包含；越界影线超过许可；BTC 目标窗口不匹配。
- 保持不变：币种池、数据窗口、候选长度、质量门禁、弱离区、突破、回踩、止损、目标、成本、冲突和资金路径。

## 冻结双层职责

### trade_core

- 边界继续使用 V31 固定窗口 `High P90 / Low P10`。
- 继续负责宽度、内部冲击、双侧触碰、包含率、漂移、方向效率和边沿交替质量门禁。
- 继续负责区间高度、弱离区、突破、回踩、失效、止损、目标和全部交易字段。
- 本轮不允许任何信号、订单或风险语义变化。

### visual_parent

- 只在对应 `trade_core` 已通过时生成并冻结。
- `body_upper = max(max(open, close))`，`body_lower = min(min(open, close))`，保证 100% 实体被包含。
- 每侧允许越界影线数为 `max(1, floor(N / 10))`。
- `wick_upper` 为删去允许数量最高 High 后的最高端点；`wick_lower` 为删去允许数量最低 Low 后的最低端点。
- 先得到 `body_wick_upper = max(wick_upper, body_upper)`、
  `body_wick_lower = min(wick_lower, body_lower)`；最终
  `visual_parent_upper = max(trade_core_upper, body_wick_upper)`、
  `visual_parent_lower = min(trade_core_lower, body_wick_lower)`，保证视觉父区间始终包含交易核心。
- 只负责绘图、形态归属和扫高/扫低展示，不参与触碰确认、突破、回踩或任何交易计算。

## 目标与数据

- 定义夹具：BTC-USDT-SWAP 15m，北京时间 `2026-07-27 06:45 -> 18:30` 的 48 根窗口。
- `trade_core` 必须保持 `65511.5 / 65050.0`。
- `visual_parent` 必须得到 `65573.1 / 64991.6`，完整包含 09:15 与 14:15 实体；每侧越界影线不得超过 4 根。
- 样本外窗口固定为北京时间 `[2026-07-28 00:00, 2026-08-09 12:15)`。
- 沿用显式 61 个 OKX USDT 永续成员；只使用本地已有且 60 天预热与评价窗口完整的成员，不同步或补造 K 线。

## 标签边界

本轮机器结果禁止读取或序列化订单、成交、退出时间、MFE、MAE、最终 R、盈亏和胜负。通过只代表双层合同具有机械可行性与交易身份 parity，不代表策略具备正收益。
