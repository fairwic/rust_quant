#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH:=}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR:=}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_STRICT:=true}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_TOP_ALERT_LIMIT:=10}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH:=}"

AGGREGATOR_PATH="${REPO_ROOT}/scripts/dev/check_full_product_health.sh"
SUMMARY_PATH="${REPO_ROOT}/scripts/dev/summarize_full_product_health.sh"
MARKDOWN_PATH="${REPO_ROOT}/scripts/dev/render_full_product_health_markdown.sh"
VALIDATOR_PATH="${REPO_ROOT}/scripts/dev/validate_full_product_health_artifacts.sh"
PUBLISHER_PATH="${REPO_ROOT}/scripts/dev/publish_full_product_health_artifact_set.sh"

BLOCKED_MARKERS=(
    ".env"
    "postgres://"
    "postgresql://"
    "mysql://"
    "database_url"
    "api_key"
    "apikey"
    "api key"
    "api_secret"
    "apisecret"
    "api secret"
    "secret"
    "passphrase"
    "cipher"
    "request_payload"
    "response_payload"
    "raw_payload"
    "request payload"
    "response payload"
    "raw payload"
    "http://"
    "https://"
    "file://"
    "/fapi/v1/order"
    "/fapi/v2/account"
    "/fapi/v1/positionRisk"
    "/fapi/v2/positionRisk"
    "/fapi/v1/positionSide/dual"
    "/fapi/v1/leverage"
    "/fapi/v1/marginType"
    "/api/commerce/internal/execution-tasks/lease"
    "/api/commerce/internal/execution-results"
    "/api/commerce/internal/order-results"
    "linkusdt"
    "link-usdt"
)

fail() {
    printf 'payment artifact smoke failed: %s\n' "$1" >&2
    exit "${2:-1}"
}

scan_text() {
    local text="$1"
    local lowered
    lowered="$(printf '%s' "${text}" | tr '[:upper:]' '[:lower:]')"
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
    [[ -f "${path}" ]] || fail "required script is missing" 1
}

validate_payment_input() {
    local path="$1"
    python3 - "${path}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    payload = json.loads(path.read_text(encoding="utf-8"))
except json.JSONDecodeError:
    print("input JSON is invalid", file=sys.stderr)
    sys.exit(11)
if not isinstance(payload, dict):
    print("input JSON must be an object", file=sys.stderr)
    sys.exit(12)
if payload.get("skipped") is True or payload.get("query_failed") is True:
    print("input JSON must be a real-count payment artifact", file=sys.stderr)
    sys.exit(13)
for key in ["wallet_payment_exception_count", "payment_entitlement_blocker_count"]:
    value = payload.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        print(f"input JSON missing non-negative integer {key}", file=sys.stderr)
        sys.exit(14)
if payload["payment_entitlement_blocker_count"] > payload["wallet_payment_exception_count"]:
    print("input JSON payment entitlement blockers exceed wallet exceptions", file=sys.stderr)
    sys.exit(15)
PY
}

if [[ -z "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}" ]]; then
    fail "FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH is required" 2
fi

if ! scan_text "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}"; then
    fail "input path contains a blocked marker" 2
fi
if [[ ! -f "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}" ]]; then
    fail "input file is missing" 2
fi

INPUT_BODY="$(<"${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}")"
if ! scan_text "${INPUT_BODY}"; then
    fail "input JSON contains a blocked marker" 2
fi
if ! validate_payment_input "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}"; then
    fail "input JSON is invalid" 2
fi

require_script "${AGGREGATOR_PATH}"
require_script "${SUMMARY_PATH}"
require_script "${MARKDOWN_PATH}"
require_script "${VALIDATOR_PATH}"
require_script "${PUBLISHER_PATH}"

if [[ -n "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR}" ]]; then
    OUTPUT_DIR="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_OUTPUT_DIR}"
else
    OUTPUT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/full-product-health-payment-smoke.XXXXXX")"
fi

if ! scan_text "${OUTPUT_DIR}"; then
    fail "output directory contains a blocked marker" 2
fi
mkdir -p "${OUTPUT_DIR}"

FULL_REPORT_PATH="${OUTPUT_DIR}/full-product-health.json"
SUMMARY_ARTIFACT_PATH="${OUTPUT_DIR}/full-product-health-summary.json"
MARKDOWN_ARTIFACT_PATH="${OUTPUT_DIR}/full-product-health.md"
VALIDATION_PATH="${OUTPUT_DIR}/full-product-health-validation.json"
if [[ -n "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH}" ]]; then
    PUBLISH_INDEX_PATH="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_PUBLISH_INDEX_PATH}"
else
    PUBLISH_INDEX_PATH="${OUTPUT_DIR}/full-product-health-publish-index.json"
fi

if ! scan_text "${PUBLISH_INDEX_PATH}"; then
    fail "publish index path contains a blocked marker" 2
fi

env -i \
    PATH="${PATH:-}" \
    FULL_PRODUCT_HEALTH_OUTPUT="json" \
    FULL_PRODUCT_HEALTH_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_SCHEMA_VERSION}" \
    FULL_PRODUCT_HEALTH_RUN_LOCAL_HEALTH=false \
    FULL_PRODUCT_HEALTH_PAYMENT_JSON_PATH="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}" \
    bash "${AGGREGATOR_PATH}" > "${FULL_REPORT_PATH}"

env -i \
    PATH="${PATH:-}" \
    FULL_PRODUCT_HEALTH_SUMMARY_OUTPUT="json" \
    FULL_PRODUCT_HEALTH_SUMMARY_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_SCHEMA_VERSION}" \
    FULL_PRODUCT_HEALTH_SUMMARY_JSON_PATH="${FULL_REPORT_PATH}" \
    FULL_PRODUCT_HEALTH_SUMMARY_TOP_ALERT_LIMIT="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_TOP_ALERT_LIMIT}" \
    bash "${SUMMARY_PATH}" > "${SUMMARY_ARTIFACT_PATH}"

if ! env -i \
    PATH="${PATH:-}" \
    FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT="markdown" \
    FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH="${SUMMARY_ARTIFACT_PATH}" \
    FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH="${FULL_REPORT_PATH}" \
    FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH="${SUMMARY_ARTIFACT_PATH}" \
    FULL_PRODUCT_HEALTH_MARKDOWN_PATH="${MARKDOWN_ARTIFACT_PATH}" \
    bash "${MARKDOWN_PATH}" > "${MARKDOWN_ARTIFACT_PATH}"; then
    fail "markdown artifact generation failed; inspect ${MARKDOWN_ARTIFACT_PATH}" 1
fi

if ! env -i \
    PATH="${PATH:-}" \
    FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT="json" \
    FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH="${FULL_REPORT_PATH}" \
    FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH="${SUMMARY_ARTIFACT_PATH}" \
    FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH="${MARKDOWN_ARTIFACT_PATH}" \
    FULL_PRODUCT_HEALTH_VALIDATION_STRICT="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_STRICT}" \
    FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_SCHEMA_VERSION}" \
    bash "${VALIDATOR_PATH}" > "${VALIDATION_PATH}"; then
    fail "artifact validation failed; inspect ${VALIDATION_PATH}" 1
fi

if ! env -i \
    PATH="${PATH:-}" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT="json" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH="${FULL_REPORT_PATH}" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH="${SUMMARY_ARTIFACT_PATH}" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH="${MARKDOWN_ARTIFACT_PATH}" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_SCHEMA_VERSION="${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_SCHEMA_VERSION}" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY="payment-artifact-smoke" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE="ci" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID="payment-artifact-smoke" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO="rust_quant" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL="/admin/artifacts/payment-artifact-smoke/full-product-health.md" \
    FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL="/admin/artifacts/payment-artifact-smoke/full-product-health.json" \
    bash "${PUBLISHER_PATH}" > "${PUBLISH_INDEX_PATH}"; then
    fail "publish index validation failed; inspect ${PUBLISH_INDEX_PATH}" 1
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_PAYMENT_SMOKE_INPUT_PATH}" \
    "${OUTPUT_DIR}" \
    "${FULL_REPORT_PATH}" \
    "${SUMMARY_ARTIFACT_PATH}" \
    "${MARKDOWN_ARTIFACT_PATH}" \
    "${VALIDATION_PATH}" \
    "${PUBLISH_INDEX_PATH}" \
    <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

input_path, output_dir, full_report_path, summary_path, markdown_path, validation_path, publish_index_path = sys.argv[1:8]

full_report = json.loads(Path(full_report_path).read_text(encoding="utf-8"))
summary = json.loads(Path(summary_path).read_text(encoding="utf-8"))
validation = json.loads(Path(validation_path).read_text(encoding="utf-8"))
publish_index = json.loads(Path(publish_index_path).read_text(encoding="utf-8"))
payment_input = json.loads(Path(input_path).read_text(encoding="utf-8"))
source_summary = full_report.get("summary") if isinstance(full_report.get("summary"), dict) else {}


def fail_contract(message):
    print(f"publish index contract failed: {message}", file=sys.stderr)
    sys.exit(21)


def as_object(value, path):
    if not isinstance(value, dict):
        fail_contract(f"{path} must be an object")
    return value


def as_items(value, path):
    if not isinstance(value, list):
        fail_contract(f"{path} must be an array")
    if any(not isinstance(item, dict) for item in value):
        fail_contract(f"{path} items must be objects")
    return value


def as_non_negative_int(value, path):
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        fail_contract(f"{path} must be a non-negative integer")
    return value


def find_playbook_item(items, code):
    for item in items:
        if item.get("section") == "payment_entitlement_health" and item.get("code") == code:
            return item
    fail_contract(f"missing payment playbook item: {code}")


publish_summary = as_object(publish_index.get("summary"), "publish_index.summary")
publish_summary_counts = as_object(
    publish_summary.get("summary"),
    "publish_index.summary.summary",
)
publish_playbook = as_object(
    publish_summary.get("operator_playbook_summary"),
    "publish_index.summary.operator_playbook_summary",
)
publish_playbook_items = as_items(
    publish_playbook.get("items"),
    "publish_index.summary.operator_playbook_summary.items",
)
publish_validation = as_object(publish_index.get("validation"), "publish_index.validation")
publish_redaction = as_object(publish_index.get("redaction"), "publish_index.redaction")

expected_wallet_count = as_non_negative_int(
    payment_input.get("wallet_payment_exception_count"),
    "input.wallet_payment_exception_count",
)
expected_blocker_count = as_non_negative_int(
    payment_input.get("payment_entitlement_blocker_count"),
    "input.payment_entitlement_blocker_count",
)
publish_wallet_count = as_non_negative_int(
    publish_summary_counts.get("wallet_payment_exception_count"),
    "publish_index.summary.summary.wallet_payment_exception_count",
)
publish_blocker_count = as_non_negative_int(
    publish_summary_counts.get("payment_entitlement_blocker_count"),
    "publish_index.summary.summary.payment_entitlement_blocker_count",
)

if publish_index.get("storageStatus") != "current":
    fail_contract("publish_index.storageStatus must be current")
if publish_index.get("stale") is not False:
    fail_contract("publish_index.stale must be false")
if publish_validation.get("status") != "ok":
    fail_contract("publish_index.validation.status must be ok")
if publish_redaction.get("status") != "ok":
    fail_contract("publish_index.redaction.status must be ok")
if publish_wallet_count != expected_wallet_count:
    fail_contract("wallet_payment_exception_count drifted between input and publish index")
if publish_blocker_count != expected_blocker_count:
    fail_contract("payment_entitlement_blocker_count drifted between input and publish index")

wallet_playbook_item = find_playbook_item(publish_playbook_items, "WALLET_PAYMENT_EXCEPTION")
blocker_playbook_item = find_playbook_item(publish_playbook_items, "PAYMENT_ENTITLEMENT_BLOCKED")
if wallet_playbook_item.get("default_next_action") != "review_wallet_payment_exceptions":
    fail_contract("WALLET_PAYMENT_EXCEPTION default_next_action drifted")
if blocker_playbook_item.get("operator_action") != "block_release_until_resolved":
    fail_contract("PAYMENT_ENTITLEMENT_BLOCKED operator_action drifted")

publish_index_contract = {
    "compatibility_contract_version": 1,
    "ready_to_render": True,
    "counter_source": "publish_index.summary.summary",
    "playbook_source": "publish_index.summary.operator_playbook_summary.items",
    "wallet_payment_exception_count": publish_wallet_count,
    "payment_entitlement_blocker_count": publish_blocker_count,
    "required_gate": {
        "storageStatus": publish_index.get("storageStatus"),
        "stale": publish_index.get("stale"),
        "validation.status": publish_validation.get("status"),
        "redaction.status": publish_redaction.get("status"),
    },
    "required_playbook_items": [
        {
            "code": wallet_playbook_item.get("code"),
            "section": wallet_playbook_item.get("section"),
            "default_next_action": wallet_playbook_item.get("default_next_action"),
        },
        {
            "code": blocker_playbook_item.get("code"),
            "section": blocker_playbook_item.get("section"),
            "operator_action": blocker_playbook_item.get("operator_action"),
        },
    ],
}

print(json.dumps(
    {
        "schema_version": 1,
        "status": "ok" if validation.get("status") == "ok" else "fail",
        "generated_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "safety": {
            "read_env": False,
            "read_database": False,
            "called_provider": False,
            "called_signed_exchange_endpoint": False,
            "mutated_task": False,
        },
        "input": {
            "payment_json": input_path,
            "mode": "explicit_payment_real_count_json",
        },
        "artifact_dir": output_dir,
        "artifacts": {
            "full_report": full_report_path,
            "summary": summary_path,
            "markdown": markdown_path,
            "validation": validation_path,
            "publish_index": publish_index_path,
        },
        "full_report_status": full_report.get("status"),
        "summary_status": summary.get("status"),
        "validation_status": validation.get("status"),
        "publish_index_status": publish_index.get("storageStatus"),
        "publish_index_validation_status": (
            publish_index.get("validation", {}).get("status")
            if isinstance(publish_index.get("validation"), dict)
            else None
        ),
        "payment": {
            "wallet_payment_exception_count": source_summary.get("wallet_payment_exception_count", 0),
            "payment_entitlement_blocker_count": source_summary.get("payment_entitlement_blocker_count", 0),
        },
        "publish_index_contract": publish_index_contract,
    },
    ensure_ascii=True,
    indent=2,
))
PY
