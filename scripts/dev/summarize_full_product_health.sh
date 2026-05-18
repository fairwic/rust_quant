#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT:=5}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_PATH:=}"

if [[ "${FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT must be json\n' >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SCHEMA_PATH="${SCRIPT_DIR}/../../docs/dev/full_product_health_artifact_schema.json"
SCHEMA_PATH="${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_PATH:-${DEFAULT_SCHEMA_PATH}}"

python3 - \
    "${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION}" \
    "${FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH}" \
    "${FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT}" \
    "${SCHEMA_PATH}" \
    <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


schema_version = int(sys.argv[1])
json_path = sys.argv[2]
top_alert_limit = max(int(sys.argv[3]), 0)
schema_path = sys.argv[4]

BLOCKED_MARKER_GROUPS = [
    ("ENV_FILE_REFERENCE", [".env"]),
    ("DB_CONNECTION_STRING", ["postgres://", "mysql://"]),
    ("DATABASE_URL_FIELD", ["database_url"]),
    ("CREDENTIAL_TOKEN", ["api_key", "apikey", "api key", "api_secret", "apisecret", "api secret", "secret"]),
    ("RAW_CONTENT", ["request_payload", "response_payload", "raw_payload", "request payload", "response payload"]),
    (
        "SIGNED_EXCHANGE_ENDPOINT",
        [
            "/fapi/v1/order",
            "/fapi/v2/account",
            "/fapi/v1/positionRisk",
            "/fapi/v2/positionRisk",
            "/fapi/v1/positionSide/dual",
        ],
    ),
    (
        "WEB_MUTATION_ENDPOINT",
        [
            "/api/commerce/internal/execution-tasks/lease",
            "/api/commerce/internal/execution-results",
            "/api/commerce/internal/order-results",
        ],
    ),
    ("URL_REFERENCE", ["https://", "http://", "file://"]),
    ("LOCAL_PATH_REFERENCE", ["/Users/", "/tmp/"]),
    ("LINK_POSITION_SYMBOL", ["LINKUSDT", "LINK-USDT"]),
]

BLOCKED_MARKERS = [
    marker for _, markers in BLOCKED_MARKER_GROUPS for marker in markers
]
PATH_SAFE_MARKER_GROUPS = [
    (code, markers)
    for code, markers in BLOCKED_MARKER_GROUPS
    if code not in {"URL_REFERENCE", "LOCAL_PATH_REFERENCE"}
]

BLOCKED_KEY_FRAGMENTS = [
    "database_url",
    "api_key",
    "apikey",
    "api_secret",
    "apisecret",
    "secret",
    "request_payload",
    "response_payload",
    "raw_payload",
    "payload",
]

DEFAULT_SECTION_NAMES = [
    "web_task_order_health",
    "news_source_ai_health",
    "quant_worker_checkpoint_audit",
    "admin_readiness",
]

SEVERITY_RANK = {
    "P0": 0,
    "P1": 1,
    "INFO": 2,
}

SECTION_CORRELATION_KEYS = {
    "web_task_order_health": [
        "signal_inbox_id",
        "execution_task_id",
        "execution_attempt_id",
        "order_result_id",
        "trade_record_id",
    ],
    "news_source_ai_health": [
        "news_id",
        "analysis_result_id",
        "external_id",
    ],
    "quant_worker_checkpoint_audit": [
        "worker_id",
        "request_id",
    ],
    "admin_readiness": [
        "admin_operation_log_id",
        "admin_module",
        "admin_action",
    ],
    "payment_entitlement_health": [
        "payment_exception_id",
        "entitlement_check_id",
        "user_id",
    ],
}

ALERT_HANDOFF_KEYS = [
    "execution_task_id",
    "order_result_id",
    "source_signal_type",
    "protection_status",
    "blocker_code",
]


class SummaryError(Exception):
    pass


def now_utc() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def has_blocked_marker(value: str) -> bool:
    return bool(blocked_marker_codes(value))


def blocked_marker_codes(value: str, *, include_local_paths: bool = True) -> list[str]:
    lowered = value.lower()
    groups = BLOCKED_MARKER_GROUPS if include_local_paths else PATH_SAFE_MARKER_GROUPS
    return [
        code
        for code, markers in groups
        if any(marker.lower() in lowered for marker in markers)
    ]


def is_blocked_key(key: Any) -> bool:
    lowered = str(key).lower()
    return any(fragment in lowered for fragment in BLOCKED_KEY_FRAGMENTS)


def sanitize_json(value: Any) -> Any:
    if isinstance(value, dict):
        sanitized = {}
        for key, item in value.items():
            if is_blocked_key(key) or has_blocked_marker(str(key)):
                continue
            sanitized[str(key)] = sanitize_json(item)
        return sanitized
    if isinstance(value, list):
        return [sanitize_json(item) for item in value]
    if isinstance(value, str):
        if has_blocked_marker(value):
            return "[redacted]"
        return value
    return value


def safe_playbook_key(value: Any, default: str) -> str:
    if not isinstance(value, str):
        return default
    if not value or is_blocked_key(value) or has_blocked_marker(value) or "/" in value or "://" in value:
        return default
    return value


def safe_status(value: Any, default: str = "ok") -> str:
    if value in {"ok", "warn", "fail"}:
        return str(value)
    return default


def safe_severity(value: Any) -> str:
    severity = str(value or "INFO").upper()
    if severity in SEVERITY_RANK:
        return severity
    return "P1"


def copy_alert_handoff_fields(source: Any) -> dict[str, Any]:
    if not isinstance(source, dict):
        return {}
    handoff = {}
    for key in ALERT_HANDOFF_KEYS:
        if key not in source or source.get(key) is None:
            continue
        value = sanitize_json(source.get(key))
        if value is None:
            continue
        if isinstance(value, str) and (not value.strip() or has_blocked_marker(value)):
            continue
        handoff[key] = value
    return handoff


def as_int(value: Any, default: int = 0) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.isdigit():
        return int(value)
    return default


def read_payload() -> dict[str, Any]:
    if json_path:
        if blocked_marker_codes(json_path, include_local_paths=False):
            raise SummaryError("summary input path was rejected")
        path = Path(json_path)
        if not path.is_file():
            raise SummaryError("summary input file is missing")
        text = path.read_text(encoding="utf-8")
    else:
        text = sys.stdin.read()

    if not text.strip():
        raise SummaryError("summary input is empty")
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as error:
        raise SummaryError(f"summary input is not valid JSON: {error}")
    if not isinstance(payload, dict):
        raise SummaryError("summary input must be a JSON object")
    return sanitize_json(payload)


def read_alert_metadata_registry() -> dict[tuple[str, str], dict[str, str]]:
    if not schema_path or blocked_marker_codes(schema_path, include_local_paths=False):
        return {}
    path = Path(schema_path)
    if not path.is_file():
        return {}
    try:
        schema = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    if not isinstance(schema, dict):
        return {}
    metadata = schema.get("alert_code_metadata")
    if not isinstance(metadata, dict):
        return {}

    registry: dict[tuple[str, str], dict[str, str]] = {}
    defaults = default_playbook_metadata()
    for section, section_items in metadata.items():
        if not isinstance(section_items, dict):
            continue
        section_key = safe_playbook_key(section, "global")
        for code, item in section_items.items():
            if not isinstance(item, dict):
                continue
            code_key = safe_playbook_key(code, "HEALTH_ALERT")
            registry[(section_key, code_key)] = {
                "owner": safe_playbook_key(item.get("owner"), defaults["owner"]),
                "default_next_action": safe_playbook_key(
                    item.get("default_next_action"), defaults["default_next_action"]
                ),
                "admin_link_target": safe_playbook_key(
                    item.get("admin_link_target"), defaults["admin_link_target"]
                ),
            }
    return registry


def default_playbook_metadata() -> dict[str, str]:
    return {
        "owner": "platform_health",
        "default_next_action": "review_full_product_health_summary",
        "admin_link_target": "admin.full_product_health.overview",
    }


def playbook_metadata_for(
    section: Any,
    code: Any,
    metadata_registry: dict[tuple[str, str], dict[str, str]],
) -> dict[str, str]:
    section_key = str(section or "")
    code_key = str(code or "HEALTH_ALERT")
    metadata = (
        metadata_registry.get((section_key, code_key))
        or metadata_registry.get(("global", code_key))
        or default_playbook_metadata()
    )
    defaults = default_playbook_metadata()
    return {
        "owner": safe_playbook_key(metadata.get("owner"), defaults["owner"]),
        "default_next_action": safe_playbook_key(
            metadata.get("default_next_action"), defaults["default_next_action"]
        ),
        "admin_link_target": safe_playbook_key(
            metadata.get("admin_link_target"), defaults["admin_link_target"]
        ),
    }


def enrich_playbook_metadata(
    item: dict[str, Any],
    metadata_registry: dict[tuple[str, str], dict[str, str]],
) -> dict[str, Any]:
    enriched = dict(item)
    enriched.update(playbook_metadata_for(enriched.get("section"), enriched.get("code"), metadata_registry))
    return sanitize_json(enriched)


def normalize_alerts(payload: dict[str, Any]) -> list[dict[str, Any]]:
    alerts = payload.get("alerts")
    if not isinstance(alerts, list):
        return []

    normalized = []
    for index, alert in enumerate(alerts):
        if not isinstance(alert, dict):
            continue
        metadata = sanitize_json(alert.get("metadata")) if isinstance(alert.get("metadata"), dict) else {}
        normalized_alert = {
            "severity": safe_severity(alert.get("severity")),
            "code": str(alert.get("code") or "HEALTH_ALERT"),
            "section": str(alert.get("section") or "admin_readiness"),
            "message": str(alert.get("message") or "health alert"),
            "metadata": metadata,
            "_index": index,
        }
        normalized_alert.update(copy_alert_handoff_fields(alert))
        for key, value in copy_alert_handoff_fields(metadata).items():
            normalized_alert.setdefault(key, value)
        normalized.append(normalized_alert)
    return [sanitize_json(alert) for alert in normalized]


def sorted_alerts(alerts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        alerts,
        key=lambda alert: (
            SEVERITY_RANK.get(str(alert.get("severity")), 9),
            as_int(alert.get("_index")),
        ),
    )


def public_alert(
    alert: dict[str, Any],
    metadata_registry: dict[tuple[str, str], dict[str, str]],
) -> dict[str, Any]:
    return enrich_playbook_metadata({
        key: value
        for key, value in alert.items()
        if key in {"severity", "code", "section", "message", *ALERT_HANDOFF_KEYS}
    }, metadata_registry)


def section_names(sections: dict[str, Any]) -> list[str]:
    names = list(DEFAULT_SECTION_NAMES)
    for name in sections.keys():
        name_text = str(name)
        if name_text not in names:
            names.append(name_text)
    return names


def alert_counts_for_section(alerts: list[dict[str, Any]], section: str) -> dict[str, int]:
    section_alerts = [alert for alert in alerts if alert.get("section") == section]
    return {
        "p0_count": sum(1 for alert in section_alerts if alert.get("severity") == "P0"),
        "p1_count": sum(1 for alert in section_alerts if alert.get("severity") == "P1"),
        "info_count": sum(1 for alert in section_alerts if alert.get("severity") == "INFO"),
    }


def build_section_views(payload: dict[str, Any], alerts: list[dict[str, Any]]) -> tuple[dict[str, str], list[dict[str, Any]]]:
    sections = payload.get("sections")
    if not isinstance(sections, dict):
        sections = {}

    statuses: dict[str, str] = {}
    checklist = []
    for name in section_names(sections):
        raw_section = sections.get(name)
        section = raw_section if isinstance(raw_section, dict) else {}
        status = safe_status(section.get("status"), "ok")
        counts = alert_counts_for_section(alerts, name)
        action_required = status != "ok" or counts["p0_count"] > 0 or counts["p1_count"] > 0
        item = {
            "section": name,
            "status": status,
            "ready": not action_required,
            "action_required": action_required,
            "p0_count": counts["p0_count"],
            "p1_count": counts["p1_count"],
            "info_count": counts["info_count"],
        }
        for key in ["live_readiness", "reason_code", "manual_review_required"]:
            if key in section:
                item[key] = sanitize_json(section[key])
        statuses[name] = status
        checklist.append(item)

    return statuses, checklist


def build_required_actions(
    alerts: list[dict[str, Any]],
    metadata_registry: dict[tuple[str, str], dict[str, str]],
) -> list[dict[str, Any]]:
    actions = []
    for alert in sorted_alerts(alerts):
        severity = alert.get("severity")
        if severity not in {"P0", "P1"}:
            continue
        action = "block_release_until_resolved" if severity == "P0" else "manual_review_before_release"
        actions.append(
            enrich_playbook_metadata(
            {
                "severity": severity,
                "code": alert.get("code"),
                "section": alert.get("section"),
                "message": alert.get("message"),
                "action": action,
            },
            metadata_registry,
        )
        )
    return [sanitize_json(item) for item in actions]


def operator_action_for_severity(severity: str) -> str:
    if severity == "P0":
        return "block_release_until_resolved"
    if severity == "P1":
        return "manual_review_before_release"
    return "observe_only"


def safe_operator_action(value: Any, severity: str) -> str:
    action = str(value or operator_action_for_severity(severity))
    if action in {
        "block_release_until_resolved",
        "manual_review_before_release",
        "observe_only",
    }:
        return action
    return operator_action_for_severity(severity)


def build_alert_taxonomy(
    payload: dict[str, Any],
    alerts: list[dict[str, Any]],
    metadata_registry: dict[tuple[str, str], dict[str, str]],
) -> list[dict[str, Any]]:
    source_taxonomy = payload.get("alert_taxonomy")
    if isinstance(source_taxonomy, list):
        taxonomy = []
        for item in source_taxonomy:
            if not isinstance(item, dict):
                continue
            section = str(item.get("section") or "admin_readiness")
            severity = safe_severity(item.get("severity"))
            correlation_keys = item.get("correlation_keys")
            if not isinstance(correlation_keys, list):
                correlation_keys = SECTION_CORRELATION_KEYS.get(section, [])
            taxonomy.append(
                enrich_playbook_metadata(
                {
                    "severity": severity,
                    "code": str(item.get("code") or "HEALTH_ALERT"),
                    "section": section,
                    "operator_action": safe_operator_action(item.get("operator_action"), severity),
                    "correlation_keys": [
                        str(key)
                        for key in correlation_keys
                        if not is_blocked_key(key) and not has_blocked_marker(str(key))
                    ],
                },
                metadata_registry,
            )
            )
        return [sanitize_json(item) for item in taxonomy]

    taxonomy = []
    for alert in alerts:
        section = str(alert.get("section") or "admin_readiness")
        severity = safe_severity(alert.get("severity"))
        taxonomy.append(
            enrich_playbook_metadata(
            {
                "severity": severity,
                "code": str(alert.get("code") or "HEALTH_ALERT"),
                "section": section,
                "operator_action": operator_action_for_severity(severity),
                "correlation_keys": SECTION_CORRELATION_KEYS.get(section, []),
            },
            metadata_registry,
        )
        )
    return [sanitize_json(item) for item in taxonomy]


def build_correlation(payload: dict[str, Any]) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    source = payload.get("correlation")
    if not isinstance(source, dict):
        source = {}
    correlation = sanitize_json(source)
    correlation_ids = []
    for key, value in correlation.items():
        if value is None:
            continue
        correlation_ids.append({"key": str(key), "value": value})
    return correlation, correlation_ids


def build_operator_playbook_summary(
    alerts: list[dict[str, Any]],
    alert_taxonomy: list[dict[str, Any]],
    metadata_registry: dict[tuple[str, str], dict[str, str]],
) -> dict[str, Any]:
    taxonomy_by_key = {
        (str(item.get("section") or ""), str(item.get("code") or "")): item
        for item in alert_taxonomy
        if isinstance(item, dict)
    }
    items = []
    for alert in sorted_alerts(alerts):
        severity = safe_severity(alert.get("severity"))
        section = str(alert.get("section") or "admin_readiness")
        code = str(alert.get("code") or "HEALTH_ALERT")
        taxonomy = taxonomy_by_key.get((section, code), {})
        item = {
            "source": "alert",
            "severity": severity,
            "code": code,
            "section": section,
            "message": str(alert.get("message") or "health alert"),
            "operator_action": operator_action_for_severity(severity),
            "correlation_keys": taxonomy.get("correlation_keys", SECTION_CORRELATION_KEYS.get(section, [])),
        }
        item.update(copy_alert_handoff_fields(alert))
        metadata = alert.get("metadata")
        if isinstance(metadata, dict) and metadata:
            item["metadata"] = metadata
        items.append(enrich_playbook_metadata(item, metadata_registry))

    blocking_count = sum(
        1 for item in items if item.get("operator_action") == "block_release_until_resolved"
    )
    manual_review_count = sum(
        1 for item in items if item.get("operator_action") == "manual_review_before_release"
    )
    observe_only_count = sum(1 for item in items if item.get("operator_action") == "observe_only")
    return sanitize_json(
        {
            "item_count": len(items),
            "blocking_item_count": blocking_count,
            "manual_review_item_count": manual_review_count,
            "observe_only_item_count": observe_only_count,
            "items": items,
        }
    )


def build_summary(payload: dict[str, Any]) -> dict[str, Any]:
    metadata_registry = read_alert_metadata_registry()
    alerts = normalize_alerts(payload)
    ordered_alerts = sorted_alerts(alerts)
    visible_top_alerts = [
        public_alert(alert, metadata_registry) for alert in ordered_alerts[:top_alert_limit]
    ]
    section_statuses, checklist = build_section_views(payload, alerts)
    required_actions = build_required_actions(alerts, metadata_registry)
    alert_taxonomy = build_alert_taxonomy(payload, alerts, metadata_registry)
    operator_playbook_summary = build_operator_playbook_summary(
        alerts, alert_taxonomy, metadata_registry
    )
    correlation, correlation_ids = build_correlation(payload)

    p0_count = sum(1 for alert in alerts if alert.get("severity") == "P0")
    p1_count = sum(1 for alert in alerts if alert.get("severity") == "P1")
    info_count = sum(1 for alert in alerts if alert.get("severity") == "INFO")
    source_summary = payload.get("summary")
    source_summary = source_summary if isinstance(source_summary, dict) else {}
    overall_status = safe_status(payload.get("status"), "ok")

    summary = {
        "overall_status": overall_status,
        "p0_count": p0_count if alerts else as_int(source_summary.get("p0_count")),
        "p1_count": p1_count if alerts else as_int(source_summary.get("p1_count")),
        "info_count": info_count if alerts else as_int(source_summary.get("info_count")),
        "section_count": len(section_statuses),
        "blocking_section_count": sum(1 for status in section_statuses.values() if status == "fail"),
        "warning_section_count": sum(1 for status in section_statuses.values() if status == "warn"),
        "top_alert_count": len(visible_top_alerts),
        "required_operator_action_count": len(required_actions),
        "alert_taxonomy_count": len(alert_taxonomy),
        "operator_playbook_item_count": operator_playbook_summary["item_count"],
        "correlation_id_count": len(correlation_ids),
        "read_only_input_count": as_int(source_summary.get("read_only_input_count")),
        "wallet_payment_exception_count": as_int(source_summary.get("wallet_payment_exception_count")),
        "payment_entitlement_blocker_count": as_int(source_summary.get("payment_entitlement_blocker_count")),
    }

    return sanitize_json(
        {
            "schema_version": schema_version,
            "source_schema_version": as_int(payload.get("schema_version"), 0),
            "status": overall_status,
            "generated_at": now_utc(),
            "source_generated_at": sanitize_json(payload.get("generated_at")),
            "summary": summary,
            "section_statuses": section_statuses,
            "checklist": checklist,
            "top_alerts": visible_top_alerts,
            "required_operator_actions": required_actions,
            "alert_taxonomy": alert_taxonomy,
            "operator_playbook_summary": operator_playbook_summary,
            "correlation": correlation,
            "correlation_ids": correlation_ids,
        }
    )


def fail_payload(message: str) -> dict[str, Any]:
    return {
        "schema_version": schema_version,
        "source_schema_version": 0,
        "status": "fail",
        "generated_at": now_utc(),
        "source_generated_at": None,
        "summary": {
            "overall_status": "fail",
            "p0_count": 1,
            "p1_count": 0,
            "info_count": 0,
            "section_count": 0,
            "blocking_section_count": 0,
            "warning_section_count": 0,
            "top_alert_count": 1,
            "required_operator_action_count": 1,
            "alert_taxonomy_count": 1,
            "operator_playbook_item_count": 1,
            "correlation_id_count": 0,
            "read_only_input_count": 0,
        },
        "section_statuses": {},
        "checklist": [],
        "top_alerts": [
            {
                "severity": "P0",
                "code": "FULL_PRODUCT_HEALTH_SUMMARY_FAILED",
                "section": "admin_readiness",
                "message": message,
            }
        ],
        "required_operator_actions": [
            {
                "severity": "P0",
                "code": "FULL_PRODUCT_HEALTH_SUMMARY_FAILED",
                "section": "admin_readiness",
                "message": message,
                "action": "block_release_until_resolved",
            }
        ],
        "alert_taxonomy": [
            {
                "severity": "P0",
                "code": "FULL_PRODUCT_HEALTH_SUMMARY_FAILED",
                "section": "admin_readiness",
                "operator_action": "block_release_until_resolved",
                "correlation_keys": SECTION_CORRELATION_KEYS["admin_readiness"],
            }
        ],
        "operator_playbook_summary": {
            "item_count": 1,
            "blocking_item_count": 1,
            "manual_review_item_count": 0,
            "observe_only_item_count": 0,
            "items": [
                {
                    "source": "alert",
                    "severity": "P0",
                    "code": "FULL_PRODUCT_HEALTH_SUMMARY_FAILED",
                    "section": "admin_readiness",
                    "message": message,
                    "operator_action": "block_release_until_resolved",
                    "owner": "platform_health",
                    "default_next_action": "inspect_summary_failure",
                    "admin_link_target": "admin.full_product_health.summary",
                    "correlation_keys": SECTION_CORRELATION_KEYS["admin_readiness"],
                }
            ],
        },
        "correlation": {},
        "correlation_ids": [],
    }


try:
    output = build_summary(read_payload())
    exit_code = 0
except SummaryError as error:
    output = sanitize_json(fail_payload(str(error)))
    exit_code = 1

rendered = json.dumps(output, ensure_ascii=True, indent=2)
if has_blocked_marker(rendered):
    output = sanitize_json(fail_payload("summary output was rejected"))
    rendered = json.dumps(output, ensure_ascii=True, indent=2)
    exit_code = 1

print(rendered)
sys.exit(exit_code)
PY
