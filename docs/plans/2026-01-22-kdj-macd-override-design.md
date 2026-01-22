# KDJ Override for MACD Filter (Vegas Strategy)

## Goal
Introduce an experimental override that allows a trade to pass the MACD falling-knife filter when KDJ K-line momentum is rising. This targets specific missed profitable signals (e.g., 2025-10-27/10-13/07-05 in backtest 68) without turning KDJ into a primary entry signal.

## Scope and Data Flow
- Scope is limited to Vegas strategy signal filtering.
- KDJ is calculated from `CandleItem` slices inside `VegasStrategy::get_trade_signal`.
- When MACD would filter a signal, check KDJ K-line slope. If `K_now > K_prev`, allow the signal to proceed and mark the override.
- KDJ values are added into `VegasIndicatorSignalValue` and serialized into `signal_result.single_value` for later inspection in `back_test_detail.signal_value`.

## Component Changes
- `crates/indicators/src/momentum/kdj.rs`: add a CandleItem-based KDJ calculator that returns the most recent K/D/J values (or the last two K values).
- `crates/indicators/src/trend/vegas/signal.rs`: extend `VegasIndicatorSignalValue` with a `kdj_value` field.
- `crates/indicators/src/trend/vegas/strategy.rs`: compute KDJ in the signal pipeline and apply the override only when MACD falling-knife triggers; record `MACD_KDJ_OVERRIDE_LONG/SHORT` in `filter_reasons` when used.

## Algorithm (Override Rule)
- Parameters: `period=9`, `signal_period=3`.
- If MACD would filter:
  - If KDJ K-line is rising (`K_now > K_prev`), do not cancel `should_buy/should_sell` and log an override reason.
  - Otherwise keep the MACD filter decision.

## Edge Cases and Error Handling
- If there are fewer than `period` candles, KDJ is unavailable and no override is applied.
- If K values are NaN/inf, treat as invalid and do not override.

## Testing and Validation
- Unit test for KDJ slope detection on synthetic candles.
- Behavior test to ensure MACD filter is overridden only when KDJ K rises.
- Backtest 68 comparison: confirm the three target signals are released and measure PnL, win rate, and drawdown.
- Query `filtered_signal_log` for `MACD_KDJ_OVERRIDE_*` to quantify overrides and shadow PnL.

## Rollout
- Start with hardcoded override for the experiment.
- If results improve, add a configuration flag to `macd_signal` (default off) before enabling in production configs.
