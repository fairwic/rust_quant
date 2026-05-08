#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${FULL_PRODUCT_HEALTH_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_FIXTURE_PATH:=${REPO_ROOT}/docs/dev/full_product_health_aggregator.fixture.json}"
: "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH:=${REPO_ROOT}/scripts/dev/check_local_service_health.sh}"
: "${FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH:=true}"
: "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_WEB_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_NEWS_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH:=}"

if [[ "${FULL_PRODUCT_HEALTH_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - \
    "${REPO_ROOT}" \
    "${FULL_PRODUCT_HEALTH_SCHEMA_VERSION}" \
    "${FULL_PRODUCT_HEALTH_FIXTURE_PATH}" \
    "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH}" \
    <<'PY'
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


repo_root = Path(sys.argv[1])
schema_version = int(sys.argv[2])
fixture_path = Path(sys.argv[3])
local_health_script_path = Path(sys.argv[4])

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
    "payload",
]

DEFAULT_SECTION_NAMES = [
    "web_task_order_health",
    "news_source_ai_health",
    "quant_worker_checkpoint_audit",
    "admin_readiness",
]

DEFAULT_CORRELATION_KEYS = [
    "news_id",
    "analysis_result_id",
    "signal_inbox_id",
    "external_id",
    "execution_task_id",
    "execution_attempt_id",
    "request_id",
    "order_result_id",
    "trade_record_id",
    "worker_id",
    "admin_operation_log_id",
    "admin_module",
    "admin_action",
]

LOCAL_ALERT_CODE_MAP = {
    "EXPECTED_WORKER_STALE": "QUANT_EXPECTED_WORKER_STALE",
    "EXCHANGE_REQUEST_AUDIT_FAILURES": "QUANT_EXCHANGE_AUDIT_FAILURES",
    "WORKER_LEASE_STALE": "QUANT_WORKER_LEASE_STALE",
    "IGNORED_STALE_WORKER": "IGNORED_HISTORICAL_WORKER",
    "EXECUTION_AUDIT_TABLE_MISSING": "EXECUTION_AUDIT_TABLE_MISSING",
    "HEALTH_CHECK_FAIL": "QUANT_LOCAL_HEALTH_FAIL",
    "HEALTH_CHECK_WARN": "QUANT_LOCAL_HEALTH_WARN",
}


class CollectorError(Exception):
    def __init__(self, code: str, message: str, severity: str = "P0") -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.severity = severity


def env_flag(name: str, default: bool) -> bool:
    value = os.environ.get(name)
    if value is None:
        return default
    return value.lower() in {"1", "true", "yes", "on"}


def safe_status(value: Any, default: str = "ok") -> str:
    if value in {"ok", "warn", "fail"}:
        return str(value)
    return default


def as_int(value: Any, default: int = 0) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.isdigit():
        return int(value)
    return default


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
            if is_blocked_key(key):
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


def read_json_path(path_text: str, label: str) -> dict[str, Any] | None:
    if not path_text:
        return None
    path = Path(path_text)
    if ".env" in str(path).lower():
        raise CollectorError("UNSAFE_INPUT_REJECTED", f"{label} input was rejected")
    if not path.is_file():
        raise CollectorError("INPUT_FILE_MISSING", f"{label} input file is missing")
    text = path.read_text(encoding="utf-8")
    if has_blocked_marker(text):
        raise CollectorError("UNSAFE_INPUT_REJECTED", f"{label} input was rejected")
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as error:
        raise CollectorError("INPUT_JSON_INVALID", f"{label} input is not valid JSON: {error}")
    if not isinstance(payload, dict):
        raise CollectorError("INPUT_JSON_INVALID", f"{label} input must be a JSON object")
    return sanitize_json(payload)


def load_schema_from_fixture() -> tuple[list[str], list[str]]:
    if not fixture_path.is_file():
        return DEFAULT_SECTION_NAMES, DEFAULT_CORRELATION_KEYS
    text = fixture_path.read_text(encoding="utf-8")
    if has_blocked_marker(text):
        raise CollectorError("UNSAFE_SCHEMA_REJECTED", "schema fixture was rejected")
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as error:
        raise CollectorError("SCHEMA_JSON_INVALID", f"schema fixture is not valid JSON: {error}")

    section_names = DEFAULT_SECTION_NAMES
    sections = payload.get("sections")
    if isinstance(sections, dict) and sections:
        section_names = [str(key) for key in sections.keys()]

    correlation_keys = DEFAULT_CORRELATION_KEYS
    correlation = payload.get("correlation")
    if isinstance(correlation, dict) and correlation:
        correlation_keys = [str(key) for key in correlation.keys()]

    return section_names, correlation_keys


def base_sections(section_names: list[str]) -> dict[str, dict[str, Any]]:
    sections: dict[str, dict[str, Any]] = {
        name: {"status": "ok", "source": "not_provided", "read_only_input": False}
        for name in section_names
    }
    sections.setdefault(
        "web_task_order_health",
        {"status": "ok", "source": "not_provided", "read_only_input": False},
    )
    sections.setdefault(
        "news_source_ai_health",
        {"status": "ok", "source": "not_provided", "read_only_input": False},
    )
    sections.setdefault(
        "quant_worker_checkpoint_audit",
        {"status": "ok", "source": "not_provided", "read_only_input": False},
    )
    sections.setdefault(
        "admin_readiness",
        {"status": "ok", "source": "not_provided", "read_only_input": False},
    )
    return sections


def base_payload() -> dict[str, Any]:
    section_names, correlation_keys = load_schema_from_fixture()
    return {
        "schema_version": schema_version,
        "status": "ok",
        "generated_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "summary": {
            "p0_count": 0,
            "p1_count": 0,
            "info_count": 0,
            "web_open_task_count": 0,
            "news_degraded_source_count": 0,
            "quant_expected_worker_failures": 0,
            "quant_expected_worker_warnings": 0,
            "quant_execution_audit_recent_failures": 0,
            "quant_execution_audit_stale_leased_workers": 0,
            "read_only_input_count": 0,
        },
        "sections": base_sections(section_names),
        "alerts": [],
        "correlation": {key: None for key in correlation_keys},
    }


def append_alert(
    payload: dict[str, Any],
    severity: str,
    code: str,
    section: str,
    message: str,
) -> None:
    payload["alerts"].append(
        sanitize_json(
            {
                "severity": severity if severity in {"P0", "P1", "INFO"} else "P1",
                "code": code,
                "section": section,
                "message": message,
            }
        )
    )


def merge_correlation(payload: dict[str, Any], data: dict[str, Any]) -> None:
    correlation = data.get("correlation")
    if not isinstance(correlation, dict):
        return
    for key, value in correlation.items():
        key_text = str(key)
        if is_blocked_key(key_text) or has_blocked_marker(key_text):
            continue
        payload["correlation"].setdefault(key_text, None)
        payload["correlation"][key_text] = sanitize_json(value)


def merge_alerts(payload: dict[str, Any], data: dict[str, Any], default_section: str) -> None:
    alerts = data.get("alerts")
    if not isinstance(alerts, list):
        return
    for alert in alerts:
        if not isinstance(alert, dict):
            continue
        append_alert(
            payload,
            str(alert.get("severity", "P1")),
            str(alert.get("code", "SECTION_ALERT")),
            str(alert.get("section", default_section)),
            str(alert.get("message", "read-only section alert")),
        )


def merge_section_input(
    payload: dict[str, Any],
    section_name: str,
    env_name: str,
    summary_count_key: str | None = None,
    input_count_key: str | None = None,
) -> None:
    data = read_json_path(os.environ.get(env_name, ""), section_name)
    if data is None:
        return

    section = {
        key: value
        for key, value in data.items()
        if key not in {"summary", "alerts", "correlation"}
    }
    section["source"] = "json_path"
    section["read_only_input"] = True
    section["status"] = safe_status(section.get("status"), "ok")
    payload["sections"][section_name].update(sanitize_json(section))
    payload["summary"]["read_only_input_count"] += 1

    if summary_count_key and input_count_key:
        payload["summary"][summary_count_key] = as_int(section.get(input_count_key), 0)

    summary = data.get("summary")
    if isinstance(summary, dict):
        if summary_count_key and input_count_key:
            payload["summary"][summary_count_key] = as_int(summary.get(input_count_key), payload["summary"][summary_count_key])

    merge_alerts(payload, data, section_name)
    merge_correlation(payload, data)


def run_local_health_script() -> dict[str, Any] | None:
    if not env_flag("FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH", True):
        return None
    if not local_health_script_path.is_file():
        raise CollectorError("LOCAL_HEALTH_SCRIPT_MISSING", "local health script is missing", "P1")

    timeout_secs = as_int(os.environ.get("FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS"), 15)
    local_env = {
        "PATH": os.environ.get("PATH", ""),
        "HEALTH_CHECK_OUTPUT": "json",
        "HEALTH_CHECK_DATABASES": "false",
        "HEALTH_CHECK_BINANCE": "false",
        "HEALTH_CHECK_EXECUTION_AUDIT": "false",
        "HEALTH_CHECK_STRICT": "false",
    }
    result = subprocess.run(
        ["bash", str(local_health_script_path)],
        env=local_env,
        text=True,
        capture_output=True,
        timeout=timeout_secs,
    )
    if not result.stdout.strip():
        raise CollectorError("LOCAL_HEALTH_OUTPUT_EMPTY", "local health output was empty", "P1")
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise CollectorError("LOCAL_HEALTH_JSON_INVALID", f"local health output is not valid JSON: {error}", "P1")
    if not isinstance(payload, dict):
        raise CollectorError("LOCAL_HEALTH_JSON_INVALID", "local health output must be a JSON object", "P1")
    return sanitize_json(payload)


def collect_local_health() -> dict[str, Any] | None:
    path_input = read_json_path(
        os.environ.get("FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH", ""),
        "quant_worker_checkpoint_audit",
    )
    if path_input is not None:
        return path_input
    return run_local_health_script()


def merge_local_health(payload: dict[str, Any], local_health: dict[str, Any] | None) -> None:
    section = payload["sections"]["quant_worker_checkpoint_audit"]
    section.update(
        {
            "status": "ok",
            "source": "not_provided",
            "read_only_input": False,
            "local_health_status": "not_collected",
            "expected_worker_failures": 0,
            "expected_worker_warnings": 0,
            "ignored_historical_worker_count": 0,
            "ignored_worker_count": 0,
            "exchange_audit_recent_failures": 0,
            "exchange_audit_stale_leased_workers": 0,
        }
    )
    if local_health is None:
        return

    summary = local_health.get("summary") if isinstance(local_health.get("summary"), dict) else {}
    local_status = safe_status(local_health.get("status"), "ok")
    section.update(
        {
            "status": local_status,
            "source": "local_json_path" if os.environ.get("FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH") else "check_local_service_health.sh",
            "read_only_input": True,
            "local_health_status": local_status,
            "expected_worker_failures": as_int(summary.get("expected_worker_failures")),
            "expected_worker_warnings": as_int(summary.get("expected_worker_warnings")),
            "ignored_historical_worker_count": as_int(summary.get("ignored_stale_worker_count")),
            "ignored_worker_count": as_int(summary.get("ignored_worker_count")),
            "exchange_audit_recent_failures": as_int(summary.get("execution_audit_recent_failures")),
            "exchange_audit_stale_leased_workers": as_int(summary.get("execution_audit_stale_leased_workers")),
        }
    )
    payload["summary"]["read_only_input_count"] += 1
    payload["summary"]["quant_expected_worker_failures"] = section["expected_worker_failures"]
    payload["summary"]["quant_expected_worker_warnings"] = section["expected_worker_warnings"]
    payload["summary"]["quant_execution_audit_recent_failures"] = section["exchange_audit_recent_failures"]
    payload["summary"]["quant_execution_audit_stale_leased_workers"] = section["exchange_audit_stale_leased_workers"]

    local_alerts = local_health.get("alerts")
    if isinstance(local_alerts, list):
        for alert in local_alerts:
            if not isinstance(alert, dict):
                continue
            source_code = str(alert.get("code", "HEALTH_CHECK_WARN"))
            append_alert(
                payload,
                str(alert.get("severity", "P1")),
                LOCAL_ALERT_CODE_MAP.get(source_code, f"QUANT_{source_code}"),
                "quant_worker_checkpoint_audit",
                str(alert.get("message", "local health alert")),
            )

    if local_status == "fail" and not any(
        alert.get("section") == "quant_worker_checkpoint_audit" and alert.get("severity") == "P0"
        for alert in payload["alerts"]
    ):
        append_alert(
            payload,
            "P0",
            "QUANT_LOCAL_HEALTH_FAIL",
            "quant_worker_checkpoint_audit",
            "local health reported fail",
        )
    elif local_status == "warn" and not any(
        alert.get("section") == "quant_worker_checkpoint_audit" and alert.get("severity") == "P1"
        for alert in payload["alerts"]
    ):
        append_alert(
            payload,
            "P1",
            "QUANT_LOCAL_HEALTH_WARN",
            "quant_worker_checkpoint_audit",
            "local health reported warn",
        )


def finalize(payload: dict[str, Any]) -> dict[str, Any]:
    append_alert(
        payload,
        "INFO",
        "READ_ONLY_COLLECTOR_ACTIVE",
        "admin_readiness",
        "Read-only collector used local JSON, schema, or subprocess inputs only; live actions were not executed.",
    )

    p0_count = sum(1 for alert in payload["alerts"] if alert.get("severity") == "P0")
    p1_count = sum(1 for alert in payload["alerts"] if alert.get("severity") == "P1")
    info_count = sum(1 for alert in payload["alerts"] if alert.get("severity") == "INFO")
    payload["summary"]["p0_count"] = p0_count
    payload["summary"]["p1_count"] = p1_count
    payload["summary"]["info_count"] = info_count

    if p0_count > 0:
        payload["status"] = "fail"
        payload["sections"]["admin_readiness"].update(
            {
                "status": "fail",
                "live_readiness": "blocked",
                "reason_code": "health_p0_alert",
                "manual_review_required": True,
            }
        )
    elif p1_count > 0:
        payload["status"] = "warn"
        payload["sections"]["admin_readiness"].update(
            {
                "status": "warn",
                "live_readiness": "manual_review",
                "reason_code": "health_p1_alert",
                "manual_review_required": True,
            }
        )
    else:
        payload["status"] = "ok"
        payload["sections"]["admin_readiness"].update(
            {
                "status": "ok",
                "live_readiness": "manual_review",
                "reason_code": "read_only_collector",
                "manual_review_required": True,
            }
        )

    payload["sections"]["admin_readiness"]["source"] = "derived"
    payload["sections"]["admin_readiness"]["read_only_input"] = True
    return sanitize_json(payload)


def fail_payload(error: CollectorError) -> dict[str, Any]:
    payload = base_payload()
    append_alert(payload, error.severity, error.code, "admin_readiness", error.message)
    return finalize(payload)


try:
    payload = base_payload()
    merge_local_health(payload, collect_local_health())
    merge_section_input(
        payload,
        "web_task_order_health",
        "FULL_PRODUCT_HEALTH_WEB_JSON_PATH",
        "web_open_task_count",
        "open_task_count",
    )
    merge_section_input(
        payload,
        "news_source_ai_health",
        "FULL_PRODUCT_HEALTH_NEWS_JSON_PATH",
        "news_degraded_source_count",
        "degraded_source_count",
    )
    merge_section_input(payload, "admin_readiness", "FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH")
    payload = finalize(payload)
except CollectorError as error:
    payload = fail_payload(error)

rendered = json.dumps(payload, ensure_ascii=True, indent=2)
if has_blocked_marker(rendered):
    payload = fail_payload(CollectorError("COLLECTOR_OUTPUT_REJECTED", "collector output was rejected"))
    rendered = json.dumps(payload, ensure_ascii=True, indent=2)

print(rendered)
PY
