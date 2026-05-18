# Execution Worker Risk Plan Contract

This handoff is scoped to the `rust_quant` execution worker. It documents the
worker-side fail-closed boundary for Web `execute_signal` tasks that require a
protective stop-loss.

## Current Worker Gate

Before any dry-run or live `place_order` call, the worker now inspects the
merged task payload. The merge keeps top-level Web fields, nested `payload_json`,
and `execution` fields in the same way as normal order mapping.

If an `execute_signal` payload declares any of these flags:

- `risk_plan.protective_stop_loss_required = true`
- `risk_plan.stop_loss_required = true`
- `execution.protective_stop_loss_required = true`
- `execution.stop_loss_required = true`
- top-level `protective_stop_loss_required = true`
- top-level `stop_loss_required = true`

then the worker requires a positive finite `selected_stop_loss_price` from:

- `risk_plan.selected_stop_loss_price`
- top-level `selected_stop_loss_price`
- `execution.selected_stop_loss_price`

News-signal tasks are also treated as protective stop-loss-required tasks even
when an upstream payload misses the explicit boolean flag. The worker currently
recognizes this through either boundary:

- `execution_tasks.news_signal_id IS NOT NULL`
- payload `source_signal_type = news_event` or `news`

This prevents a news execution task from becoming executable just because the
payload shape drifted and omitted `protective_stop_loss_required`.

The worker also validates any explicit direction from:

- `risk_plan.direction`
- top-level `direction`
- top-level `position_side`
- top-level `side`
- top-level `signal_type`

Allowed direction values are `long`, `buy`, `open_long`, `short`, `sell`, and
`open_short`.

If the stop-loss price is missing, the explicit direction is invalid, or an
available entry price proves the stop-loss is on the wrong side of the entry,
the worker returns `ExecutionTaskReportRequest::failed(...)` with
`risk_contract.place_order_allowed = false`. This happens before
`place_order_with_audit`, so no exchange request is created for that task.

For news-signal failures the raw risk-contract evidence includes whichever
source marker is available, for example `risk_contract.news_signal_id` or
`risk_contract.source_signal_type`. This evidence is diagnostic only; it does
not authorize replay, rebind, close, cancel, or any exchange mutation.

## Still Missing

This gate only prevents unsafe order placement. It does not yet attach
stop-loss or take-profit instructions to the exchange order request.

After a live open order is confirmed as filled, the worker also keeps the task
out of a fully completed state when a protective stop-loss was required but no
protection order sync has been confirmed. The report contract becomes:

- `execution_status = pending_protection_sync`
- `order_status` keeps the exchange order status, for example `FILLED`
- `error_message = protective stop-loss required but protection order sync is not confirmed`
- `raw_payload_json.protection_sync.status = pending_protection_sync`
- `raw_payload_json.protection_sync.protective_order_confirmed = false`
- `raw_payload_json.protection_sync.exchange_protective_order_supported = false`
- `raw_payload_json.protection_sync.place_order_allowed = false`

This is intentionally a contract-only closure. It prevents Web/Admin from
mistaking a filled open order for a fully protected completed task, while also
making clear that the current exchange SDK path has not placed or confirmed a
real protective stop-loss order.

The next implementation has two viable paths:

1. Extend `OrderPlacementRequest`.
   Add optional `stop_loss`, `take_profit`, and source metadata fields to the
   request. Each exchange adapter then maps those fields to native attached
   algo orders or rejects the request with a clear unsupported-feature error.
   This is cleaner for exchanges that can atomically place an order with
   protection.

2. Add a second-stage protection task.
   Keep the open order request unchanged. After the open order is confirmed,
   generate a separate protection-sync task that places or updates stop-loss
   and take-profit orders. This is safer for exchanges where protective orders
   must be submitted after fill confirmation, but it needs idempotent protection
   order keys and recovery when the open succeeds but protection sync fails.

For production live trading, the worker should require one of these paths before
allowing `protective_stop_loss_required` tasks to become fully executable.
