# quant_core Execution Boundaries

`rust_quant` owns the execution worker and the local `quant_core` database. It must not become the C-side commerce system. `rust_quan_web` remains the source of truth for users, subscriptions, encrypted API credentials, execution tasks, order results, and user trade records.

## quant_core Owns

| Area | Purpose | Example tables |
| --- | --- | --- |
| Market data cache | Exchange-normalized candles, tickers, funding, open interest, and local replay inputs used by strategies and backtests. | `market_candles`, `market_snapshots` |
| Strategy signal state | Strategy-generated signal snapshots, filter reasons, indicator snapshots, and runtime state. | `strategy_configs`, `risk_configs`, `indicator_snapshots`, `strategy_signals`, `strategy_run_states` |
| Worker checkpoint | Idempotency and resume state for `rust_quant` jobs, including the last leased task id/time and polling cursors. | `execution_worker_checkpoints` |
| Exchange request audit | Local technical audit for requests sent through `crypto_exc_all`: exchange, symbol, request id, latency, status, error, redacted payload. | `exchange_request_audit_logs` |
| Backtest and simulation | Backtest logs/details and dry-run execution artifacts that are not user-facing business facts. | `backtest_runs`, `backtest_results`, `backtest_trades` |

The first Postgres target DDL for these tables lives at `sql/postgres_quant_core.sql`.

## rust_quan_web Owns

| Area | Purpose | Current tables |
| --- | --- | --- |
| C-side identity and entitlement | Buyer profile, membership, subscription status, and strategy access. | `users`, `memberships`, `strategy_combo_subscriptions` |
| User exchange credentials | Encrypted API key material and validation status. `rust_quant` may fetch decrypted material for a single execution attempt, but must not persist plaintext in `quant_core`. | `user_api_credentials` |
| Execution task queue | Pending/leased/completed task facts created from user subscriptions and news signals. | `execution_tasks`, `execution_task_attempts` |
| User-facing order result | Business record of submitted/filled/failed orders and user trade history. | `exchange_order_results`, `user_trade_records` |
| Notifications | Delivery facts for email/SMS/in-app notifications. | `notification_logs`, `combo_signal_delivery_logs` |

## Worker Contract

1. `rust_quant` leases tasks from `rust_quan_web`.
2. For live orders, `rust_quant` resolves the user's exchange config from `rust_quan_web` for that attempt only.
3. `rust_quant` calls `crypto_exc_all` to fetch market data or submit/cancel orders.
4. `rust_quant` writes only technical checkpoints/audits to `quant_core`.
5. `rust_quant` reports execution/order results back to `rust_quan_web`; `rust_quan_web` writes user-facing order and trade facts.

Dry-run mode follows the same lease/report flow but returns a simulated order ack and does not require user exchange credentials.

## Local Startup Smoke Boundary

`scripts/dev/run_live_strategy_quant_core_smoke.sh` is the bounded local check for the realtime-strategy startup path:

- `CANDLE_SOURCE=quant_core` reads startup warmup and non-WebSocket snap candles from `quant_core` Postgres sharded candle tables.
- `STRATEGY_CONFIG_SOURCE=quant_core` loads enabled configs from `quant_core.public.strategy_configs`.
- `STRATEGY_SIGNAL_DISPATCH_MODE=web` prevents the CLI from directly submitting exchange orders; any generated signal is handed to `rust_quan_web` as an execution task.
- `IS_OPEN_SOCKET=false` and `EXIT_AFTER_REAL_STRATEGY_ONESHOT=true` keep the smoke one-shot instead of a long-running realtime process.
- `EXECUTION_WORKER_DRY_RUN=true` documents the intended local execution boundary, but the script does not start the execution worker.

This smoke validates startup wiring and data-source selection. It does not prove WebSocket runtime behavior, exchange order placement, or user-facing order settlement.
