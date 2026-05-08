#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_ADMIN_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS:=7200}"
: "${FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN:=psql}"

if [[ "${FULL_PRODUCT_HEALTH_ADMIN_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_ADMIN_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL}" \
    "${FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS}" \
    "${FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS}" \
    "${FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN}" \
    <<'PY'
import json
import os
import shutil
import subprocess
import sys
from typing import Any


database_url = sys.argv[1].strip()
lookback_secs = int(sys.argv[2])
query_timeout_secs = int(sys.argv[3])
psql_bin = sys.argv[4]

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
    "api_key_cipher",
    "api_secret_cipher",
    "passphrase_cipher",
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
    "linkusdt",
    "link-usdt",
]

BLOCKED_KEY_FRAGMENTS = [
    "database_url",
    "api_key",
    "apikey",
    "api_secret",
    "apisecret",
    "secret",
    "cipher",
    "passphrase",
    "request_payload",
    "response_payload",
    "raw_payload",
    "payload",
    "admin_username",
    "target_id",
]


def as_int(value: Any, default: int = 0) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        try:
            return int(value)
        except ValueError:
            return default
    return default


def safe_positive_int(value: int, default: int) -> int:
    return value if value > 0 else default


lookback_secs = safe_positive_int(lookback_secs, 7200)
query_timeout_secs = safe_positive_int(query_timeout_secs, 15)


def has_blocked_marker(value: str) -> bool:
    lowered = value.lower()
    return any(marker.lower() in lowered for marker in BLOCKED_MARKERS)


def is_blocked_key(key: Any) -> bool:
    lowered = str(key).lower()
    return any(fragment in lowered for fragment in BLOCKED_KEY_FRAGMENTS)


def sanitize_json(value: Any) -> Any:
    if isinstance(value, dict):
        sanitized: dict[str, Any] = {}
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


def base_payload(status: str, source: str, read_only_input: bool) -> dict[str, Any]:
    return {
        "status": status,
        "source": source,
        "database_engine": "postgresql",
        "read_only_input": read_only_input,
        "lookback_secs": lookback_secs,
        "required_action_count": 0,
        "recent_operation_count": 0,
        "high_risk_operation_count": 0,
        "failed_operation_count": 0,
        "missing_required_action_count": 0,
        "readiness_blocker_count": 0,
        "manual_review_count": 0,
        "live_readiness": "manual_review",
        "reason_code": "admin_input_not_collected",
        "manual_review_required": True,
        "sample": {},
        "alerts": [],
        "correlation": {
            "admin_operation_log_id": None,
            "admin_module": None,
            "admin_action": None,
        },
    }


def append_alert(
    payload: dict[str, Any],
    severity: str,
    code: str,
    message: str,
) -> None:
    payload["alerts"].append(
        {
            "severity": severity if severity in {"P0", "P1", "INFO"} else "P1",
            "code": code,
            "section": "admin_readiness",
            "message": message,
        }
    )


def skipped_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "skipped", False)
    payload["skipped"] = True
    append_alert(payload, "INFO", code, message)
    return payload


def degraded_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "quant_admin_readonly_db", True)
    payload["query_failed"] = True
    append_alert(payload, "P1", code, message)
    return payload


def render(payload: dict[str, Any]) -> None:
    sanitized = sanitize_json(payload)
    rendered = json.dumps(sanitized, ensure_ascii=True, indent=2)
    if has_blocked_marker(rendered):
        fallback = base_payload("warn", "redacted", False)
        fallback["skipped"] = True
        append_alert(
            fallback,
            "P1",
            "ADMIN_INPUT_OUTPUT_REJECTED",
            "Admin health input contained unsafe content and was replaced.",
        )
        rendered = json.dumps(sanitize_json(fallback), ensure_ascii=True, indent=2)
    print(rendered)


def postgres_health_sql() -> str:
    return f"""
WITH params AS (
    SELECT {lookback_secs}::int AS lookback_secs
),
required_actions(module, action) AS (
    VALUES
        ('quant_execution_tasks', 'risk_review_confirm'),
        ('quant_execution_tasks', 'risk_review_cancel'),
        ('quant_api_keys', 'api_key_upsert'),
        ('quant_onchain_provider_controls', 'onchain_provider_control_upsert'),
        ('quant_strategy_configs', 'strategy_config_upsert'),
        ('quant_backtests', 'backtest_run'),
        ('quant_exchange_symbols', 'exchange_symbol_sync'),
        ('quant_news_analysis', 'manual_ai_analysis')
),
recent_logs AS (
    SELECT
        id,
        module,
        action,
        created_at,
        payload,
        lower(COALESCE(payload ->> 'outcome', payload ->> 'status', '')) AS outcome_text,
        lower(COALESCE(payload ->> 'liveReadiness', payload ->> 'readinessStatus', '')) AS readiness_text,
        CASE
            WHEN jsonb_typeof(payload -> 'blockers') = 'array'
            THEN jsonb_array_length(payload -> 'blockers')
            ELSE 0
        END AS blocker_count,
        EXTRACT(EPOCH FROM (NOW() - created_at))::bigint AS age_secs
    FROM admin_operation_logs
    CROSS JOIN params p
    WHERE created_at >= NOW() - make_interval(secs => p.lookback_secs)
),
high_risk_logs AS (
    SELECT rl.*
    FROM recent_logs rl
    LEFT JOIN required_actions ra
      ON ra.module = rl.module
     AND ra.action = rl.action
    WHERE ra.action IS NOT NULL
       OR rl.module LIKE 'quant_%'
),
action_coverage AS (
    SELECT
        ra.module,
        ra.action,
        COUNT(rl.id)::int AS recent_count
    FROM required_actions ra
    LEFT JOIN recent_logs rl
      ON rl.module = ra.module
     AND rl.action = ra.action
    GROUP BY ra.module, ra.action
),
summary AS (
    SELECT
        (SELECT COUNT(*)::int FROM required_actions) AS required_action_count,
        (SELECT COUNT(*)::int FROM recent_logs) AS recent_operation_count,
        (SELECT COUNT(*)::int FROM high_risk_logs) AS high_risk_operation_count,
        (
            SELECT COUNT(*)::int
            FROM high_risk_logs
            WHERE outcome_text IN ('failed', 'failure', 'error')
        ) AS failed_operation_count,
        (
            SELECT COUNT(*)::int
            FROM action_coverage
            WHERE recent_count = 0
        ) AS missing_required_action_count,
        (
            SELECT COUNT(*)::int
            FROM high_risk_logs
            WHERE readiness_text = 'blocked'
               OR outcome_text = 'blocked'
               OR blocker_count > 0
        ) AS readiness_blocker_count,
        (
            SELECT COUNT(*)::int
            FROM high_risk_logs
            WHERE readiness_text IN ('manual_review', 'review_required')
               OR lower(COALESCE(payload ->> 'manualReviewRequired', 'false')) = 'true'
        ) AS manual_review_count
),
sample_row AS (
    SELECT
        id,
        module,
        action,
        COALESCE(NULLIF(outcome_text, ''), NULLIF(readiness_text, ''), 'unknown') AS outcome,
        age_secs
    FROM high_risk_logs
    ORDER BY
        CASE
            WHEN readiness_text = 'blocked' OR blocker_count > 0 THEN 0
            WHEN outcome_text IN ('failed', 'failure', 'error') THEN 1
            WHEN readiness_text IN ('manual_review', 'review_required') THEN 2
            ELSE 3
        END,
        created_at DESC,
        id DESC
    LIMIT 1
)
SELECT json_build_object(
    'status',
        CASE
            WHEN readiness_blocker_count > 0 OR missing_required_action_count > 0 THEN 'fail'
            WHEN failed_operation_count > 0 OR manual_review_count > 0 THEN 'warn'
            ELSE 'ok'
        END,
    'source', 'quant_admin_readonly_db',
    'database_engine', 'postgresql',
    'read_only_input', TRUE,
    'lookback_secs', (SELECT lookback_secs FROM params),
    'required_action_count', required_action_count,
    'recent_operation_count', recent_operation_count,
    'high_risk_operation_count', high_risk_operation_count,
    'failed_operation_count', failed_operation_count,
    'missing_required_action_count', missing_required_action_count,
    'readiness_blocker_count', readiness_blocker_count,
    'manual_review_count', manual_review_count,
    'live_readiness',
        CASE
            WHEN readiness_blocker_count > 0 OR missing_required_action_count > 0 THEN 'blocked'
            WHEN failed_operation_count > 0 OR manual_review_count > 0 THEN 'manual_review'
            ELSE 'manual_review'
        END,
    'reason_code',
        CASE
            WHEN readiness_blocker_count > 0 OR missing_required_action_count > 0 THEN 'admin_readiness_blocked'
            WHEN failed_operation_count > 0 OR manual_review_count > 0 THEN 'admin_review_required'
            ELSE 'admin_readonly_collector'
        END,
    'manual_review_required', TRUE,
    'sample',
        COALESCE(
            (
                SELECT json_build_object(
                    'admin_operation_log_id', id,
                    'module', module,
                    'action', action,
                    'outcome', outcome,
                    'age_secs', age_secs
                )
                FROM sample_row
            ),
            '{{}}'::json
        ),
    'alerts',
        (
            SELECT COALESCE(json_agg(alert), '[]'::json)
            FROM (
                SELECT json_build_object(
                    'severity', 'P0',
                    'code', 'ADMIN_LIVE_READINESS_BLOCKED',
                    'section', 'admin_readiness',
                    'message', 'Admin readiness has blockers or required audit coverage is missing.'
                ) AS alert
                WHERE readiness_blocker_count > 0 OR missing_required_action_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'ADMIN_HIGH_RISK_OPERATION_FAILED',
                    'section', 'admin_readiness',
                    'message', 'Recent high-risk admin operation failed.'
                ) AS alert
                WHERE failed_operation_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'ADMIN_ACTION_AUDIT_MISSING',
                    'section', 'admin_readiness',
                    'message', 'One or more required high-risk admin actions have no recent audit log.'
                ) AS alert
                WHERE missing_required_action_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'ADMIN_READINESS_REVIEW_REQUIRED',
                    'section', 'admin_readiness',
                    'message', 'Admin readiness still requires manual review.'
                ) AS alert
                WHERE manual_review_count > 0
            ) alert_rows
        ),
    'correlation',
        COALESCE(
            (
                SELECT json_build_object(
                    'admin_operation_log_id', id,
                    'admin_module', module,
                    'admin_action', action
                )
                FROM sample_row
            ),
            json_build_object(
                'admin_operation_log_id', NULL,
                'admin_module', NULL,
                'admin_action', NULL
            )
        )
)::text
FROM summary;
"""


def run_postgres_query() -> dict[str, Any]:
    if shutil.which(psql_bin) is None:
        return skipped_payload("ADMIN_INPUT_SKIPPED", "psql was not available for the read-only Admin input.")

    result = subprocess.run(
        [psql_bin, database_url, "-v", "ON_ERROR_STOP=1", "-Atc", postgres_health_sql()],
        text=True,
        capture_output=True,
        timeout=query_timeout_secs,
        env={
            "PATH": os.environ.get("PATH", ""),
            "PGCONNECT_TIMEOUT": str(min(query_timeout_secs, 10)),
        },
    )
    if result.returncode != 0:
        return degraded_payload("ADMIN_INPUT_QUERY_FAILED", "Read-only Admin health query failed.")

    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        return degraded_payload("ADMIN_INPUT_QUERY_EMPTY", "Read-only Admin health query returned no JSON.")

    try:
        payload = json.loads(lines[-1])
    except json.JSONDecodeError:
        return degraded_payload("ADMIN_INPUT_JSON_INVALID", "Read-only Admin health query returned invalid JSON.")
    if not isinstance(payload, dict):
        return degraded_payload("ADMIN_INPUT_JSON_INVALID", "Read-only Admin health query returned a non-object JSON value.")
    return payload


if not database_url:
    render(skipped_payload("ADMIN_INPUT_SKIPPED", "Explicit read-only Admin database input was not provided."))
elif ".env" in database_url.lower():
    render(skipped_payload("ADMIN_INPUT_SKIPPED", "Explicit read-only Admin database input was rejected."))
elif not database_url.lower().startswith(("postgres://", "postgresql://")):
    render(skipped_payload("ADMIN_INPUT_SKIPPED", "Only PostgreSQL Admin input is supported by this producer."))
else:
    render(run_postgres_query())
PY
