#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:="1.91.1"}"
: "${QUANT_CORE_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_core"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${CANDLE_SOURCE:="quant_core"}"
: "${STRATEGY_CONFIG_SOURCE:="quant_core"}"
: "${STRATEGY_SIGNAL_DISPATCH_MODE:="web"}"
: "${SMOKE_TIMEOUT_SECS:="90"}"
: "${APPLY_DDL:="true"}"
: "${RUN_EXECUTION_WORKER_AFTER_STRATEGY:="true"}"

case "${CANDLE_SOURCE}" in
    quant_core | postgres | pg) ;;
    *)
        echo "Refusing to run: live strategy smoke must read candles from quant_core." >&2
        echo "Set CANDLE_SOURCE=quant_core." >&2
        exit 2
        ;;
esac

case "${STRATEGY_CONFIG_SOURCE}" in
    quant_core | postgres) ;;
    *)
        echo "Refusing to run: live strategy smoke must load strategy_configs from quant_core." >&2
        echo "Set STRATEGY_CONFIG_SOURCE=quant_core." >&2
        exit 2
        ;;
esac

case "${STRATEGY_SIGNAL_DISPATCH_MODE}" in
    web | quant_web | execution_tasks) ;;
    *)
        echo "Refusing to run: live strategy smoke must dispatch signals to rust_quan_web, not direct exchange orders." >&2
        echo "Set STRATEGY_SIGNAL_DISPATCH_MODE=web." >&2
        exit 2
        ;;
esac

export RUSTUP_TOOLCHAIN
export QUANT_CORE_DATABASE_URL
export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export CANDLE_SOURCE
export STRATEGY_CONFIG_SOURCE
export STRATEGY_SIGNAL_DISPATCH_MODE
export EXECUTION_WORKER_DRY_RUN=true
export IS_RUN_REAL_STRATEGY=true
export IS_OPEN_SOCKET=false
export IS_BACK_TEST=false
export IS_RUN_SYNC_DATA_JOB=false
export IS_RUN_EXECUTION_WORKER=false
export EXIT_AFTER_REAL_STRATEGY_ONESHOT=true
export APP_ENV="${APP_ENV:-local}"

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
    export RUSTC
fi

cd "${REPO_ROOT}"

if [[ "${APPLY_DDL}" == "true" || "${APPLY_DDL}" == "1" || "${APPLY_DDL}" == "yes" ]]; then
    ./scripts/dev/ddl_smoke.sh
fi

echo
echo "Live strategy quant_core startup smoke"
echo "  quant_core db: ${QUANT_CORE_DATABASE_URL}"
echo "  candle_source: ${CANDLE_SOURCE}"
echo "  strategy_config_source: ${STRATEGY_CONFIG_SOURCE}"
echo "  signal_dispatch: ${STRATEGY_SIGNAL_DISPATCH_MODE}"
echo "  web: ${RUST_QUAN_WEB_BASE_URL}"
echo "  timeout: ${SMOKE_TIMEOUT_SECS}s"
echo "  websocket: ${IS_OPEN_SOCKET}"
echo "  real_exchange_orders: disabled by web dispatch"
echo "  run_worker_after_strategy: ${RUN_EXECUTION_WORKER_AFTER_STRATEGY}"

if command -v rustup >/dev/null 2>&1; then
    smoke_cmd=(rustup run "${RUSTUP_TOOLCHAIN}" cargo run --bin rust_quant "$@")
else
    smoke_cmd=(cargo run --bin rust_quant "$@")
fi

if command -v timeout >/dev/null 2>&1; then
    timeout "${SMOKE_TIMEOUT_SECS}" "${smoke_cmd[@]}"
elif command -v gtimeout >/dev/null 2>&1; then
    gtimeout "${SMOKE_TIMEOUT_SECS}" "${smoke_cmd[@]}"
else
    "${smoke_cmd[@]}" &
    child_pid=$!
    deadline=$((SECONDS + SMOKE_TIMEOUT_SECS))
    while kill -0 "${child_pid}" >/dev/null 2>&1; do
        if (( SECONDS >= deadline )); then
            echo "Smoke timeout after ${SMOKE_TIMEOUT_SECS}s; stopping rust_quant." >&2
            kill "${child_pid}" >/dev/null 2>&1 || true
            wait "${child_pid}" || true
            exit 124
        fi
        sleep 1
    done
    wait "${child_pid}"
fi

case "${RUN_EXECUTION_WORKER_AFTER_STRATEGY}" in
    true | TRUE | 1 | yes | YES)
        echo
        echo "Running execution worker dry-run follow-up"
        ./scripts/dev/run_execution_worker_dry_run.sh
        ;;
    false | FALSE | 0 | no | NO) ;;
    *)
        echo "Refusing to run: RUN_EXECUTION_WORKER_AFTER_STRATEGY must be true or false." >&2
        exit 2
        ;;
esac
