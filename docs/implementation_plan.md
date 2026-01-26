# Implementation Plan - Restore Vegas Strategy Regression

## Goal Description

Restore the `min_trend_move_pct` parameter and associated logic in the Vegas Strategy. This feature was introduced to fix logic for Backtest ID 71 and was present in the high-performing Backtest ID 80, but is currently missing from the codebase. Its absence is likely the cause of the performance regression.

## User Review Required

> [!IMPORTANT]
> This is a restoration of lost logic. The default value will be set to `0.08` (8%) as per Iteration Log and ID 80 config.

## Proposed Changes

### [Crates/Indicators]

#### [MODIFY] [config.rs](file:///Users/mac2/onions/rust_quant/crates/indicators/src/trend/vegas/config.rs)

- Add `min_trend_move_pct` field to `FibRetracementSignalConfig` struct.
- Set default value to `0.08` in `Default` impl or via helper function.

#### [MODIFY] [strategy.rs](file:///Users/mac2/onions/rust_quant/crates/indicators/src/trend/vegas/strategy.rs)

- In `generate_signal` (or relevant main logic), locate the `strict_major_trend` filtering block.
- Calculate the swing move percentage using `swing_high` and `swing_low` from `fib_retracement_value`.
- Add a condition: Apply strict major trend filtering **ONLY** if the swing move percentage is greater than `min_trend_move_pct`.

## Verification Plan

### Automated Tests

- Run `cargo check` and `cargo build` to ensure compilation.
- I will verify the logic by running a backtest using the ID 80 configuration (or ensuring defaults match ID 80) and observing if the results align closer to ID 80 (Profit ~2480, Sharpe ~1.8).

### Manual Verification

- We will trigger the specific regression case (ID 71 or 15655 mentioned in log) if possible, or simply rely on the aggregate backtest metrics improving back to baseline.
