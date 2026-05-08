#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PUBLISHER_SCRIPT="${REPO_ROOT}/scripts/dev/publish_full_product_health_artifact_set.sh"

# Explicit-path-only smoke. Do not read .env. Do not call signed exchange
# endpoints like /fapi/v1/order. Do not touch LINKUSDT. Do not call
# /api/commerce/internal/execution-tasks/lease. Do not send Authorization
# headers or API key / api_secret material. Never print raw_payload or local
# filesystem paths. Block markers include postgres://, mysql://, api_key,
# api_secret, raw_payload, and .env.
# Required explicit artifact inputs are still the publisher inputs:
# FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH
# FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH
# FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH
# FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT
# FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW
: "${ADMIN_INGEST_URL:=}"
: "${ADMIN_INGEST_ALLOW_REMOTE:=false}"

if [[ ! -x "${PUBLISHER_SCRIPT}" ]]; then
    printf 'publisher script is required\n' >&2
    exit 2
fi

work_dir="$(mktemp -d)"
payload_path="${work_dir}/payload.json"
response_path="${work_dir}/response.json"
curl_stderr_path="${work_dir}/curl.stderr"
trap 'rm -rf "${work_dir}"' EXIT

"${PUBLISHER_SCRIPT}" >"${payload_path}"

if [[ ! -s "${payload_path}" ]]; then
    printf 'publisher returned empty payload\n' >&2
    exit 1
fi

if [[ -z "${ADMIN_INGEST_URL}" ]]; then
    cat "${payload_path}"
    exit 0
fi

python3 - "${ADMIN_INGEST_URL}" "${ADMIN_INGEST_ALLOW_REMOTE}" <<'PY'
import sys
from urllib.parse import urlparse

url = sys.argv[1].strip()
allow_remote = sys.argv[2].strip().lower() in {"1", "true", "yes"}

parsed = urlparse(url)
if not parsed.scheme or not parsed.netloc:
    raise SystemExit("ADMIN_INGEST_URL must be an absolute URL")
if parsed.scheme not in {"http", "https"}:
    raise SystemExit("ADMIN_INGEST_URL must use http or https")
host = (parsed.hostname or "").lower()
is_local = host in {"127.0.0.1", "localhost"}
if not is_local and not allow_remote:
    raise SystemExit("ADMIN_INGEST_URL must target localhost/127.0.0.1 unless ADMIN_INGEST_ALLOW_REMOTE=true")
PY

http_status="$(
    curl \
        --silent \
        --show-error \
        --output "${response_path}" \
        --write-out '%{http_code}' \
        --request POST \
        --header 'Content-Type: application/json' \
        --data-binary "@${payload_path}" \
        "${ADMIN_INGEST_URL}" \
        2>"${curl_stderr_path}"
)"

python3 - "${ADMIN_INGEST_URL}" "${response_path}" "${http_status}" "${curl_stderr_path}" <<'PY'
import json
import sys
from pathlib import Path
from urllib.parse import urlparse

url = sys.argv[1]
response_path = Path(sys.argv[2])
http_status = sys.argv[3]
curl_stderr_path = Path(sys.argv[4])

parsed = urlparse(url)
host = parsed.hostname or ""
path = parsed.path or "/"
remote = host not in {"127.0.0.1", "localhost"}

response_json = {}
response_text = response_path.read_text(encoding="utf-8").strip() if response_path.exists() else ""
if response_text:
    try:
        loaded = json.loads(response_text)
    except json.JSONDecodeError:
        loaded = {}
    if isinstance(loaded, dict):
        for field in ("status", "requestId", "code"):
            value = loaded.get(field)
            if isinstance(value, (str, int, float, bool)) or value is None:
                response_json[field] = value

curl_error = curl_stderr_path.read_text(encoding="utf-8").strip() if curl_stderr_path.exists() else ""
payload = {
    "mode": "post",
    "destination": {
        "host": host,
        "path": path,
        "remote": remote,
    },
    "http": {
        "status": int(http_status) if http_status.isdigit() else 0,
        "ok": http_status.isdigit() and 200 <= int(http_status) < 300,
    },
    "response": response_json,
}
if curl_error:
    payload["http"]["error"] = "curl_request_failed"
    print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    raise SystemExit(1)
print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
if not payload["http"]["ok"]:
    raise SystemExit(1)
PY
