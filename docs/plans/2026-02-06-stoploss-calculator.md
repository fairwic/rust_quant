# StopLoss Calculator Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Introduce a shared `StopLossCalculator` and refactor live/backtest order paths to compute stop-loss candidates first, then select the tightest valid stop via a single shared function.

**Architecture:** Add a small shared module under `crates/strategies` that selects the tightest valid stop-loss from a candidate list given side and entry price. Upstream callers compute candidate prices (signal stop-loss + max-loss stop) and pass them in. Replace duplicated per-site logic in services and execution layers with the shared selector.

**Tech Stack:** Rust, existing strategies framework

---

### Task 1: Add StopLossCalculator module with tests

**Files:**
- Create: `crates/strategies/src/framework/risk/stop_loss_calculator.rs`
- Modify: `crates/strategies/src/framework/risk/mod.rs`
- Modify: `crates/strategies/src/framework/backtest/mod.rs`
- Test: `crates/strategies/src/framework/risk/stop_loss_calculator.rs`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::{StopLossCalculator, StopLossSide};

    #[test]
    fn select_tightest_long_stop_from_candidates() {
        let entry = 100.0;
        let candidates = vec![95.0, 97.0, 90.0];
        let selected = StopLossCalculator::select(StopLossSide::Long, entry, &candidates);
        assert_eq!(selected, Some(97.0));
    }

    #[test]
    fn select_tightest_short_stop_from_candidates() {
        let entry = 100.0;
        let candidates = vec![105.0, 103.0, 110.0];
        let selected = StopLossCalculator::select(StopLossSide::Short, entry, &candidates);
        assert_eq!(selected, Some(103.0));
    }

    #[test]
    fn ignores_invalid_candidates() {
        let entry = 100.0;
        let candidates = vec![100.0, 101.0];
        let selected = StopLossCalculator::select(StopLossSide::Long, entry, &candidates);
        assert_eq!(selected, None);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q -p rust-quant-strategies`
Expected: FAIL (module not found)

**Step 3: Write minimal implementation**

Implement:
- `enum StopLossSide { Long, Short }`
- `struct StopLossCalculator;`
- `fn select(side, entry, candidates) -> Option<f64>`
  - Long: choose highest candidate `< entry`
  - Short: choose lowest candidate `> entry`
  - Ignore NaN/invalid

**Step 4: Run test to verify it passes**

Run: `cargo test -q -p rust-quant-strategies`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/strategies/src/framework/risk/stop_loss_calculator.rs \
        crates/strategies/src/framework/risk/mod.rs \
        crates/strategies/src/framework/backtest/mod.rs
git commit -m "feat(风控): 新增止损选择器"
```

---

### Task 2: Refactor live order path to use StopLossCalculator

**Files:**
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`
- Test: `crates/services/src/strategy/strategy_execution_service.rs`

**Step 1: Write the failing test**

Add a test that constructs candidate stop-loss values and expects selection to match the shared calculator.

```rust
#[test]
fn live_stop_loss_uses_shared_selector() {
    use rust_quant_strategies::framework::risk::{StopLossCalculator, StopLossSide};
    let entry = 100.0;
    let candidates = vec![95.0, 97.0];
    let selected = StopLossCalculator::select(StopLossSide::Long, entry, &candidates);
    assert_eq!(selected, Some(97.0));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q -p rust-quant-services`
Expected: FAIL (missing shared selector or unused in code)

**Step 3: Write minimal implementation**

- Remove `compute_initial_stop_loss` and replace with:
  - Build candidate list: `max_loss_stop`, `signal_stop_loss` (if enabled + present)
  - Call `StopLossCalculator::select` with side/entry
- Ensure `final_stop_loss` is `Some` or return error if none

**Step 4: Run test to verify it passes**

Run: `cargo test -q -p rust-quant-services`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/services/src/strategy/strategy_execution_service.rs
git commit -m "refactor(实盘): 统一止损选择逻辑"
```

---

### Task 3: Refactor execution swap order path to use StopLossCalculator

**Files:**
- Modify: `crates/execution/src/order_manager/swap_order_service.rs`
- Test: `crates/execution/src/order_manager/swap_order_service.rs`

**Step 1: Write the failing test**

Add a unit test mirroring live selection logic.

```rust
#[test]
fn swap_order_stop_loss_uses_shared_selector() {
    use rust_quant_strategies::framework::risk::{StopLossCalculator, StopLossSide};
    let entry = 100.0;
    let candidates = vec![102.0, 105.0];
    let selected = StopLossCalculator::select(StopLossSide::Short, entry, &candidates);
    assert_eq!(selected, Some(102.0));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q -p rust-quant-execution`
Expected: FAIL

**Step 3: Write minimal implementation**

- Replace in-function stop-loss calculation with candidate list + shared selector
- Validate selected stop-loss and error if None

**Step 4: Run test to verify it passes**

Run: `cargo test -q -p rust-quant-execution`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/execution/src/order_manager/swap_order_service.rs
git commit -m "refactor(执行): 统一止损选择逻辑"
```

---

### Task 4: Full verification

**Files:**
- None

**Step 1: Run targeted tests**

Run: `cargo test -q -p rust-quant-strategies`
Expected: PASS

Run: `cargo test -q -p rust-quant-services`
Expected: PASS

Run: `cargo test -q -p rust-quant-execution`
Expected: PASS

**Step 2: Run full test suite**

Run: `cargo test -q`
Expected: PASS

**Step 3: Commit (if needed)**

Only if further changes are required.

---

**Plan complete.**
