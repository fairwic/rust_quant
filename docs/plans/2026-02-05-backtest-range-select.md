# Backtest Range Selection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ensure specified backtests honor `kline_start_time`/`kline_end_time` when loading candles so results match the baseline window.

**Architecture:** Derive a candle time range from `ParamMergeBuilder` configs, pass it to the candle query via `SelectTime`, and unit-test the range derivation logic.

**Tech Stack:** Rust, sqlx, rust_quant orchestration/services/market.

### Task 1: Add failing tests for backtest range derivation

**Files:**
- Modify: `crates/orchestration/src/backtest/runner.rs`
- Test: `crates/orchestration/src/backtest/runner.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn derive_select_time_uses_min_start_and_max_end() {
    let params = vec![
        ParamMergeBuilder::build().kline_start_time(100).kline_end_time(200),
        ParamMergeBuilder::build().kline_start_time(50).kline_end_time(300),
    ];

    let select_time = derive_select_time(&params).expect("select_time should exist");

    assert_eq!(select_time.start_time, 50);
    assert_eq!(select_time.end_time, Some(300));
    assert!(matches!(select_time.direct, TimeDirect::AFTER));
}

#[test]
fn derive_select_time_returns_none_when_unset() {
    let params = vec![ParamMergeBuilder::build()];
    assert!(derive_select_time(&params).is_none());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -q derive_select_time`
Expected: FAIL with assertion error (function returns `None` or wrong range).

### Task 2: Implement range derivation + pass SelectTime into candle load

**Files:**
- Modify: `crates/orchestration/src/backtest/runner.rs`
- Modify: `crates/orchestration/src/backtest/executor.rs`

**Step 1: Write minimal implementation**

```rust
fn derive_select_time(params: &[ParamMergeBuilder]) -> Option<SelectTime> { /* ... */ }
```

- Use min non-zero start and max non-zero end across params.
- If both set: `TimeDirect::AFTER` with `end_time=Some(max_end)`.
- If only start set: `TimeDirect::AFTER` with `end_time=None`.
- If only end set: `TimeDirect::BEFORE` with `start_time=end`.

Update `BacktestExecutor::load_and_convert_candle_data` to accept `Option<SelectTime>` and pass it to `get_candle_data_confirm`.
Update call sites:
- Random tests: pass `None`.
- Specified tests: compute `derive_select_time` from params and pass `Some(select_time)` when available.

**Step 2: Run tests to verify they pass**

Run: `cargo test -q derive_select_time`
Expected: PASS.

### Task 3: Re-run backtest and compare to baseline

**Files:**
- None (runtime verification)

**Step 1: Run backtest**

Run:
```bash
IS_BACK_TEST=1 IS_RUN_REAL_STRATEGY=0 IS_OPEN_SOCKET=0 IS_RUN_SYNC_DATA_JOB=0 \
ENABLE_SPECIFIED_TEST_VEGAS=true ENABLE_RANDOM_TEST_VEGAS=false \
ENABLE_SPECIFIED_TEST_NWE=false ENABLE_RANDOM_TEST_NWE=false \
TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' \
cargo run --bin rust_quant
```

**Step 2: Query latest backtest metrics**

Run:
```bash
docker exec -i mysql mysql -uroot -pexample test -e "select id,win_rate,profit,final_fund,sharpe_ratio,max_drawdown,open_positions_num,kline_start_time,kline_end_time from back_test_log order by id desc limit 1;"
```

Expected: `kline_start_time`/`kline_end_time` match baseline window, metrics align with baseline ID 31.

### Task 4: Commit

**Step 1: Commit changes**

```bash
git add crates/orchestration/src/backtest/runner.rs crates/orchestration/src/backtest/executor.rs
git commit -m "refactor(回测): 按配置时间范围加载K线"
```

