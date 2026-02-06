# Signal Engine Architecture Design

**Goal**
- 将信号生成独立为通用引擎，实盘/回测共用同一套信号逻辑。
- 保持当前最优基线的止盈止损语义不变。
- 提升可读性：模块化但不引入多层 trait，调用链路保持直线、浅层。

## 核心原则
- **信号引擎独立**：只负责信号生成，不下单、不计算资金。
- **风控统一**：止盈止损逻辑抽成 RiskEngine，沿用现有 risk_config 语义。
- **浅层结构**：最多两层模块深度；内部拆分到函数级别即可。
- **无多层 trait**：采用 struct + enum 分发（match），显式可读。

## 模块结构（建议）
```
crates/signal_engine/
  lib.rs
  signal_engine.rs
  risk_engine.rs
  engine_types.rs
  components/
    ema.rs
    rsi.rs
    market_structure.rs
    leg_detection.rs
    ...
```

### SignalEngine（核心）
- API:
  - `SignalEngine::new(config)`
  - `SignalEngine::warmup(candles)`
  - `SignalEngine::next(candle) -> SignalDecision`
  - `SignalEngine::snapshot() -> SignalSnapshot`
- 内部流程：
  1. `update_indicators()`
  2. `build_conditions()`
  3. `weight_vote()`
  4. `emit_signal()`

### RiskEngine（止盈止损）
- API:
  - `RiskEngine::new(risk_config)`
  - `RiskEngine::on_open(position, candle, snapshot) -> RiskDecision`
  - `RiskEngine::on_update(position, candle, snapshot) -> ExitDecision`
- **保持现有 risk_config 行为与字段**：
  - `max_loss_percent`
  - `atr_take_profit_ratio`
  - `fixed_signal_kline_take_profit_ratio`
  - `is_used_signal_k_line_stop_loss`
  - 其他现有字段按当前基线语义执行

## 组件化但不使用多层 trait
- 使用 `enum SignalComponent` 统一组件分发：
```rust
enum SignalComponent {
  Ema(EmaComponent),
  Rsi(RsiComponent),
  MarketStructure(MarketStructureComponent),
}
impl SignalComponent {
  fn update(&mut self, candle: &CandleItem) { ... }
  fn conditions(&self) -> Vec<SignalCondition> { ... }
}
```
- 组件为普通 `struct`，只暴露 `update` 与 `conditions`，无 trait 继承链。

## 回测/实盘接入
### 回测链路
```
历史K线迭代 -> SignalEngine.next -> RiskEngine.on_update -> 回测执行器
```

### 实盘链路
```
实时K线 -> SignalEngine.next -> RiskEngine.on_update -> 实盘执行器
```

**关键**：两条链路复用同一 SignalEngine + RiskEngine，确保信号与风控一致。

## 可读性要求
- 不引入 `pipeline/stages/...` 多级目录。
- 主流程保持一条直线：`signal -> risk -> executor`。
- 业务逻辑集中在 `signal_engine.rs` 与 `risk_engine.rs`，阅读成本低。

## 迁移策略（概览）
- 抽取现有指标状态与信号生成逻辑到 `components/*`。
- 现有策略仅保留“配置 + 权重 + 调用引擎”。
- 回测与实盘执行层只负责成交处理，不再参与信号生成。

## 测试策略
- 单元测试：组件 update/conditions 与权重投票。
- 集成测试：SignalEngine -> RiskEngine -> BacktestExecutor 输出一致性。
- 回归：与当前基线回测结果一致（指定时间窗口）。
