# Trading System Ops Design (OMS/Portfolio/Audit/Isolation)

**Goal**
- 增加生产级 OMS（订单状态机）、Portfolio（统一资金/持仓口径）、Audit/Logging 审计链、多策略/多品种隔离。
- 保持回测/实盘一致性：信号引擎与风控引擎共用，执行层分离。
- 采用 MySQL 作为唯一审计与状态存储。

## Architecture Overview
主链路保持直线：
```
SignalEngine -> RiskEngine -> OrderManager -> OrderRouter -> PortfolioManager
```
- SignalEngine / RiskEngine 复用回测与实盘逻辑。
- OrderManager 负责状态机与审计。
- OrderRouter 负责接入撮合模型或交易所 API。
- PortfolioManager 负责全局资金与策略子账本。

## Module Layout (建议)
```
crates/trading/
  strategy/
    strategy_context.rs
    strategy_runtime.rs
  order/
    order.rs
    order_state.rs
    order_manager.rs
    order_router.rs
  portfolio/
    portfolio.rs
    position.rs
    risk_limits.rs
  audit/
    audit_logger.rs
    audit_models.rs
```

## OMS / Order State Machine
状态枚举：
```
New -> Submitted -> PartiallyFilled -> Filled
New -> CancelRequested -> Canceled
Submitted -> Rejected
```
- 每次状态变化写 `order_state_log`，记录 from/to、来源、交易所订单号。
- OrderManager 只处理订单生命周期，不直接更新 Portfolio。
- 成交回报驱动持仓更新（回测由撮合器模拟，实盘由交易所回报驱动）。

## Portfolio & Strategy Isolation
- `StrategyContext`：策略独立视图（资金配额、持仓子账本、风控状态）。
- `PortfolioManager`：全局资金/风险/敞口汇总。
- 每笔订单必须绑定 `run_id/strategy_id`，持仓按策略分账，组合层统一汇总。
- 支持策略级限制：`max_equity / max_margin / max_position_size`。

## Audit / Logging Chain (MySQL)
核心审计表：
- `strategy_run`：策略运行实例
- `signal_snapshot_log`：信号快照
- `risk_decision_log`：风控决策
- `order_decision_log`：下单意图
- `orders`：订单主表
- `order_state_log`：订单状态机日志
- `positions`：持仓主表
- `portfolio_snapshot_log`：组合快照

**审计链路原则**：任何订单都可追溯到对应的信号与风控决策。

## Monitoring (生产级)
- 指标：订单延迟、拒单率、滑点、成交率、策略收益/回撤。
- 日志：signal/risk/order/position 关键节点都写 audit。
- 告警：异常回报、连续拒单、风控触发异常、资金不足。

## Error Handling & Recovery
- OMS 支持超时重试/撤单，状态机可恢复。
- StrategyContext 可在断线后恢复状态（从 MySQL 快照加载）。
- Portfolio 可通过快照与订单日志重放。

## Testing Strategy
- 单元测试：订单状态机、风控决策、审计日志写入。
- 集成测试：SignalEngine -> RiskEngine -> OMS -> Portfolio 全链路。
- 回归测试：与当前最优基线回测结果一致（指定时间窗口）。

## Migration Notes
- 不改变现有 signal/risk 语义，只将执行与审计能力增强。
- 逐步接入：先回测验证 -> 再实盘灰度。
