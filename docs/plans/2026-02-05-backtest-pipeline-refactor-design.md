# Backtest Pipeline Refactor Design (Backtest ID 41)

**Goal**: 以 `backtest_id=41` 为基准，硬删除未启用/未参与权重的回测模块，统一回测路径为 pipeline，仅保留必要逻辑与日志，提升可读性与模块化。

## Scope
- **仅保留 pipeline 回测路径**，移除 legacy backtest engine 与对比测试。
- **硬删除** `weight=0` 或 `is_open=false` 的模块代码与日志链路。
- **硬删除**风险配置中明确关闭的分支逻辑与字段。
- **不修改 DB schema**（旧字段允许存在于 JSON，但将被忽略）。

## Baseline (backtest_id=41)
- Strategy: `vegas`, Period: `4H`
- Signal weights: `MarketStructure=0.0`, `FakeBreakout=0.0`
- Risk config disabled: `is_one_k_line_diff_stop_loss=false`, `is_move_stop_open_price_when_touch_price=false`,
  `is_counter_trend_pullback_take_profit=false`, `validate_signal_tp=false`, `tighten_vegas_risk=false`

## Deletion Targets
### Signals / Indicators
- **MarketStructure**: indicator implementation, config, indicator values, signal weight mapping, vegas strategy wiring
- **FakeBreakout**: signal weight enum/condition only

### Risk / TP/SL
- Remove disabled flags & code paths:
  - `is_one_k_line_diff_stop_loss`
  - `is_move_stop_open_price_when_touch_price`
  - `is_counter_trend_pullback_take_profit`
  - `validate_signal_tp`
  - `tighten_vegas_risk`
- Remove related signal/position fields & indicator helpers

## Pipeline Only
- 保留并统一使用 pipeline 回测路径
- 删除 legacy `run_back_test` / `run_back_test_generic`
- adapter/trait_impl 仅走 pipeline 入口

## Non-Goals
- 不调整数据库表结构
- 不引入新策略/新指标
- 不改变 41 的核心交易逻辑（仅删除无效分支与模块）

## Risks & Mitigations
- **影响共享框架**：删除模块会影响其它策略
  - 通过编译期错误驱动全量清理与引用更新
- **配置 JSON 旧字段残留**：
  - 维持 serde 默认忽略行为，运行期不报错

## Testing
- 重点验证回测 pipeline 的基本运行与日志写入
- 通过单测/集成测试确保无残留引用
