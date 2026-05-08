#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT:=json}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH:=}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH:=}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH:=}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_STRICT:=false}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_VERSION:=1}"
: "${FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH:=}"

if [[ "${FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT}" != "json" ]]; then
    printf 'FULL_PRODUCT_HEALTH_VALIDATION_OUTPUT must be json\n' >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SCHEMA_PATH="${SCRIPT_DIR}/../../docs/dev/full_product_health_artifact_schema.json"
SCHEMA_PATH="${FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_PATH:-${DEFAULT_SCHEMA_PATH}}"

python3 - \
    "${FULL_PRODUCT_HEALTH_VALIDATION_FULL_REPORT_PATH}" \
    "${FULL_PRODUCT_HEALTH_VALIDATION_SUMMARY_PATH}" \
    "${FULL_PRODUCT_HEALTH_VALIDATION_MARKDOWN_PATH}" \
    "${FULL_PRODUCT_HEALTH_VALIDATION_STRICT}" \
    "${FULL_PRODUCT_HEALTH_VALIDATION_SCHEMA_VERSION}" \
    "${SCHEMA_PATH}" \
    <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


full_report_path = sys.argv[1]
summary_path = sys.argv[2]
markdown_path = sys.argv[3]
strict = sys.argv[4].lower() == "true"
schema_version = int(sys.argv[5])
schema_path = sys.argv[6]

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


def generated_at() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def marker_codes(value: str) -> list[str]:
    lowered = value.lower()
    codes: list[str] = []
    for code, patterns in blocked_marker_groups:
        if any(pattern.lower() in lowered for pattern in patterns):
            codes.append(code)
    return codes


def safe_path(path: str) -> str:
    if not path:
        return ""
    if marker_codes(path):
        return "[redacted]"
    return path


def finding(
    code: str,
    artifact: str,
    message: str,
    *,
    severity: str = "P0",
    marker_code: str | None = None,
    field: str | None = None,
) -> dict[str, Any]:
    item: dict[str, Any] = {
        "severity": severity,
        "code": code,
        "artifact": artifact,
        "message": message,
    }
    if marker_code:
        item["marker_code"] = marker_code
    if field:
        item["field"] = field
    return item


def schema_finding_severity() -> str:
    return "P0" if strict else "P1"


def schema_array(
    schema: dict[str, Any],
    key: str,
    findings: list[dict[str, Any]],
) -> list[str]:
    value = schema.get(key)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        findings.append(
            finding(
                "SCHEMA_FIELD_INVALID",
                "schema",
                "schema field should be an array of strings",
                severity=schema_finding_severity(),
                field=key,
            )
        )
        return []
    return value


def schema_string_array_object(
    schema: dict[str, Any],
    key: str,
    findings: list[dict[str, Any]],
) -> dict[str, set[str]]:
    value = schema.get(key)
    if not isinstance(value, dict):
        findings.append(
            finding(
                "SCHEMA_FIELD_INVALID",
                "schema",
                "schema field should be an object of string arrays",
                severity=schema_finding_severity(),
                field=key,
            )
        )
        return {}
    result: dict[str, set[str]] = {}
    for section, codes in value.items():
        section_name = str(section)
        if not isinstance(codes, list) or not all(isinstance(code, str) for code in codes):
            findings.append(
                finding(
                    "SCHEMA_FIELD_INVALID",
                    "schema",
                    "schema object values should be arrays of strings",
                    severity=schema_finding_severity(),
                    field=f"{key}.{section_name}",
                )
            )
            continue
        result[section_name] = set(codes)
    return result


def schema_artifact_array(
    schema: dict[str, Any],
    artifact: str,
    key: str,
    findings: list[dict[str, Any]],
) -> list[str]:
    artifact_schemas = schema.get("artifact_schemas")
    if not isinstance(artifact_schemas, dict):
        findings.append(
            finding(
                "SCHEMA_FIELD_INVALID",
                "schema",
                "schema artifact_schemas should be an object",
                severity=schema_finding_severity(),
                field="artifact_schemas",
            )
        )
        return []
    artifact_schema = artifact_schemas.get(artifact)
    if not isinstance(artifact_schema, dict):
        findings.append(
            finding(
                "SCHEMA_FIELD_INVALID",
                "schema",
                "schema artifact entry should be an object",
                severity=schema_finding_severity(),
                field=f"artifact_schemas.{artifact}",
            )
        )
        return []
    value = artifact_schema.get(key)
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        findings.append(
            finding(
                "SCHEMA_FIELD_INVALID",
                "schema",
                "schema artifact field should be an array of strings",
                severity=schema_finding_severity(),
                field=f"artifact_schemas.{artifact}.{key}",
            )
        )
        return []
    return value


def load_validation_schema(
    path: str,
    findings: list[dict[str, Any]],
) -> tuple[dict[str, Any], dict[str, Any]]:
    record: dict[str, Any] = {
        "path": safe_path(path),
        "exists": False,
        "json_valid": False,
        "schema_version": None,
    }
    if not path:
        findings.append(
            finding(
                "SCHEMA_PATH_MISSING",
                "schema",
                "validation schema path is not configured",
                severity=schema_finding_severity(),
            )
        )
        return record, {}
    if marker_codes(path):
        findings.append(
            finding(
                "SENSITIVE_PATH_MARKER",
                "schema",
                "schema path contains a blocked sensitive marker",
                marker_code=marker_codes(path)[0],
            )
        )
    file_path = Path(path)
    if not file_path.is_file():
        findings.append(
            finding(
                "SCHEMA_MISSING",
                "schema",
                "validation schema file is missing",
                severity=schema_finding_severity(),
            )
        )
        return record, {}
    record["exists"] = True
    text = file_path.read_text(encoding="utf-8")
    scan_text("schema", text, findings)
    try:
        schema = json.loads(text)
    except json.JSONDecodeError:
        findings.append(
            finding(
                "SCHEMA_INVALID_JSON",
                "schema",
                "validation schema JSON could not be parsed",
                severity=schema_finding_severity(),
            )
        )
        return record, {}
    if not isinstance(schema, dict):
        findings.append(
            finding(
                "SCHEMA_FIELD_INVALID",
                "schema",
                "validation schema root must be an object",
                severity=schema_finding_severity(),
            )
        )
        return record, {}
    record["json_valid"] = True
    record["schema_version"] = schema.get("schema_version")
    return record, schema


def scan_text(artifact: str, text: str, findings: list[dict[str, Any]]) -> int:
    count = 0
    for code in marker_codes(text):
        findings.append(
            finding(
                "SENSITIVE_MARKER_FOUND",
                artifact,
                "artifact contains a blocked sensitive marker",
                marker_code=code,
            )
        )
        count += 1
    return count


def validate_enum_value(
    artifact: str,
    field: str,
    value: Any,
    allowed_values: list[str],
    findings: list[dict[str, Any]],
    *,
    severity: str = "P0",
) -> None:
    if value is None:
        return
    if not allowed_values:
        return
    if not isinstance(value, str) or value not in allowed_values:
        findings.append(
            finding(
                "INVALID_ENUM_VALUE",
                artifact,
                "artifact field contains an unsupported enum value",
                severity=severity,
                field=field,
            )
        )


def validate_alert_enum_values(
    artifact: str,
    payload: dict[str, Any],
    list_field: str,
    findings: list[dict[str, Any]],
) -> None:
    items = payload.get(list_field)
    if items is None:
        return
    if not isinstance(items, list):
        findings.append(
            finding(
                "INVALID_JSON_SHAPE",
                artifact,
                "artifact field should be an array",
                field=list_field,
            )
        )
        return
    for index, item in enumerate(items):
        if not isinstance(item, dict):
            findings.append(
                finding(
                    "INVALID_JSON_SHAPE",
                    artifact,
                    "artifact list item should be an object",
                    field=f"{list_field}[{index}]",
                )
            )
            continue
        validate_enum_value(
            artifact,
            f"{list_field}[{index}].severity",
            item.get("severity"),
            allowed_severity_values,
            findings,
            severity="P1",
        )


def validate_alert_taxonomy_values(
    artifact: str,
    payload: dict[str, Any],
    findings: list[dict[str, Any]],
) -> None:
    items = payload.get("alert_taxonomy")
    if items is None:
        return
    if not isinstance(items, list):
        findings.append(
            finding(
                "INVALID_JSON_SHAPE",
                artifact,
                "alert taxonomy should be an array",
                field="alert_taxonomy",
            )
        )
        return
    for index, item in enumerate(items):
        if not isinstance(item, dict):
            findings.append(
                finding(
                    "INVALID_JSON_SHAPE",
                    artifact,
                    "alert taxonomy item should be an object",
                    field=f"alert_taxonomy[{index}]",
                )
            )
            continue
        for field_name in ["severity", "code", "section", "operator_action"]:
            if not isinstance(item.get(field_name), str):
                findings.append(
                    finding(
                        "INVALID_JSON_SHAPE",
                        artifact,
                        "alert taxonomy field should be a string",
                        field=f"alert_taxonomy[{index}].{field_name}",
                    )
                )
        validate_enum_value(
            artifact,
            f"alert_taxonomy[{index}].severity",
            item.get("severity"),
            allowed_severity_values,
            findings,
            severity="P1",
        )
        validate_enum_value(
            artifact,
            f"alert_taxonomy[{index}].operator_action",
            item.get("operator_action"),
            allowed_operator_action_values,
            findings,
            severity="P1",
        )
        validate_alert_taxonomy_code_values(
            artifact,
            f"alert_taxonomy[{index}].code",
            item.get("section"),
            item.get("code"),
            findings,
        )
        correlation_keys = item.get("correlation_keys")
        if not isinstance(correlation_keys, list) or not all(
            isinstance(key, str) for key in correlation_keys
        ):
            findings.append(
                finding(
                    "INVALID_JSON_SHAPE",
                    artifact,
                    "alert taxonomy correlation_keys should be an array of strings",
                    field=f"alert_taxonomy[{index}].correlation_keys",
                )
            )


def validate_alert_taxonomy_code_values(
    artifact: str,
    field: str,
    section: Any,
    code: Any,
    findings: list[dict[str, Any]],
) -> None:
    if not allowed_alert_code_values or code is None:
        return
    section_name = str(section or "")
    allowed_codes = set(allowed_alert_code_values.get("global", set()))
    allowed_codes.update(allowed_alert_code_values.get(section_name, set()))
    if not isinstance(code, str) or code not in allowed_codes:
        findings.append(
            finding(
                "INVALID_ALERT_CODE",
                artifact,
                "alert taxonomy code is not registered for its section",
                field=field,
            )
        )


def read_file_text(artifact: str, path: str, findings: list[dict[str, Any]]) -> tuple[bool, str]:
    if not path:
        findings.append(finding("MISSING_ARTIFACT_PATH", artifact, "artifact path is not configured"))
        return False, ""
    if marker_codes(path):
        findings.append(
            finding(
                "SENSITIVE_PATH_MARKER",
                artifact,
                "artifact path contains a blocked sensitive marker",
                marker_code=marker_codes(path)[0],
            )
        )
    file_path = Path(path)
    if not file_path.is_file():
        findings.append(finding("MISSING_ARTIFACT", artifact, "artifact file is missing"))
        return False, ""
    return True, file_path.read_text(encoding="utf-8")


def validate_json_artifact(
    artifact: str,
    path: str,
    required_fields: list[str],
    required_summary_fields: list[str],
    findings: list[dict[str, Any]],
) -> dict[str, Any]:
    record: dict[str, Any] = {
        "path": safe_path(path),
        "exists": False,
        "json_valid": False,
        "missing_fields": [],
        "missing_summary_fields": [],
    }
    exists, text = read_file_text(artifact, path, findings)
    record["exists"] = exists
    if not exists:
        return record
    scan_text(artifact, text, findings)
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        findings.append(finding("INVALID_JSON", artifact, "artifact JSON could not be parsed"))
        return record
    if not isinstance(payload, dict):
        findings.append(finding("INVALID_JSON_SHAPE", artifact, "artifact JSON root must be an object"))
        return record
    record["json_valid"] = True
    missing = [field for field in required_fields if field not in payload]
    record["missing_fields"] = missing
    for field in missing:
        findings.append(
            finding(
                "MISSING_REQUIRED_FIELD",
                artifact,
                "artifact JSON is missing a required top-level field",
                field=field,
            )
        )
    summary = payload.get("summary")
    if required_summary_fields:
        if not isinstance(summary, dict):
            for field in required_summary_fields:
                field_path = f"summary.{field}"
                record["missing_summary_fields"].append(field_path)
                findings.append(
                    finding(
                        "MISSING_REQUIRED_FIELD",
                        artifact,
                        "artifact JSON is missing a required summary field",
                        field=field_path,
                    )
                )
        else:
            missing_summary = [
                f"summary.{field}" for field in required_summary_fields if field not in summary
            ]
            record["missing_summary_fields"] = missing_summary
            for field in missing_summary:
                findings.append(
                    finding(
                        "MISSING_REQUIRED_FIELD",
                        artifact,
                        "artifact JSON is missing a required summary field",
                        field=field,
                    )
                )
    if artifact in {"full_report", "summary"}:
        validate_enum_value(
            artifact,
            "status",
            payload.get("status"),
            allowed_status_values,
            findings,
        )
    if artifact == "full_report":
        validate_alert_enum_values(artifact, payload, "alerts", findings)
        validate_alert_taxonomy_values(artifact, payload, findings)
    if artifact == "summary":
        if isinstance(summary, dict):
            validate_enum_value(
                artifact,
                "summary.overall_status",
                summary.get("overall_status"),
                allowed_status_values,
                findings,
            )
        validate_alert_enum_values(artifact, payload, "top_alerts", findings)
        validate_alert_enum_values(artifact, payload, "required_operator_actions", findings)
        validate_alert_taxonomy_values(artifact, payload, findings)
    return record


def validate_markdown_artifact(
    path: str,
    findings: list[dict[str, Any]],
) -> dict[str, Any]:
    artifact = "markdown"
    record: dict[str, Any] = {
        "path": safe_path(path),
        "provided": bool(path),
        "exists": False,
        "missing_markers": [],
    }
    if not path:
        return record
    exists, text = read_file_text(artifact, path, findings)
    record["exists"] = exists
    if not exists:
        return record
    scan_text(artifact, text, findings)
    missing = [marker for marker in required_markdown_markers if marker not in text]
    record["missing_markers"] = missing
    for marker in missing:
        findings.append(
            finding(
                "MISSING_MARKDOWN_MARKER",
                artifact,
                "markdown artifact is missing a required section marker",
                field=marker,
            )
        )
    return record


findings: list[dict[str, Any]] = []
schema_record, validation_schema = load_validation_schema(schema_path, findings)
allowed_status_values = schema_array(validation_schema, "status_values", findings) if validation_schema else []
allowed_severity_values = (
    schema_array(validation_schema, "severity_values", findings) if validation_schema else []
)
allowed_operator_action_values = (
    schema_array(validation_schema, "operator_action_values", findings) if validation_schema else []
)
allowed_alert_code_values = (
    schema_string_array_object(validation_schema, "alert_code_values", findings)
    if validation_schema
    else {}
)
required_full_report_fields = (
    schema_artifact_array(validation_schema, "full_report", "required_top_level", findings)
    if validation_schema
    else []
)
required_full_report_summary_fields = (
    schema_artifact_array(validation_schema, "full_report", "required_summary_fields", findings)
    if validation_schema
    else []
)
required_summary_fields = (
    schema_artifact_array(validation_schema, "summary", "required_top_level", findings)
    if validation_schema
    else []
)
required_summary_summary_fields = (
    schema_artifact_array(validation_schema, "summary", "required_summary_fields", findings)
    if validation_schema
    else []
)
required_markdown_markers = (
    schema_array(validation_schema, "markdown_required_markers", findings) if validation_schema else []
)
artifacts = {
    "full_report": validate_json_artifact(
        "full_report",
        full_report_path,
        required_full_report_fields,
        required_full_report_summary_fields,
        findings,
    ),
    "summary": validate_json_artifact(
        "summary",
        summary_path,
        required_summary_fields,
        required_summary_summary_fields,
        findings,
    ),
    "markdown": validate_markdown_artifact(markdown_path, findings),
}

missing_artifact_count = sum(
    1
    for artifact in artifacts.values()
    if artifact.get("provided", True) and not artifact.get("exists")
)
json_parse_error_count = sum(
    1
    for name, artifact in artifacts.items()
    if name != "markdown" and artifact.get("exists") and not artifact.get("json_valid")
)
missing_required_field_count = sum(
    len(artifact.get("missing_fields", [])) + len(artifact.get("missing_summary_fields", []))
    for artifact in artifacts.values()
)
sensitive_marker_count = sum(1 for item in findings if item["code"] in {"SENSITIVE_MARKER_FOUND", "SENSITIVE_PATH_MARKER"})
artifact_count = sum(1 for artifact in artifacts.values() if artifact.get("provided", True))
if any(item["severity"] == "P0" for item in findings):
    status = "fail"
elif findings:
    status = "warn"
else:
    status = "ok"

payload = {
    "schema_version": schema_version,
    "status": status,
    "generated_at": generated_at(),
    "schema": schema_record,
    "summary": {
        "artifact_count": artifact_count,
        "missing_artifact_count": missing_artifact_count,
        "json_parse_error_count": json_parse_error_count,
        "missing_required_field_count": missing_required_field_count,
        "sensitive_marker_count": sensitive_marker_count,
        "finding_count": len(findings),
    },
    "artifacts": artifacts,
    "findings": findings,
}

print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
sys.exit(1 if strict and findings else 0)
PY
