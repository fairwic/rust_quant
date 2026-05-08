#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SMOKE_SCRIPT="${REPO_ROOT}/scripts/dev/smoke_publish_full_product_health_admin_ingest.sh"
MOCK_RECEIVER_SCRIPT="${REPO_ROOT}/scripts/dev/mock_full_product_health_admin_ingest_receiver.py"

# Contract smoke stays local-only. Do not read .env. Do not call signed
# exchange endpoints, lease/report/order mutation endpoints, or any live probe.
# The mock receiver binds to 127.0.0.1 only and stdout prints just a sanitized
# delivery summary instead of the raw POST payload.
if [[ ! -x "${SMOKE_SCRIPT}" ]]; then
    printf 'smoke script is required\n' >&2
    exit 2
fi

if [[ ! -f "${MOCK_RECEIVER_SCRIPT}" ]]; then
    printf 'mock receiver script is required\n' >&2
    exit 2
fi

required_artifact_env=(
    "FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH"
    "FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH"
    "FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH"
    "FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT"
)

missing_artifact_env=()
for required_var in "${required_artifact_env[@]}"; do
    if [[ -z "${!required_var:-}" ]]; then
        missing_artifact_env+=("${required_var}")
    fi
done

if (( ${#missing_artifact_env[@]} > 0 )); then
    printf 'contract smoke requires explicit artifact env before starting mock receiver\n' >&2
    printf 'missing env:' >&2
    for missing_var in "${missing_artifact_env[@]}"; do
        printf ' %s' "${missing_var}" >&2
    done
    printf '\n' >&2
    printf 'safety boundary: does not read .env; does not scan directories; does not call exchange, signed, account, order, or position endpoints\n' >&2
    exit 2
fi

work_dir="$(mktemp -d)"
ready_path="${work_dir}/receiver-ready.json"
capture_path="${work_dir}/receiver-capture.json"
delivery_path="${work_dir}/delivery.json"
receiver_log_path="${work_dir}/receiver.stderr"
receiver_pid=""
cleanup() {
    if [[ -n "${receiver_pid}" ]] && kill -0 "${receiver_pid}" 2>/dev/null; then
        kill "${receiver_pid}" 2>/dev/null || true
        wait "${receiver_pid}" 2>/dev/null || true
    fi
    rm -rf "${work_dir}"
}
trap cleanup EXIT

python3 "${MOCK_RECEIVER_SCRIPT}" \
    --ready-path "${ready_path}" \
    --capture-path "${capture_path}" \
    --path /admin/ingest \
    2>"${receiver_log_path}" &
receiver_pid="$!"

for _ in {1..50}; do
    if [[ -s "${ready_path}" ]]; then
        break
    fi
    if ! kill -0 "${receiver_pid}" 2>/dev/null; then
        printf 'mock receiver exited before becoming ready\n' >&2
        cat "${receiver_log_path}" >&2 || true
        exit 1
    fi
    sleep 0.1
done

if [[ ! -s "${ready_path}" ]]; then
    printf 'mock receiver did not become ready\n' >&2
    exit 1
fi

admin_ingest_url="$(
    python3 - "${ready_path}" <<'PY'
import json
import sys
from pathlib import Path

ready = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(f"http://127.0.0.1:{ready['port']}{ready['path']}")
PY
)"

ADMIN_INGEST_URL="${admin_ingest_url}" "${SMOKE_SCRIPT}" >"${delivery_path}"
wait "${receiver_pid}"
receiver_pid=""

if [[ ! -s "${capture_path}" ]]; then
    printf 'mock receiver capture is required\n' >&2
    exit 1
fi

python3 - "${capture_path}" "${delivery_path}" <<'PY'
import json
import sys
from pathlib import Path

capture = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
delivery = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))

request = capture.get("request", {})
body = request.get("body", {})

if request.get("method") != "POST":
    raise SystemExit("contract smoke expected POST")
if request.get("contentType") != "application/json":
    raise SystemExit("contract smoke expected application/json content type")
if request.get("hasAuthorization"):
    raise SystemExit("contract smoke must not send Authorization")
if body.get("redactionStatus") != "ok":
    raise SystemExit("contract smoke requires redaction status ok")
if body.get("sensitiveMarkerCount") != 0:
    raise SystemExit("contract smoke requires zero sensitive markers")
if body.get("blockedMarkers") or body.get("localPathMarkers"):
    raise SystemExit("contract smoke captured blocked markers or local paths")

summary = {
    "mode": "mock_contract",
    "request": {
        "method": request.get("method"),
        "path": request.get("path"),
        "contentType": request.get("contentType"),
        "hasAuthorization": request.get("hasAuthorization"),
        "body": {
            "sha256": body.get("sha256"),
            "bytes": body.get("bytes"),
            "redactionStatus": body.get("redactionStatus"),
            "sensitiveMarkerCount": body.get("sensitiveMarkerCount"),
            "operatorRunId": body.get("operatorRunId"),
        },
    },
    "delivery": delivery,
}
print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
PY
