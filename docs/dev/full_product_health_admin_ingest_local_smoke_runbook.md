# Full Product Health Admin Ingest Local Smoke

## Scope

`scripts/dev/smoke_publish_full_product_health_admin_ingest.sh` is the local
no-secret smoke for the `rust_quant` publisher -> Admin ingest handoff.

Safety boundary:

- does not read `.env`
- does not call signed/account/order/position exchange endpoints
- does not place orders
- does not touch `LINKUSDT`
- does not call Web lease/report/mutate task endpoints
- does not add `Authorization` headers or shared secrets
- does not print DB URL, API key/secret, raw payload, or local filesystem path

The script only accepts explicit artifact paths via the existing
`FULL_PRODUCT_HEALTH_ARTIFACT_SET_*` inputs. It reuses
`publish_full_product_health_artifact_set.sh` to build the redacted ingest
payload.

Before the contract wrapper starts its localhost receiver, it requires these
explicit env vars to be present:

- `FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH`
- `FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH`
- `FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH`
- `FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT`

Missing any of them is a deliberate safe failure. The wrapper exits before it
binds a port, does not read `.env`, does not scan directories for artifacts,
and does not call any exchange or Web mutation endpoint.

## Dry Run

Without `ADMIN_INGEST_URL`, the script prints the redacted JSON payload to
stdout and performs no network call:

```bash
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=docs/dev/full_product_health_examples/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW=2026-05-07T01:05:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY=phase-53-local-smoke \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE=ci \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID=phase-53-dry-run \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA=abcdef1 \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO=rust_quant \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json \
./scripts/dev/smoke_publish_full_product_health_admin_ingest.sh
```

Expected result: stdout is a parseable, redacted Admin ingest payload. It must
not include database URLs, API keys, API secrets, raw request/response payloads,
or local paths.

## Local Mock POST

Start a localhost receiver. This keeps the smoke inside the explicit local
boundary:

```bash
python3 - <<'PY'
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        _ = self.rfile.read(length)
        self.send_response(202)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"status":"accepted","requestId":"mock-1"}')

    def log_message(self, fmt, *args):
        return

HTTPServer(("127.0.0.1", 18080), Handler).serve_forever()
PY
```

Then run the smoke against the explicit localhost URL:

```bash
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=docs/dev/full_product_health_examples/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW=2026-05-07T01:05:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY=phase-53-local-smoke \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE=ci \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID=phase-53-post \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA=abcdef1 \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO=rust_quant \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json \
ADMIN_INGEST_URL=http://127.0.0.1:18080/admin/ingest \
./scripts/dev/smoke_publish_full_product_health_admin_ingest.sh
```

Expected result: stdout is a small delivery summary, for example:

```json
{"mode":"post","destination":{"host":"127.0.0.1","path":"/admin/ingest","remote":false},"http":{"status":202,"ok":true},"response":{"status":"accepted","requestId":"mock-1"}}
```

The POST request includes only `Content-Type: application/json`. It must not
include `Authorization` headers or shared secrets.

## Contract Smoke With Safe Mock Receiver

If you want one command that proves the localhost POST contract, redaction, and
stdout safety together, run the contract wrapper. It starts a local receiver on
`127.0.0.1`, posts through the existing smoke script, validates the captured
request, and prints only a sanitized delivery summary:

```bash
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_REPORT_PATH=docs/dev/full_product_health_examples/full-product-health.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SUMMARY_PATH=docs/dev/full_product_health_examples/full-product-health-summary.json \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_PATH=docs/dev/full_product_health_examples/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_STORED_AT=2026-05-07T01:03:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_NOW=2026-05-07T01:05:00Z \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_GENERATED_BY=phase-54-local-contract \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_TRIGGER_TYPE=ci \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_RUN_ID=phase-54-contract \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_COMMIT_SHA=abcdef1 \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_SOURCE_REPO=rust_quant \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_MARKDOWN_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.md \
FULL_PRODUCT_HEALTH_ARTIFACT_SET_FULL_ARTIFACT_URL=/admin/artifacts/health-2026-05-07T01-00-00Z/full-product-health.json \
./scripts/dev/smoke_publish_full_product_health_admin_ingest_contract.sh
```

Expected result: stdout is compact JSON like:

```json
{"mode":"mock_contract","request":{"method":"POST","path":"/admin/ingest","contentType":"application/json","hasAuthorization":false,"body":{"sha256":"...","bytes":2448,"redactionStatus":"ok","sensitiveMarkerCount":0,"operatorRunId":"phase-54-contract"}},"delivery":{"mode":"post","destination":{"host":"127.0.0.1","path":"/admin/ingest","remote":false},"http":{"status":202,"ok":true},"response":{"status":"accepted","requestId":"mock-contract-1"}}}
```

This summary must not contain database URLs, API keys, API secrets, `.env`,
raw request/response payloads, or local filesystem paths.

## Missing Artifact Env Safety Failure

Running the contract wrapper without explicit artifact env is expected to fail
fast with stderr-only guidance:

```bash
./scripts/dev/smoke_publish_full_product_health_admin_ingest_contract.sh
```

Expected result:

- non-zero exit
- no stdout payload
- stderr lists the missing `FULL_PRODUCT_HEALTH_ARTIFACT_SET_*` vars
- stderr reminds operators that the wrapper does not read `.env`, does not
  scan directories, and does not call exchange endpoints

## Remote Explicit URL

The default script policy only allows `http://127.0.0.1/...` or
`http://localhost/...`. If an operator intentionally wants a non-local explicit
URL, they must opt in:

```bash
ADMIN_INGEST_ALLOW_REMOTE=true
```

This remains an explicit operator action. The script still does not inject
authorization secrets and still uses only the artifact paths you provided.
