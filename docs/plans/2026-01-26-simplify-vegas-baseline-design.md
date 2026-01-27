# Vegas 基线策略简化设计

**目标**：基于 back_test_id=116 的真实使用情况，移除回测过程中未触发/未使用的策略逻辑，使策略更简洁，同时保持回测指标与基线几乎一致（允许 ≤1% 偏差）。

## 背景与约束
- 只针对 Vegas 4H 策略的实现逻辑做减法，不改数据库历史 K 线。
- 不使用 git worktree，直接在当前工作区操作。
- 允许跳过新增测试，用“回测对比 + 指标使用清单”验证。

## 设计要点（简化范围）
1) **基于 back_test_id=116 的指标/过滤器使用清单**锁定“未被触发”的逻辑：
   - 过滤器层：`EMA_DISTANCE_FILTER_LONG` 与 `MACD_MOMENTUM_WEAK_*` 没有出现；
   - 辅助函数：`detect_multi_body_engulfing` 仅存在于测试中，未被策略路径调用。
2) **保留所有在回测中实际触发的逻辑**：如 MACD Falling Knife、EMA 距离空头过滤、追涨追跌确认、极端 K 过滤、结构与 Fib 严格趋势过滤、止损来源（Engulfing/KlineHammer/LargeEntity/Fib）。
3) **最小影响修改**：仅删除未使用分支与对应文档说明，避免大范围结构性重构；保持配置 JSON 兼容性优先。

## 验证方式
- **回测对比**：运行一次新回测，与基线 ID=116 指标对比（win_rate、profit、sharpe、max_dd 等），允许 ≤1% 偏差。
- **指标使用清单**：使用 back_test_detail 分析脚本输出 top indicators，确认与基线一致。
- 更新 `docs/VEGAS_ITERATION_LOG.md` 记录删减项与回测结果。

## 风险与对策
- 风险：删除逻辑影响其他配置场景。
- 对策：只删“未触发分支/测试辅助函数”，保留核心过滤与信号路径；用回测对比兜底。
