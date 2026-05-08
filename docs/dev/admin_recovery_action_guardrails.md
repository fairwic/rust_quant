# Admin Recovery Action Guardrails

This Phase 48 contract defines the safety boundary for the future Admin
recovery-action workbench. The health and diagnosis pages remain read-only
inputs. A UI must not become an execution surface unless the action class,
preview, confirmation, audit, idempotency, redaction, rate limit, and RBAC
requirements below are implemented together.

## Non-Goals And Hard Boundaries

The initial workbench must not read `.env`, must not call local services, must
not call signed/account/order/position endpoint paths, must not call
lease/report/mutate task endpoint paths, must enforce
`no lease/report/mutate task endpoint`, must not create a real order, must not
change leverage, margin mode, or position mode, and must not use or touch
`LINKUSDT`.

Health artifacts and readiness panels may suggest an action class, but they may
not execute recovery directly. Any missing guardrail means the action stays
hidden or disabled. The default state for every non-read-only action is
`disable by default` until the workbench can prove the required preview,
confirmation, audit, and RBAC checks.

## Action Classes

| Class | Meaning | Initial UI state |
| --- | --- | --- |
| `read_only` | Show existing health, audit, correlation, and dry-run preview facts. No mutation and no external side effect. | Enabled for viewers with artifact access. |
| `guarded_recovery` | Mutates internal business or admin state but does not call signed exchange, order, position, lease, or report paths. | Disabled until preview, reason, audit, idempotency, rate limit, and RBAC are implemented. |
| `manual_approval` | Requires a second approver or runbook handoff because impact is broad, ambiguous, or may affect live readiness. | Disabled until approval workflow and audit chain exist. |
| `disabled_until_live_order_closed` | Any action that might conflict with an open or unknown live order, position, task lease, or close workflow. | Disabled while `OPEN_LIVE_ORDER_PRESENT` or `live_order_not_closed` is true or unknown. |

## Global Action Requirements

Every `guarded_recovery` or `manual_approval` action must collect and persist:

- `reason`: operator-entered reason, not a free-form secret dump.
- `impact_objects`: typed IDs such as user id, strategy slug, symbol, task id,
  notification id, news id, analysis result id, or admin operation log id.
- `dry_run_preview`: immutable preview of rows or messages that would change,
  with before/after summaries and no raw payload.
- `idempotency_key`: stable key derived from action type, impact object ids, and
  requested state so browser refresh or retry cannot duplicate the action.
- `rate_limit`: per action, per operator, and per impact object throttles.
- `rbac_role`: minimum role and permission scope evaluated on the server.
- `operator_confirmed_at`: explicit confirmation timestamp after preview.
- `audit_log`: append-only admin operation log with request id, outcome,
  redacted preview hash, idempotency key, and approver id when required.

The server must reject direct mutation requests that skip preview or reuse a
stale preview token. Preview tokens should expire quickly and should bind to the
same operator, action type, impact objects, and idempotency key.

## Disabled And Read-Only Preview Server Contract

The first service-side contract is a disabled/read-only preview surface. It is
safe to implement before any recovery mutation exists because GET preview
endpoints are read-only and may use only stored health artifacts plus read-only
DB views.

Disabled actions return `enabled: false`, `disabled_reason_code`, action class,
required role, impact object locators, and a redacted explanation. They must not
return a runnable command, mutation URL, raw SQL, raw provider payload, or any
exchange endpoint.

Read-only preview responses may include `preview_token`, `preview_expires_at`,
`preview_hash`, `redacted_preview`, `idempotency_key`, and `audit_log` locator
metadata. These fields are descriptive until a later confirmed mutation API is
implemented; no mutation before confirmation is allowed.

The server performs server-side RBAC for every preview and disabled action
response. Frontend hidden or disabled controls are not authorization. Preview
tokens bind to operator id, action, action class, impact objects,
idempotency_key, and preview hash, and they expire before they can be reused as
stale authority.

## Server Acceptance Conditions

A service implementation must satisfy these conditions before any action is
enabled:

- disabled actions serialize as `enabled: false` with a stable
  `disabled_reason_code`.
- GET preview endpoints are read-only and must not call live probes, exchange
  signed/order/position paths, or lease/report/mutate task paths.
- server-side RBAC rejects unauthorized preview and recovery attempts.
- no mutation before confirmation and no mutation before a fresh preview token.
- same `idempotency_key` must not execute twice; retries return the previous
  outcome or a duplicate-suppressed result.
- `audit_log` records preview, confirmation, execution outcome, failure code,
  operator, approver when required, redacted preview hash, and idempotency key.
- dry-run preview must not contain raw payload, secrets, full provider content,
  database URLs, API keys, passphrases, ciphers, or `LINKUSDT`.
- `redacted_preview` stores counts, IDs, status enums, timestamps, hashes, and
  short reason codes only.
- rate limit decisions are evaluated on the server and are auditable.
- any missing preview, audit, idempotency, RBAC, rate-limit, or redaction
  requirement keeps the action disabled.

## Redaction Contract

Recovery action preview, audit, UI, logs, metrics, exported artifacts, and error
messages must redact or omit these fields and markers:

- `.env`
- `database_url`
- `api_key`
- `api_secret`
- `passphrase`
- `cipher`
- `request_payload`
- `response_payload`
- `raw_payload`
- `signed_endpoint`
- `account_endpoint`
- `order_endpoint`
- `position_endpoint`
- provider raw request, provider raw response, full news body, buyer email,
  administrator username, target original value, access token, refresh token,
  webhook URL, phone, and address
- `LINKUSDT`

Allowed context is limited to stable IDs, counts, status enums, timestamps,
hashes, short reason codes, and already-redacted display labels. If validation
finds a sensitive marker, the UI may show the marker code and artifact name but
must not echo the blocked text.

## Live Order Boundary

`OPEN_LIVE_ORDER_PRESENT` means there is any known live order, position, pending
close, leased execution task, missing order result, or unknown close status in
the selected user/strategy/symbol/task scope. `live_order_not_closed` means the
workbench cannot prove from read-only facts that the live order chain is closed.

While either condition is true or unknown:

- task retry, task release, strategy/user/symbol unpause, reanalysis that can
  submit signals, and symbol sync promotion are `disabled_until_live_order_closed`.
- pause actions that only prevent new work may remain available as
  `manual_approval` when they do not interrupt a close workflow.
- notification retry may remain `guarded_recovery` only when it sends a
  previously redacted notification and cannot trigger execution.
- the workbench must show "manual approval" for any ambiguous state.
- the implementation must enforce no signed/order/position endpoint, no
  lease/report/mutate task endpoint, and no real order.

## Initial Recovery Actions

| Action | Initial class | Required preview | Initial rule |
| --- | --- | --- | --- |
| `notification_retry` | `guarded_recovery` | Notification id, recipient channel type, last outcome, retry count, redacted message hash. | Allowed only for already-rendered redacted notifications; never resend raw provider payload; rate limit per recipient and notification id. |
| `task_retry` | `disabled_until_live_order_closed` | Task id, task status, attempt count, last error class, order result and trade record presence. | Start disabled. Enable later only for failed internal tasks with no open live order, no active lease, and an idempotent retry key. |
| `task_release` | `disabled_until_live_order_closed` | Task id, lease owner, lease age, worker heartbeat, attempt state, close/order chain state. | Start disabled. Later requires stale lease proof, second confirmation, and audit; never call lease/report endpoints from Admin. |
| `pause_user` | `manual_approval` | User id, active subscription/task counts, open order scope, affected strategies. | Can only pause new task creation; unpause is disabled while live order state is unknown. |
| `pause_strategy` | `manual_approval` | Strategy slug, active combo/task counts, open order scope, latest health alerts. | Can only pause new tasks for the strategy; must not cancel or close existing work. |
| `pause_symbol` | `manual_approval` | Exchange, symbol, active task counts, open order scope, latest symbol health. | Can only pause new tasks. `LINKUSDT` target is blocked in the initial workbench. |
| `manual_ai_reanalysis` | `manual_approval` | News id, analysis result id, prompt key/version, provider health, signal submission setting. | Analysis-only preview first. Any mode that submits a signal or execution task is disabled until live order state is closed and separately approved. |
| `symbol_sync` | `manual_approval` | Exchange, symbol filter, current stored symbol count, proposed additions/updates/deletions. | Preview must be read-only first. Promotion must use public metadata only, be rate limited, and stay blocked for `LINKUSDT` in the initial workbench. |

## RBAC Matrix

| Action | Minimum `rbac_role` | Approval |
| --- | --- | --- |
| `read_only` diagnosis and previews | `viewer` | None |
| `notification_retry` | `operator` | Single confirmation |
| `task_retry` | `senior_operator` | Single confirmation after live-order closed proof |
| `task_release` | `senior_operator` | Second approver required |
| `pause_user` | `risk_operator` | Second approver required for production |
| `pause_strategy` | `risk_operator` | Second approver required for production |
| `pause_symbol` | `risk_operator` | Second approver required for production |
| `manual_ai_reanalysis` | `ai_operator` | Second approver when signal submission is enabled |
| `symbol_sync` | `platform_operator` | Second approver for production promotion |

RBAC is enforced by the server. Frontend hiding is only a convenience and must
not be treated as authorization.

## Audit Event Contract

Every non-read-only action writes one append-only `audit_log` event before and
after execution:

- `request_id`
- `action`
- `action_class`
- `reason`
- `impact_objects`
- `rbac_role`
- `dry_run_preview_id`
- `dry_run_preview_hash`
- `idempotency_key`
- `rate_limit_bucket`
- `operator_id`
- `operator_confirmed_at`
- `approver_id`
- `outcome`
- `failure_code`
- `created_at`

Audit payloads must store redacted summaries and hashes, not raw payloads or
secrets. The same `idempotency_key` may return the previous outcome, but it must
not execute the action twice.

## Admin Workbench Starting Rules

The first Admin recovery workbench should ship in this order:

1. Render all actions from this contract as disabled controls with reasons from
   health artifacts and readiness summaries.
2. Implement `read_only` previews using stored artifacts and read-only DB views
   only.
3. Implement `notification_retry` as the first `guarded_recovery` action because
   it can be constrained to redacted message replay and cannot create orders.
4. Keep `task_retry`, `task_release`, `manual_ai_reanalysis` with signal
   submission, and `symbol_sync` promotion disabled until live-order closure
   proof, idempotency, audit, rate limiting, and RBAC are implemented server
   side.
5. Keep pause actions limited to preventing new work; do not use them to close,
   cancel, reduce, or otherwise manage live orders.
