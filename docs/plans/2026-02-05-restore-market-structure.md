# Restore MarketStructure Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore MarketStructure signal generation (including direction voting even with weight=0) so the backtest reproduces baseline 31.

**Architecture:** Reintroduce the MarketStructure indicator + signal wiring from the main branch into the refactored worktree. Add tests that assert MarketStructure parsing and direction voting, and verify `market_structure_value` appears in vegas signal output. Then re-run the backtest against the baseline window.

**Tech Stack:** Rust, indicators/strategies/orchestration crates, sqlx, MySQL backtest logs.

### Task 1: Add failing tests for MarketStructure presence + direction voting

**Files:**
- Modify: `crates/indicators/src/trend/signal_weight.rs`
- Modify: `crates/indicators/src/trend/vegas/signal.rs`

**Step 1: Write the failing tests**

`crates/indicators/src/trend/signal_weight.rs` (new tests in `#[cfg(test)]`):

```rust
#[test]
fn market_structure_signal_type_parses() {
    let parsed = serde_json::from_str::<SignalType>("\"MarketStructure\"");
    assert!(parsed.is_ok());
}

#[test]
fn market_structure_vote_applies_even_with_zero_weight() {
    let ms = serde_json::from_str::<SignalType>("\"MarketStructure\"").unwrap();
    let weights = SignalWeightsConfig {
        weights: vec![
            (SignalType::SimpleBreakEma2through, 1.0),
            (ms, 0.0),
        ],
        min_total_weight: 1.0,
    };

    let score = weights.calculate_score(vec![
        (
            SignalType::SimpleBreakEma2through,
            SignalCondition::PriceBreakout {
                price_above: true,
                price_below: false,
            },
        ),
        (
            ms,
            SignalCondition::MarketStructure {
                is_bullish: false,
                is_bearish: true,
            },
        ),
    ]);

    assert_eq!(score.signal_result, Some(SignalDirect::IsShort));
}
```

`crates/indicators/src/trend/vegas/signal.rs` (update test):

```rust
#[test]
fn vegas_signal_value_includes_market_structure() {
    let signal_value = VegasSignalValue::default();
    let json = serde_json::to_value(&signal_value).unwrap();
    assert!(json.get("market_structure_value").is_some());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -q market_structure`
Expected: FAIL (parsing `MarketStructure` fails or signal value missing).

### Task 2: Restore MarketStructure indicator + signal wiring

**Files:**
- Create: `crates/indicators/src/pattern/market_structure_indicator.rs`
- Modify: `crates/indicators/src/pattern/mod.rs`
- Modify: `crates/indicators/src/trend/signal_weight.rs`
- Modify: `crates/indicators/src/trend/vegas/config.rs`
- Modify: `crates/indicators/src/trend/vegas/indicator_combine.rs`
- Modify: `crates/indicators/src/trend/vegas/signal.rs`
- Modify: `crates/indicators/src/trend/vegas/strategy.rs`
- Modify: `crates/strategies/src/framework/backtest/indicators.rs`
- Modify: `crates/orchestration/src/infra/job_param_generator.rs`
- Modify: `crates/orchestration/src/infra/strategy_config.rs`

**Step 1: Restore MarketStructure indicator implementation**

Copy from main branch versions into this worktree:
- `crates/indicators/src/pattern/market_structure_indicator.rs`
- `crates/indicators/src/pattern/mod.rs` (export module)

**Step 2: Restore SignalType + condition handling**

In `crates/indicators/src/trend/signal_weight.rs`:
- Add `MarketStructure` back to `SignalType`.
- Add `SignalCondition::MarketStructure { is_bullish: bool, is_bearish: bool }`.
- Handle `MarketStructure` in `evaluate_condition` to emit `SignalDirect`.
- Update default weights list to include `MarketStructure` (match main).
- Update/remove tests that asserted MarketStructure removal.

**Step 3: Restore Vegas signal value + indicator combine**

In `crates/indicators/src/trend/vegas/signal.rs`:
- Add `market_structure_value: MarketStructureValue` to `VegasSignalValue`.

In `crates/indicators/src/trend/vegas/indicator_combine.rs`:
- Add optional `market_structure_indicator: Option<MarketStructureIndicator>`.

In `crates/indicators/src/trend/vegas/config.rs`:
- Add `MarketStructureConfig` and `Default` (use main branch).

In `crates/indicators/src/trend/vegas/strategy.rs`:
- Add `market_structure_signal` field to `VegasStrategy`.
- When enabled, create `MarketStructureIndicator` in `build_indicator_combine`.
- In `generate_signal_conditions`, add `SignalCondition::MarketStructure` derived from `market_structure_value`.

In `crates/strategies/src/framework/backtest/indicators.rs`:
- Wire `market_structure_indicator.next` into `VegasSignalValue.market_structure_value`.

In orchestration config conversion:
- `crates/orchestration/src/infra/job_param_generator.rs`: add `market_structure_signal` field to builder + pass into `VegasStrategy`.
- `crates/orchestration/src/infra/strategy_config.rs`: plumb `market_structure_signal` from config into param.

**Step 4: Run tests to verify they pass**

Run: `cargo test -q market_structure`
Expected: PASS.

### Task 3: Re-run backtest and compare to baseline 31

**Files:**
- None (runtime verification)

**Step 1: Run backtest**

```bash
IS_BACK_TEST=1 IS_RUN_REAL_STRATEGY=0 IS_OPEN_SOCKET=0 IS_RUN_SYNC_DATA_JOB=0 \
ENABLE_SPECIFIED_TEST_VEGAS=true ENABLE_RANDOM_TEST_VEGAS=false \
ENABLE_SPECIFIED_TEST_NWE=false ENABLE_RANDOM_TEST_NWE=false \
TIGHTEN_VEGAS_RISK=0 DB_HOST='mysql://root:example@localhost:33306/test?ssl-mode=DISABLED' \
cargo run --bin rust_quant
```

**Step 2: Query latest backtest metrics**

```bash
.venv/bin/python - <<'PY'
import pymysql
conn = pymysql.connect(host='127.0.0.1', port=33306, user='root', password='example', database='test', charset='utf8mb4')
try:
    with conn.cursor() as cur:
        cur.execute("select id,win_rate,profit,final_fund,sharpe_ratio,max_drawdown,open_positions_num,kline_start_time,kline_end_time,kline_nums from back_test_log order by id desc limit 1")
        print(cur.fetchone())
finally:
    conn.close()
PY
```

Expected: metrics align with baseline 31 (same window + comparable stats).

### Task 4: Commit

**Step 1: Commit changes**

```bash
git add crates/indicators/src crates/strategies/src crates/orchestration/src
git commit -m "refactor(回测): 恢复MarketStructure信号"
```

