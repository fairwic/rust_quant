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
| Web section sampling | no | yes | no | `FULL_PRODUCT_HEALTH_WEB_DATABASE_URL=mysql://readonly@host/quant_web ./scripts/dev/build_full_product_health_web_input.sh` |
| News section sampling | no | yes | no | `FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL=postgres://readonly@host/quant_news ./scripts/dev/build_full_product_health_news_input.sh` |
| Admin section sampling | no | yes | no | `FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL=postgres://readonly@host/quant_admin ./scripts/dev/build_full_product_health_admin_input.sh` |
| Local worker health child process | no | local explicit opt-in only | no | `FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH=true ./scripts/dev/run_full_product_health_ci.sh` |
| Binance live ETH micro smoke | no | no | yes | `./scripts/dev/run_binance_live_eth_micro_order_smoke.sh` |
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

Read-only DB opt-in may sample aggregate counts from `quant_web`, `quant_news`,
and Admin-owned readiness/audit tables. It must not write to any database, must
not call lease/report/mutate task endpoints, and must not include raw payloads,
API keys, secrets, passphrases, ciphers, signed exchange endpoints, or database
connection strings in generated artifacts.

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

- `./scripts/dev/run_binance_live_eth_micro_order_smoke.sh`
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
   `correlation_ids`.
2. Link to `full-product-health.md` for operator-readable detail.
3. Use `full-product-health-validation.json` to block artifact upload or show
   schema/safety findings.
4. Drill into `full-product-health.json` only for section-specific diagnosis.

Admin must ignore unknown fields and should not remove support for known fields
until the schema version changes and examples are updated in the same change.

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
