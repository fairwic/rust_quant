#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${WEB_BACKEND_DIR:="${REPO_ROOT}/../rust_quan_web/backend"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${WEB_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_web"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${RISK_CLOSE_SMOKE_BUYER_EMAIL:="demo-risk-close-worker@example.com"}"
: "${RISK_CLOSE_SMOKE_SYMBOL:="ETH-USDT-SWAP"}"
: "${EXECUTION_WORKER_LEASE_LIMIT:="100"}"

WEB_RISK_CLOSE_SMOKE="${WEB_BACKEND_DIR}/scripts/dev/smoke_risk_close_review_loop.sh"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required command: $1" >&2
        exit 1
    fi
}

psql_args() {
    if command -v psql >/dev/null 2>&1; then
        printf '%s\0' psql "$WEB_DATABASE_URL"
        return
    fi

    if command -v podman >/dev/null 2>&1 &&
        podman container exists "$POSTGRES_CONTAINER" >/dev/null 2>&1; then
        printf '%s\0' podman exec "$POSTGRES_CONTAINER" psql "$WEB_DATABASE_URL"
        return
    fi

    echo "psql was not found, and podman container '$POSTGRES_CONTAINER' is unavailable" >&2
    exit 1
}

run_web_tsv() {
    local sql="$1"
    local -a cmd=()
    while IFS= read -r -d '' arg; do
        cmd+=("$arg")
    done < <(psql_args)

    "${cmd[@]}" -v ON_ERROR_STOP=1 -At -F $'\t' -c "$sql"
}

run_web_table() {
    local sql="$1"
    local -a cmd=()
    while IFS= read -r -d '' arg; do
        cmd+=("$arg")
    done < <(psql_args)

    "${cmd[@]}" -v ON_ERROR_STOP=1 -P pager=off -c "$sql"
}

require_cmd curl
require_cmd sed

if [[ ! -x "$WEB_RISK_CLOSE_SMOKE" ]]; then
    echo "Web risk close smoke script is missing or not executable: $WEB_RISK_CLOSE_SMOKE" >&2
    exit 1
fi

if ! curl -fsS -m 3 "$RUST_QUAN_WEB_BASE_URL/" >/dev/null 2>&1; then
    echo "Web backend is not reachable at $RUST_QUAN_WEB_BASE_URL" >&2
    echo "Start rust_quan_web/backend first, then rerun this script." >&2
    exit 1
fi

echo "== pending close worker e2e smoke =="
echo "web_base_url=$RUST_QUAN_WEB_BASE_URL"
echo "web_database_url=$WEB_DATABASE_URL"
echo "buyer_email=$RISK_CLOSE_SMOKE_BUYER_EMAIL symbol=$RISK_CLOSE_SMOKE_SYMBOL"

echo
echo "1) create pending_close task via Web risk review smoke"
web_smoke_output="$(
    RISK_CLOSE_SMOKE_STOP_AFTER_REVIEW=1 \
        BASE_URL="$RUST_QUAN_WEB_BASE_URL" \
        EXECUTION_EVENT_SECRET="$EXECUTION_EVENT_SECRET" \
        DATABASE_URL="$WEB_DATABASE_URL" \
        POSTGRES_CONTAINER="$POSTGRES_CONTAINER" \
        RISK_CLOSE_SMOKE_BUYER_EMAIL="$RISK_CLOSE_SMOKE_BUYER_EMAIL" \
        RISK_CLOSE_SMOKE_SYMBOL="$RISK_CLOSE_SMOKE_SYMBOL" \
        "$WEB_RISK_CLOSE_SMOKE"
)"
printf '%s\n' "$web_smoke_output"

close_task_id="$(
    printf '%s\n' "$web_smoke_output" |
        sed -nE 's/.*close_task_id=([0-9]+).*/\1/p' |
        tail -n 1
)"

if [[ -z "$close_task_id" ]]; then
    echo "Web risk close smoke did not print close_task_id" >&2
    exit 1
fi

pending_close_count="$(
    run_web_tsv "
        SELECT COUNT(*)
        FROM execution_tasks
        WHERE task_type = 'risk_control_close_candidate'
          AND task_status = 'pending_close';
    "
)"

if [[ ! "$EXECUTION_WORKER_LEASE_LIMIT" =~ ^[0-9]+$ ]]; then
    echo "EXECUTION_WORKER_LEASE_LIMIT must be a positive integer: $EXECUTION_WORKER_LEASE_LIMIT" >&2
    exit 1
fi

if [[ ! "$pending_close_count" =~ ^[0-9]+$ ]]; then
    echo "pending close count query returned an invalid value: $pending_close_count" >&2
    exit 1
fi

effective_lease_limit="$EXECUTION_WORKER_LEASE_LIMIT"
if (( pending_close_count > effective_lease_limit )); then
    effective_lease_limit="$pending_close_count"
fi

echo "pending_close_count=$pending_close_count effective_lease_limit=$effective_lease_limit"

echo
echo "2) run rust_quant dry-run worker for pending_close task"
RUST_QUAN_WEB_BASE_URL="$RUST_QUAN_WEB_BASE_URL" \
    EXECUTION_EVENT_SECRET="$EXECUTION_EVENT_SECRET" \
    EXECUTION_WORKER_LEASE_LIMIT="$effective_lease_limit" \
    EXECUTION_WORKER_TASK_TYPES=risk_control_close_candidate \
    EXECUTION_WORKER_TASK_STATUSES=pending_close \
    EXECUTION_WORKER_DRY_RUN=true \
    "${SCRIPT_DIR}/run_execution_worker_dry_run.sh"

echo
echo "3) verify Web task, order result, and trade record"
result_row="$(
    run_web_tsv "
        SELECT
            t.task_status,
            COALESCE(o.order_side, ''),
            COALESCE(o.order_status, ''),
            COALESCE(tr.trade_status, '')
        FROM execution_tasks t
        LEFT JOIN exchange_order_results o ON o.execution_task_id = t.id
        LEFT JOIN user_trade_records tr ON tr.execution_task_id = t.id
        WHERE t.id = $close_task_id
          AND t.task_status = 'completed'
          AND o.order_side = 'sell'
        ORDER BY o.id DESC NULLS LAST, tr.id DESC NULLS LAST
        LIMIT 1;
    "
)"

if [[ -z "$result_row" ]]; then
    echo "pending_close task was not completed with a sell order_side by worker: task_id=$close_task_id" >&2
    run_web_table "
        SELECT
            t.id AS task_id,
            t.task_type,
            t.task_status,
            o.id AS order_result_id,
            o.order_side,
            o.order_status,
            tr.id AS trade_record_id,
            tr.trade_status
        FROM execution_tasks t
        LEFT JOIN exchange_order_results o ON o.execution_task_id = t.id
        LEFT JOIN user_trade_records tr ON tr.execution_task_id = t.id
        WHERE t.id = $close_task_id
        ORDER BY o.id DESC NULLS LAST, tr.id DESC NULLS LAST;
    "
    exit 1
fi

IFS=$'\t' read -r task_status order_side order_status trade_status <<<"$result_row"
echo "verified close_task_id=$close_task_id task_status=$task_status order_side=$order_side order_status=$order_status trade_status=$trade_status"

run_web_table "
    SELECT
        t.id AS task_id,
        t.task_type,
        t.task_status,
        (t.request_payload_json::json -> 'close_order') AS close_order,
        o.id AS order_result_id,
        o.exchange,
        o.order_side,
        o.order_status,
        tr.id AS trade_record_id,
        tr.trade_status
    FROM execution_tasks t
    LEFT JOIN exchange_order_results o ON o.execution_task_id = t.id
    LEFT JOIN user_trade_records tr ON tr.execution_task_id = t.id
    WHERE t.id = $close_task_id
    ORDER BY o.id DESC NULLS LAST, tr.id DESC NULLS LAST;
"

echo
echo "pending close worker e2e smoke completed for close_task_id=$close_task_id"
