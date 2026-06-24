# Full Product Health Admin/CI Handoff

This guide is the handoff contract for Admin dashboards and CI jobs that consume
schema-driven full-product health artifacts.

## Default Boundary

The default handoff path is intentionally safe: no-env, no-service, no-exchange.
Default CI commands must not read `.env`, must not access local services, must
not call exchanges, must not place orders, must not lease task, must not report
result, must not mutate task, and must not touch `LINKUSDT`.

中文安全边界：默认 CI 不读取 `.env`，不访问本地服务，不外呼交易所，不下单，不 lease task，不 report result，不 mutate task，不触碰 `LINKUSDT`。需要只读 DB URL 时必须显式 opt-in；实盘 smoke 绝不能在默认 CI 调用。

The default CI output is file-only:

- full report JSON: `full-product-health.json`
- summary JSON: `full-product-health-summary.json`
- optional Markdown: `full-product-health.md`
- optional validation JSON: `full-product-health-validation.json`

Admin should consume generated artifacts or persisted copies of them. It should
not shell out from a request handler and should not parse free-form log text.

## Command Matrix

| Lane | Default CI | Requires explicit read-only DB URL | Never run in default CI | Command |
| --- | --- | --- | --- | --- |
| JSON artifacts only | yes | no | no | `FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never ./scripts/dev/run_full_product_health_ci.sh` |
| JSON + Markdown artifact | yes | no | no | `FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never ./scripts/dev/run_full_product_health_ci.sh` |
| JSON + Markdown + validation artifact | yes | no | no | `FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH=/tmp/full-product-health-ci/full-product-health-validation.json FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never ./scripts/dev/run_full_product_health_ci.sh` |
| Local Admin ingest mock contract | no | explicit artifact env only | no | `FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=docs/dev/full_product_health_examples/full-product-health-summary.json FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW=2026-05-07T01:05:00Z FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY=phase-55-local-contract FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE=ci FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID=phase-55-contract FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA=abcdef1 FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO=rust_quant FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json ./scripts/dev/smoke_publish_full_product_health_admin_ingest_contract.sh` |
| Missing artifact env preflight | no | no | no | `./scripts/dev/smoke_publish_full_product_health_admin_ingest_contract.sh` |
| Strict validation gate | yes | no | no | `FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true ./scripts/dev/validate_full_product_health_artifacts.sh` |
| Candidate schema validation | yes | no | no | `FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH=/tmp/full_product_health_artifact_schema.candidate.json FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true ./scripts/dev/validate_full_product_health_artifacts.sh` |
| Standalone Markdown render | yes | no | no | `FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH=/tmp/full-product-health-ci/full-product-health-summary.json FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json FULL_PRODUCT_HEALTH_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md ./scripts/dev/render_full_product_health_markdown.sh` |
| Payment real-count artifact smoke | yes | explicit redacted JSON file only | no | `FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR=/tmp/full-product-health-payment-smoke ./scripts/dev/smoke_full_product_health_payment_artifact_handoff.sh` |
| Web section sampling | no | yes | no | `FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=mysql://readonly@host/quant_web ./scripts/dev/build_full_product_health_web_input.sh` |
| Payment entitlement sampling | no | yes | no | `FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL=postgres://readonly@host/quant_web ./scripts/dev/build_full_product_health_payment_input.sh` |
| News section sampling | no | yes | no | `FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=postgres://readonly@host/quant_news ./scripts/dev/build_full_product_health_news_input.sh` |
| Admin section sampling | no | yes | no | `FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=postgres://readonly@host/quant_admin ./scripts/dev/build_full_product_health_admin_input.sh` |
| Local worker health child process | no | local explicit opt-in only | no | `FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=true ./scripts/dev/run_full_product_health_ci.sh` |
| Binance Rust-native ETH micro live validation | no | no | yes | `cargo run -q -p rust-quant-cli --bin binance_eth_micro_live_validation` |
| Binance legacy live order smoke | no | no | yes | `./scripts/dev/run_binance_live_order_smoke.sh` |

## Default CI Safe

Use this lane when CI has no secrets and no service network:

```bash
FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci \
FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false \
FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never \
./scripts/dev/run_full_product_health_ci.sh
```

This command may produce skipped sections. That is expected in default CI. It
still verifies the artifact shape and produces Admin-consumable JSON.

To add a human-readable artifact without changing the safety boundary:

```bash
FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci \
FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false \
FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never \
./scripts/dev/run_full_product_health_ci.sh
```

To write validation output in the same default lane:

```bash
FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci \
FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true \
FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH=/tmp/full-product-health-ci/full-product-health-validation.json \
FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false \
FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS=never \
./scripts/dev/run_full_product_health_ci.sh
```

The wrapper calls child scripts through a minimal env allowlist. It passes only
artifact paths and `FULL_PRODUCT_HEALTH_*` read-only inputs.

## Read-Only DB Opt-In

Use this lane only when CI or an operator job has explicit read-only DB URLs.
These commands are not default CI commands.

```bash
FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=mysql://readonly@host/quant_web \
FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL=postgres://readonly@host/quant_web \
FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=postgres://readonly@host/quant_news \
FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=postgres://readonly@host/quant_admin \
FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR=/tmp/full-product-health-ci \
FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS=true \
FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH=/tmp/full-product-health-ci/full-product-health-validation.json \
FULL_PRODUCT_HEALTH_CI_VALIDATION_STRICT=true \
FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=false \
./scripts/dev/run_full_product_health_ci.sh
```

Read-only DB opt-in may sample aggregate counts from `quant_web`, payment
entitlement breakpoints, `quant_news`, and Admin-owned readiness/audit tables.
It must not write to any database, must not call lease/report/mutate task
endpoints, and must not include raw payloads, tx raw bodies, API keys, secrets,
passphrases, ciphers, signed exchange endpoints, or database connection strings
in generated artifacts.

Payment entitlement read-only DB opt-in is the 只读 DB opt-in lane covered by
`consumer_contracts.payment_entitlement_health_states`. It has a stable
three-state contract: `skipped / query_failed / real_count`. `skipped` means
`FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL` was not provided and must not be
treated as zero incidents. `query_failed` means the URL was provided but the
read-only PostgreSQL query failed. `real_count` means the query returned
`wallet_payment_exception_count` and `payment_entitlement_blocker_count`.
Operators can override only the payment producer binary and timeout with
`FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN` and
`FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS`; these values are runtime
inputs only and must not appear in uploaded artifacts.

For local Admin/CI contract drift checks, use a redacted payment real-count JSON
instead of a real DB. The reusable operator-safe smoke command is:

```bash
FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json \
FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR=/tmp/full-product-health-payment-smoke \
FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH=/tmp/full-product-health-payment-smoke/full-product-health-publish-index.json \
./scripts/dev/smoke_full_product_health_payment_artifact_handoff.sh
```

It writes `full-product-health.json`,
`full-product-health-summary.json`, `full-product-health.md`,
`full-product-health-validation.json`, and
`full-product-health-publish-index.json` under the output directory, then
prints a small manifest with the generated paths, publish/index status, and
payment counts. If
`FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR` is omitted, the script creates a
temporary output directory and keeps it for operator review.

The publish/index artifact is produced by
`publish_full_product_health_artifact_set.sh` before any real upload or Admin
storage write. Admin/CI should consume `storageStatus`,
`summary.summary.wallet_payment_exception_count`,
`summary.summary.payment_entitlement_blocker_count`, and
`summary.operator_playbook_summary.items[]` from that index. The
`WALLET_PAYMENT_EXCEPTION` item must preserve
`default_next_action=review_wallet_payment_exceptions`; the
`PAYMENT_ENTITLEMENT_BLOCKED` item must preserve the blocking
`operator_action=block_release_until_resolved`.
Missing payment counters in the publish index mean artifact drift/unknown, not
zero incidents. Admin/CI must not re-query DB from this handoff path; use the
stored publish index only, and render it as latest-ready only when
`storageStatus=current`, `stale=false`, `validation.status=ok`, and
`redaction.status=ok`.

The wrapper is intentionally file-only: it requires
`FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH`, rejects missing files, invalid
JSON, and JSON containing blocked sensitive markers, and runs the aggregator,
summary, Markdown renderer, validator, and publisher through an allowlisted
environment. It does not read `.env`, does not connect to a DB, does not call
providers or signed exchange endpoints, does not lease/report/mutate tasks, and
does not touch `LINKUSDT`.

The equivalent expanded command path is:

```bash
FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json \
FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH=false \
./scripts/dev/check_full_product_health.sh > full-product-health-payment-fixture.json

FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH=full-product-health-payment-fixture.json \
./scripts/dev/summarize_full_product_health.sh > full-product-health-payment-fixture-summary.json

FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH=full-product-health-payment-fixture.json \
FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH=full-product-health-payment-fixture-summary.json \
FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md \
FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true \
./scripts/dev/validate_full_product_health_artifacts.sh
```

The fixture set covers `payment-entitlement-health-skipped.json`,
`payment-entitlement-health-query-failed.json`, and
`payment-entitlement-health-real-count.json`. It is operator-safe: 不连接真实 DB,
不读取 `.env`, 不外呼交易所, and 不 lease/report/mutate task.

If a downstream pipeline wants to validate a candidate schema before adopting it:

```bash
FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH=/tmp/full_product_health_artifact_schema.candidate.json \
FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json \
FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true \
./scripts/dev/validate_full_product_health_artifacts.sh
```

## Markdown And Validation Outputs

Markdown is for people. Admin and CI should still bind to the JSON schema and
summary fields.

```bash
FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH=/tmp/full-product-health-ci/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json \
FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
./scripts/dev/render_full_product_health_markdown.sh
```

Validation is for upload gates and Admin handoff safety checks:

```bash
FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH=/tmp/full-product-health-ci/full-product-health.json \
FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH=/tmp/full-product-health-ci/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH=/tmp/full-product-health-ci/full-product-health.md \
FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true \
./scripts/dev/validate_full_product_health_artifacts.sh
```

`FULL_PRODUCT_HEALTH_VALIDATION_STRICT=true` means any finding returns non-zero.
Without strict mode, schema issues are still emitted as findings, but the command
can be used as a non-blocking report.

## Never Run In Default CI

The following commands are explicitly outside the default Admin/CI handoff lane:

- `cargo run -q -p rust-quant-cli --bin binance_eth_micro_live_validation`
- `./scripts/dev/run_binance_live_eth_micro_order_smoke.sh` (deprecated fail-fast guard)
- `./scripts/dev/run_binance_live_order_smoke.sh`
- `HEALTH_CHECK_BINANCE=true ./scripts/dev/check_local_service_health.sh`
- Any command that reads `.env`.
- Any command that calls Binance signed/account/order/position endpoints.
- Any command that opens, closes, reduces, or verifies a real position.
- Any command that uses the existing real `LINKUSDT` position.
- Any command that leases execution tasks, reports order results, or mutates
  execution task state.

Live ETH micro smoke remains a manual guarded procedure only. It requires
explicit operator approval and preflight outside default CI.

## Admin Consumer Handoff

Admin should treat `docs/dev/full_product_health_artifact_schema.json` as the
machine-readable contract and `docs/dev/full_product_health_examples/` as the
redacted example set.

Recommended Admin binding order:

1. Read `full-product-health-summary.json` for `summary.overall_status`,
   `checklist`, `top_alerts`, `required_operator_actions`, and
   `alert_taxonomy`, `operator_playbook_summary`, and `correlation_ids`.
2. Link to `full-product-health.md` for operator-readable detail.
3. Use `full-product-health-validation.json` to block artifact upload or show
   schema/safety findings.
4. Drill into `full-product-health.json` only for section-specific diagnosis.

Admin must ignore unknown fields and should not remove support for known fields
until the schema version changes and examples are updated in the same change.

## Read-Only Operator Surfaces

The Admin latest-artifact response may expose `paymentPublishIndex` and
readiness fields derived from the stored summary, validation, redaction, and
publish index. These are stable display surfaces, not action surfaces.
Each field is a read-only operator surface.

Stable readiness inputs are `ready`, `stale`, `staleReason`,
`summary.summary.overall_status`, `summary.section_statuses`,
`summary.checklist[].ready`, `summary.checklist[].action_required`,
`summary.checklist[].live_readiness`,
`summary.checklist[].manual_review_required`,
`summary.required_operator_actions`, `summary.read_only_input_count`,
`validation.status`, `validation.summary.sensitive_marker_count`, and
`redaction.status`. Stable payment publish-index inputs are
`paymentPublishIndex.status`, `paymentPublishIndex.readyToRender`,
`paymentPublishIndex.walletPaymentExceptionCount`,
`paymentPublishIndex.paymentEntitlementBlockerCount`,
`paymentPublishIndex.counterSource`, `paymentPublishIndex.playbookSource`,
`paymentPublishIndex.validationStatus`, `paymentPublishIndex.redactionStatus`,
and `paymentPublishIndex.playbookItems[]`.

Admin/CI must treat missing readiness fields or missing `paymentPublishIndex`
fields as artifact drift/unknown and render not-ready. These fields may show
operator next-action labels and read-only links, but they must not automatically
run recovery, call provider APIs, call signed exchange endpoints, place orders,
lease/report/mutate execution tasks, run local probes, or touch `LINKUSDT`.
They must not automatically trigger recovery or mutation.

`walletPaymentConfig` is an Admin-only config snapshot or draft. It must carry
and display `walletPaymentConfig.source.kind=admin_process_env_snapshot` or
`admin_managed_config_draft`; missing source means artifact drift/unknown and
not ready. This surface only describes the Admin process environment or
Admin-managed draft that produced the artifact. It must not represent Web wallet provider readiness,
payment publish readiness, or live release readiness. If Web
wallet readiness is missing, unavailable, or inconsistent with the Admin config
surface, Admin/CI must render degraded/unknown and not ready.
The not-ready decision table is explicit:
`source_kind_missing_or_not_allowed_admin_config_source`,
`status_configured_without_web_wallet_provider_readiness`, `status_draft`,
`status_degraded`, `status_unknown`, `web_wallet_provider_readiness_missing`,
`web_wallet_provider_readiness_unknown`,
`web_wallet_provider_readiness_incomplete`, and
`web_wallet_provider_readiness_inconsistent_with_admin_snapshot` cannot be
ready. Web wallet readiness is incomplete when the stored latest response lacks
the Web provider readiness fields needed to compare against the Admin snapshot.
These cases cannot be ready.
`alert_taxonomy[]` is the stable alert-to-action drill-down map. Each item
links `severity`, `code`, and `section` to an `operator_action`, optional
playbook metadata (`owner`, `default_next_action`, `admin_link_target`), and
safe `correlation_keys[]`. The keys point at `correlation` / `correlation_ids`
entries across News, Web, worker checkpoints, and Admin audit logs; the taxonomy
contains key names only and must not contain raw payloads, local paths,
credentials, signed exchange endpoints, or position symbols.
`alert_taxonomy[].code`, `alerts[].code`, and `top_alerts[].code` must be
present in `alert_code_values[section]` or `alert_code_values.global` in
`full_product_health_artifact_schema.json`. The sibling
`alert_code_metadata[section][code]` registry supplies Admin playbook defaults:
`owner`, `default_next_action`, and `admin_link_target`. Validator strict mode
rejects unregistered codes so CI and Admin can detect producer/schema drift
before showing a playbook.

`operator_playbook_summary` is the compact JSON contract for Admin and CI
operator queues. Use `blocking_item_count`, `manual_review_item_count`, and
`observe_only_item_count` for dashboards, then render
`operator_playbook_summary.items[]` as the actionable list. Each item contains
`source`, `severity`, `code`, `section`, `operator_action`, `owner`,
`default_next_action`, and `admin_link_target`. Consumers must ignore unknown
appended item fields and must not infer actions from free-form `message` text
when these structured fields are present.

## Redacted Ingest Payload Fixture

`publish_full_product_health_artifact_set.sh` is the Admin ingest handoff
publisher. It reads only explicit artifact paths, embeds the redacted summary
and validation payloads, derives storage metadata, and rejects local filesystem
paths or sensitive markers in handoff URLs and operator metadata.

Reference fixture:

- `docs/dev/full_product_health_examples/admin-ingest-handoff.json`

Safe smoke command:

```bash
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=docs/dev/full_product_health_examples/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW=2026-05-07T01:05:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY=phase-52-contract-test \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE=ci \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID=phase-52-fixture \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA=abcdef1 \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO=rust_quant \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json \
./scripts/dev/publish_full_product_health_artifact_set.sh
```

Local no-secret ingest smoke:

```bash
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=docs/dev/full_product_health_examples/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW=2026-05-07T01:05:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY=phase-53-local-smoke \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE=ci \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID=phase-53-dry-run \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA=abcdef1 \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO=rust_quant \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json \
./scripts/dev/smoke_publish_full_product_health_admin_ingest.sh
```

See `docs/dev/full_product_health_admin_ingest_local_smoke_runbook.md` for the
localhost mock receiver flow, the one-shot contract smoke wrapper, and the
remote explicit URL opt-in rule.

The contract wrapper has an explicit preflight gate. It requires
`FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH`,
`FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH`,
`FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH`, and
`FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT` to be provided directly in the
command environment. Missing inputs are a safe failure: stderr explains the
missing vars, stdout stays empty, the wrapper does not read `.env`, does not
scan directories, and does not start the localhost receiver.

Default CI and local mock contract lanes are explicit-artifact only: they do
not read `.env`, does not scan directories as part of artifact discovery, and
do not call exchanges just to discover artifacts.

Expected top-level payload fields:

- `artifactSetId`
- `schemaVersion`
- `storedAt`
- `sourceGeneratedAt`
- `summaryHash`
- `validationHash`
- `fullArtifactHash`
- `markdownHash`
- `storageStatus`
- `retentionClass`
- `artifactSlaSeconds`
- `stale`
- `staleReason`
- `summary`
- `validation`
- `redaction`
- `markdownUrl`
- `fullArtifactUrl`
- `operatorMetadata`

The published payload must not contain local filesystem paths, DB URLs, API
keys, API secrets, raw request/response payloads, or signed exchange endpoint
paths.
