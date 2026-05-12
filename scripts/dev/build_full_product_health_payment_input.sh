#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_PAYMENT_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS:=86400}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS:=1800}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN:=psql}"

if [[ "${FULL_PRODUCT_HEALTH_PAYMENT_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_PAYMENT_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL}" \
    "${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL}" \
    "${FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS}" \
    "${FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS}" \
    "${FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS}" \
    "${FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN}" \
    <<'PY'
import json
import os
import shutil
import subprocess
import sys
from typing import Any


payment_database_url = sys.argv[1].strip()
web_database_url = sys.argv[2].strip()
lookback_secs = int(sys.argv[3]) if sys.argv[3].strip().isdigit() else 86400
confirmation_timeout_secs = int(sys.argv[4]) if sys.argv[4].strip().isdigit() else 1800
query_timeout_secs = int(sys.argv[5]) if sys.argv[5].strip().isdigit() else 15
psql_bin = sys.argv[6]

database_url = payment_database_url or web_database_url

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
    "http://",
    "https://",
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
        "confirmation_timeout_secs": confirmation_timeout_secs,
        "wallet_payment_exception_count": 0,
        "payment_entitlement_blocker_count": 0,
        "sample": {},
        "alerts": [],
        "correlation": {
            "payment_exception_id": None,
            "entitlement_check_id": None,
            "user_id": None,
        },
    }


def append_alert(payload: dict[str, Any], severity: str, code: str, message: str) -> None:
    payload["alerts"].append(
        {
            "severity": severity if severity in {"P0", "P1", "INFO"} else "P1",
            "code": code,
            "section": "payment_entitlement_health",
            "message": message,
        }
    )


def skipped_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "skipped", False)
    payload["skipped"] = True
    append_alert(payload, "INFO", code, message)
    return payload


def degraded_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "quant_web_payment_readonly_db", True)
    payload["query_failed"] = True
    append_alert(payload, "P1", code, message)
    return payload


def payment_health_sql() -> str:
    return f"""
WITH params AS (
    SELECT
        {lookback_secs}::int AS lookback_secs,
        {confirmation_timeout_secs}::int AS confirmation_timeout_secs
),
latest_tx AS (
    SELECT DISTINCT ON (payment_intent_id)
        payment_intent_id,
        id AS payment_transaction_id,
        status AS transaction_status,
        confirmed_at,
        created_at AS transaction_created_at,
        updated_at AS transaction_updated_at
    FROM payment_transactions
    WHERE provider = 'wallet'
    ORDER BY payment_intent_id, updated_at DESC NULLS LAST, id DESC
),
base AS (
    SELECT
        intents.id AS payment_intent_id,
        intents.order_id AS membership_order_id,
        intents.buyer_email,
        intents.status AS intent_status,
        intents.expires_at,
        intents.created_at,
        intents.updated_at,
        orders.membership_id,
        orders.status AS order_status,
        tx.payment_transaction_id,
        tx.transaction_status,
        tx.confirmed_at,
        COALESCE(tx.transaction_updated_at, intents.updated_at, intents.created_at) AS last_activity_at
    FROM payment_intents intents
    LEFT JOIN latest_tx tx ON tx.payment_intent_id = intents.id
    LEFT JOIN membership_orders orders
      ON intents.order_type = 'membership_order'
     AND orders.id = intents.order_id
    CROSS JOIN params p
    WHERE intents.provider = 'wallet'
      AND COALESCE(tx.transaction_updated_at, intents.updated_at, intents.created_at)
          >= NOW() - make_interval(secs => p.lookback_secs)
),
coded AS (
    SELECT
        base.*,
        CASE
            WHEN lower(intent_status) IN ('requires_payment', 'processing')
             AND expires_at < NOW()::timestamp
                THEN 'wallet_intent_expired'
            WHEN lower(intent_status) = 'processing'
             AND payment_transaction_id IS NULL
                THEN 'wallet_missing_transaction'
            WHEN lower(intent_status) = 'processing'
             AND lower(COALESCE(transaction_status, '')) = 'pending_confirmation'
             AND last_activity_at < NOW()::timestamp - make_interval(secs => (SELECT confirmation_timeout_secs FROM params))
                THEN 'wallet_confirmation_timeout'
            WHEN lower(intent_status) = 'failed'
              OR lower(COALESCE(transaction_status, '')) = 'failed'
                THEN 'wallet_verification_failed'
            WHEN lower(intent_status) = 'succeeded'
             AND (
                membership_id IS NULL
                OR lower(COALESCE(order_status, '')) NOT IN ('paid', 'succeeded', 'effective', 'completed')
             )
                THEN 'wallet_entitlement_missing'
            ELSE NULL
        END AS exception_code,
        GREATEST(
            FLOOR(EXTRACT(EPOCH FROM (NOW()::timestamp - last_activity_at)) / 60)::bigint,
            0
        ) AS age_minutes
    FROM base
),
summary AS (
    SELECT
        COUNT(*) FILTER (WHERE exception_code IS NOT NULL)::int AS wallet_payment_exception_count,
        COUNT(*) FILTER (WHERE exception_code = 'wallet_entitlement_missing')::int AS payment_entitlement_blocker_count
    FROM coded
),
sample_row AS (
    SELECT
        payment_intent_id,
        membership_order_id,
        payment_transaction_id,
        exception_code,
        age_minutes
    FROM coded
    WHERE exception_code IS NOT NULL
    ORDER BY
        CASE
            WHEN exception_code = 'wallet_entitlement_missing' THEN 0
            WHEN exception_code = 'wallet_verification_failed' THEN 1
            WHEN exception_code = 'wallet_confirmation_timeout' THEN 2
            WHEN exception_code = 'wallet_missing_transaction' THEN 3
            WHEN exception_code = 'wallet_intent_expired' THEN 4
            ELSE 5
        END,
        age_minutes DESC,
        payment_intent_id DESC
    LIMIT 1
)
SELECT json_build_object(
    'status',
        CASE
            WHEN payment_entitlement_blocker_count > 0 THEN 'fail'
            WHEN wallet_payment_exception_count > 0 THEN 'warn'
            ELSE 'ok'
        END,
    'source', 'quant_web_payment_readonly_db',
    'database_engine', 'postgresql',
    'read_only_input', TRUE,
    'lookback_secs', (SELECT lookback_secs FROM params),
    'confirmation_timeout_secs', (SELECT confirmation_timeout_secs FROM params),
    'wallet_payment_exception_count', wallet_payment_exception_count,
    'payment_entitlement_blocker_count', payment_entitlement_blocker_count,
    'sample',
        COALESCE(
            (
                SELECT json_build_object(
                    'payment_intent_id', payment_intent_id,
                    'membership_order_id', membership_order_id,
                    'payment_transaction_id', payment_transaction_id,
                    'exception_code', exception_code,
                    'age_minutes', age_minutes
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
                    'severity', 'P1',
                    'code', 'WALLET_PAYMENT_EXCEPTION',
                    'section', 'payment_entitlement_health',
                    'message', 'wallet payment exceptions require review',
                    'metadata', json_build_object(
                        'wallet_payment_exception_count', wallet_payment_exception_count,
                        'payment_entitlement_blocker_count', payment_entitlement_blocker_count,
                        'sample_kind', 'wallet_payment_exception'
                    )
                ) AS alert
                WHERE wallet_payment_exception_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P0',
                    'code', 'PAYMENT_ENTITLEMENT_BLOCKED',
                    'section', 'payment_entitlement_health',
                    'message', 'wallet payment succeeded but entitlement is still blocked',
                    'metadata', json_build_object(
                        'wallet_payment_exception_count', wallet_payment_exception_count,
                        'payment_entitlement_blocker_count', payment_entitlement_blocker_count,
                        'sample_kind', 'payment_entitlement'
                    )
                ) AS alert
                WHERE payment_entitlement_blocker_count > 0
            ) alert_rows
        ),
    'correlation',
        COALESCE(
            (
                SELECT json_build_object(
                    'payment_exception_id', payment_intent_id,
                    'entitlement_check_id', membership_order_id,
                    'user_id', NULL
                )
                FROM sample_row
            ),
            json_build_object(
                'payment_exception_id', NULL,
                'entitlement_check_id', NULL,
                'user_id', NULL
            )
        )
)::text
FROM summary;
"""


def run_postgres_query() -> dict[str, Any]:
    if shutil.which(psql_bin) is None:
        return degraded_payload(
            "PAYMENT_INPUT_QUERY_FAILED",
            "Read-only payment entitlement query failed.",
        )
    try:
        result = subprocess.run(
            [psql_bin, database_url, "-v", "ON_ERROR_STOP=1", "-Atc", payment_health_sql()],
            text=True,
            capture_output=True,
            timeout=max(query_timeout_secs, 1),
            env={
                "PATH": os.environ.get("PATH", ""),
                "PGCONNECT_TIMEOUT": str(min(query_timeout_secs, 10)),
            },
        )
    except subprocess.SubprocessError:
        return degraded_payload(
            "PAYMENT_INPUT_QUERY_FAILED",
            "Read-only payment entitlement query failed.",
        )
    if result.returncode != 0:
        return degraded_payload(
            "PAYMENT_INPUT_QUERY_FAILED",
            "Read-only payment entitlement query failed.",
        )

    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        return degraded_payload(
            "PAYMENT_INPUT_QUERY_FAILED",
            "Read-only payment entitlement query failed.",
        )
    try:
        payload = json.loads(lines[-1])
    except json.JSONDecodeError:
        return degraded_payload(
            "PAYMENT_INPUT_QUERY_FAILED",
            "Read-only payment entitlement query failed.",
        )
    if not isinstance(payload, dict):
        return degraded_payload(
            "PAYMENT_INPUT_QUERY_FAILED",
            "Read-only payment entitlement query failed.",
        )
    return payload


def render(payload: dict[str, Any]) -> None:
    sanitized = sanitize_json(payload)
    rendered = json.dumps(sanitized, ensure_ascii=True, indent=2)
    if has_blocked_marker(rendered):
        fallback = skipped_payload(
            "PAYMENT_INPUT_SKIPPED",
            "Payment entitlement input contained unsafe content and was replaced.",
        )
        rendered = json.dumps(sanitize_json(fallback), ensure_ascii=True, indent=2)
    print(rendered)


if not database_url:
    render(
        skipped_payload(
            "PAYMENT_INPUT_SKIPPED",
            "Explicit read-only payment database input was not provided.",
        )
    )
elif ".env" in database_url.lower():
    render(
        skipped_payload(
            "PAYMENT_INPUT_SKIPPED",
            "Explicit read-only payment database input was rejected.",
        )
    )
elif not database_url.lower().startswith(("postgres://", "postgresql://")):
    render(
        skipped_payload(
            "PAYMENT_INPUT_SKIPPED",
            "Only PostgreSQL payment input is supported by this producer.",
        )
    )
else:
    render(run_postgres_query())
PY
