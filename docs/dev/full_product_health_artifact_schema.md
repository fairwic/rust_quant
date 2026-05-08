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
- Required `operator_playbook_summary` fields: `item_count`,
  `blocking_item_count`, `manual_review_item_count`,
  `observe_only_item_count`, and `items`.
- `operator_playbook_summary.items[]` is the compact Admin/CI playbook list.
  Each item mirrors the alert code registry fields `owner`,
  `default_next_action`, and `admin_link_target`, plus `source`, `severity`,
  `code`, `section`, and `operator_action`. Consumers should use the count
  fields for dashboards and ignore unknown appended item fields.
- `section_statuses` is the preferred Admin/CI summary lookup. Consumers should
  ignore unknown section keys.

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

`alert_code_metadata` is the playbook-facing companion registry. For every code
listed in `alert_code_values`, it records:

- `owner`: a stable owner key such as `web_execution`, `news_ops`, `quant_ops`,
  `admin_ops`, or `platform_health`.
- `default_next_action`: a safe action key that Admin can show before a richer
  playbook exists.
- `admin_link_target`: a stable Admin route key. It is not a local filesystem
  path, remote URL, exchange endpoint, raw payload pointer, or live symbol.

When adding a new producer alert, add the code to the correct registry section
in `full_product_health_artifact_schema.json` before emitting it in
`alerts[]`, `top_alerts[]`, or `alert_taxonomy[]`, and add matching
`alert_code_metadata`. Do not use the registry for local paths, URLs, raw
payloads, credentials, signed endpoints, or live symbols.

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
