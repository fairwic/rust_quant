# ICT Structure Breakout (Rolling High/Low) Design

## Goal
Add an independent ICT/SMC-style structure breakout indicator that can directly open trades when aligned with the 4H EMA major trend. This must not change existing MarketStructure logic or weights.

## Scope
- New indicator using rolling high/low structure points (no right-side confirmation).
- Swing BOS only for direct-open signals.
- EMA major trend alignment required.
- Configurable and toggleable via strategy config.
- Preserve existing MarketStructure indicator and SignalType weights.

## Non-Goals
- Replace or modify existing MarketStructure indicator behavior.
- Add CHOCH or internal BOS signals to direct-open logic.
- Use higher-timeframe data (1D) for trend alignment.

## Algorithm
Inputs
- `swing_length = 12`
- `threshold_pct = 0.0`
- Rolling window uses previous `swing_length` candles only (exclude current).

State
- Rolling window of last N candles.
- `last_bos_dir` and `last_bos_level` to prevent repeated BOS on same level.

Steps per candle
1) Compute `swing_high` and `swing_low` from the previous window.
2) BOS checks using current close:
   - `close > swing_high * (1 + threshold_pct)` -> `swing_bullish_bos = true`.
   - `close < swing_low * (1 - threshold_pct)` -> `swing_bearish_bos = true`.
3) Only count a BOS if previous close had not already crossed that level.
4) Update rolling window with current candle.

## Strategy Integration
- New config: `ict_structure_breakout_signal` in Vegas strategy config.
- New signal value added to `VegasIndicatorSignalValue`.
- New indicator added to `IndicatorCombine` and `get_multi_indicator_values`.
- In `VegasStrategy.get_trade_signal`, if `direct_open = true` and BOS aligns with EMA major trend, override `signal_direction`.

Direction filter
- Short only when `ema_values.is_short_trend = true`.
- Long only when `ema_values.is_long_trend = true`.

## Configuration
New config struct (Vegas)
- `is_open: bool`
- `swing_length: usize`
- `threshold_pct: f64`
- `direct_open: bool`

Defaults
- `is_open = true`
- `swing_length = 12`
- `threshold_pct = 0.0`
- `direct_open = true`

## Observability
- Indicator values recorded in `signal_value` snapshot.
- BOS signals visible in debug output (via `vegas_kline_debug`).

## Test Plan
- Unit test: rolling high/low BOS detection with synthetic candles.
- Regression check: run `vegas_kline_debug` on `back_test_id=40` at `2026-01-29 08:00:00` and `20:00:00` to confirm BOS behavior and EMA alignment.

## Rollout
- Ship behind config toggle.
- Validate on latest backtest then enable by default if results match expectations.
