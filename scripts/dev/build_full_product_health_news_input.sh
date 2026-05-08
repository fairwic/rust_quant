#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_NEWS_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS:=1800}"
: "${FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD:=3}"
: "${FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN:=psql}"

if [[ "${FULL_PRODUCT_HEALTH_NEWS_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_NEWS_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL}" \
    "${FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS}" \
    "${FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS}" \
    "${FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS}" \
    "${FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD}" \
    "${FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS}" \
    "${FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN}" \
    <<'PY'
import json
import os
import shutil
import subprocess
import sys
from typing import Any


database_url = sys.argv[1].strip()
lookback_secs = int(sys.argv[2])
stale_analysis_secs = int(sys.argv[3])
failed_job_secs = int(sys.argv[4])
source_failure_threshold = int(sys.argv[5])
query_timeout_secs = int(sys.argv[6])
psql_bin = sys.argv[7]

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
    "request_json",
    "response_json",
    "response_text",
    "raw_response",
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
    "request_json",
    "response_json",
    "response_text",
    "raw_response",
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
stale_analysis_secs = safe_positive_int(stale_analysis_secs, 1800)
failed_job_secs = safe_positive_int(failed_job_secs, 3600)
source_failure_threshold = safe_positive_int(source_failure_threshold, 3)
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
        "stale_analysis_secs": stale_analysis_secs,
        "failed_job_secs": failed_job_secs,
        "source_failure_threshold": source_failure_threshold,
        "source_count": 0,
        "degraded_source_count": 0,
        "paused_source_count": 0,
        "retryable_source_count": 0,
        "recent_news_count": 0,
        "signal_candidate_count": 0,
        "recent_ai_analysis_count": 0,
        "actionable_analysis_count": 0,
        "failed_analysis_job_count": 0,
        "stuck_analysis_job_count": 0,
        "provider_failure_count": 0,
        "active_prompt_config_count": 0,
        "sample": {},
        "alerts": [],
        "correlation": {
            "news_id": None,
            "analysis_result_id": None,
            "external_id": None,
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
            "section": "news_source_ai_health",
            "message": message,
        }
    )


def skipped_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "skipped", False)
    payload["skipped"] = True
    append_alert(payload, "INFO", code, message)
    return payload


def degraded_payload(code: str, message: str) -> dict[str, Any]:
    payload = base_payload("warn", "quant_news_readonly_db", True)
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
            "NEWS_INPUT_OUTPUT_REJECTED",
            "News health input contained unsafe content and was replaced.",
        )
        rendered = json.dumps(sanitize_json(fallback), ensure_ascii=True, indent=2)
    print(rendered)


def postgres_health_sql() -> str:
    return f"""
WITH params AS (
    SELECT
        {lookback_secs}::int AS lookback_secs,
        {stale_analysis_secs}::int AS stale_analysis_secs,
        {failed_job_secs}::int AS failed_job_secs,
        {source_failure_threshold}::int AS source_failure_threshold
),
source_relation AS (
    SELECT to_regclass('public.news_source_health') AS relation
),
source_rows AS (
    SELECT
        source,
        enabled,
        status,
        consecutive_failures,
        last_success_at,
        last_failure_at,
        paused_until,
        CASE
            WHEN enabled = false THEN 'disabled'
            WHEN status = 'paused' AND paused_until > NOW() THEN 'paused'
            WHEN status = 'paused' THEN 'retryable'
            WHEN status = 'degraded' THEN 'degraded'
            ELSE 'active'
        END AS effective_status
    FROM news_source_states
),
source_summary AS (
    SELECT
        COUNT(*)::int AS source_count,
        COUNT(*) FILTER (
            WHERE effective_status IN ('degraded', 'paused', 'retryable')
               OR consecutive_failures >= (SELECT source_failure_threshold FROM params)
        )::int AS degraded_source_count,
        COUNT(*) FILTER (WHERE effective_status = 'paused')::int AS paused_source_count,
        COUNT(*) FILTER (WHERE effective_status = 'retryable')::int AS retryable_source_count
    FROM source_rows
),
recent_news_union AS (
    SELECT news_id, source, status, is_signal_candidate, deep_analyzed_at,
        COALESCE(updated_at, published_at, created_at) AS observed_at
    FROM news_items
    CROSS JOIN params p
    WHERE COALESCE(updated_at, published_at, created_at) >= NOW() - make_interval(secs => p.lookback_secs)
      AND COALESCE(is_deleted, false) = false
    UNION ALL
    SELECT news_id, source, status, is_signal_candidate, deep_analyzed_at,
        COALESCE(updated_at, published_at, created_at) AS observed_at
    FROM news_items_jinse
    CROSS JOIN params p
    WHERE COALESCE(updated_at, published_at, created_at) >= NOW() - make_interval(secs => p.lookback_secs)
      AND COALESCE(is_deleted, false) = false
    UNION ALL
    SELECT news_id, source, status, is_signal_candidate, deep_analyzed_at,
        COALESCE(updated_at, published_at, created_at) AS observed_at
    FROM news_items_theblockbeats
    CROSS JOIN params p
    WHERE COALESCE(updated_at, published_at, created_at) >= NOW() - make_interval(secs => p.lookback_secs)
      AND COALESCE(is_deleted, false) = false
    UNION ALL
    SELECT news_id, source, status, is_signal_candidate, deep_analyzed_at,
        COALESCE(updated_at, published_at, created_at) AS observed_at
    FROM news_items_coindesk
    CROSS JOIN params p
    WHERE COALESCE(updated_at, published_at, created_at) >= NOW() - make_interval(secs => p.lookback_secs)
      AND COALESCE(is_deleted, false) = false
    UNION ALL
    SELECT news_id, source, status, is_signal_candidate, deep_analyzed_at,
        COALESCE(updated_at, published_at, created_at) AS observed_at
    FROM news_items_seekingalpha
    CROSS JOIN params p
    WHERE COALESCE(updated_at, published_at, created_at) >= NOW() - make_interval(secs => p.lookback_secs)
      AND COALESCE(is_deleted, false) = false
),
recent_news AS (
    SELECT DISTINCT ON (source, news_id)
        news_id, source, status, is_signal_candidate, deep_analyzed_at, observed_at
    FROM recent_news_union
    ORDER BY source, news_id, observed_at DESC
),
news_summary AS (
    SELECT
        COUNT(*)::int AS recent_news_count,
        COUNT(*) FILTER (WHERE is_signal_candidate = true)::int AS signal_candidate_count
    FROM recent_news
),
recent_ai AS (
    SELECT
        id,
        news_id,
        source,
        prompt_key,
        prompt_version,
        signal,
        created_at,
        EXTRACT(EPOCH FROM (NOW() - created_at))::bigint AS age_secs
    FROM news_ai_analysis_results
    CROSS JOIN params p
    WHERE created_at >= NOW() - make_interval(secs => p.lookback_secs)
),
ai_summary AS (
    SELECT
        COUNT(*)::int AS recent_ai_analysis_count,
        COUNT(*) FILTER (
            WHERE lower(COALESCE(signal, 'none')) NOT IN ('none', 'hold', 'ignore')
        )::int AS actionable_analysis_count
    FROM recent_ai
),
job_summary AS (
    SELECT
        COUNT(*) FILTER (
            WHERE lower(status) IN ('failed', 'error')
              AND COALESCE(finished_at, updated_at, created_at) >= NOW() - make_interval(secs => (SELECT failed_job_secs FROM params))
        )::int AS failed_analysis_job_count,
        COUNT(*) FILTER (
            WHERE lower(status) IN ('running', 'processing', 'locked')
              AND (
                  locked_until < NOW()
                  OR COALESCE(started_at, updated_at, created_at) < NOW() - make_interval(secs => (SELECT stale_analysis_secs FROM params))
              )
        )::int AS stuck_analysis_job_count
    FROM news_analysis_jobs
),
provider_summary AS (
    SELECT
        COUNT(*) FILTER (
            WHERE success = false
               OR COALESCE(status_code, 200) >= 400
               OR error_message IS NOT NULL
        )::int AS provider_failure_count
    FROM news_provider_call_logs
    CROSS JOIN params p
    WHERE created_at >= NOW() - make_interval(secs => p.lookback_secs)
),
prompt_summary AS (
    SELECT
        COUNT(*) FILTER (WHERE is_active = true)::int AS active_prompt_config_count
    FROM ai_prompt_configs
),
source_sample AS (
    SELECT source, effective_status, consecutive_failures
    FROM source_rows
    WHERE effective_status IN ('degraded', 'paused', 'retryable')
       OR consecutive_failures >= (SELECT source_failure_threshold FROM params)
    ORDER BY
        CASE effective_status
            WHEN 'paused' THEN 0
            WHEN 'degraded' THEN 1
            WHEN 'retryable' THEN 2
            ELSE 3
        END,
        consecutive_failures DESC,
        source
    LIMIT 1
),
ai_sample AS (
    SELECT id, news_id, source, prompt_key, prompt_version, signal, age_secs
    FROM recent_ai
    ORDER BY
        CASE
            WHEN lower(COALESCE(signal, 'none')) NOT IN ('none', 'hold', 'ignore') THEN 0
            ELSE 1
        END,
        created_at DESC,
        id DESC
    LIMIT 1
),
combined AS (
    SELECT
        p.lookback_secs,
        p.stale_analysis_secs,
        p.failed_job_secs,
        p.source_failure_threshold,
        COALESCE(ss.source_count, 0) AS source_count,
        COALESCE(ss.degraded_source_count, 0) AS degraded_source_count,
        COALESCE(ss.paused_source_count, 0) AS paused_source_count,
        COALESCE(ss.retryable_source_count, 0) AS retryable_source_count,
        COALESCE(ns.recent_news_count, 0) AS recent_news_count,
        COALESCE(ns.signal_candidate_count, 0) AS signal_candidate_count,
        COALESCE(ai.recent_ai_analysis_count, 0) AS recent_ai_analysis_count,
        COALESCE(ai.actionable_analysis_count, 0) AS actionable_analysis_count,
        COALESCE(js.failed_analysis_job_count, 0) AS failed_analysis_job_count,
        COALESCE(js.stuck_analysis_job_count, 0) AS stuck_analysis_job_count,
        COALESCE(ps.provider_failure_count, 0) AS provider_failure_count,
        COALESCE(pr.active_prompt_config_count, 0) AS active_prompt_config_count
    FROM params p
    CROSS JOIN source_relation sr
    CROSS JOIN source_summary ss
    CROSS JOIN news_summary ns
    CROSS JOIN ai_summary ai
    CROSS JOIN job_summary js
    CROSS JOIN provider_summary ps
    CROSS JOIN prompt_summary pr
)
SELECT json_build_object(
    'status',
        CASE
            WHEN degraded_source_count > 0
              OR failed_analysis_job_count > 0
              OR stuck_analysis_job_count > 0
              OR provider_failure_count > 0
              OR active_prompt_config_count = 0
              OR (recent_news_count > 0 AND recent_ai_analysis_count = 0)
            THEN 'warn'
            ELSE 'ok'
        END,
    'source', 'quant_news_readonly_db',
    'database_engine', 'postgresql',
    'read_only_input', TRUE,
    'lookback_secs', lookback_secs,
    'stale_analysis_secs', stale_analysis_secs,
    'failed_job_secs', failed_job_secs,
    'source_failure_threshold', source_failure_threshold,
    'source_count', source_count,
    'degraded_source_count', degraded_source_count,
    'paused_source_count', paused_source_count,
    'retryable_source_count', retryable_source_count,
    'recent_news_count', recent_news_count,
    'signal_candidate_count', signal_candidate_count,
    'recent_ai_analysis_count', recent_ai_analysis_count,
    'actionable_analysis_count', actionable_analysis_count,
    'failed_analysis_job_count', failed_analysis_job_count,
    'stuck_analysis_job_count', stuck_analysis_job_count,
    'provider_failure_count', provider_failure_count,
    'active_prompt_config_count', active_prompt_config_count,
    'sample',
        COALESCE(
            (
                SELECT json_build_object(
                    'source', source_sample.source,
                    'effective_status', source_sample.effective_status,
                    'consecutive_failures', source_sample.consecutive_failures,
                    'news_id', ai_sample.news_id,
                    'analysis_result_id', ai_sample.id,
                    'analysis_signal', ai_sample.signal,
                    'age_secs', ai_sample.age_secs
                )
                FROM source_sample
                FULL JOIN ai_sample ON TRUE
                LIMIT 1
            ),
            '{{}}'::json
        ),
    'alerts',
        (
            SELECT COALESCE(json_agg(alert), '[]'::json)
            FROM (
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'NEWS_SOURCE_DEGRADED',
                    'section', 'news_source_ai_health',
                    'message', 'One or more news sources are degraded, paused, or retryable.'
                ) AS alert
                WHERE degraded_source_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'NEWS_AI_PROVIDER_UNAVAILABLE',
                    'section', 'news_source_ai_health',
                    'message', 'Recent AI provider calls failed or active prompt config is missing.'
                ) AS alert
                WHERE provider_failure_count > 0 OR active_prompt_config_count = 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'NEWS_ANALYSIS_JOB_FAILED',
                    'section', 'news_source_ai_health',
                    'message', 'Recent news analysis jobs failed.'
                ) AS alert
                WHERE failed_analysis_job_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'NEWS_ANALYSIS_JOB_STUCK',
                    'section', 'news_source_ai_health',
                    'message', 'News analysis jobs are stale or lock-expired.'
                ) AS alert
                WHERE stuck_analysis_job_count > 0
                UNION ALL
                SELECT json_build_object(
                    'severity', 'P1',
                    'code', 'NO_RECENT_AI_ANALYSIS',
                    'section', 'news_source_ai_health',
                    'message', 'Recent news exists but no recent AI analysis was recorded.'
                ) AS alert
                WHERE recent_news_count > 0 AND recent_ai_analysis_count = 0
            ) alert_rows
        ),
    'correlation',
        COALESCE(
            (
                SELECT json_build_object(
                    'news_id', news_id,
                    'analysis_result_id', id,
                    'external_id', NULL
                )
                FROM ai_sample
            ),
            json_build_object(
                'news_id', NULL,
                'analysis_result_id', NULL,
                'external_id', NULL
            )
        )
)::text
FROM combined;
"""


def run_postgres_query() -> dict[str, Any]:
    if shutil.which(psql_bin) is None:
        return skipped_payload("NEWS_INPUT_SKIPPED", "psql was not available for the read-only News input.")

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
        return degraded_payload("NEWS_INPUT_QUERY_FAILED", "Read-only News health query failed.")

    lines = [line.strip() for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        return degraded_payload("NEWS_INPUT_QUERY_EMPTY", "Read-only News health query returned no JSON.")

    try:
        payload = json.loads(lines[-1])
    except json.JSONDecodeError:
        return degraded_payload("NEWS_INPUT_JSON_INVALID", "Read-only News health query returned invalid JSON.")
    if not isinstance(payload, dict):
        return degraded_payload("NEWS_INPUT_JSON_INVALID", "Read-only News health query returned a non-object JSON value.")
    return payload


if not database_url:
    render(skipped_payload("NEWS_INPUT_SKIPPED", "Explicit read-only News database input was not provided."))
elif ".env" in database_url.lower():
    render(skipped_payload("NEWS_INPUT_SKIPPED", "Explicit read-only News database input was rejected."))
elif not database_url.lower().startswith(("postgres://", "postgresql://")):
    render(skipped_payload("NEWS_INPUT_SKIPPED", "Only PostgreSQL News input is supported by this producer."))
else:
    render(run_postgres_query())
PY
