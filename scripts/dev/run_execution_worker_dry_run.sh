#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:="1.91.1"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${EXECUTION_WORKER_ID:="rust_quant_local_dry_run"}"
: "${EXECUTION_WORKER_LEASE_LIMIT:="10"}"
: "${EXECUTION_WORKER_RUN_ONCE:="true"}"
: "${EXECUTION_WORKER_ONLY:="true"}"
: "${EXECUTION_WORKER_DRY_RUN:="true"}"
: "${EXECUTION_WORKER_DEFAULT_EXCHANGE:="binance"}"
: "${EXECUTION_WORKER_TASK_TYPES:="execute_signal,risk_control_close_candidate"}"
: "${EXECUTION_WORKER_TASK_STATUSES:="pending,pending_close"}"
: "${QUANT_CORE_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_core"}"

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
fi

case "${EXECUTION_WORKER_DRY_RUN}" in
    true | TRUE | 1 | yes | YES) ;;
    *)
        echo "Refusing to run: scripts/dev/run_execution_worker_dry_run.sh only supports dry-run execution." >&2
        echo "Set EXECUTION_WORKER_DRY_RUN=true or use a different live-order entrypoint." >&2
        exit 2
        ;;
esac

export RUSTUP_TOOLCHAIN
export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export EXECUTION_WORKER_ID
export EXECUTION_WORKER_LEASE_LIMIT
export EXECUTION_WORKER_RUN_ONCE
export EXECUTION_WORKER_ONLY
export EXECUTION_WORKER_DRY_RUN
export EXECUTION_WORKER_DEFAULT_EXCHANGE
export EXECUTION_WORKER_TASK_TYPES
export EXECUTION_WORKER_TASK_STATUSES
export QUANT_CORE_DATABASE_URL
export RUSTC
export IS_RUN_EXECUTION_WORKER=true
export IS_BACK_TEST=false
export IS_OPEN_SOCKET=false
export IS_RUN_REAL_STRATEGY=false
export IS_RUN_SYNC_DATA_JOB=false

echo "Execution worker dry-run smoke"
echo "  web: ${RUST_QUAN_WEB_BASE_URL}"
echo "  worker_id: ${EXECUTION_WORKER_ID}"
echo "  lease_limit: ${EXECUTION_WORKER_LEASE_LIMIT}"
echo "  run_once: ${EXECUTION_WORKER_RUN_ONCE}"
echo "  task_types: ${EXECUTION_WORKER_TASK_TYPES}"
echo "  task_statuses: ${EXECUTION_WORKER_TASK_STATUSES}"
echo "  quant_core db: ${QUANT_CORE_DATABASE_URL}"

cd "${REPO_ROOT}"
if command -v rustup >/dev/null 2>&1; then
    exec rustup run "${RUSTUP_TOOLCHAIN}" cargo run --bin rust_quant "$@"
fi

exec cargo run --bin rust_quant "$@"
