# Trading System Ops Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在回测链路中落地 OMS / Portfolio / Audit 审计链与多策略隔离，保持 Signal/Risk 语义不变，并为实盘复用留出接口。

**Architecture:** 新增 `rust-quant-trading` crate 承载 OMS/Portfolio/Audit 组件；回测 Pipeline 仅记录审计事件（AuditTrail），由 BacktestService 落库；实盘将来可直接使用相同的 AuditTrail 与 OMS 组件。

**Tech Stack:** Rust, sqlx(MySQL), serde_json, uuid

---

### Task 1: 创建 trading crate 骨架 + 订单状态机测试 (RED)

**Files:**
- Create: `crates/trading/Cargo.toml`
- Create: `crates/trading/src/lib.rs`
- Create: `crates/trading/src/order/mod.rs`
- Create: `crates/trading/src/order/order_state.rs`
- Modify: `Cargo.toml`
- Test: `crates/trading/src/order/order_state.rs`

**Step 1: 写失败测试 (订单状态机)**

```rust
#[cfg(test)]
mod tests {
    use super::{OrderState, OrderStateMachine};

    #[test]
    fn order_state_transitions() {
        let mut sm = OrderStateMachine::new();
        assert_eq!(sm.state(), OrderState::New);
        sm.submit().unwrap();
        assert_eq!(sm.state(), OrderState::Submitted);
        sm.fill().unwrap();
        assert_eq!(sm.state(), OrderState::Filled);
    }
}
```

**Step 2: 运行测试确认失败**

Run: `cargo test -q -p rust-quant-trading order_state_transitions`
Expected: FAIL (missing OrderState/OrderStateMachine)

**Step 3: 写最小实现**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState { New, Submitted, PartiallyFilled, Filled, CancelRequested, Canceled, Rejected }

pub struct OrderStateMachine { state: OrderState }
impl OrderStateMachine {
    pub fn new() -> Self { Self { state: OrderState::New } }
    pub fn state(&self) -> OrderState { self.state }
    pub fn submit(&mut self) -> Result<(), String> { /* ... */ }
    pub fn fill(&mut self) -> Result<(), String> { /* ... */ }
    pub fn cancel(&mut self) -> Result<(), String> { /* ... */ }
    pub fn reject(&mut self) -> Result<(), String> { /* ... */ }
}
```

**Step 4: 运行测试确认通过**

Run: `cargo test -q -p rust-quant-trading order_state_transitions`
Expected: PASS

**Step 5: Commit**

```bash
git add Cargo.toml crates/trading

git commit -m "feat(交易): 新增OMS订单状态机"
```

---

### Task 2: AuditTrail 事件模型 + 单测 (RED->GREEN)

**Files:**
- Create: `crates/trading/src/audit/mod.rs`
- Modify: `crates/trading/src/lib.rs`
- Test: `crates/trading/src/audit/mod.rs`

**Step 1: 写失败测试 (AuditTrail 记录事件)**

```rust
#[cfg(test)]
mod tests {
    use super::{AuditTrail, SignalSnapshot};

    #[test]
    fn audit_trail_records_signal() {
        let mut trail = AuditTrail::new("run-1".to_string());
        trail.record_signal(SignalSnapshot { ts: 1, payload: "{}".to_string(), filtered: false, filter_reasons: vec![] });
        assert_eq!(trail.signal_snapshots.len(), 1);
    }
}
```

**Step 2: 运行测试确认失败**

Run: `cargo test -q -p rust-quant-trading audit_trail_records_signal`
Expected: FAIL

**Step 3: 写最小实现**

```rust
#[derive(Debug, Clone)]
pub struct SignalSnapshot { pub ts: i64, pub payload: String, pub filtered: bool, pub filter_reasons: Vec<String> }

#[derive(Debug, Default)]
pub struct AuditTrail {
    pub run_id: String,
    pub signal_snapshots: Vec<SignalSnapshot>,
    // risk_decisions, order_decisions, order_states, positions, portfolio_snapshots
}

impl AuditTrail {
    pub fn new(run_id: String) -> Self { Self { run_id, ..Default::default() } }
    pub fn record_signal(&mut self, s: SignalSnapshot) { self.signal_snapshots.push(s); }
}
```

**Step 4: 运行测试确认通过**

Run: `cargo test -q -p rust-quant-trading audit_trail_records_signal`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/trading

git commit -m "feat(审计): 新增AuditTrail事件模型"
```

---

### Task 3: PortfolioManager + 单测 (RED->GREEN)

**Files:**
- Create: `crates/trading/src/portfolio/mod.rs`
- Test: `crates/trading/src/portfolio/mod.rs`

**Step 1: 写失败测试 (成交更新持仓与资金)**

```rust
#[cfg(test)]
mod tests {
    use super::{PortfolioManager, FillEvent};

    #[test]
    fn portfolio_updates_on_fill() {
        let mut pm = PortfolioManager::new(100.0);
        pm.apply_fill(FillEvent { side: "BUY".to_string(), qty: 1.0, price: 10.0 });
        assert!(pm.total_equity() <= 100.0); // 最小断言，保证状态变化
    }
}
```

**Step 2: 运行测试确认失败**

Run: `cargo test -q -p rust-quant-trading portfolio_updates_on_fill`
Expected: FAIL

**Step 3: 写最小实现**

```rust
pub struct FillEvent { pub side: String, pub qty: f64, pub price: f64 }

pub struct PortfolioManager { total_equity: f64 }
impl PortfolioManager {
    pub fn new(total_equity: f64) -> Self { Self { total_equity } }
    pub fn apply_fill(&mut self, _fill: FillEvent) { /* 简化: 暂不改变 equity */ }
    pub fn total_equity(&self) -> f64 { self.total_equity }
}
```

**Step 4: 运行测试确认通过**

Run: `cargo test -q -p rust-quant-trading portfolio_updates_on_fill`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/trading

git commit -m "feat(组合): 新增PortfolioManager骨架"
```

---

### Task 4: 回测 Pipeline 产出 AuditTrail (RED->GREEN)

**Files:**
- Modify: `crates/strategies/src/framework/backtest/types.rs`
- Modify: `crates/strategies/src/framework/backtest/pipeline/context.rs`
- Modify: `crates/strategies/src/framework/backtest/pipeline/runner.rs`
- Modify: `crates/strategies/src/framework/backtest/pipeline/stages/signal.rs`
- Modify: `crates/strategies/src/framework/backtest/pipeline/stages/position.rs`
- Test: `crates/strategies/src/framework/backtest/adapter.rs`

**Step 1: 写失败测试 (回测结果包含审计事件)**

```rust
#[test]
fn pipeline_backtest_records_audit_trail() {
    let candles: Vec<crate::CandleItem> = (0..800)
        .map(|i| crate::CandleItem { o: 100.0, h: 101.0, l: 99.0, c: 100.0, v: 1.0, ts: i, confirm: 1 })
        .collect();

    #[derive(Debug, Clone, Default)]
    struct Strategy;
    impl crate::framework::backtest::adapter::IndicatorStrategyBacktest for Strategy {
        type IndicatorCombine = (); type IndicatorValues = ();
        fn min_data_length(&self) -> usize { 3 }
        fn init_indicator_combine(&self) -> Self::IndicatorCombine { () }
        fn build_indicator_values(_: &mut (), _: &crate::CandleItem) -> () { () }
        fn generate_signal(&mut self, candles: &[crate::CandleItem], _: &mut (), _: &crate::framework::backtest::types::BasicRiskStrategyConfig) -> crate::framework::backtest::types::SignalResult {
            let mut s = crate::framework::backtest::types::SignalResult::default();
            s.ts = candles.last().unwrap().ts;
            s.open_price = candles.last().unwrap().c;
            s.should_buy = s.ts % 2 == 0;
            s
        }
    }

    let risk = crate::framework::backtest::types::BasicRiskStrategyConfig::default();
    let result = crate::framework::backtest::adapter::run_indicator_strategy_backtest("TEST", Strategy::default(), &candles, risk);
    assert!(!result.audit_trail.signal_snapshots.is_empty());
}
```

**Step 2: 运行测试确认失败**

Run: `cargo test -q -p rust-quant-strategies pipeline_backtest_records_audit_trail`
Expected: FAIL (audit_trail 字段缺失)

**Step 3: 实现最小功能**
- `BackTestResult` 增加 `audit_trail: AuditTrail`
- `BacktestContext` 增加 `audit_trail` 字段
- `SignalStage` 在生成 signal 后写入 `SignalSnapshot`
- `PositionStage` 比较前后状态，产生 `OrderDecision` / `OrderStateLog` / `PortfolioSnapshot`
- `PipelineRunner` 返回 `audit_trail`

**Step 4: 运行测试确认通过**

Run: `cargo test -q -p rust-quant-strategies pipeline_backtest_records_audit_trail`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/strategies

git commit -m "feat(回测): 产出审计链AuditTrail"
```

---

### Task 5: 增加 MySQL 审计链表 Migration

**Files:**
- Create: `migrations/20260206090000_create_trading_audit_tables.sql`

**Step 1: 写迁移文件**

```sql
CREATE TABLE IF NOT EXISTS strategy_run (
  id BIGINT AUTO_INCREMENT PRIMARY KEY,
  run_id VARCHAR(64) NOT NULL,
  strategy_id VARCHAR(64) NOT NULL,
  inst_id VARCHAR(32) NOT NULL,
  period VARCHAR(16) NOT NULL,
  start_at TIMESTAMP NULL,
  end_at TIMESTAMP NULL,
  status VARCHAR(16) NOT NULL DEFAULT 'RUNNING',
  UNIQUE KEY uk_run_id (run_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS signal_snapshot_log (
  id BIGINT AUTO_INCREMENT PRIMARY KEY,
  run_id VARCHAR(64) NOT NULL,
  kline_ts BIGINT NOT NULL,
  filtered TINYINT NOT NULL DEFAULT 0,
  filter_reasons JSON NULL,
  signal_json JSON NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  KEY idx_run_ts (run_id, kline_ts)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- risk_decision_log / order_decision_log / orders / order_state_log / positions / portfolio_snapshot_log
```

**Step 2: Commit**

```bash
git add migrations/20260206090000_create_trading_audit_tables.sql

git commit -m "feat(数据库): 新增审计链表结构"
```

---

### Task 6: 审计仓储与 BacktestService 落库

**Files:**
- Modify: `crates/domain/src/traits/repository_trait.rs`
- Create: `crates/domain/src/entities/audit.rs`
- Modify: `crates/domain/src/entities/mod.rs`
- Create: `crates/infrastructure/src/repositories/audit_repository.rs`
- Modify: `crates/infrastructure/src/repositories/mod.rs`
- Modify: `crates/services/src/strategy/backtest_service.rs`
- Modify: `crates/orchestration/src/backtest/runner.rs`

**Step 1: 写失败测试 (AuditRepository trait)**

```rust
// crates/domain/src/traits/repository_trait.rs
#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn insert_strategy_run(&self, run: &StrategyRun) -> Result<u64>;
    async fn insert_signal_snapshots(&self, snapshots: &[SignalSnapshot]) -> Result<u64>;
    // 其余 insert_* 方法
}
```

**Step 2: 运行编译确认失败**

Run: `cargo check -q -p rust-quant-domain`
Expected: FAIL (StrategyRun/SignalSnapshot 未定义)

**Step 3: 写最小实现**
- 在 `crates/domain/src/entities/audit.rs` 增加 `StrategyRun/SignalSnapshot/...` 结构体
- 在 `audit_repository.rs` 实现 sqlx 批量插入
- `BacktestService` 新增 `save_audit_trail` 方法并在 `save_backtest_log` 调用
- `BacktestRunner::new` 注入 `SqlxAuditRepository`

**Step 4: 运行测试/编译确认通过**

Run: `cargo check -q -p rust-quant-services`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/domain crates/infrastructure crates/services crates/orchestration

git commit -m "feat(审计): 回测落库审计链"
```

---

### Task 7: 验证 (执行计划时使用)

**Step 1: 运行回测单元测试**
Run: `cargo test -q -p rust-quant-strategies pipeline_backtest_records_audit_trail`
Expected: PASS

**Step 2: 运行 trading crate 测试**
Run: `cargo test -q -p rust-quant-trading`
Expected: PASS

**Step 3: 全量测试 (如时间允许)**
Run: `cargo test -q`
Expected: PASS

---

**Execution Options**
- Subagent-Driven (this session)
- Parallel Session (separate, use executing-plans)
