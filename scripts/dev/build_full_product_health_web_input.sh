#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_WEB_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS:=900}"
: "${FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS:=900}"
: "${FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_WEB_PSQL_BIN:=psql}"

if [[ "${FULL_PRODUCT_HEALTH_WEB_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_WEB_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL}" \
    "${FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS}" \
    "${FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS}" \
    "${FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS}" \
    "${FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS}" \
    "${FULL_PRODUCT_HEALTH_WEB_PSQL_BIN}" \
    <<'PY'
import json
import os
import shutil
import subprocess
import sys
from typing import Any


database_url = sys.argv[1].strip()
lookback_secs = int(sys.argv[2])
stale_task_secs = int(sys.argv[3])
missing_result_secs = int(sys.argv[4])
query_timeout_secs = int(sys.argv[5])
psql_bin = sys.argv[6]

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
    "request_payload",
    "response_payload",
    "raw_payload",
    "payload",
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


lookback_secs = safe_positive_int(lookback_secs, 3600)
stale_task_secs = safe_positive_int(stale_task_secs, 900)
missing_result_secs = safe_positive_int(missing_result_secs, 900)
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
        "stale_task_secs": stale_task_secs,
        "missing_result_secs": missing_result_secs,
        "open_task_count": 0,
        "stale_task_count": 0,
        "missing_order_result_count": 0,
        "failed_task_count": 0,
        "retry_backlog_count": 0,
        "delivery_blocker_count": 0,
        "recent_order_result_count": 0,
        "recent_trade_record_count": 0,
        "sample": {},
        "alerts": [],
        "correlation": {
            "signal_inbox_id": None,
            "execution_task_id": None,
            "execution_attempt_id": None,
            "order_result_id": None,
            "trade_record_id": None,
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
            "section": "web_task_order_health",
            "message": message,
        }
    )


def skipped_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "skipped", False)
    payload["skipped"] = True
    append_alert(payload, "INFO", code, message)
    return payload


def degraded_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "quant_web_readonly_db", True)
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
            "WEB_INPUT_OUTPUT_REJECTED",
            "Web health input contained unsafe content and was replaced.",
        )
        rendered = json.dumps(sanitize_json(fallback), ensure_ascii=True, indent=2)
    print(rendered)


def postgres_health_sql() -> str:
    return f"""
WITH params AS (
    SELECT
        {lookback_secs}::int AS lookback_secs,
        {stale_task_secs}::int AS stale_task_secs,
        {missing_result_secs}::int AS missing_result_secs
),
recent_tasks AS (
    SELECT
        et.id,
        nsi.id AS news_signal_id,
        et.task_status,
        et.task_type,
        et.lease_until,
        et.created_at,
        et.updated_at,
        EXTRACT(EPOCH FROM (NOW() - COALESCE(et.updated_at, et.created_at)))::bigint AS age_secs
    FROM execution_tasks et
    LEFT JOIN news_signal_inbox nsi ON nsi.id = et.news_signal_id
    CROSS JOIN params p
    WHERE COALESCE(et.updated_at, et.created_at) >= NOW() - make_interval(secs => p.lookback_secs)
),
task_links AS (
    SELECT
        rt.id,
        rt.news_signal_id,
        rt.task_status,
        rt.task_type,
        rt.lease_until,
        rt.age_secs,
        COALESCE(attempts.attempt_count, 0) AS attempt_count,
        attempts.latest_attempt_id,
        attempts.latest_attempt_status,
        COALESCE(orders.order_result_count, 0) AS order_result_count,
        orders.latest_order_result_id,
        COALESCE(trades.trade_record_count, 0) AS trade_record_count,
        trades.latest_trade_record_id
    FROM recent_tasks rt
    LEFT JOIN LATERAL (
        SELECT
            COUNT(*)::int AS attempt_count,
            (ARRAY_AGG(id ORDER BY created_at DESC, id DESC))[1] AS latest_attempt_id,
            (ARRAY_AGG(attempt_status ORDER BY created_at DESC, id DESC))[1] AS latest_attempt_status
        FROM execution_task_attempts eta
        WHERE eta.execution_task_id = rt.id
    ) attempts ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            COUNT(*)::int AS order_result_count,
            MAX(id) AS latest_order_result_id
        FROM exchange_order_results eor
        WHERE eor.execution_task_id = rt.id
    ) orders ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            COUNT(*)::int AS trade_record_count,
            MAX(id) AS latest_trade_record_id
        FROM user_trade_records utr
        WHERE utr.execution_task_id = rt.id
    ) trades ON TRUE
),
task_summary AS (
    SELECT
        COUNT(*) FILTER (
            WHERE lower(task_status) NOT IN ('completed', 'failed', 'error', 'cancelled', 'canceled', 'skipped')
        )::int AS open_task_count,
        COUNT(*) FILTER (
            WHERE lower(task_status) IN ('leased', 'processing', 'running')
              AND (
                  lease_until < NOW()
                  OR age_secs >= (SELECT stale_task_secs FROM params)
              )
        )::int AS stale_task_count,
        COUNT(*) FILTER (
            WHERE lower(task_status) = 'completed'
              AND age_secs >= (SELECT missing_result_secs FROM params)
              AND (order_result_count = 0 OR trade_record_count = 0)
        )::int AS missing_order_result_count,
        COUNT(*) FILTER (
            WHERE lower(task_status) IN ('failed', 'error')
        )::int AS failed_task_count,
        COUNT(*) FILTER (
            WHERE lower(COALESCE(latest_attempt_status, '')) IN ('failed', 'error', 'retryable')
              OR (attempt_count > 1 AND lower(task_status) NOT IN ('completed', 'cancelled', 'canceled', 'skipped'))
        )::int AS retry_backlog_count,
        COALESCE(SUM(order_result_count), 0)::int AS recent_order_result_count,
        COALESCE(SUM(trade_record_count), 0)::int AS recent_trade_record_count
    FROM task_links
),
delivery_summary AS (
    SELECT
        COUNT(*) FILTER (
            WHERE lower(api_execution_status) IN ('blocked', 'failed')
               OR lower(sms_status) = 'failed'
               OR lower(email_status) = 'failed'
        )::int AS delivery_blocker_count
    FROM combo_signal_delivery_logs cdl
    CROSS JOIN params p
    WHERE COALESCE(cdl.updated_at, cdl.created_at, cdl.generated_at) >= NOW() - make_interval(secs => p.lookback_secs)
),
sample_row AS (
    SELECT
        news_signal_id AS signal_inbox_id,
        id AS execution_task_id,
        latest_attempt_id AS execution_attempt_id,
        latest_order_result_id AS order_result_id,
        latest_trade_record_id AS trade_record_id,
        task_status,
        age_secs
    FROM task_links
    ORDER BY
        CASE
            WHEN lower(task_status) = 'completed' AND age_secs >= (SELECT missing_result_secs FROM params)
                 AND (order_result_count = 0 OR trade_record_count = 0) THEN 0
            WHEN lower(task_status) IN ('leased', 'processing', 'running') THEN 1
            WHEN lower(task_status) IN ('failed', 'error') THEN 2
            ELSE 3
        END,
        age_secs DESC,
        id DESC
    LIMIT 1
),
combined AS (
    SELECT
        p.lookback_secs,
        p.stale_task_secs,
        p.missing_result_secs,
        COALESCE(ts.open_task_count, 0) AS open_task_count,
        COALESCE(ts.stale_task_count, 0) AS stale_task_count,
        COALESCE(ts.missing_order_result_count, 0) AS missing_order_result_count,
        COALESCE(ts.failed_task_count, 0) AS failed_task_count,
        COALESCE(ts.retry_backlog_count, 0) AS retry_backlog_count,
        COALESCE(ds.delivery_blocker_count, 0) AS delivery_blocker_count,
        COALESCE(ts.recent_order_result_count, 0) AS recent_order_result_count,
        COALESCE(ts.recent_trade_record_count, 0) AS recent_trade_record_count
    FROM params p
    CROSS JOIN task_summary ts
    CROSS JOIN delivery_summary ds
)
SELECT json_build_object(
    'status',
        CASE
            WHEN stale_task_count > 0 OR missing_order_result_count > 0 THEN 'fail'
            WHEN failed_task_count > 0 OR retry_backlog_count > 0 OR delivery_blocker_count > 0 THEN 'warn'
            ELSE 'ok'
        END,
    'source', 'quant_web_readonly_db',
    'database_engine', 'postgresql',
    'read_only_input', TRUE,
    'lookback_secs', lookback_secs,
    'stale_task_secs', stale_task_secs,
    'missing_result_secs', missing_result_secs,
    'open_task_count', open_task_count,
    'stale_task_count', stale_task_count,
    'missing_order_result_count', missing_order_result_count,
    'failed_task_count', failed_task_count,
    'retry_backlog_count', retry_backlog_count,
    'delivery_blocker_count', delivery_blocker_count,
    'recent_order_result_count', recent_order_result_count,
    'recent_trade_record_count', recent_trade_record_count,
    'sample',
        COALESCE(
            (
                SELECT json_build_object(
                    'signal_inbox_id', signal_inbox_id,
                    'execution_task_id', execution_task_id,
                    'execution_attempt_id', execution_attempt_id,
                    'order_result_id', order_result_id,
                    'trade_record_id', trade_record_id,
                    'task_status', task_status,
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
                    'code', 'WEB_EXECUTION_TASK_STALE',
                    'section', 'web_task_order_health',
                    'message', 'Web execution tasks have stale leases or processing state.'
                ) AS alert
                WHERE stale_task_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P0',
                    'code', 'WEB_ORDER_RESULT_MISSING',
                    'section', 'web_task_order_health',
                    'message', 'Completed Web execution tasks are missing order or trade records.'
                ) AS alert
                WHERE missing_order_result_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'WEB_RETRY_BACKLOG',
                    'section', 'web_task_order_health',
                    'message', 'Recent Web execution tasks have failed attempts or retry backlog.'
                ) AS alert
                WHERE failed_task_count > 0 OR retry_backlog_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'WEB_DELIVERY_BLOCKER',
                    'section', 'web_task_order_health',
                    'message', 'Recent Web delivery logs include blocked or failed channels.'
                ) AS alert
                WHERE delivery_blocker_count > 0
            ) alert_rows
        ),
    'correlation',
        COALESCE(
            (
                SELECT json_build_object(
                    'signal_inbox_id', signal_inbox_id,
                    'execution_task_id', execution_task_id,
                    'execution_attempt_id', execution_attempt_id,
                    'order_result_id', order_result_id,
                    'trade_record_id', trade_record_id
                )
                FROM sample_row
            ),
            json_build_object(
                'signal_inbox_id', NULL,
                'execution_task_id', NULL,
                'execution_attempt_id', NULL,
                'order_result_id', NULL,
                'trade_record_id', NULL
            )
        )
)::text
FROM combined;
"""


def run_postgres_query() -> dict[str, Any]:
    if shutil.which(psql_bin) is None:
        return skipped_payload("WEB_INPUT_SKIPPED", "psql was not available for the read-only Web input.")

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
        return degraded_payload("WEB_INPUT_QUERY_FAILED", "Read-only Web health query failed.")

    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        return degraded_payload("WEB_INPUT_QUERY_EMPTY", "Read-only Web health query returned no JSON.")

    try:
        payload = json.loads(lines[-1])
    except json.JSONDecodeError:
        return degraded_payload("WEB_INPUT_JSON_INVALID", "Read-only Web health query returned invalid JSON.")
    if not isinstance(payload, dict):
        return degraded_payload("WEB_INPUT_JSON_INVALID", "Read-only Web health query returned a non-object JSON value.")
    return payload


if not database_url:
    render(skipped_payload("WEB_INPUT_SKIPPED", "Explicit read-only Web database input was not provided."))
elif ".env" in database_url.lower():
    render(skipped_payload("WEB_INPUT_SKIPPED", "Explicit read-only Web database input was rejected."))
elif not database_url.lower().startswith(("postgres://", "postgresql://")):
    render(skipped_payload("WEB_INPUT_SKIPPED", "Only PostgreSQL Web input is supported by this producer."))
else:
    render(run_postgres_query())
PY
