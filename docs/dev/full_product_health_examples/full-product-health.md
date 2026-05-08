# Full Product Health

**Status:** warn

## Counts

| Metric | Value |
| --- | --- |
| p0_count | 0 |
| p1_count | 1 |
| info_count | 1 |
| section_count | 4 |
| blocking_section_count | 0 |
| warning_section_count | 1 |
| top_alert_count | 2 |
| required_operator_action_count | 1 |
| read_only_input_count | 4 |

## Top Alerts

| Severity | Code | Section | Message |
| --- | --- | --- | --- |
| P1 | NEWS_SOURCE_DEGRADED | news_source_ai_health | Example source has repeated read-only collection warnings. |
| INFO | MOCK_DEV_BOUNDARY_ACTIVE | admin_readiness | Example artifact uses fixture-only data and no live collection. |

## Checklist

| Section | Status | Ready | Action Required | P0 | P1 | Info | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- |
| web_task_order_health | ok | yes | no | 0 | 0 | 0 | WEB_TASK_ORDER_READY |
| news_source_ai_health | warn | no | yes | 0 | 1 | 0 | NEWS_SOURCE_DEGRADED |
| quant_worker_checkpoint_audit | ok | yes | no | 0 | 0 | 0 | QUANT_WORKER_READY |
| admin_readiness | ok | yes | no | 0 | 0 | 1 | ADMIN_READINESS_REVIEW |

## Artifact Paths

| Artifact | Path |
| --- | --- |
| Full report JSON | docs/dev/full_product_health_examples/full-product-health.json |
| Summary JSON | docs/dev/full_product_health_examples/full-product-health-summary.json |
| Markdown | docs/dev/full_product_health_examples/full-product-health.md |

## Skipped Sections

No skipped sections.
