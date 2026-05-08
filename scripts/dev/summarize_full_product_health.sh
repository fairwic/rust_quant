#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT:=5}"

if [[ "${FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION}" \
    "${FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH}" \
    "${FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT}" \
    <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


schema_version = int(sys.argv[1])
json_path = sys.argv[2]
top_alert_limit = max(int(sys.argv[3]), 0)

BLOCKED_MARKERS = [
    ".env",
    "postgres://",
    "mysql://",
    "database_url",
    "api_key",
    "apikey",
    "api key",
    "api_secret",
    "apisecret",
    "api secret",
    "secret",
    "request_payload",
    "response_payload",
    "raw_payload",
    "request payload",
    "response payload",
    "/fapi/v1/order",
    "/fapi/v2/account",
    "/fapi/v1/positionRisk",
    "/fapi/v2/positionRisk",
    "/fapi/v1/positionSide/dual",
    "/api/commerce/internal/execution-tasks/lease",
    "/api/commerce/internal/execution-results",
    "/api/commerce/internal/order-results",
    "LINKUSDT",
    "LINK-USDT",
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
}


class SummaryError(Exception):
    pass


def now_utc() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def has_blocked_marker(value: str) -> bool:
    lowered = value.lower()
    return any(marker.lower() in lowered for marker in BLOCKED_MARKERS)


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


def safe_status(value: Any, default: str = "ok") -> str:
    if value in {"ok", "warn", "fail"}:
        return str(value)
    return default


def safe_severity(value: Any) -> str:
    severity = str(value or "INFO").upper()
    if severity in SEVERITY_RANK:
        return severity
    return "P1"


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
        if ".env" in json_path.lower():
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


def normalize_alerts(payload: dict[str, Any]) -> list[dict[str, Any]]:
    alerts = payload.get("alerts")
    if not isinstance(alerts, list):
        return []

    normalized = []
    for index, alert in enumerate(alerts):
        if not isinstance(alert, dict):
            continue
        normalized.append(
            {
                "severity": safe_severity(alert.get("severity")),
                "code": str(alert.get("code") or "HEALTH_ALERT"),
                "section": str(alert.get("section") or "admin_readiness"),
                "message": str(alert.get("message") or "health alert"),
                "_index": index,
            }
        )
    return [sanitize_json(alert) for alert in normalized]


def sorted_alerts(alerts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        alerts,
        key=lambda alert: (
            SEVERITY_RANK.get(str(alert.get("severity")), 9),
            as_int(alert.get("_index")),
        ),
    )


def public_alert(alert: dict[str, Any]) -> dict[str, Any]:
    return {
        key: value
        for key, value in alert.items()
        if key in {"severity", "code", "section", "message"}
    }


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


def build_required_actions(alerts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    actions = []
    for alert in sorted_alerts(alerts):
        severity = alert.get("severity")
        if severity not in {"P0", "P1"}:
            continue
        action = "block_release_until_resolved" if severity == "P0" else "manual_review_before_release"
        actions.append(
            {
                "severity": severity,
                "code": alert.get("code"),
                "section": alert.get("section"),
                "message": alert.get("message"),
                "action": action,
            }
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


def build_alert_taxonomy(payload: dict[str, Any], alerts: list[dict[str, Any]]) -> list[dict[str, Any]]:
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
                }
            )
        return [sanitize_json(item) for item in taxonomy]

    taxonomy = []
    for alert in alerts:
        section = str(alert.get("section") or "admin_readiness")
        severity = safe_severity(alert.get("severity"))
        taxonomy.append(
            {
                "severity": severity,
                "code": str(alert.get("code") or "HEALTH_ALERT"),
                "section": section,
                "operator_action": operator_action_for_severity(severity),
                "correlation_keys": SECTION_CORRELATION_KEYS.get(section, []),
            }
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


def build_summary(payload: dict[str, Any]) -> dict[str, Any]:
    alerts = normalize_alerts(payload)
    ordered_alerts = sorted_alerts(alerts)
    visible_top_alerts = [public_alert(alert) for alert in ordered_alerts[:top_alert_limit]]
    section_statuses, checklist = build_section_views(payload, alerts)
    required_actions = build_required_actions(alerts)
    alert_taxonomy = build_alert_taxonomy(payload, alerts)
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
        "correlation_id_count": len(correlation_ids),
        "read_only_input_count": as_int(source_summary.get("read_only_input_count")),
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
