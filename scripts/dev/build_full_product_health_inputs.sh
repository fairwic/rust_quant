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
: "${FULL_PRODUCT_HEALTH_KEEP_INPUTS:=false}"
: "${FULL_PRODUCT_HEALTH_WEB_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_web_input.sh}"
: "${FULL_PRODUCT_HEALTH_NEWS_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_news_input.sh}"
: "${FULL_PRODUCT_HEALTH_ADMIN_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_admin_input.sh}"
: "${FULL_PRODUCT_HEALTH_AGGREGATOR_PATH:=${REPO_ROOT}/scripts/dev/check_full_product_health.sh}"
: "${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS:=900}"
: "${FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS:=900}"
: "${FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_WEB_PSQL_BIN:=psql}"
: "${FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS:=1800}"
: "${FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD:=3}"
: "${FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN:=psql}"
: "${FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS:=7200}"
: "${FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN:=psql}"

if [[ "${FULL_PRODUCT_HEALTH_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_OUTPUT must be json\n' >&2
    exit 1
fi

BLOCKED_MARKERS=(
    ".env"
    "postgres://"
    "mysql://"
    "database_url"
    "api_key"
    "apikey"
    "api key"
    "api_secret"
    "apisecret"
    "api secret"
    "secret"
    "request_payload"
    "response_payload"
    "raw_payload"
    "request payload"
    "response payload"
    "/fapi/v1/order"
    "/fapi/v2/account"
    "/fapi/v1/positionRisk"
    "/fapi/v2/positionRisk"
    "/fapi/v1/positionSide/dual"
    "/api/commerce/internal/execution-tasks/lease"
    "/api/commerce/internal/execution-results"
    "/api/commerce/internal/order-results"
    "linkusdt"
    "link-usdt"
)

EXPECTED_SKIPPED_CODES=(
    "WEB_INPUT_SKIPPED"
    "NEWS_INPUT_SKIPPED"
    "ADMIN_INPUT_SKIPPED"
)

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/full-product-health-inputs.XXXXXX")"

cleanup() {
    if [[ "${FULL_PRODUCT_HEALTH_KEEP_INPUTS}" != "true" ]]; then
        rm -rf "${TMP_DIR}"
    fi
}
trap cleanup EXIT

emit_fail() {
    python3 - "$1" <<'PY'
import json
import sys
from datetime import datetime, timezone

message = sys.argv[1]
print(json.dumps(
    {
        "schema_version": 1,
        "status": "fail",
        "generated_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "summary": {
            "p0_count": 1,
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
        "sections": {
            "web_task_order_health": {"status": "ok", "source": "not_collected", "read_only_input": False},
            "news_source_ai_health": {"status": "ok", "source": "not_collected", "read_only_input": False},
            "quant_worker_checkpoint_audit": {"status": "ok", "source": "not_collected", "read_only_input": False},
            "admin_readiness": {
                "status": "fail",
                "source": "input_runner",
                "read_only_input": True,
                "live_readiness": "blocked",
                "reason_code": "input_runner_failed",
                "manual_review_required": True,
            },
        },
        "alerts": [
            {
                "severity": "P0",
                "code": "FULL_PRODUCT_INPUT_RUNNER_FAILED",
                "section": "admin_readiness",
                "message": message,
            }
        ],
        "correlation": {},
    },
    ensure_ascii=True,
    indent=2,
))
PY
}

scan_file() {
    local path="$1"
    local lowered
    lowered="$(tr '[:upper:]' '[:lower:]' < "${path}")"
    local marker
    local lowered_marker
    for marker in "${BLOCKED_MARKERS[@]}"; do
        lowered_marker="$(printf '%s' "${marker}" | tr '[:upper:]' '[:lower:]')"
        if [[ "${lowered}" == *"${lowered_marker}"* ]]; then
            return 1
        fi
    done
    return 0
}

require_script() {
    local path="$1"
    if [[ ! -f "${path}" ]]; then
        emit_fail "required read-only health script is missing"
        exit 1
    fi
}

run_web_producer() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_WEB_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_WEB_DATABASE_URL="${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS="${FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS="${FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_PSQL_BIN="${FULL_PRODUCT_HEALTH_WEB_PSQL_BIN}" \
        bash "${FULL_PRODUCT_HEALTH_WEB_INPUT_PRODUCER_PATH}" > "$1"
}

run_news_producer() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_NEWS_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL="${FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS="${FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS="${FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD="${FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD}" \
        FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN="${FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN}" \
        bash "${FULL_PRODUCT_HEALTH_NEWS_INPUT_PRODUCER_PATH}" > "$1"
}

run_admin_producer() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_ADMIN_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL="${FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN="${FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN}" \
        bash "${FULL_PRODUCT_HEALTH_ADMIN_INPUT_PRODUCER_PATH}" > "$1"
}

run_aggregator() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_SCHEMA_VERSION}" \
        FULL_PRODUCT_HEALTH_FIXTURE_PATH="${FULL_PRODUCT_HEALTH_FIXTURE_PATH}" \
        FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH="${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH}" \
        FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH="${FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH}" \
        FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH="${FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH}" \
        FULL_PRODUCT_HEALTH_WEB_JSON_PATH="$1" \
        FULL_PRODUCT_HEALTH_NEWS_JSON_PATH="$2" \
        FULL_PRODUCT_HEALTH_ADMIN_JSON_PATH="$3" \
        bash "${FULL_PRODUCT_HEALTH_AGGREGATOR_PATH}" > "$4"
}

require_script "${FULL_PRODUCT_HEALTH_WEB_INPUT_PRODUCER_PATH}"
require_script "${FULL_PRODUCT_HEALTH_NEWS_INPUT_PRODUCER_PATH}"
require_script "${FULL_PRODUCT_HEALTH_ADMIN_INPUT_PRODUCER_PATH}"
require_script "${FULL_PRODUCT_HEALTH_AGGREGATOR_PATH}"

WEB_JSON_PATH="${TMP_DIR}/web-task-order-health.json"
NEWS_JSON_PATH="${TMP_DIR}/news-source-ai-health.json"
ADMIN_JSON_PATH="${TMP_DIR}/admin-readiness.json"
FULL_JSON_PATH="${TMP_DIR}/full-product-health.json"

run_web_producer "${WEB_JSON_PATH}"
run_news_producer "${NEWS_JSON_PATH}"
run_admin_producer "${ADMIN_JSON_PATH}"

for generated_input in "${WEB_JSON_PATH}" "${NEWS_JSON_PATH}" "${ADMIN_JSON_PATH}"; do
    if ! scan_file "${generated_input}"; then
        emit_fail "generated read-only input was rejected by safety scan"
        exit 1
    fi
done

run_aggregator "${WEB_JSON_PATH}" "${NEWS_JSON_PATH}" "${ADMIN_JSON_PATH}" "${FULL_JSON_PATH}"

if ! scan_file "${FULL_JSON_PATH}"; then
    emit_fail "merged health report was rejected by safety scan"
    exit 1
fi

cat "${FULL_JSON_PATH}"
