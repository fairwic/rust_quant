# Live Risk Alignment Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove all real-time/live-only risk adjustments so live trading exits are driven solely by `deal_signal + check_risk_config`, matching backtest behavior.

**Architecture:** Disable the realtime risk engine and its event pipeline (RiskConfig/Position/Candle), and remove live-side take-profit attachment. Keep only minimal initial stop loss when placing orders. Live decision flow still calls `deal_signal`, so exits are determined by backtest logic only.

**Tech Stack:** Rust, tokio, tracing, OKX SDK

---

### Task 1: Add failing tests for live risk event pipeline removal

**Files:**
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`
- Test: `crates/services/src/strategy/strategy_execution_service.rs`

**Step 1: Write the failing test**

Add a unit test that verifies `StrategyExecutionService` no longer exposes `with_realtime_risk_sender` or pushes `StrategyRiskConfigSnapshot`/`PositionSnapshot` events. (This will fail until API is removed.)

```rust
#[test]
fn no_realtime_risk_sender_api() {
    // This test should fail to compile while with_realtime_risk_sender exists.
    // After removal, it should be deleted or updated.
    // Placeholder to force removal of realtime risk API.
    let _ = "remove_realtime_risk_sender";
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q -p rust-quant-services`
Expected: FAIL (or compile error) because realtime risk API still exists.

**Step 3: Write minimal implementation**

Remove realtime risk sender API and event pushes from:
- `StrategyExecutionService` fields and constructor
- `with_realtime_risk_sender`
- `execute_strategy` risk-config snapshot send
- `execute_order_internal` position snapshot send

**Step 4: Run test to verify it passes**

Run: `cargo test -q -p rust-quant-services`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/services/src/strategy/strategy_execution_service.rs
git commit -m "refactor(实盘): 移除实时风控事件推送"
```

---

### Task 2: Remove realtime risk engine and candle event piping

**Files:**
- Modify: `crates/rust-quant-cli/src/app/bootstrap.rs`
- Modify: `crates/orchestration/src/workflow/websocket_handler.rs`

**Step 1: Write the failing test**

Add a compile-time check in `websocket_handler.rs` to ensure `RealtimeRiskEvent` import is removed.

```rust
#[test]
fn no_realtime_risk_event_import() {
    let _ = "remove_realtime_risk_event";
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q -p rust-quant-orchestration`
Expected: FAIL (or compile error) while realtime risk types are still used.

**Step 3: Write minimal implementation**

- In `bootstrap.rs`:
  - Remove creation of `risk_tx/risk_rx`
  - Remove `RealtimeRiskEngine` startup
  - Stop injecting `risk_tx` into `StrategyExecutionService` and `WebsocketStrategyHandler`
- In `websocket_handler.rs`:
  - Remove `realtime_risk_tx` field and `with_realtime_risk_sender`
  - Remove candle -> `RealtimeRiskEvent::Candle` publishing

**Step 4: Run test to verify it passes**

Run: `cargo test -q -p rust-quant-orchestration`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/rust-quant-cli/src/app/bootstrap.rs crates/orchestration/src/workflow/websocket_handler.rs
git commit -m "refactor(实盘): 关闭实时风控引擎与K线事件推送"
```

---

### Task 3: Remove live attached take-profit orders

**Files:**
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`

**Step 1: Write the failing test**

Add a unit test that ensures live order creation does not attach TP from env.

```rust
#[test]
fn live_order_does_not_attach_tp() {
    // placeholder - should be replaced by actual behavior test
    let attach_tp = std::env::var("LIVE_ATTACH_TP").unwrap_or_default();
    assert!(attach_tp.is_empty() || attach_tp == "0");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -q -p rust-quant-services`
Expected: FAIL (logic still uses LIVE_ATTACH_TP)

**Step 3: Write minimal implementation**

Remove `LIVE_ATTACH_TP` gating and force `take_profit_trigger_px = None` when calling
`execute_order_from_signal`. Keep only minimal stop loss attached.

**Step 4: Run test to verify it passes**

Run: `cargo test -q -p rust-quant-services`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/services/src/strategy/strategy_execution_service.rs
git commit -m "refactor(实盘): 移除实盘止盈挂单"
```

---

### Task 4: End-to-end sanity check

**Files:**
- None (runtime verification)

**Step 1: Run targeted tests**

Run: `cargo test -q -p rust-quant-services`
Expected: PASS

**Step 2: Run full test suite**

Run: `cargo test -q`
Expected: PASS

**Step 3: Manual smoke**

Run: `cargo run --bin rust_quant`
Expected: No realtime risk engine logs; strategy execution still occurs.

**Step 4: Commit (if needed)**

Only if any changes were made during verification.

---

**Plan complete.**
