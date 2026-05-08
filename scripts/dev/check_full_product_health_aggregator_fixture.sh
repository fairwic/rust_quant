#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${FULL_PRODUCT_HEALTH_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_FIXTURE_PATH:=${REPO_ROOT}/docs/dev/full_product_health_aggregator.fixture.json}"
: "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH:=${REPO_ROOT}/scripts/dev/check_local_service_health.sh}"
: "${FULL_PRODUCT_HEALTH_SCHEMA_VERSION:=1}"

if [[ "${FULL_PRODUCT_HEALTH_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_OUTPUT must be json\n' >&2
    exit 1
fi

python3 - "${FULL_PRODUCT_HEALTH_FIXTURE_PATH}" "${FULL_PRODUCT_HEALTH_LOCAL_HEALTH_SCRIPT_PATH}" "${BASH_SOURCE[0]}" "${FULL_PRODUCT_HEALTH_SCHEMA_VERSION}" <<'PY'
import json
import sys
from pathlib import Path


fixture_path = Path(sys.argv[1])
local_health_script_path = Path(sys.argv[2])
runner_script_path = Path(sys.argv[3])
schema_version = int(sys.argv[4])

fixture_markers = [
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
]

local_health_endpoint_markers = [
    "/fapi/v1/order",
    "/fapi/v2/account",
    "/fapi/v1/positionRisk",
    "/fapi/v2/positionRisk",
    "/fapi/v1/positionSide/dual",
    "/fapi/v1/leverage",
    "/fapi/v1/marginType",
    "/api/commerce/internal/execution-tasks/lease",
    "/api/commerce/internal/execution-results",
    "/api/commerce/internal/order-results",
    "/risk-review",
    "linkusdt",
    "link-usdt",
]

required_top_level_keys = [
    "schema_version",
    "status",
    "generated_at",
    "summary",
    "sections",
    "alerts",
    "correlation",
]

required_section_keys = [
    "web_task_order_health",
    "news_source_ai_health",
    "quant_worker_checkpoint_audit",
    "admin_readiness",
]

required_correlation_keys = [
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
]


def fail_payload(message: str) -> dict:
    return {
        "schema_version": schema_version,
        "status": "fail",
        "generated_at": "fixture-validation-failed",
        "summary": {
            "p0_count": 1,
            "p1_count": 0,
            "info_count": 0,
            "web_open_task_count": 0,
            "news_degraded_source_count": 0,
            "quant_expected_worker_failures": 0,
            "quant_expected_worker_warnings": 0,
        },
        "sections": {
            "web_task_order_health": {"status": "ok"},
            "news_source_ai_health": {"status": "ok"},
            "quant_worker_checkpoint_audit": {"status": "ok"},
            "admin_readiness": {
                "status": "fail",
                "live_readiness": "blocked",
                "reason_code": "fixture_validation_failed",
                "manual_review_required": True,
            },
        },
        "alerts": [
            {
                "severity": "P0",
                "code": "FIXTURE_VALIDATION_FAILED",
                "section": "admin_readiness",
                "message": message,
            }
        ],
        "correlation": {
            "news_id": None,
            "analysis_result_id": None,
            "signal_inbox_id": None,
            "external_id": None,
            "execution_task_id": None,
            "execution_attempt_id": None,
            "request_id": None,
            "order_result_id": None,
            "trade_record_id": None,
            "worker_id": None,
        },
    }


def emit(payload: dict, exit_code: int) -> None:
    print(json.dumps(payload, ensure_ascii=True, indent=2))
    raise SystemExit(exit_code)


def read_lowered(path: Path) -> str:
    return path.read_text(encoding="utf-8").lower()


for required_path in [fixture_path, local_health_script_path, runner_script_path]:
    if not required_path.is_file():
        emit(
            fail_payload(f"required read-only input missing: {required_path.name}"),
            1,
        )

fixture_text = read_lowered(fixture_path)
local_health_text = read_lowered(local_health_script_path)

for marker in fixture_markers:
    if marker.lower() in fixture_text:
        emit(
            fail_payload(
                f"sensitive marker blocked in full_product_health_aggregator.fixture.json: {marker}"
            ),
            1,
        )

for marker in local_health_endpoint_markers:
    if marker.lower() in local_health_text:
        emit(
            fail_payload(
                f"unsafe endpoint marker blocked in check_local_service_health.sh: {marker}"
            ),
            1,
        )

try:
    payload = json.loads(fixture_path.read_text(encoding="utf-8"))
except json.JSONDecodeError as error:
    emit(fail_payload(f"fixture json parse failed: {error}"), 1)

for key in required_top_level_keys:
    if key not in payload:
        emit(fail_payload(f"fixture missing top-level field: {key}"), 1)

if payload.get("schema_version") != schema_version:
    emit(
        fail_payload(
            f"fixture schema_version mismatch: expected {schema_version}, got {payload.get('schema_version')}"
        ),
        1,
    )

if payload.get("status") not in {"ok", "warn", "fail"}:
    emit(fail_payload("fixture status must be one of ok/warn/fail"), 1)

for field in ["summary", "sections", "correlation"]:
    if not isinstance(payload.get(field), dict):
        emit(fail_payload(f"fixture field must be an object: {field}"), 1)

if not isinstance(payload.get("alerts"), list):
    emit(fail_payload("fixture field must be an array: alerts"), 1)

sections = payload["sections"]
for key in required_section_keys:
    if not isinstance(sections.get(key), dict):
        emit(fail_payload(f"fixture missing section object: {key}"), 1)

for index, alert in enumerate(payload["alerts"]):
    if not isinstance(alert, dict):
        emit(fail_payload(f"fixture alert must be an object at index {index}"), 1)
    for key in ["severity", "code", "section", "message"]:
        if not isinstance(alert.get(key), str):
            emit(
                fail_payload(
                    f"fixture alert field must be a string at index {index}: {key}"
                ),
                1,
            )

correlation = payload["correlation"]
for key in required_correlation_keys:
    if key not in correlation:
        emit(fail_payload(f"fixture correlation missing field: {key}"), 1)

emit(payload, 0)
PY
