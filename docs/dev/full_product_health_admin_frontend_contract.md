# Admin / Frontend Consumption Contract

This document defines the small UI-facing contract for full-product health
artifacts. It narrows how Admin and frontend screens should read the existing
schema, summary, validation, and Markdown outputs without adding live probes.

## Primary Artifacts

Admin should consume stored artifacts produced by `run_full_product_health_ci.sh`.
It should not shell out from a request handler.

- `full-product-health-summary.json`: primary dashboard input.
- `full-product-health.json`: drill-down input for section diagnosis.
- `full-product-health-validation.json`: upload gate and safety finding input.
- `full-product-health.md`: operator-readable detail link only.

Use `validate_full_product_health_artifacts.sh` before publishing or storing a
new artifact set. Report-only lanes may use `FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never`
or `FAIL_ON_STATUS=never`, but that must not make a non-ready state look ready.

## Stored Artifact API Contract

Future Admin should read health artifacts through a stored-artifact endpoint,
not by running local commands from a request handler:

- GET `/admin/quant/full-product-health/latest`
- response fields: `artifactSetId`, `storedAt`, `summary`, `validation`,
  `sourceGeneratedAt`, `markdownUrl`, `fullArtifactUrl`, `ready`, `stale`,
  `staleReason`, `artifactSlaSeconds`, `operatorMetadata`, and `redaction`
- `summary` is the stored `full-product-health-summary.json` payload.
- `validation` is the stored `full-product-health-validation.json` payload,
  reduced to stable status, summary, and finding locator fields.
- `markdownUrl` points to the operator-readable Markdown artifact.
- `fullArtifactUrl` points to the full JSON artifact for drill-down.
- `ready` is computed from stored summary and validation fields only.
- `stale` is true when `storedAt`, `generated_at`, or `source_generated_at`
  exceeds the product freshness SLA.
- `redaction` records whether the stored set passed sensitive-marker
  validation and must not contain source text, raw payload, or secrets.

The handler safety boundary is strict:

- handler must not shell out
- handler must not read `.env`
- handler must not run live probes
- handler must not call signed/account/order/position endpoints
- handler must not call lease/report/mutate task endpoints
- handler must not compute readiness from command exit code
- handler must not accept direct file paths from request parameters
- handler must not mutate task state
- handler must read from stored artifact storage only

## Stored Artifact Storage Model

The stored artifact layer is an index plus immutable blob set. It is a future
Admin backend storage contract only; it does not require a request handler to
run health scripts or probe live services.

### Storage Index

Each stored set has one index row:

- `artifactSetId`: immutable id for the uploaded set, preferably generated from
  the source timestamp and a short content hash.
- `schemaVersion`: artifact schema version. Current consumers bind to version
  `1` and ignore unknown appended fields.
- `storedAt`: time the backend accepted and persisted the set.
- `sourceGeneratedAt`: source artifact generation time, copied from
  `generated_at` or `source_generated_at` when present.
- `summaryHash`: SHA-256 of `full-product-health-summary.json`.
- `validationHash`: SHA-256 of `full-product-health-validation.json`.
- `fullArtifactHash`: SHA-256 of `full-product-health.json`.
- `markdownHash`: SHA-256 of `full-product-health.md`.
- `storageStatus`: one of `current`, `superseded`, or `rejected`.
- `retentionClass`: one of `current`, `historical`, or `rejected`.
- `artifactSlaSeconds`: freshness budget used for live-readiness rendering.
- `staleReason`: `null` when fresh, otherwise a short reason code such as
  `stored_at_expired`, `source_generated_at_expired`, or `missing_source_time`.
- `markdownUrl`: authorized URL for the Markdown artifact.
- `fullArtifactUrl`: authorized URL for the full JSON artifact.

Only one latest valid artifact set may be marked `current`. A hash mismatch marks the set rejected before it can be selected as latest. Rejected artifact sets are kept for investigation but must never be returned as ready. The storage rule is: rejected artifact sets cannot be rendered as ready.

### Freshness SLA

Initial live-readiness consumers should use `artifactSlaSeconds=900` unless an
operator config explicitly chooses a stricter value. Staleness is computed from
both `storedAt` and `sourceGeneratedAt`; if either timestamp is missing or older
than the SLA, `stale=true` and `staleReason` is populated. Stale artifacts may
remain visible for diagnosis, but stale cannot be rendered as ready.

### Retention

Storage keeps the latest valid artifact set plus historical valid sets for at least 30 days. Rejected artifact sets are retained for at least 7 days with
their validation summary and hashes, but without raw source text. Index metadata
needed for audit, including `artifactSetId`, hashes, status, and operator
metadata, should be retained for at least 180 days.

### Operator Metadata

`operatorMetadata` records how the set arrived:

- `operatorMetadata.generatedBy`: CI job, operator job, or service account id.
- `operatorMetadata.triggerType`: `ci`, `operator_upload`, or `scheduled`.
- `operatorMetadata.runId`: CI run id, workflow id, or local operator run id.
- `operatorMetadata.commitSha`: source commit when known.
- `operatorMetadata.sourceRepo`: repo or service that produced the artifact.

These fields are audit context only. They must not include API credentials,
database URLs, raw payloads, or local filesystem secrets.

## URL Authorization

`markdownUrl` and `fullArtifactUrl` are authorized download URLs. They must be
scoped to the current Admin user, the artifact set, and the requested artifact
type. They should be short-lived, revocable, and checked server side.

The overview endpoint requires `artifact:health:read`. Downloading Markdown or
full JSON requires `artifact:health:download`. The handler must not expose local filesystem paths, must not proxy arbitrary URLs, and must not return a runnable command or storage bucket internals. If a URL is unavailable, return `null` and
a stable reason code instead of widening access.

## Validation Finding Redaction

The redaction rule is: validation findings only return `code`, `artifact`, `field`, and `marker`. They must not return source text, must not return raw payload, must not return database URL, must not return API key, must not return secret, must not return cipher, and must not return signed endpoint. The UI can show the marker code
and artifact name, but not the blocked content.

## Handler Acceptance Tests

Backend acceptance tests for `GET /admin/quant/full-product-health/latest`
should lock these constraints:

- handler must read only the stored artifact index and stored artifact blobs.
- handler must validate summary, validation, full JSON, and Markdown hashes
  before selecting a current set.
- handler must not accept direct file paths from request parameters.
- handler must not shell out.
- handler must not read `.env`.
- handler must not run live probes.
- handler must not call signed/account/order/position endpoints.
- handler must not call lease/report/mutate task endpoints.
- handler must not mutate task state.
- handler must not compute readiness from command exit code.
- handler must compute `ready=false` whenever validation is not `ok`, sensitive
  markers exist, the set is stale, or the set is not the latest valid artifact
  set.

Example envelope shape:

```json
{
  "artifactSetId": "health-2026-05-08T09-00-00Z",
  "storedAt": "2026-05-08T09:00:30Z",
  "sourceGeneratedAt": "2026-05-08T09:00:00Z",
  "summary": {},
  "validation": {},
  "markdownUrl": "/admin/artifacts/health-2026-05-08T09-00-00Z/full-product-health.md",
  "fullArtifactUrl": "/admin/artifacts/health-2026-05-08T09-00-00Z/full-product-health.json",
  "ready": false,
  "stale": false,
  "staleReason": null,
  "artifactSlaSeconds": 900,
  "operatorMetadata": {
    "generatedBy": "ci",
    "triggerType": "ci",
    "runId": "workflow-123",
    "commitSha": "abcdef0",
    "sourceRepo": "rust_quant"
  },
  "redaction": {
    "status": "ok",
    "sensitive_marker_count": 0
  }
}
```

## Stable Fields

Bind the first Admin screen to these fields:

- `summary.overall_status`
- `section_statuses`
- `checklist[].ready`
- `checklist[].action_required`
- `checklist[].p0_count`
- `checklist[].p1_count`
- `top_alerts[].severity`
- `top_alerts[].code`
- `top_alerts[].section`
- `top_alerts[].message`
- `required_operator_actions[].action`
- `correlation_ids[]`
- `validation.summary.sensitive_marker_count`
- `validation.findings[]`

Consumers must ignore unknown appended fields for schema version `1`.

## Status Mapping

Map artifact and section status values consistently:

- `ok` -> green/pass
- `warn` -> amber/review
- `fail` -> red/blocking

Map alert severity values consistently:

- `P0` -> blocking
- `P1` -> manual review
- `INFO` -> context only

Map operator actions as explicit callouts:

- `block_release_until_resolved`
- `manual_review_before_release`

The UI can show green only when the selected readiness scope has no blocking,
review, skipped, or validation safety condition.

## Do Not Interpret As Ready

The following conditions must not be shown as ready, even if a CI job exits zero
or a report-only lane was used:

- `summary.overall_status != "ok"`
- `section_statuses.* == "warn"`
- `section_statuses.* == "fail"`
- `checklist[].ready == false`
- `checklist[].action_required == true`
- `top_alerts[].severity == "P0"`
- `required_operator_actions` is not empty
- `validation.status != "ok"`
- `validation.summary.sensitive_marker_count > 0`
- `*_INPUT_SKIPPED`
- `read_only_input_count == 0`
- `admin_readiness.live_readiness` is `blocked` or `review`
- `manual_review_required == true`

Skipped input is visible context, not success. A skipped section can exist in a
safe default CI lane and still require a separate read-only collection before a
production readiness claim.

## Redaction Requirements

Admin and frontend displays must preserve the artifact redaction boundary:

- must not read `.env`
- must not call local services
- must not call signed/account/order/position endpoints
- must not lease task
- must not report result
- must not mutate task
- must not place orders
- must not touch `LINKUSDT`
- must render `[redacted]`
- must not show raw database URLs
- must not show API keys
- must not show request or response payloads

If validation reports a sensitive marker, show the finding code, artifact name,
and field or marker code only. Do not echo the blocked source text.

## Refresh And CI Artifact Usage

Recommended flow:

1. CI or an operator job writes the artifact set to durable storage.
2. The validator runs in strict mode before the artifact set is marked current.
3. Admin reads the latest valid `full-product-health-summary.json` for the main
   panel and links to `full-product-health.md`.
4. Admin reads `full-product-health-validation.json` to show schema or safety
   findings next to the artifact timestamp.
5. Admin opens `full-product-health.json` only for section drill-down.

The frontend should display `generated_at` and `source_generated_at` when
available. If the artifact is stale by the product SLA, show stale/review
instead of ready.

## Frontend Display Rules

Use `section_statuses` for the compact section grid and `checklist` for row
detail. Sort `top_alerts` by artifact order, with `P0` above `P1` above `INFO`
when the artifact does not already provide a ranked view.

Use `correlation_ids[]` for copyable IDs and links between News, Web, Quant, and
Admin timelines. Missing correlation IDs should render as absent, not as errors.

For release or live-readiness badges, compute readiness from status, checklist,
operator actions, validation status, sensitive marker count, skipped input, and
staleness. Never compute readiness from command exit code alone.
