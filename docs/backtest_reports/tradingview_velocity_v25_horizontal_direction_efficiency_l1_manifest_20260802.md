# TradingView Velocity V25 横盘方向效率 L1 预注册

## 假设卡片

- 研究等级：L1 无标签可行性扫描。
- 基线版本：`volume_recent_horizontal_first_break_close_back_short_15m_research_v24`。
- 候选版本：
  - 邻域下界：`volume_recent_horizontal_direction_efficiency_30_first_break_close_back_short_15m_research_v25a`；
  - 预注册主候选：`volume_recent_horizontal_direction_efficiency_35_first_break_close_back_short_15m_research_v25b`；
  - 邻域上界：`volume_recent_horizontal_direction_efficiency_40_first_break_close_back_short_15m_research_v25c`。
- 唯一变量：最近有效 8 根横盘区的收盘方向效率上限。
- 因果定义：`abs(第 8 根收盘 - 第 1 根收盘) / sum(abs(相邻两根收盘变化))`；分母为零时记为 `0`。只读取突破棒之前已经完成的 8 根 K 线。
- 预注册阈值：`0.30 / 0.35 / 0.40`；`0.35` 为主候选，另外两个阈值只检查同一变量的邻域稳定性，禁止读取结果标签后更换主候选。
- 预计影响：预计过滤 V24 候选的 `20%～60%`，同时保留至少 5 个独立事件。
- 停止条件：ICP 目标反例在 `0.35` 下仍形成候选；三个阈值只影响 1～2 个事件；主候选剩余不足 5 个独立事件；定义需要再改变横盘新鲜度、触碰、确认棒、止损或目标才能成立。

## 已冻结的目标样本

- 必须拒绝：`ICP-USDT-SWAP`，2026-06-29 09:00（Asia/Shanghai）。现行 V24 选择 05:45～07:30 的 8 根窗口，收盘从 `2.118` 上升到 `2.144`，方向效率为 `0.52`，属于方向性修复而非横盘。
- 必须继续拒绝：`SHIB-USDT-SWAP`，2026-07-06 15m 趋势延续反例。
- 必须保留：至少 3 个 V24 候选通过主阈值，且不能只集中在单币、单月或单一 60 分钟事件簇。

## 保持不变

1. 冻结 Top60 版本、15 分钟周期、评价窗口、数据来源和信号时序保持 V24 同一身份；运行后记录新鲜数据指纹。
2. 仍在突破前 48 根内从近到远寻找连续 8 根候选区；宽度、上下沿触碰、前后半段边界漂移和首次收盘突破规则不变。
3. 仍只检查突破后第 1 或第 2 根完成棒；不恢复“必须再次扫过突破棒高点”。
4. 阴线、收盘跌回冻结上沿、收盘位置、拒绝量能、最低 1.5R、冻结止损/目标、下一根开盘成交、冲突和退出政策均不改变。
5. V24 保持原样并存；V25 只存在于 Research 回放，不创建 Pine，不接入 Paper、ReadOnly、Live 或生产调度。

## L1 标签边界与数据身份

- L1 只比较 V24/V25 的候选身份、方向效率阈值、币种、月份和预先固定的 60 分钟事件簇；禁止读取退出时间、MFE、MAE、最终 R、胜负或 PnL 选择阈值。
- 预期基线身份：`top60_v36_direct_kline_20260721_frozen_20260723`，manifest SHA256 `3fd267ca5cf1ecee8199232729da0e6db917803f6e7a1b363fa84e0ba75d5a4f`，评价起点 `1751328000000`。实际评价上界、纳入成员和数据 SHA256 以本轮只读加载结果为准。
- 只有主候选 `0.35` 满足全部 L1 门禁，才允许只对它进入 L2；L2 成本后边际不为正时立即停止。
