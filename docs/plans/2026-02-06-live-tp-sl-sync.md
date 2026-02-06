# Live TP/SL Sync Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make live trading update TP/SL every K-line using the same risk logic as backtest, including ATR tiered take-profit and moving stop-loss.

**Architecture:** Extract a shared `compute_current_targets` function from backtest risk logic to produce per-K-line effective TP/SL. Live execution computes targets each K-line, cancels existing close-algo orders, and places new TP/SL algo orders when targets change (with epsilon debounce). OKX API calls are wrapped in `OkxOrderService` using raw JSON requests for cancel/place close-algo orders.

**Tech Stack:** Rust, existing backtest framework (`crates/strategies`), OKX REST client (`okx` crate), `DashMap` live state cache.

---

### Task 1: Ensure `okx/` source exists in the worktree

**Files:**
- Modify: `docs/plans/2026-02-06-live-tp-sl-sync.md`

**Step 1: Check for `okx/` directory**

Run: `ls okx`
Expected: directory exists (if missing, copy from main workspace).

**Step 2: Copy `okx/` from main workspace if missing**

Run:
```bash
cp -R /Users/mac2/onions/rust_quant/okx /Users/mac2/onions/rust_quant/.worktrees/live-tp-sl-sync/okx
```
Expected: `okx/` present in worktree.

**Step 3: Commit (if needed)**

If `okx/` is required for build and not tracked, skip commit and note it as local setup.

---

### Task 2: Extract shared `compute_current_targets` (tests first)

**Files:**
- Modify: `crates/strategies/src/framework/backtest/risk.rs`
- Modify: `crates/strategies/src/framework/backtest/mod.rs`
- Test: `crates/strategies/src/framework/backtest/risk.rs`

**Step 1: Write failing tests for target selection**

Add a `#[cfg(test)]` module in `crates/strategies/src/framework/backtest/risk.rs`:

```rust
#[test]
fn compute_targets_prefers_tightest_stop_loss_and_nearest_tp_long() {
    let mut position = TradePosition::default();
    position.trade_side = TradeSide::Long;
    position.open_price = 100.0;
    position.position_nums = 1.0;
    position.signal_kline_stop_close_price = Some(95.0);
    position.move_stop_open_price = Some(98.0);
    position.atr_take_ratio_profit_price = Some(120.0);
    position.long_signal_take_profit_price = Some(110.0);

    let candle = CandleItem { o: 100.0, h: 105.0, l: 99.0, c: 102.0, v: 1.0, ts: 1, confirm: 1 };
    let risk = BasicRiskStrategyConfig { max_loss_percent: 0.05, ..Default::default() };

    let targets = compute_current_targets(&position, &candle, &risk);
    assert_eq!(targets.stop_loss, Some(98.0));
    assert_eq!(targets.take_profit, Some(110.0));
}

#[test]
fn compute_targets_prefers_tightest_stop_loss_and_nearest_tp_short() {
    let mut position = TradePosition::default();
    position.trade_side = TradeSide::Short;
    position.open_price = 100.0;
    position.position_nums = 1.0;
    position.signal_kline_stop_close_price = Some(106.0);
    position.move_stop_open_price = Some(103.0);
    position.atr_take_ratio_profit_price = Some(80.0);
    position.short_signal_take_profit_price = Some(90.0);

    let candle = CandleItem { o: 100.0, h: 101.0, l: 95.0, c: 97.0, v: 1.0, ts: 1, confirm: 1 };
    let risk = BasicRiskStrategyConfig { max_loss_percent: 0.05, ..Default::default() };

    let targets = compute_current_targets(&position, &candle, &risk);
    assert_eq!(targets.stop_loss, Some(103.0));
    assert_eq!(targets.take_profit, Some(90.0));
}
```

**Step 2: Run tests (expect failure)**

Run: `cargo test -p rust-quant-strategies compute_targets -- --nocapture`
Expected: FAIL (function not found / assertions fail).

**Step 3: Implement minimal shared logic**

In `crates/strategies/src/framework/backtest/risk.rs`:
- Add struct:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExitTargets {
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub stop_reason: Option<String>,
    pub take_reason: Option<String>,
}
```

- Add helper for max-loss stop price (reuse logic from `check_max_loss_stop`):

```rust
fn compute_effective_max_loss(position: &TradePosition, ctx: &ExitContext, base: f64, dynamic: bool) -> f64 { /* same logic as check_max_loss_stop */ }
```

- Add helper to select tightest stop loss:

```rust
fn select_tightest_stop(side: TradeSide, entry: f64, candidates: &[f64]) -> Option<f64> {
    let mut vals: Vec<f64> = candidates.iter().copied().filter(|v| v.is_finite()).collect();
    if vals.is_empty() { return None; }
    Some(match side {
        TradeSide::Long => *vals.iter().max_by(|a,b| a.partial_cmp(b).unwrap()).unwrap(),
        TradeSide::Short => *vals.iter().min_by(|a,b| a.partial_cmp(b).unwrap()).unwrap(),
    })
}
```

- Add helper to select nearest take-profit:

```rust
fn select_nearest_tp(side: TradeSide, entry: f64, candidates: &[f64]) -> Option<f64> {
    let filtered: Vec<f64> = candidates
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .filter(|v| match side { TradeSide::Long => *v > entry, TradeSide::Short => *v < entry })
        .collect();
    if filtered.is_empty() { return None; }
    Some(match side {
        TradeSide::Long => *filtered.iter().min_by(|a,b| a.partial_cmp(b).unwrap()).unwrap(),
        TradeSide::Short => *filtered.iter().max_by(|a,b| a.partial_cmp(b).unwrap()).unwrap(),
    })
}
```

- Implement `compute_current_targets`:

```rust
pub fn compute_current_targets(position: &TradePosition, candle: &CandleItem, risk: &BasicRiskStrategyConfig) -> ExitTargets {
    let ctx = ExitContext::new(position, candle);
    let effective_max_loss = compute_effective_max_loss(position, &ctx, risk.max_loss_percent, risk.dynamic_max_loss.unwrap_or(true));
    let max_loss_stop = ctx.stop_loss_price(effective_max_loss);

    let mut stop_candidates = vec![max_loss_stop];
    if let Some(px) = position.signal_kline_stop_close_price { stop_candidates.push(px); }
    if let Some(px) = position.move_stop_open_price { stop_candidates.push(px); }

    let stop_loss = select_tightest_stop(ctx.side, ctx.entry, &stop_candidates);

    let mut tp_candidates = Vec::new();
    if let Some(px) = position.atr_take_profit_level_3 { tp_candidates.push(px); }
    if let Some(px) = position.atr_take_ratio_profit_price { tp_candidates.push(px); }
    if let Some(px) = position.fixed_take_profit_price { tp_candidates.push(px); }
    match ctx.side {
        TradeSide::Long => if let Some(px) = position.long_signal_take_profit_price { tp_candidates.push(px); },
        TradeSide::Short => if let Some(px) = position.short_signal_take_profit_price { tp_candidates.push(px); },
    }
    let take_profit = select_nearest_tp(ctx.side, ctx.entry, &tp_candidates);

    ExitTargets {
        stop_loss,
        take_profit,
        stop_reason: None,
        take_reason: None,
    }
}
```

**Step 4: Update `check_max_loss_stop` to reuse helper**

Replace internal duplicated logic with `compute_effective_max_loss` to keep logic identical.

**Step 5: Export the function**

In `crates/strategies/src/framework/backtest/mod.rs`, add:
```rust
pub use risk::compute_current_targets;
pub use risk::ExitTargets;
```

**Step 6: Run tests**

Run: `cargo test -p rust-quant-strategies compute_targets -- --nocapture`
Expected: PASS.

**Step 7: Commit**

```bash
git add crates/strategies/src/framework/backtest/risk.rs crates/strategies/src/framework/backtest/mod.rs

git commit -m "feat(风控): 抽取实盘止盈止损目标"
```

---

### Task 3: Track and compare live TP/SL targets per strategy

**Files:**
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`
- Modify: `crates/services/src/strategy/live_decision.rs`
- Test: `crates/services/src/strategy/live_decision.rs`

**Step 1: Add a new live cache struct and map**

In `StrategyExecutionService` add:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
struct LiveExitTargets {
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    algo_ids: Vec<String>,
}
```

and add a new `DashMap<i64, LiveExitTargets>` field. Initialize it in `new()`.

**Step 2: Add epsilon compare helper**

```rust
fn approx_eq(a: Option<f64>, b: Option<f64>, eps: f64) -> bool { /* compare or both None */ }
```

**Step 3: Compute targets each K-line**

After `apply_live_decision` in `handle_live_decision`, if `state.trade_position.is_some()` call:

```rust
let targets = compute_current_targets(position, trigger_candle, &decision_risk);
```

and compare with cached `LiveExitTargets`. Only sync when `stop_loss` or `take_profit` differ beyond epsilon.

**Step 4: Add a small unit test for comparison logic**

In `crates/services/src/strategy/live_decision.rs` tests, add a test that verifies `approx_eq` behavior and that a change triggers “needs sync” flag (extracted helper).

**Step 5: Run tests**

Run: `cargo test -p rust-quant-services live_decision -- --nocapture`
Expected: PASS.

**Step 6: Commit**

```bash
git add crates/services/src/strategy/strategy_execution_service.rs crates/services/src/strategy/live_decision.rs

git commit -m "feat(实盘): 追踪并对比TP/SL目标"
```

---

### Task 4: Add OKX close-algo cancel & place helpers

**Files:**
- Modify: `crates/services/src/exchange/okx_order_service.rs`
- Modify: `crates/services/src/exchange/mod.rs`
- Test: `crates/services/tests/okx_simulated_order_flow.rs`

**Step 1: Add request builders (tests first)**

Add pure functions in `okx_order_service.rs` (near top):

```rust
fn build_cancel_close_algo_body(inst_id: &str, algo_ids: &[String]) -> serde_json::Value {
    serde_json::json!({
        "instId": inst_id,
        "algoIds": algo_ids,
    })
}

fn build_place_close_algo_body(
    inst_id: &str,
    mgn_mode: &str,
    side: &str,
    pos_side: &str,
    tp: Option<f64>,
    sl: Option<f64>,
) -> serde_json::Value {
    serde_json::json!({
        "instId": inst_id,
        "tdMode": mgn_mode,
        "side": side,
        "posSide": pos_side,
        "algoType": "conditional",
        "closeFraction": "1",
        "tpTriggerPx": tp.map(|v| format!("{:.8}", v)),
        "tpOrdPx": tp.map(|_| "-1".to_string()),
        "tpTriggerPxType": tp.map(|_| "last".to_string()),
        "slTriggerPx": sl.map(|v| format!("{:.8}", v)),
        "slOrdPx": sl.map(|_| "-1".to_string()),
        "slTriggerPxType": sl.map(|_| "last".to_string()),
    })
}
```

Add tests in `crates/services/tests/okx_simulated_order_flow.rs` to validate JSON field presence.

**Step 2: Implement OKX API calls**

In `OkxOrderService`, add:

```rust
pub async fn cancel_close_algos(&self, api: &ExchangeApiConfig, inst_id: &str, algo_ids: &[String]) -> Result<()> { /* POST /api/v5/trade/cancel-algos */ }

pub async fn place_close_algo(&self, api: &ExchangeApiConfig, inst_id: &str, mgn_mode: &str, side: &str, pos_side: &str, tp: Option<f64>, sl: Option<f64>) -> Result<()> { /* POST /api/v5/trade/order-algo */ }
```

Use `OkxClient::send_request` like in `OkxStopLossAmender` with the JSON bodies above.

**Step 3: Run tests**

Run: `cargo test -p rust-quant-services okx_simulated_order_flow -- --nocapture`
Expected: PASS (unit tests that check JSON only, not live API).

**Step 4: Commit**

```bash
git add crates/services/src/exchange/okx_order_service.rs crates/services/tests/okx_simulated_order_flow.rs

git commit -m "feat(交易所): 支持撤旧重挂TP/SL"
```

---

### Task 5: Wire TP/SL sync into live execution

**Files:**
- Modify: `crates/services/src/strategy/strategy_execution_service.rs`

**Step 1: Implement sync function**

Add method in `StrategyExecutionService`:

```rust
async fn sync_close_algos(
    &self,
    api: &ExchangeApiConfig,
    inst_id: &str,
    mgn_mode: &str,
    side: &str,
    pos_side: &str,
    tp: Option<f64>,
    sl: Option<f64>,
    existing_algo_ids: &[String],
) -> Result<()> { /* cancel + place */ }
```

**Step 2: Call sync after `apply_live_decision`**

- Get current position (from `state.trade_position`) and exchange position info (`get_positions`) for `mgn_mode` and `close_order_algo` IDs.
- If targets changed: call `cancel_close_algos` then `place_close_algo`.
- Update `live_exit_targets` cache.

**Step 3: Run tests**

Run: `cargo test -p rust-quant-services strategy_execution_service -- --nocapture`
Expected: PASS (or skip if tests require live API; log and proceed).

**Step 4: Commit**

```bash
git add crates/services/src/strategy/strategy_execution_service.rs

git commit -m "feat(实盘): K线级同步TP/SL"
```

---

### Task 6: Full regression check

**Step 1: Run focused tests**

Run:
```bash
cargo test -p rust-quant-strategies compute_targets -- --nocapture
cargo test -p rust-quant-services live_decision -- --nocapture
```
Expected: PASS.

**Step 2: Commit plan doc update (if any)**

```bash
git add docs/plans/2026-02-06-live-tp-sl-sync.md

git commit -m "docs(方案): 增加实盘TP/SL同步方案"
```

---

## Notes
- `cancel-algos` / `order-algo` endpoint names and request fields are based on OKX v5 conventions; verify against your account permissions. If OKX returns parameter errors, adjust in Task 4 before proceeding.
- `epsilon` can be configured via env var `LIVE_TP_SL_EPSILON` (default `1e-6`) to reduce unnecessary rehangs.
