#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${FULL_PRODUCT_HEALTH_CI_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR:=${REPO_ROOT}/target/full-product-health-ci}"
: "${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH:=}"
: "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH:=}"
: "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH:=}"
: "${FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS:=false}"
: "${FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH:=}"
: "${FULL_PRODUCT_HEALTH_CI_VALIDATION_STRICT:=true}"
: "${FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS:=fail}"
: "${FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH:=false}"
: "${FULL_PRODUCT_HEALTH_CI_INPUT_BUILDER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_inputs.sh}"
: "${FULL_PRODUCT_HEALTH_CI_SUMMARY_SCRIPT_PATH:=${REPO_ROOT}/scripts/dev/summarize_full_product_health.sh}"
: "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_SCRIPT_PATH:=${REPO_ROOT}/scripts/dev/render_full_product_health_markdown.sh}"
: "${FULL_PRODUCT_HEALTH_CI_VALIDATOR_PATH:=${REPO_ROOT}/scripts/dev/validate_full_product_health_artifacts.sh}"
: "${FULL_PRODUCT_HEALTH_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_FIXTURE_PATH:=${REPO_ROOT}/docs/dev/full_product_health_aggregator.fixture.json}"
: "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH:=${REPO_ROOT}/scripts/dev/check_local_service_health.sh}"
: "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_KEEP_INPUTS:=false}"
: "${FULL_PRODUCT_HEALTH_WEB_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_web_input.sh}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_payment_input.sh}"
: "${FULL_PRODUCT_HEALTH_NEWS_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_news_input.sh}"
: "${FULL_PRODUCT_HEALTH_ADMIN_INPUT_PRODUCER_PATH:=${REPO_ROOT}/scripts/dev/build_full_product_health_admin_input.sh}"
: "${FULL_PRODUCT_HEALTH_AGGREGATOR_PATH:=${REPO_ROOT}/scripts/dev/check_full_product_health.sh}"
: "${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS:=3600}"
: "${FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS:=900}"
: "${FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS:=900}"
: "${FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_WEB_PSQL_BIN:=psql}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL:=}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS:=86400}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS:=1800}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS:=15}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN:=psql}"
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
: "${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT:=5}"

if [[ "${FULL_PRODUCT_HEALTH_CI_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_CI_OUTPUT must be json\n' >&2
    exit 2
fi

if [[ -z "${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}" ]]; then
    FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH="${FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR}/full-product-health.json"
fi
if [[ -z "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}" ]]; then
    FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH="${FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR}/full-product-health-summary.json"
fi
if [[ "${FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS}" == "true" && -z "${FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH}" ]]; then
    FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH="${FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR}/full-product-health-validation.json"
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

require_script() {
    local path="$1"
    if [[ ! -f "${path}" ]]; then
        printf 'required CI health script is missing: %s\n' "${path}" >&2
        exit 2
    fi
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

json_status() {
    local path="$1"
    python3 - "$path" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
summary = payload.get("summary")
if not isinstance(summary, dict):
    summary = {}
status = summary.get("overall_status") or payload.get("status") or "fail"
print(status if status in {"ok", "warn", "fail"} else "fail")
PY
}

run_input_builder() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_SCHEMA_VERSION}" \
        FULL_PRODUCT_HEALTH_FIXTURE_PATH="${FULL_PRODUCT_HEALTH_FIXTURE_PATH}" \
        FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH="${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH}" \
        FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH="${FULL_PRODUCT_HEALTH_CI_RUN_LOCAL_HEALTH}" \
        FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH="${FULL_PRODUCT_HEALTH_LOCAL_JSON_PATH}" \
        FULL_PRODUCT_HEALTH_KEEP_INPUTS="${FULL_PRODUCT_HEALTH_KEEP_INPUTS}" \
        FULL_PRODUCT_HEALTH_WEB_INPUT_PRODUCER_PATH="${FULL_PRODUCT_HEALTH_WEB_INPUT_PRODUCER_PATH}" \
        FULL_PRODUCT_HEALTH_PAYMENT_INPUT_PRODUCER_PATH="${FULL_PRODUCT_HEALTH_PAYMENT_INPUT_PRODUCER_PATH}" \
        FULL_PRODUCT_HEALTH_NEWS_INPUT_PRODUCER_PATH="${FULL_PRODUCT_HEALTH_NEWS_INPUT_PRODUCER_PATH}" \
        FULL_PRODUCT_HEALTH_ADMIN_INPUT_PRODUCER_PATH="${FULL_PRODUCT_HEALTH_ADMIN_INPUT_PRODUCER_PATH}" \
        FULL_PRODUCT_HEALTH_AGGREGATOR_PATH="${FULL_PRODUCT_HEALTH_AGGREGATOR_PATH}" \
        FULL_PRODUCT_HEALTH_WEB_DATABASE_URL="${FULL_PRODUCT_HEALTH_WEB_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_WEB_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS="${FULL_PRODUCT_HEALTH_WEB_STALE_TASK_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS="${FULL_PRODUCT_HEALTH_WEB_MISSING_RESULT_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_WEB_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_WEB_PSQL_BIN="${FULL_PRODUCT_HEALTH_WEB_PSQL_BIN}" \
        FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL="${FULL_PRODUCT_HEALTH_PAYMENT_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_PAYMENT_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_PAYMENT_CONFIRMATION_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_PAYMENT_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN="${FULL_PRODUCT_HEALTH_PAYMENT_PSQL_BIN}" \
        FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL="${FULL_PRODUCT_HEALTH_NEWS_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_NEWS_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS="${FULL_PRODUCT_HEALTH_NEWS_STALE_ANALYSIS_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS="${FULL_PRODUCT_HEALTH_NEWS_FAILED_JOB_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD="${FULL_PRODUCT_HEALTH_NEWS_SOURCE_FAILURE_THRESHOLD}" \
        FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_NEWS_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN="${FULL_PRODUCT_HEALTH_NEWS_PSQL_BIN}" \
        FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL="${FULL_PRODUCT_HEALTH_ADMIN_DATABASE_URL}" \
        FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS="${FULL_PRODUCT_HEALTH_ADMIN_LOOKBACK_SECS}" \
        FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS="${FULL_PRODUCT_HEALTH_ADMIN_QUERY_TIMEOUT_SECS}" \
        FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN="${FULL_PRODUCT_HEALTH_ADMIN_PSQL_BIN}" \
        bash "${FULL_PRODUCT_HEALTH_CI_INPUT_BUILDER_PATH}" > "${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}"
}

run_summary() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION}" \
        FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH="${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}" \
        FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT="${FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT}" \
        bash "${FULL_PRODUCT_HEALTH_CI_SUMMARY_SCRIPT_PATH}" > "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}"
}

run_markdown() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT="markdown" \
        FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH="${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}" \
        FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH="${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}" \
        FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH="${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}" \
        FULL_PRODUCT_HEALTH_MARKDOWN_PATH="${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}" \
        bash "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_SCRIPT_PATH}" > "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}"
}

run_validation() {
    env -i \
        PATH="${PATH:-}" \
        FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT="json" \
        FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_VERSION}" \
        FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH="${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}" \
        FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH="${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}" \
        FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH="${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}" \
        FULL_PRODUCT_HEALTH_VALIDATION_STRICT="${FULL_PRODUCT_HEALTH_CI_VALIDATION_STRICT}" \
        bash "${FULL_PRODUCT_HEALTH_CI_VALIDATOR_PATH}" > "${FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH}"
}

exit_code_for_status() {
    local status="$1"
    case "${FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS}" in
        fail)
            [[ "${status}" == "fail" ]]
            ;;
        warn)
            [[ "${status}" == "fail" || "${status}" == "warn" ]]
            ;;
        never)
            return 1
            ;;
        *)
            printf 'FULL_PRODUCT_HEALTH_CI_FAIL_ON_STATUS must be fail, warn, or never\n' >&2
            exit 2
            ;;
    esac
}

require_script "${FULL_PRODUCT_HEALTH_CI_INPUT_BUILDER_PATH}"
require_script "${FULL_PRODUCT_HEALTH_CI_SUMMARY_SCRIPT_PATH}"
if [[ -n "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}" ]]; then
    require_script "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_SCRIPT_PATH}"
fi
if [[ "${FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS}" == "true" ]]; then
    require_script "${FULL_PRODUCT_HEALTH_CI_VALIDATOR_PATH}"
fi

mkdir -p "${FULL_PRODUCT_HEALTH_CI_ARTIFACT_DIR}"
mkdir -p "$(dirname "${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}")"
mkdir -p "$(dirname "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}")"
if [[ -n "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}" ]]; then
    mkdir -p "$(dirname "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}")"
fi
if [[ -n "${FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH}" ]]; then
    mkdir -p "$(dirname "${FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH}")"
fi

run_input_builder
if ! scan_file "${FULL_PRODUCT_HEALTH_CI_FULL_REPORT_PATH}"; then
    printf 'full product health report artifact was rejected by safety scan\n' >&2
    exit 1
fi

run_summary
if ! scan_file "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}"; then
    printf 'full product health summary artifact was rejected by safety scan\n' >&2
    exit 1
fi

if [[ -n "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}" ]]; then
    run_markdown
    if ! scan_file "${FULL_PRODUCT_HEALTH_CI_MARKDOWN_PATH}"; then
        printf 'full product health markdown artifact was rejected by safety scan\n' >&2
        exit 1
    fi
fi

if [[ "${FULL_PRODUCT_HEALTH_CI_VALIDATE_ARTIFACTS}" == "true" ]]; then
    run_validation
    if ! scan_file "${FULL_PRODUCT_HEALTH_CI_VALIDATION_PATH}"; then
        printf 'full product health validation artifact was rejected by safety scan\n' >&2
        exit 1
    fi
fi

cat "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}"

overall_status="$(json_status "${FULL_PRODUCT_HEALTH_CI_SUMMARY_PATH}")"
if exit_code_for_status "${overall_status}"; then
    exit 1
fi
exit 0
