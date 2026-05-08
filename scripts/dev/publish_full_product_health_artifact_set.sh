#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SLA_SECONDS:=900}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORAGE_STATUS:=current}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_RETENTION_CLASS:=current}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_ID:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL:=}"
: "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL:=}"

if [[ "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_ARTIFACT_SET_OUTPUT must be json\n' >&2
    exit 2
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SCHEMA_VERSION}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SLA_SECONDS}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORAGE_STATUS}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_RETENTION_CLASS}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_ID}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL}" \
    "${FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL}" \
    <<'PY'
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


blocked_marker_groups = [
    ("ENV_FILE_REFERENCE", [".env"]),
    ("DB_CONNECTION_STRING", ["postgres://", "postgresql://", "mysql://"]),
    ("DATABASE_URL_FIELD", ["database_url"]),
    ("CREDENTIAL_TOKEN", ["api_key", "apikey", "api key", "api_secret", "apisecret", "api secret", "secret"]),
    ("CIPHER_OR_PASSPHRASE", ["passphrase", "cipher"]),
    ("RAW_CONTENT", ["request_payload", "response_payload", "raw_payload", "request payload", "response payload", "raw payload"]),
    (
        "SIGNED_EXCHANGE_ENDPOINT",
        [
            "/fapi/v1/order",
            "/fapi/v2/account",
            "/fapi/v1/positionRisk",
            "/fapi/v2/positionRisk",
            "/fapi/v1/positionSide/dual",
            "/fapi/v1/leverage",
            "/fapi/v1/marginType",
        ],
    ),
    (
        "WEB_MUTATION_ENDPOINT",
        [
            "/api/commerce/internal/execution-tasks/lease",
            "/api/commerce/internal/execution-results",
            "/api/commerce/internal/order-results",
        ],
    ),
    ("LINK_POSITION_SYMBOL", ["LINKUSDT", "LINK-USDT"]),
]

allowed_storage_status = {"current", "superseded", "rejected"}
allowed_retention_class = {"current", "historical", "rejected"}
allowed_trigger_types = {"", "ci", "operator_upload", "scheduled"}
local_path_prefixes = (
    "/users/",
    "/home/",
    "/tmp/",
    "/var/",
    "/private/",
    "/volumes/",
    "/opt/",
    "/etc/",
)


def iso_now(override: str) -> str:
    if override:
        return parse_timestamp(override).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_timestamp(value: str) -> datetime:
    normalized = value.strip()
    if normalized.endswith("Z"):
        normalized = normalized[:-1] + "+00:00"
    parsed = datetime.fromisoformat(normalized)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def marker_codes(value: str) -> list[str]:
    lowered = value.lower()
    codes: list[str] = []
    for code, patterns in blocked_marker_groups:
        if any(pattern.lower() in lowered for pattern in patterns):
            codes.append(code)
    return codes


def safe_string(value: str) -> str:
    return "[redacted]" if marker_codes(value) else value


def finding(
    code: str,
    artifact: str,
    message: str,
    *,
    severity: str = "P0",
    field: str | None = None,
    marker: str | None = None,
) -> dict[str, Any]:
    item: dict[str, Any] = {
        "severity": severity,
        "code": code,
        "artifact": artifact,
        "message": message,
    }
    if field:
        item["field"] = field
    if marker:
        item["marker"] = marker
    return item


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def sha256_json(payload: dict[str, Any]) -> str:
    return sha256_text(json.dumps(payload, sort_keys=True, separators=(",", ":")))


def count_markers(text: str, artifact: str, findings: list[dict[str, Any]]) -> int:
    count = 0
    for marker in marker_codes(text):
        findings.append(
            finding(
                "SENSITIVE_MARKER_BLOCKED",
                artifact,
                "artifact content contains a blocked sensitive marker",
                marker=marker,
            )
        )
        count += 1
    return count


def read_text_artifact(
    artifact: str,
    path_value: str,
    *,
    expect_json: bool,
    findings: list[dict[str, Any]],
) -> dict[str, Any]:
    record: dict[str, Any] = {
        "name": artifact,
        "exists": False,
        "hash": None,
        "text": None,
        "payload": None,
        "sensitive_marker_count": 0,
    }
    if not path_value:
        findings.append(finding("ARTIFACT_PATH_MISSING", artifact, "artifact path is required"))
        return record
    if marker_codes(path_value):
        findings.append(
            finding(
                "SENSITIVE_PATH_MARKER",
                artifact,
                "artifact path contains a blocked sensitive marker",
                marker=marker_codes(path_value)[0],
                severity="P1",
            )
        )
    path = Path(path_value)
    if not path.is_file():
        findings.append(finding("ARTIFACT_MISSING", artifact, "artifact file does not exist"))
        return record
    record["exists"] = True
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        findings.append(finding("ARTIFACT_NOT_UTF8", artifact, "artifact file must be utf-8 text"))
        return record
    record["text"] = text
    record["hash"] = sha256_text(text)
    record["sensitive_marker_count"] = count_markers(text, artifact, findings)
    if expect_json:
        try:
            payload = json.loads(text)
        except json.JSONDecodeError:
            findings.append(finding("INVALID_JSON", artifact, "artifact JSON could not be parsed"))
            return record
        if not isinstance(payload, dict):
            findings.append(finding("INVALID_JSON_SHAPE", artifact, "artifact JSON root must be an object"))
            return record
        record["payload"] = payload
    return record


def pick_source_generated_at(summary_payload: dict[str, Any] | None, full_payload: dict[str, Any] | None) -> str | None:
    for payload in (summary_payload, full_payload):
        if not isinstance(payload, dict):
            continue
        for key in ("source_generated_at", "generated_at"):
            value = payload.get(key)
            if isinstance(value, str) and value.strip():
                return value.strip()
    return None


def sanitize_metadata_field(
    field: str,
    value: str,
    findings: list[dict[str, Any]],
) -> str:
    if not value:
        return value
    local_marker = local_path_marker(value)
    if local_marker:
        findings.append(
            finding(
                "OPERATOR_METADATA_REDACTED",
                "operatorMetadata",
                "operator metadata contained a local filesystem path",
                severity="P1",
                field=field,
                marker=local_marker,
            )
        )
        return "[redacted]"
    if marker_codes(value):
        findings.append(
            finding(
                "OPERATOR_METADATA_REDACTED",
                "operatorMetadata",
                "operator metadata contained a blocked sensitive marker",
                severity="P1",
                field=field,
                marker=marker_codes(value)[0],
            )
        )
        return "[redacted]"
    return value


def local_path_marker(value: str) -> str | None:
    normalized = value.strip()
    if not normalized:
        return None
    lowered = normalized.lower()
    if lowered.startswith("file://"):
        return "LOCAL_FILE_URL"
    if any(lowered.startswith(prefix) for prefix in local_path_prefixes):
        return "LOCAL_FILESYSTEM_PATH"
    if len(normalized) >= 3 and normalized[1] == ":" and normalized[2] in ("\\", "/"):
        return "WINDOWS_ABSOLUTE_PATH"
    return None


def sanitize_handoff_url(
    field: str,
    value: str,
    default_value: str,
    findings: list[dict[str, Any]],
) -> str | None:
    candidate = value.strip() if value.strip() else default_value
    local_marker = local_path_marker(candidate)
    if local_marker:
        findings.append(
            finding(
                "LOCAL_PATH_URL_BLOCKED",
                "artifact_set",
                "handoff url must not expose a local filesystem path",
                severity="P1",
                field=field,
                marker=local_marker,
            )
        )
        return None
    blocked_markers = marker_codes(candidate)
    if blocked_markers:
        findings.append(
            finding(
                "SENSITIVE_URL_MARKER",
                "artifact_set",
                "handoff url contained a blocked sensitive marker",
                severity="P1",
                field=field,
                marker=blocked_markers[0],
            )
        )
        return None
    return candidate


full_report_path = sys.argv[1]
summary_path = sys.argv[2]
markdown_path = sys.argv[3]
stored_at_raw = sys.argv[4]
now_raw = sys.argv[5]
schema_version = int(sys.argv[6])
sla_seconds = int(sys.argv[7])
requested_storage_status = sys.argv[8]
requested_retention_class = sys.argv[9]
requested_artifact_set_id = sys.argv[10]
generated_by = sys.argv[11]
trigger_type = sys.argv[12]
run_id = sys.argv[13]
commit_sha = sys.argv[14]
source_repo = sys.argv[15]
requested_markdown_url = sys.argv[16]
requested_full_artifact_url = sys.argv[17]

findings: list[dict[str, Any]] = []
full_report = read_text_artifact("full_report", full_report_path, expect_json=True, findings=findings)
summary = read_text_artifact("summary", summary_path, expect_json=True, findings=findings)
markdown = read_text_artifact("markdown", markdown_path, expect_json=False, findings=findings)

if requested_storage_status not in allowed_storage_status:
    findings.append(
        finding(
            "INVALID_STORAGE_STATUS",
            "artifact_set",
            "storageStatus must be current, superseded, or rejected",
            field="storageStatus",
        )
    )
if requested_retention_class not in allowed_retention_class:
    findings.append(
        finding(
            "INVALID_RETENTION_CLASS",
            "artifact_set",
            "retentionClass must be current, historical, or rejected",
            field="retentionClass",
        )
    )
if trigger_type not in allowed_trigger_types:
    findings.append(
        finding(
            "INVALID_TRIGGER_TYPE",
            "operatorMetadata",
            "triggerType must be ci, operator_upload, or scheduled",
            severity="P1",
            field="triggerType",
        )
    )
if sla_seconds <= 0:
    findings.append(
        finding(
            "INVALID_SLA_SECONDS",
            "artifact_set",
            "artifactSlaSeconds must be a positive integer",
            field="artifactSlaSeconds",
        )
    )

effective_now = iso_now(now_raw)
stored_at = stored_at_raw or effective_now
stored_at_parsed: datetime | None = None
source_generated_at = pick_source_generated_at(summary.get("payload"), full_report.get("payload"))
source_generated_at_parsed: datetime | None = None

try:
    stored_at_parsed = parse_timestamp(stored_at)
    stored_at = stored_at_parsed.replace(microsecond=0).isoformat().replace("+00:00", "Z")
except ValueError:
    findings.append(finding("INVALID_STORED_AT", "artifact_set", "storedAt must be a valid ISO-8601 timestamp", field="storedAt"))

if source_generated_at:
    try:
        source_generated_at_parsed = parse_timestamp(source_generated_at)
        source_generated_at = source_generated_at_parsed.replace(microsecond=0).isoformat().replace("+00:00", "Z")
    except ValueError:
        findings.append(
            finding(
                "INVALID_SOURCE_GENERATED_AT",
                "artifact_set",
                "sourceGeneratedAt must be a valid ISO-8601 timestamp",
                field="sourceGeneratedAt",
            )
        )
else:
    findings.append(
        finding(
            "SOURCE_GENERATED_AT_MISSING",
            "artifact_set",
            "sourceGeneratedAt could not be derived from summary or full artifact",
            field="sourceGeneratedAt",
        )
    )

now_parsed = parse_timestamp(effective_now)
stale = False
stale_reason: str | None = None
if stored_at_parsed is not None and stored_at_parsed > now_parsed:
    stale = True
    stale_reason = "stored_at_in_future"
elif stored_at_parsed is None:
    stale = True
    stale_reason = "invalid_stored_at"
elif source_generated_at_parsed is None:
    stale = True
    stale_reason = "missing_source_generated_at"
elif source_generated_at_parsed > stored_at_parsed:
    stale = True
    stale_reason = "source_generated_after_stored_at"
    findings.append(
        finding(
            "SOURCE_AFTER_STORED_AT",
            "artifact_set",
            "sourceGeneratedAt must not be later than storedAt",
            field="sourceGeneratedAt",
        )
    )
elif (now_parsed - stored_at_parsed).total_seconds() > sla_seconds:
    stale = True
    stale_reason = "stored_at_expired"
elif (now_parsed - source_generated_at_parsed).total_seconds() > sla_seconds:
    stale = True
    stale_reason = "source_generated_at_expired"

sanitized_generated_by = sanitize_metadata_field("generatedBy", generated_by, findings)
sanitized_trigger_type = sanitize_metadata_field("triggerType", trigger_type, findings)
sanitized_run_id = sanitize_metadata_field("runId", run_id, findings)
sanitized_commit_sha = sanitize_metadata_field("commitSha", commit_sha, findings)
sanitized_source_repo = sanitize_metadata_field("sourceRepo", source_repo, findings)

missing_artifact_count = sum(1 for item in (full_report, summary, markdown) if not item["exists"])
invalid_artifact_count = sum(
    1
    for item in (full_report, summary)
    if item["exists"] and item["payload"] is None
)
sensitive_marker_count = sum(item["sensitive_marker_count"] for item in (full_report, summary, markdown))
redacted_field_count = sum(1 for item in (sanitized_generated_by, sanitized_trigger_type, sanitized_run_id, sanitized_commit_sha, sanitized_source_repo) if item == "[redacted]")

summary_hash = summary["hash"] or ""
full_artifact_hash = full_report["hash"] or ""
markdown_hash = markdown["hash"] or ""

if requested_artifact_set_id:
    artifact_set_id = safe_string(requested_artifact_set_id)
else:
    source_token = (source_generated_at or "unknown").replace(":", "-")
    hash_seed = summary_hash or full_artifact_hash or sha256_text(source_generated_at or effective_now)
    artifact_set_id = f"health-{source_token}-{hash_seed[:12]}"

markdown_url = sanitize_handoff_url(
    "markdownUrl",
    requested_markdown_url,
    f"/admin/artifacts/{artifact_set_id}/full-product-health.md",
    findings,
)
full_artifact_url = sanitize_handoff_url(
    "fullArtifactUrl",
    requested_full_artifact_url,
    f"/admin/artifacts/{artifact_set_id}/full-product-health.json",
    findings,
)

validation_status = "ok" if not findings else "fail"
validation_payload = {
    "status": validation_status,
    "generatedAt": effective_now,
    "summary": {
        "artifactCount": 3,
        "missingArtifactCount": missing_artifact_count,
        "invalidArtifactCount": invalid_artifact_count,
        "sensitiveMarkerCount": sensitive_marker_count,
        "redactedFieldCount": redacted_field_count,
        "findingCount": len(findings),
    },
    "findings": findings,
}
validation_hash = sha256_json(validation_payload)

storage_status = requested_storage_status
retention_class = requested_retention_class
if validation_status != "ok":
    storage_status = "rejected"
    retention_class = "rejected"

payload = {
    "artifactSetId": artifact_set_id,
    "schemaVersion": schema_version,
    "storedAt": stored_at,
    "sourceGeneratedAt": source_generated_at,
    "summaryHash": summary_hash,
    "validationHash": validation_hash,
    "fullArtifactHash": full_artifact_hash,
    "markdownHash": markdown_hash,
    "storageStatus": storage_status,
    "retentionClass": retention_class,
    "artifactSlaSeconds": sla_seconds,
    "stale": stale,
    "staleReason": stale_reason,
    "summary": summary["payload"],
    "validation": validation_payload,
    "redaction": {
        "status": "ok" if sensitive_marker_count == 0 and redacted_field_count == 0 else "fail",
        "sensitiveMarkerCount": sensitive_marker_count,
        "redactedFieldCount": redacted_field_count,
    },
    "markdownUrl": markdown_url,
    "fullArtifactUrl": full_artifact_url,
    "operatorMetadata": {
        "generatedBy": sanitized_generated_by or None,
        "triggerType": sanitized_trigger_type or None,
        "runId": sanitized_run_id or None,
        "commitSha": sanitized_commit_sha or None,
        "sourceRepo": sanitized_source_repo or None,
    },
}

json.dump(payload, sys.stdout, ensure_ascii=True, indent=2)
sys.stdout.write("\n")
sys.exit(0 if validation_status == "ok" else 1)
PY
