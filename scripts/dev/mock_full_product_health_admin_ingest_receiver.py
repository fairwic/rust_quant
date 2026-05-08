#!/usr/bin/env python3
import argparse
import hashlib
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path


BLOCKED_MARKER_GROUPS = [
    ("ENV_FILE_REFERENCE", [".env"]),
    ("DB_CONNECTION_STRING", ["postgres://", "postgresql://", "mysql://"]),
    ("CREDENTIAL_TOKEN", ["api_key", "apikey", "api secret", "api_secret", "secret"]),
    ("RAW_CONTENT", ["request_payload", "response_payload", "raw_payload"]),
    (
        "SIGNED_EXCHANGE_ENDPOINT",
        [
            "/fapi/v1/order",
            "/fapi/v2/account",
            "/fapi/v1/positionRisk",
            "/fapi/v2/positionRisk",
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

LOCAL_PATH_PREFIXES = ("/users/", "/home/", "/tmp/", "/var/", "/private/", "/volumes/", "/opt/", "/etc/")


def marker_codes(value: str) -> list[str]:
    lowered = value.lower()
    matches: list[str] = []
    for code, patterns in BLOCKED_MARKER_GROUPS:
        if any(pattern.lower() in lowered for pattern in patterns):
            matches.append(code)
    return matches


def local_path_markers(value: str) -> list[str]:
    lowered = value.lower()
    markers: list[str] = []
    if lowered.startswith("file://"):
        markers.append("LOCAL_FILE_URL")
    if any(lowered.startswith(prefix) for prefix in LOCAL_PATH_PREFIXES):
        markers.append("LOCAL_FILESYSTEM_PATH")
    if len(value) >= 3 and value[1] == ":" and value[2] in ("\\", "/"):
        markers.append("WINDOWS_ABSOLUTE_PATH")
    return markers


def summarize_payload(body_text: str) -> dict:
    payload = json.loads(body_text)
    if not isinstance(payload, dict):
        raise ValueError("payload root must be an object")
    redaction = payload.get("redaction")
    operator_metadata = payload.get("operatorMetadata")
    if not isinstance(redaction, dict):
        raise ValueError("payload.redaction must be an object")
    if not isinstance(operator_metadata, dict):
        raise ValueError("payload.operatorMetadata must be an object")
    sensitive_markers = sorted(set(marker_codes(body_text)))
    local_markers = sorted(set(local_path_markers(body_text)))
    return {
        "sha256": hashlib.sha256(body_text.encode("utf-8")).hexdigest(),
        "bytes": len(body_text.encode("utf-8")),
        "redactionStatus": redaction.get("status"),
        "sensitiveMarkerCount": redaction.get("sensitiveMarkerCount"),
        "operatorRunId": operator_metadata.get("runId"),
        "blockedMarkers": sensitive_markers,
        "localPathMarkers": local_markers,
    }


def build_handler(capture_path: Path, expected_path: str):
    class Handler(BaseHTTPRequestHandler):
        server_version = "LocalAdminIngestMock/1.0"
        sys_version = ""

        def do_POST(self):
            if self.path != expected_path:
                self.send_error(404)
                return
            content_length = int(self.headers.get("Content-Length", "0"))
            body_bytes = self.rfile.read(content_length)
            body_text = body_bytes.decode("utf-8")
            body_summary = summarize_payload(body_text)
            capture = {
                "request": {
                    "method": "POST",
                    "path": self.path,
                    "contentType": self.headers.get("Content-Type", ""),
                    "hasAuthorization": "Authorization" in self.headers,
                    "body": body_summary,
                },
                "response": {"status": "accepted", "requestId": "mock-contract-1"},
            }
            capture_path.write_text(
                json.dumps(capture, ensure_ascii=False, separators=(",", ":")),
                encoding="utf-8",
            )
            response = json.dumps(capture["response"], ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            self.send_response(202)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)

        def log_message(self, fmt, *args):
            return

    return Handler


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ready-path", required=True)
    parser.add_argument("--capture-path", required=True)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--path", default="/admin/ingest")
    args = parser.parse_args()

    ready_path = Path(args.ready_path)
    capture_path = Path(args.capture_path)
    ready_path.parent.mkdir(parents=True, exist_ok=True)
    capture_path.parent.mkdir(parents=True, exist_ok=True)

    handler = build_handler(capture_path, args.path)
    server = HTTPServer(("127.0.0.1", args.port), handler)
    ready_payload = {"host": "127.0.0.1", "port": server.server_address[1], "path": args.path}
    ready_path.write_text(json.dumps(ready_payload, separators=(",", ":")), encoding="utf-8")
    server.timeout = 15
    server.handle_request()
    server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
