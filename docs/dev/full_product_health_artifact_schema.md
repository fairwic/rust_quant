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
  `sections`, `alerts`, `correlation`.
- Required `summary` fields: `p0_count`, `p1_count`, `info_count`,
  `read_only_input_count`.
- `sections` is a keyed object. Existing section names are stable, but section
  detail fields are append-only.
- `alerts[]` items must use the stable severity enum and must be safe to render
  without showing credentials, request bodies, or exchange endpoint text.

Summary example: `full_product_health_examples/full-product-health-summary.json`.

- Required top-level fields: `schema_version`, `source_schema_version`, `status`,
  `generated_at`, `source_generated_at`, `summary`, `section_statuses`,
  `checklist`, `top_alerts`, `required_operator_actions`, `correlation`,
  `correlation_ids`.
- Required `summary` fields: `overall_status`, `p0_count`, `p1_count`,
  `info_count`, `section_count`, `blocking_section_count`,
  `warning_section_count`, `top_alert_count`,
  `required_operator_action_count`, `correlation_id_count`,
  `read_only_input_count`.
- `section_statuses` is the preferred Admin/CI summary lookup. Consumers should
  ignore unknown section keys.

Markdown example: `full_product_health_examples/full-product-health.md`.

- Required markers: `# Full Product Health`, `**Status:**`, `## Counts`,
  `## Top Alerts`, `## Checklist`, `## Artifact Paths`, `## Skipped Sections`.
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
