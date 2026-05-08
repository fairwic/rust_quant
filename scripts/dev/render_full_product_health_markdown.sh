#!/usr/bin/env bash
set -euo pipefail

: "${FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT:=markdown}"
: "${FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH:=}"
: "${FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH:=}"
: "${FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH:=}"
: "${FULL_PRODUCT_HEALTH_MARKDOWN_PATH:=}"

if [[ "${FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT}" != "markdown" ]]; then
    printf 'FULL_PRODUCT_HEALTH_MARKDOWN_OUTPUT must be markdown\n' >&2
    exit 2
fi

python3 - \
    "${FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_JSON_PATH}" \
    "${FULL_PRODUCT_HEALTH_MARKDOWN_FULL_REPORT_PATH}" \
    "${FULL_PRODUCT_HEALTH_MARKDOWN_SUMMARY_PATH}" \
    "${FULL_PRODUCT_HEALTH_MARKDOWN_PATH}" \
    <<'PY'
import json
import sys
from pathlib import Path
from typing import Any


summary_json_path = sys.argv[1]
full_report_path = sys.argv[2]
summary_path = sys.argv[3]
markdown_path = sys.argv[4]

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
    "/fapi/v1/order",
    "/fapi/v2/account",
    "/fapi/v1/positionRisk",
    "/fapi/v2/positionRisk",
    "/fapi/v1/positionSide/dual",
    "/api/commerce/internal/execution-tasks/lease",
    "/api/commerce/internal/execution-results",
    "/api/commerce/internal/order-results",
    "LINKUSDT",
    "LINK-USDT",
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

COUNT_KEYS = [
    "p0_count",
    "p1_count",
    "info_count",
    "section_count",
    "blocking_section_count",
    "warning_section_count",
    "top_alert_count",
    "required_operator_action_count",
    "alert_taxonomy_count",
    "correlation_id_count",
    "read_only_input_count",
]

PLAYBOOK_COUNT_KEYS = [
    "item_count",
    "blocking_item_count",
    "manual_review_item_count",
    "observe_only_item_count",
]


class MarkdownError(Exception):
    pass


def has_blocked_marker(value: str) -> bool:
    lowered = value.lower()
    return any(marker.lower() in lowered for marker in BLOCKED_MARKERS)


def is_blocked_key(key: Any) -> bool:
    lowered = str(key).lower()
    return any(fragment in lowered for fragment in BLOCKED_KEY_FRAGMENTS)


def sanitize_json(value: Any) -> Any:
    if isinstance(value, dict):
        sanitized = {}
        for key, item in value.items():
            if is_blocked_key(key) or has_blocked_marker(str(key)):
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


def markdown_cell(value: Any) -> str:
    if value is None or value == "":
        return "-"
    text = str(sanitize_json(value))
    if has_blocked_marker(text):
        return "[redacted]"
    return text.replace("|", "\\|").replace("\n", " ").replace("\r", " ").strip()


def artifact_path(value: str) -> str:
    if not value:
        return "[not provided]"
    if has_blocked_marker(value):
        return "[redacted]"
    return markdown_cell(value)


def read_summary() -> dict[str, Any]:
    if summary_json_path:
        if ".env" in summary_json_path.lower():
            raise MarkdownError("summary input path was rejected")
        path = Path(summary_json_path)
        if not path.is_file():
            raise MarkdownError("summary input file is missing")
        text = path.read_text(encoding="utf-8")
    else:
        text = sys.stdin.read()
    if not text.strip():
        raise MarkdownError("summary input is empty")
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as error:
        raise MarkdownError(f"summary input is not valid JSON: {error}")
    if not isinstance(payload, dict):
        raise MarkdownError("summary input must be a JSON object")
    return sanitize_json(payload)


def list_items(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, dict)]


def as_bool_text(value: Any) -> str:
    if value is True:
        return "yes"
    if value is False:
        return "no"
    return markdown_cell(value)


def skipped_sections(payload: dict[str, Any]) -> list[dict[str, Any]]:
    skipped: list[dict[str, Any]] = []
    seen: set[tuple[str, str]] = set()

    for item in list_items(payload.get("checklist")):
        reason = str(item.get("reason_code") or "")
        is_skipped = item.get("skipped") is True or reason.endswith("_SKIPPED")
        if not is_skipped:
            continue
        section = str(item.get("section") or "unknown")
        key = (section, reason or "SKIPPED")
        if key in seen:
            continue
        seen.add(key)
        skipped.append(
            {
                "section": section,
                "status": item.get("status") or "-",
                "code": reason or "SKIPPED",
                "message": item.get("message") or "section input skipped",
            }
        )

    for alert in list_items(payload.get("top_alerts")):
        code = str(alert.get("code") or "")
        if not code.endswith("_SKIPPED") and "skipped" not in str(alert.get("message") or "").lower():
            continue
        section = str(alert.get("section") or "unknown")
        key = (section, code or "SKIPPED")
        if key in seen:
            continue
        seen.add(key)
        skipped.append(
            {
                "section": section,
                "status": "-",
                "code": code or "SKIPPED",
                "message": alert.get("message") or "section input skipped",
            }
        )

    return skipped


def render_table(headers: list[str], rows: list[list[Any]]) -> list[str]:
    lines = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    for row in rows:
        lines.append("| " + " | ".join(markdown_cell(value) for value in row) + " |")
    return lines


def render_markdown(payload: dict[str, Any]) -> str:
    summary = payload.get("summary")
    summary = summary if isinstance(summary, dict) else {}
    status = markdown_cell(summary.get("overall_status") or payload.get("status") or "unknown")

    lines: list[str] = [
        "# Full Product Health",
        "",
        f"**Status:** {status}",
        "",
        "## Counts",
        "",
    ]
    lines.extend(render_table(["Metric", "Value"], [[key, summary.get(key, 0)] for key in COUNT_KEYS]))

    lines.extend(
        [
            "",
            "## Top Alerts",
            "",
        ]
    )
    alerts = list_items(payload.get("top_alerts"))
    if alerts:
        lines.extend(
            render_table(
                ["Severity", "Code", "Section", "Message"],
                [
                    [
                        alert.get("severity"),
                        alert.get("code"),
                        alert.get("section"),
                        alert.get("message"),
                    ]
                    for alert in alerts
                ],
            )
        )
    else:
        lines.append("No top alerts.")

    playbook = payload.get("operator_playbook_summary")
    playbook = playbook if isinstance(playbook, dict) else {}
    lines.extend(
        [
            "",
            "## Operator Playbook Summary",
            "",
        ]
    )
    lines.extend(
        render_table(
            ["Metric", "Value"],
            [[key, playbook.get(key, 0)] for key in PLAYBOOK_COUNT_KEYS],
        )
    )
    playbook_items = list_items(playbook.get("items"))
    if playbook_items:
        lines.extend(
            [
                "",
            ]
        )
        lines.extend(
            render_table(
                [
                    "Source",
                    "Severity",
                    "Code",
                    "Section",
                    "Operator Action",
                    "Owner",
                    "Default Next Action",
                    "Admin Link Target",
                ],
                [
                    [
                        item.get("source"),
                        item.get("severity"),
                        item.get("code"),
                        item.get("section"),
                        item.get("operator_action"),
                        item.get("owner"),
                        item.get("default_next_action"),
                        item.get("admin_link_target"),
                    ]
                    for item in playbook_items
                ],
            )
        )
    else:
        lines.append("")
        lines.append("No operator playbook items.")

    lines.extend(
        [
            "",
            "## Checklist",
            "",
        ]
    )
    checklist = list_items(payload.get("checklist"))
    if checklist:
        lines.extend(
            render_table(
                ["Section", "Status", "Ready", "Action Required", "P0", "P1", "Info", "Reason"],
                [
                    [
                        item.get("section"),
                        item.get("status"),
                        as_bool_text(item.get("ready")),
                        as_bool_text(item.get("action_required")),
                        item.get("p0_count", 0),
                        item.get("p1_count", 0),
                        item.get("info_count", 0),
                        item.get("reason_code") or "-",
                    ]
                    for item in checklist
                ],
            )
        )
    else:
        lines.append("No checklist items.")

    lines.extend(
        [
            "",
            "## Artifact Paths",
            "",
        ]
    )
    lines.extend(
        render_table(
            ["Artifact", "Path"],
            [
                ["full_report_json", artifact_path(full_report_path)],
                ["summary_json", artifact_path(summary_path)],
                ["markdown", artifact_path(markdown_path)],
            ],
        )
    )

    lines.extend(
        [
            "",
            "## Skipped Sections",
            "",
        ]
    )
    skipped = skipped_sections(payload)
    if skipped:
        lines.extend(
            render_table(
                ["Section", "Status", "Code", "Message"],
                [
                    [
                        item.get("section"),
                        item.get("status"),
                        item.get("code"),
                        item.get("message"),
                    ]
                    for item in skipped
                ],
            )
        )
    else:
        lines.append("No skipped sections.")

    return "\n".join(lines).rstrip() + "\n"


def fail_markdown(message: str) -> str:
    return "\n".join(
        [
            "# Full Product Health",
            "",
            "**Status:** fail",
            "",
            "## Top Alerts",
            "",
            "| Severity | Code | Section | Message |",
            "| --- | --- | --- | --- |",
            f"| P0 | FULL_PRODUCT_HEALTH_MARKDOWN_FAILED | admin_readiness | {markdown_cell(message)} |",
            "",
            "## Operator Playbook Summary",
            "",
            "No operator playbook items.",
            "",
        ]
    )


exit_code = 0
try:
    rendered = render_markdown(read_summary())
except MarkdownError as error:
    rendered = fail_markdown(str(error))
    exit_code = 1

if has_blocked_marker(rendered):
    rendered = fail_markdown("markdown output was rejected")
    exit_code = 1

print(rendered, end="")
sys.exit(exit_code)
PY
