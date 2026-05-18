# Full Product Health Artifact Schema

This document is the stable consumer contract for Admin and CI. The machine-readable
source is `full_product_health_artifact_schema.json`; the safe examples live in
`full_product_health_examples/`.

`validate_full_product_health_artifacts.sh` reads this JSON schema by default.
Downstream CI may override it with `FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH`
when validating a candidate schema. A missing or invalid schema must be reported
as an explicit validator finding, and strict mode must fail instead of silently
falling back to script-local field or enum lists.

## Required Fields

Full report example: `full_product_health_examples/full-product-health.json`.

- Required top-level fields: `schema_version`, `status`, `generated_at`, `summary`,
  `sections`, `alerts`, `alert_taxonomy`, `correlation`.
- Required `summary` fields: `p0_count`, `p1_count`, `info_count`,
  `read_only_input_count`.
- Payment entitlement inputs may append `wallet_payment_exception_count` and
  `payment_entitlement_blocker_count` to the full report summary.
- `sections` is a keyed object. Existing section names are stable, but section
  detail fields are append-only.
- `alerts[]` items must use the stable severity enum and must be safe to render
  without showing credentials, request bodies, or exchange endpoint text.
- `alert_taxonomy[]` is the stable drill-down map from an alert to its
  `section`, `severity`, `code`, `operator_action`, optional playbook metadata
  (`owner`, `default_next_action`, `admin_link_target`), and allowed
  `correlation_keys`. It contains ID key names only, not raw payloads or secret
  values.
- `alert_taxonomy[].code` must be registered in `alert_code_values[section]` or
  `alert_code_values.global`. The validator rejects unregistered codes in strict
  mode so Admin can bind code-specific playbooks without parsing free-form text.
- `alerts[].code` and `top_alerts[].code` must follow the same registry rule;
  emitted alert drift is rejected before Admin renders a playbook.

Summary example: `full_product_health_examples/full-product-health-summary.json`.

- Required top-level fields: `schema_version`, `source_schema_version`, `status`,
  `generated_at`, `source_generated_at`, `summary`, `section_statuses`,
  `checklist`, `top_alerts`, `required_operator_actions`, `alert_taxonomy`,
  `operator_playbook_summary`, `correlation`, `correlation_ids`.
- Required `summary` fields: `overall_status`, `p0_count`, `p1_count`,
  `info_count`, `section_count`, `blocking_section_count`,
  `warning_section_count`, `top_alert_count`,
  `required_operator_action_count`, `alert_taxonomy_count`,
  `correlation_id_count`,
  `read_only_input_count`.
- Payment entitlement summaries may append `wallet_payment_exception_count` and
  `payment_entitlement_blocker_count` for Admin/CI counters.
- Required `operator_playbook_summary` fields: `item_count`,
  `blocking_item_count`, `manual_review_item_count`,
  `observe_only_item_count`, and `items`.
- `operator_playbook_summary.items[]` is the compact Admin/CI playbook list.
  Each item mirrors the alert code registry fields `owner`,
  `default_next_action`, and `admin_link_target`, plus `source`, `severity`,
  `code`, `section`, `operator_action`, and optional sanitized `metadata`.
  Consumers should use the count fields for dashboards and ignore unknown
  appended item fields.
- `section_statuses` is the preferred Admin/CI summary lookup. Consumers should
  ignore unknown section keys.

## Consumer Contract Versions

`consumer_contracts.operator_playbook_summary.compatibility_contract_version`
is the versioned compatibility record for the Admin/CI playbook queue. Version
`1` binds producers to the paths listed in
`consumer_contracts.operator_playbook_summary.producer_required_paths`:

- `summary.operator_playbook_summary` and its required count fields.
- `summary.operator_playbook_summary.items`.
- Markdown marker `## Operator Playbook Summary` for human review.
- `admin_ingest.summary.operator_playbook_summary` for the stored Admin handoff
  fixture.

Admin and CI must consume JSON first: use
`operator_playbook_summary.items[]` as the actionable list and use
`item_count`, `blocking_item_count`, `manual_review_item_count`, and
`observe_only_item_count` for dashboard totals. Consumers must ignore unknown
appended `operator_playbook_summary` fields and item fields under schema version
`1`; removing or renaming any producer-required path requires a new
`compatibility_contract_version`.

`consumer_contracts.payment_publish_index_read_only_consumption.compatibility_contract_version`
is the publish-index contract for payment health. Version `1` binds Admin and CI
to the stored publish index fields, not to a live query:

- Read `wallet_payment_exception_count` and
  `payment_entitlement_blocker_count` from
  `publish_index.summary.summary`.
- Treat a missing wallet counter as artifact drift or unknown, not as zero
  incidents.
- Render payment next actions from
  `publish_index.summary.operator_playbook_summary.items[]` before Markdown or
  log text.
- Preserve `default_next_action=review_wallet_payment_exceptions` for
  `WALLET_PAYMENT_EXCEPTION`.
- Preserve `operator_action=block_release_until_resolved` for
  `PAYMENT_ENTITLEMENT_BLOCKED`.
- Render the index as latest-ready only when `storageStatus=current`,
  `stale=false`, `validation.status=ok`, and `redaction.status=ok`.

`consumer_contracts.admin_latest_artifact_readiness_envelope.compatibility_contract_version`
is the latest-artifact readiness contract for Admin. Version `1` binds the
stored response envelope and panel projection to these stable fields:

- `latest.ready`, `latest.stale`, and `latest.staleReason`.
- `latest.summary.summary.overall_status`.
- `latest.summary.section_statuses`.
- `latest.summary.checklist[].ready`,
  `latest.summary.checklist[].action_required`,
  `latest.summary.checklist[].live_readiness`, and
  `latest.summary.checklist[].manual_review_required` when emitted by the
  `admin_readiness` section.
- `latest.summary.required_operator_actions` and
  `latest.summary.read_only_input_count`.
- `latest.validation.status`,
  `latest.validation.summary.sensitive_marker_count`, and
  `latest.redaction.status`.
- `latest.paymentPublishIndex.status` and
  `latest.paymentPublishIndex.readyToRender`.
- `latest.walletPaymentConfig.source`.

These fields are a read-only operator surface. Admin may render badges,
blockers, counters, and next-action labels from them, but must not trigger
recovery, provider calls, execution task lease/report/mutate, signed exchange
calls, order placement, or live smoke automatically.
They must not automatically trigger recovery or mutation. A missing readiness or
`paymentPublishIndex` field is artifact drift/unknown and must render not-ready,
not a default ready state. Removing or renaming any producer-required path
requires a new `compatibility_contract_version`.

`consumer_contracts.admin_wallet_payment_config_env_snapshot.compatibility_contract_version`
is the Admin wallet payment config boundary. Version `1` exists only to prevent
Admin UI drift:

- `walletPaymentConfig` is an Admin-only config snapshot or draft.
- `walletPaymentConfig.source.kind` must be `admin_process_env_snapshot` or
  `admin_managed_config_draft`, and the source must be visible wherever the
  config surface is displayed.
- `walletPaymentConfig` must not represent Web wallet provider readiness.
- Admin must not infer Web payment/provider readiness, payment publish readiness,
  or live release readiness from this snapshot alone.
- If Web wallet readiness is missing or inconsistent with the Admin snapshot,
  render degraded/unknown and not ready.
- Missing `walletPaymentConfig.source` is artifact drift/unknown and must render
  not ready.
- The not-ready decision table is explicit:
  `source_kind_missing_or_not_allowed_admin_config_source`,
  `status_configured_without_web_wallet_provider_readiness`, `status_draft`,
  `status_degraded`, `status_unknown`, `web_wallet_provider_readiness_missing`,
  `web_wallet_provider_readiness_unknown`,
  `web_wallet_provider_readiness_incomplete`, and
  `web_wallet_provider_readiness_inconsistent_with_admin_snapshot` cannot be
  ready. Web wallet readiness is incomplete when Web provider readiness is
  missing any required stored artifact field or cannot be compared to the Admin
  snapshot. These cases cannot be ready.
Markdown example: `full_product_health_examples/full-product-health.md`.

- Required markers: `# Full Product Health`, `**Status:**`, `## Counts`,
  `## Top Alerts`, `## Operator Playbook Summary`, `## Checklist`,
  `## Artifact Paths`, `## Skipped Sections`.
- Markdown is for human review only. Admin and CI should bind to JSON artifacts
  when they need durable fields.

Validation example: `full_product_health_examples/full-product-health-validation.json`.

- Required top-level fields: `schema_version`, `status`, `generated_at`,
  `summary`, `artifacts`, `findings`.
- Required `summary` fields: `artifact_count`, `missing_artifact_count`,
  `json_parse_error_count`, `missing_required_field_count`,
  `sensitive_marker_count`, `finding_count`.
- `artifacts.*.missing_nested_fields` records schema-driven nested contract
  gaps such as `operator_playbook_summary.items`, so CI can fail before Admin
  stores an incomplete handoff fixture.
- `findings[]` must contain only safe codes, marker codes, artifact names, and
  field names. It must not echo the blocked source text.

## Status Values

Allowed status values are:

- `ok`: all configured artifacts are usable.
- `warn`: artifacts are usable, but at least one non-blocking product section
  needs review.
- `fail`: a blocking section, malformed artifact, missing required field, or
  blocked marker prevents promotion.

## Severity Values

Allowed alert and action severity values are:

- `P0`: blocks release or live promotion.
- `P1`: requires operator review before release or live promotion.
- `INFO`: context only.

## Operator Action Values

Allowed `alert_taxonomy[].operator_action` values are:

- `block_release_until_resolved`: a `P0` alert blocks release or live promotion.
- `manual_review_before_release`: a `P1` alert requires operator review.
- `observe_only`: an `INFO` alert is context only.

## Alert Code Values

`alert_code_values` is the explicit registry for `alert_taxonomy[].code`,
`alerts[].code`, and `top_alerts[].code`.

- `global`: generic collector or fixture-safe codes that may appear in any
  section.
- `web_task_order_health`: Web task/order/input health codes.
- `news_source_ai_health`: News source, AI provider, and analysis job codes.
- `quant_worker_checkpoint_audit`: local service health, worker checkpoint, and
  execution audit codes.
- `admin_readiness`: Admin audit/readiness and full-product summary failure
  codes.
- `payment_entitlement_health`: wallet payment input, exception, and payment
  entitlement access codes, including `PAYMENT_INPUT_SKIPPED`,
  `PAYMENT_INPUT_QUERY_FAILED`, `WALLET_PAYMENT_EXCEPTION`, and
  `PAYMENT_ENTITLEMENT_BLOCKED`.

`alert_code_metadata` is the playbook-facing companion registry. For every code
listed in `alert_code_values`, it records:

- `owner`: a stable owner key such as `web_execution`, `news_ops`, `quant_ops`,
  `admin_ops`, `commerce_billing`, or `platform_health`.
- `default_next_action`: a safe action key that Admin can show before a richer
  playbook exists.
- `admin_link_target`: a stable Admin route key. It is not a local filesystem
  path, remote URL, exchange endpoint, raw payload pointer, or live symbol.

When adding a new producer alert, add the code to the correct registry section
in `full_product_health_artifact_schema.json` before emitting it in
`alerts[]`, `top_alerts[]`, or `alert_taxonomy[]`, and add matching
`alert_code_metadata`. Do not use the registry for local paths, URLs, raw
payloads, credentials, signed endpoints, or live symbols.

Payment entitlement alerts use stable playbook actions:

- `PAYMENT_INPUT_SKIPPED`: `provide_payment_read_only_input`.
- `PAYMENT_INPUT_QUERY_FAILED`: `inspect_payment_read_only_input`.
- `WALLET_PAYMENT_EXCEPTION`: `review_wallet_payment_exceptions`.
- `PAYMENT_ENTITLEMENT_BLOCKED`: `reconcile_payment_entitlement`.

## Payment Entitlement State Contract

`consumer_contracts.payment_entitlement_health_states` pins the CI/Admin
contract for the payment entitlement producer. This is the 只读 DB opt-in
contract for payment entitlement health. The stable states are
`skipped / query_failed / real_count`.

- `skipped`: no explicit `FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL` was
  provided. The section is `warn`, `source=skipped`,
  `read_only_input=false`, and uses `PAYMENT_INPUT_SKIPPED`. Consumers must not
  interpret this as zero incidents.
- `query_failed`: an explicit read-only DB opt-in was provided, but the
  PostgreSQL query could not return counts. The section is `warn`,
  `source=quant_web_payment_readonly_db`, `read_only_input=true`, and uses
  `PAYMENT_INPUT_QUERY_FAILED`.
- `real_count`: an explicit read-only DB opt-in produced counts. The section is
  `source=quant_web_payment_readonly_db`, `read_only_input=true`, and exposes
  `wallet_payment_exception_count` plus `payment_entitlement_blocker_count`.
  Status is `ok`, `warn`, or `fail` according to those counts.

The real read-only lane is configured with
`FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL`,
`FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN`, and
`FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS`. Artifact output must not
include DB URLs, secrets, raw payloads, tx refs, payer/payee refs, local paths,
signed exchange endpoints, Web mutation endpoints, or live symbols.

The schema declares three synthetic state examples for Admin/CI drift checks:
`payment-entitlement-health-skipped.json`,
`payment-entitlement-health-query-failed.json`, and
`payment-entitlement-health-real-count.json`. These are section-level fixtures,
not live DB samples. The operator-safe fixture path validates artifacts without
connecting to a real DB:

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

For the payment handoff smoke that mirrors the final publish/index gate without
connecting to DB/provider:

```bash
FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH=docs/dev/full_product_health_examples/payment-entitlement-health-real-count.json \
FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR=/tmp/full-product-health-payment-smoke \
FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH=/tmp/full-product-health-payment-smoke/full-product-health-publish-index.json \
./scripts/dev/smoke_full_product_health_payment_artifact_handoff.sh
```

That smoke writes `full-product-health.md`,
`full-product-health-validation.json`, and
`full-product-health-publish-index.json` in addition to full report and summary.
Before Admin stores or renders the artifact set, consumers should check
`storageStatus`, then consume
`summary.summary.wallet_payment_exception_count` and
`summary.operator_playbook_summary.items[]` from the publish index.

This command is operator-safe: 不连接真实 DB, 不读取 `.env`, 不外呼交易所, and
不 lease/report/mutate task.

## Append-Only Boundary

For schema version `1`, producers may append fields under the paths listed in
`append_only_paths`. Consumers must ignore unknown appended fields.

These changes are breaking and require a new `schema_version`:

- Removing a required field.
- Renaming a required field.
- Changing the meaning of an existing enum value.
- Replacing an object or array with a different JSON type.
- Making a currently optional artifact mandatory without adding a new version.

## Safe Example Boundary

The example set is intentionally synthetic. It uses fixture IDs, stable section
names, and non-production messages. It must not contain real database connection
strings, exchange credentials, pass phrases, encrypted credential blobs, raw
request or response bodies, Binance signed/account/order/position endpoint text,
Web mutation endpoint text, or the protected live position symbol. The contract
test validates the example set against the schema file and runs the validator
against the JSON and Markdown examples in strict mode.
