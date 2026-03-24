# Vegas Filter Refactor No Regression Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor duplicated Vegas post-filter logic without changing backtest behavior or output metrics.

**Architecture:** Keep the existing filter order, env-flag semantics, and returned reason strings unchanged. Only extract duplicated predicates into shared helpers and keep call sites behaviorally identical. Validate with a before/after backtest run against the same target set and compare resulting `back_test_log` rows.

**Tech Stack:** Rust workspace, Vegas strategy, sqlx/MySQL backtest log, Cargo.

### Task 1: Freeze baseline

**Files:**
- Modify: none
- Inspect: `/Users/mac2/onions/rust_quant/crates/rust-quant-cli/src/app/bootstrap.rs`
- Inspect: `/Users/mac2/onions/rust_quant/crates/indicators/src/trend/vegas/strategy.rs`

**Step 1: Read the current backtest target selection**

Run: `nl -ba /Users/mac2/onions/rust_quant/crates/rust-quant-cli/src/app/bootstrap.rs | sed -n '1,90p'`

**Step 2: Record current database max id**

Run: `mysql -h 127.0.0.1 -P 33306 -u root -pexample test -e "select max(id) as max_id from back_test_log;"`

**Step 3: Run baseline backtest with explicit target set**

Run: `TIGHTEN_VEGAS_RISK=0 BACKTEST_ONLY_INST_IDS=ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' cargo run -p rust-quant-cli`

**Step 4: Query newly inserted rows**

Run: `mysql -h 127.0.0.1 -P 33306 -u root -pexample test -e "select id,inst_type,time,win_rate,profit,final_fund,sharpe_ratio,max_drawdown from back_test_log where id > <baseline_max_id> order by id;"`

### Task 2: Extract duplicated predicates only

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/indicators/src/trend/vegas/strategy.rs`

**Step 1: Extract shared helper for deep negative hammer long classification**

Keep:
- identical thresholds
- identical env flags
- identical block/protect outputs at call sites

**Step 2: Extract shared helper for repair long classification**

Keep:
- identical predicate fields
- identical reason strings
- identical stop-loss and filter behavior

**Step 3: Do not change filter ordering**

Keep the existing sequence in `get_trade_signal`.

### Task 3: Add minimal no-regression unit coverage

**Files:**
- Modify: `/Users/mac2/onions/rust_quant/crates/indicators/src/trend/vegas/strategy.rs`

**Step 1: Add tests proving extracted helpers match previous behavior**

Target:
- deep negative hammer classifier
- repair long classifier

**Step 2: Run targeted strategy tests**

Run: `cargo test -p rust-quant-indicators vegas::strategy -- --nocapture`

### Task 4: Re-run backtest and compare

**Files:**
- Modify: none

**Step 1: Run the same backtest command again after refactor**

Run: `TIGHTEN_VEGAS_RISK=0 BACKTEST_ONLY_INST_IDS=ETH-USDT-SWAP,BTC-USDT-SWAP,SOL-USDT-SWAP DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' cargo run -p rust-quant-cli`

**Step 2: Compare metrics pairwise**

Compare:
- `inst_type`
- `time`
- `win_rate`
- `profit`
- `final_fund`
- `sharpe_ratio`
- `max_drawdown`

**Step 3: If any metric differs, stop and treat refactor as behavior-changing**

Do not keep the refactor unless results are identical.
