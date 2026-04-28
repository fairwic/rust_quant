#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:="1.91.1"}"
: "${QUANT_CORE_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_core"}"
: "${WEB_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_web"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${SMOKE_SYMBOL:="ETH-USDT-SWAP"}"
: "${SYNC_ONLY_PERIODS:="1m"}"
: "${SMOKE_PERIOD:="${SYNC_ONLY_PERIODS}"}"
: "${SMOKE_STRATEGY_KEY:="vegas"}"
: "${SMOKE_STRATEGY_VERSION:="smoke-binance-websocket-eth-1m"}"
: "${SMOKE_STRATEGY_NAME:="Vegas Binance WebSocket Smoke"}"
: "${SMOKE_MIN_K_LINE_NUM:="60"}"
: "${SMOKE_SYNC_TIMEOUT_SECS:="180"}"
: "${SMOKE_LIVE_TIMEOUT_SECS:="180"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${QUANT_CORE_POSTGRES_DB:="quant_core"}"
: "${WEB_POSTGRES_DB:="quant_web"}"

WEB_SEED_SCRIPT="${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh"
LOG_FILE="${TMPDIR:-/tmp}/rust_quant_binance_websocket_smoke.$$.log"
RUST_QUANT_PID=""

run_with_timeout() {
    local timeout_secs="$1"
    shift

    if command -v timeout >/dev/null 2>&1; then
        timeout "${timeout_secs}" "$@"
        return
    fi

    if command -v gtimeout >/dev/null 2>&1; then
        gtimeout "${timeout_secs}" "$@"
        return
    fi

    "$@"
}

run_quant_sql() {
    if command -v psql >/dev/null 2>&1; then
        psql "${QUANT_CORE_DATABASE_URL}" -v ON_ERROR_STOP=1 "$@"
        return
    fi

    if command -v podman >/dev/null 2>&1 &&
        podman container exists "${POSTGRES_CONTAINER}" >/dev/null 2>&1; then
        podman exec -i "${POSTGRES_CONTAINER}" psql \
            -U "${POSTGRES_USER}" \
            -d "${QUANT_CORE_POSTGRES_DB}" \
            -v ON_ERROR_STOP=1 \
            "$@"
        return
    fi

    echo "Refusing to run: neither psql nor podman container '${POSTGRES_CONTAINER}' is available for quant_core." >&2
    exit 2
}

run_web_sql() {
    if command -v psql >/dev/null 2>&1; then
        psql "${WEB_DATABASE_URL}" -v ON_ERROR_STOP=1 "$@"
        return
    fi

    if command -v podman >/dev/null 2>&1 &&
        podman container exists "${POSTGRES_CONTAINER}" >/dev/null 2>&1; then
        podman exec -i "${POSTGRES_CONTAINER}" psql \
            -U "${POSTGRES_USER}" \
            -d "${WEB_POSTGRES_DB}" \
            -v ON_ERROR_STOP=1 \
            "$@"
        return
    fi

    echo "Refusing to run: neither psql nor podman container '${POSTGRES_CONTAINER}' is available for quant_web." >&2
    exit 2
}

query_quant_scalar() {
    run_quant_sql -Atc "$1"
}

query_web_scalar() {
    run_web_sql -Atc "$1"
}

cleanup() {
    if [[ -n "${RUST_QUANT_PID}" ]] && kill -0 "${RUST_QUANT_PID}" >/dev/null 2>&1; then
        kill "${RUST_QUANT_PID}" >/dev/null 2>&1 || true
        wait "${RUST_QUANT_PID}" >/dev/null 2>&1 || true
    fi

    run_quant_sql <<SQL >/dev/null 2>&1 || true
DELETE FROM strategy_configs
WHERE version = '${SMOKE_STRATEGY_VERSION}'
  AND exchange = 'binance'
  AND symbol = '${SMOKE_SYMBOL}'
  AND timeframe = '${SMOKE_PERIOD}';
SQL
}

trap cleanup EXIT

if [[ ! -x "${WEB_SEED_SCRIPT}" ]]; then
    echo "Refusing to run: missing executable Web seed script: ${WEB_SEED_SCRIPT}" >&2
    exit 2
fi

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
fi

export RUSTUP_TOOLCHAIN
export QUANT_CORE_DATABASE_URL
export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export WEB_DATABASE_URL
export RUSTC

cd "${REPO_ROOT}"

echo
echo "Seeding Web demo combo for Binance websocket smoke"
DATABASE_URL="${WEB_DATABASE_URL}" \
TRADE_SIGNAL_SMOKE_STRATEGY_SLUG="${SMOKE_STRATEGY_KEY}" \
EXECUTION_DEMO_STRATEGY_TITLE="Vegas Binance WebSocket Smoke" \
TRADE_SIGNAL_SMOKE_SYMBOL="${SMOKE_SYMBOL}" \
    "${WEB_SEED_SCRIPT}"

BASE_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = '${SMOKE_STRATEGY_KEY}' AND symbol = '${SMOKE_SYMBOL}';")"
BASE_TASK_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM execution_tasks;")"

SMOKE_TABLE="$(printf '%s' "${SMOKE_SYMBOL}" | tr '[:upper:]' '[:lower:]')_candles_$(printf '%s' "${SMOKE_PERIOD}" | tr '[:upper:]' '[:lower:]')"

echo
echo "Preparing temporary runtime strategy config"
run_quant_sql <<SQL >/dev/null
WITH seed AS (
    SELECT config, risk_config
    FROM strategy_configs
    WHERE strategy_key = '${SMOKE_STRATEGY_KEY}'
      AND exchange = 'binance'
      AND symbol = '${SMOKE_SYMBOL}'
      AND timeframe = '4H'
      AND enabled = true
      AND version NOT LIKE 'smoke-binance-websocket-%'
    ORDER BY created_at DESC
    LIMIT 1
)
INSERT INTO strategy_configs (
    strategy_key,
    strategy_name,
    version,
    exchange,
    symbol,
    timeframe,
    enabled,
    config,
    risk_config
)
SELECT
    '${SMOKE_STRATEGY_KEY}',
    '${SMOKE_STRATEGY_NAME}',
    '${SMOKE_STRATEGY_VERSION}',
    'binance',
    '${SMOKE_SYMBOL}',
    '${SMOKE_PERIOD}',
    true,
    jsonb_set(
        jsonb_set(seed.config, '{period}', to_jsonb('${SMOKE_PERIOD}'::text), true),
        '{min_k_line_num}',
        to_jsonb(${SMOKE_MIN_K_LINE_NUM}::int),
        true
    ),
    seed.risk_config
FROM seed
ON CONFLICT (strategy_key, version, exchange, symbol, timeframe)
DO UPDATE SET
    strategy_name = EXCLUDED.strategy_name,
    enabled = EXCLUDED.enabled,
    config = EXCLUDED.config,
    risk_config = EXCLUDED.risk_config,
    updated_at = NOW();
SQL

echo
echo "Removing stale unconfirmed 1m candles before sync"
run_quant_sql <<SQL >/dev/null
DO \$\$
BEGIN
    IF to_regclass('"${SMOKE_TABLE}"') IS NOT NULL THEN
        EXECUTE 'DELETE FROM "${SMOKE_TABLE}" WHERE confirm <> ''1''';
    END IF;
END
\$\$;
SQL

echo
echo "Syncing latest Binance confirmed candles into quant_core"
if command -v rustup >/dev/null 2>&1; then
    run_with_timeout "${SMOKE_SYNC_TIMEOUT_SECS}" env \
        APP_ENV=local \
        CANDLE_SOURCE=quant_core \
        STRATEGY_CONFIG_SOURCE=quant_core \
        DEFAULT_EXCHANGE=binance \
        MARKET_DATA_EXCHANGE=binance \
        IS_RUN_SYNC_DATA_JOB=true \
        IS_RUN_REAL_STRATEGY=false \
        IS_OPEN_SOCKET=false \
        IS_BACK_TEST=false \
        IS_RUN_EXECUTION_WORKER=false \
        EXIT_AFTER_SYNC=true \
        SYNC_SKIP_TICKERS=true \
        SYNC_LATEST_ONLY=true \
        SYNC_ONLY_INST_IDS="${SMOKE_SYMBOL}" \
        SYNC_ONLY_PERIODS="${SMOKE_PERIOD}" \
        rustup run "${RUSTUP_TOOLCHAIN}" cargo run --bin rust_quant
else
    run_with_timeout "${SMOKE_SYNC_TIMEOUT_SECS}" env \
        APP_ENV=local \
        CANDLE_SOURCE=quant_core \
        STRATEGY_CONFIG_SOURCE=quant_core \
        DEFAULT_EXCHANGE=binance \
        MARKET_DATA_EXCHANGE=binance \
        IS_RUN_SYNC_DATA_JOB=true \
        IS_RUN_REAL_STRATEGY=false \
        IS_OPEN_SOCKET=false \
        IS_BACK_TEST=false \
        IS_RUN_EXECUTION_WORKER=false \
        EXIT_AFTER_SYNC=true \
        SYNC_SKIP_TICKERS=true \
        SYNC_LATEST_ONLY=true \
        SYNC_ONLY_INST_IDS="${SMOKE_SYMBOL}" \
        SYNC_ONLY_PERIODS="${SMOKE_PERIOD}" \
        cargo run --bin rust_quant
fi

CONFIRMED_COUNT="$(query_quant_scalar "SELECT COUNT(*) FROM \"${SMOKE_TABLE}\" WHERE confirm = '1';")"
LATEST_CONFIRMED_TS="$(query_quant_scalar "SELECT COALESCE(MAX(ts), 0) FROM \"${SMOKE_TABLE}\" WHERE confirm = '1';")"

if [[ "${CONFIRMED_COUNT}" -lt "${SMOKE_MIN_K_LINE_NUM}" ]]; then
    echo "Refusing to continue: ${SMOKE_TABLE} confirmed candles=${CONFIRMED_COUNT}, expected at least ${SMOKE_MIN_K_LINE_NUM}." >&2
    exit 2
fi

echo
echo "Starting Binance websocket live-strategy smoke"
echo "  symbol: ${SMOKE_SYMBOL}"
echo "  period: ${SMOKE_PERIOD}"
echo "  confirmed_candles: ${CONFIRMED_COUNT}"
echo "  latest_confirmed_ts: ${LATEST_CONFIRMED_TS}"
echo "  baseline_signal_id: ${BASE_SIGNAL_ID}"
echo "  baseline_task_id: ${BASE_TASK_ID}"
echo "  log_file: ${LOG_FILE}"

if command -v rustup >/dev/null 2>&1; then
    env \
        APP_ENV=local \
        CANDLE_SOURCE=quant_core \
        STRATEGY_CONFIG_SOURCE=quant_core \
        DEFAULT_EXCHANGE=binance \
        MARKET_DATA_EXCHANGE=binance \
        IS_RUN_SYNC_DATA_JOB=false \
        IS_RUN_REAL_STRATEGY=true \
        IS_OPEN_SOCKET=true \
        IS_BACK_TEST=false \
        IS_RUN_EXECUTION_WORKER=false \
        STRATEGY_SIGNAL_DISPATCH_MODE=web \
        EXECUTION_WORKER_DRY_RUN=true \
        RUST_QUANT_SMOKE_FORCE_SIGNAL=buy \
        rustup run "${RUSTUP_TOOLCHAIN}" cargo run --bin rust_quant >"${LOG_FILE}" 2>&1 &
else
    env \
        APP_ENV=local \
        CANDLE_SOURCE=quant_core \
        STRATEGY_CONFIG_SOURCE=quant_core \
        DEFAULT_EXCHANGE=binance \
        MARKET_DATA_EXCHANGE=binance \
        IS_RUN_SYNC_DATA_JOB=false \
        IS_RUN_REAL_STRATEGY=true \
        IS_OPEN_SOCKET=true \
        IS_BACK_TEST=false \
        IS_RUN_EXECUTION_WORKER=false \
        STRATEGY_SIGNAL_DISPATCH_MODE=web \
        EXECUTION_WORKER_DRY_RUN=true \
        RUST_QUANT_SMOKE_FORCE_SIGNAL=buy \
        cargo run --bin rust_quant >"${LOG_FILE}" 2>&1 &
fi
RUST_QUANT_PID=$!

NEW_SIGNAL_ID=""
DEADLINE=$((SECONDS + SMOKE_LIVE_TIMEOUT_SECS))
while (( SECONDS < DEADLINE )); do
    if ! kill -0 "${RUST_QUANT_PID}" >/dev/null 2>&1; then
        echo "rust_quant exited before websocket smoke observed a new strategy signal." >&2
        break
    fi

    NEW_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = '${SMOKE_STRATEGY_KEY}' AND symbol = '${SMOKE_SYMBOL}' AND id > ${BASE_SIGNAL_ID};")"
    if [[ -n "${NEW_SIGNAL_ID}" && "${NEW_SIGNAL_ID}" != "0" ]]; then
        break
    fi

    sleep 5
done

if [[ -z "${NEW_SIGNAL_ID}" || "${NEW_SIGNAL_ID}" == "0" ]]; then
    echo
    echo "WebSocket smoke failed to create a fresh rust_quant strategy signal." >&2
    echo "Last rust_quant log lines:" >&2
    tail -n 80 "${LOG_FILE}" >&2 || true
    exit 1
fi

echo
echo "New strategy signal observed: id=${NEW_SIGNAL_ID}"
run_web_sql <<SQL
SELECT id, source, strategy_slug, symbol, generated_at
FROM strategy_signal_inbox
WHERE id = ${NEW_SIGNAL_ID};
SQL

echo
echo "Running execution worker dry-run follow-up"
./scripts/dev/run_execution_worker_dry_run.sh

NEW_TASK_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM execution_tasks WHERE id > ${BASE_TASK_ID};")"
if [[ -z "${NEW_TASK_ID}" || "${NEW_TASK_ID}" == "0" ]]; then
    echo "Expected a new execution task after websocket strategy signal, but none was found." >&2
    exit 1
fi

echo
echo "Latest execution task created by websocket smoke"
run_web_sql <<SQL
SELECT id, strategy_signal_id, combo_id, buyer_email, strategy_slug, symbol, task_status, scheduled_at, updated_at
FROM execution_tasks
WHERE id = ${NEW_TASK_ID};
SQL

echo
echo "WebSocket strategy signal log excerpts"
grep -E "Binance K线确认|等待WebSocket确认K线触发|已提交策略信号到 rust_quan_web" "${LOG_FILE}" | tail -n 20 || true

echo
echo "Binance websocket quant_core smoke complete."
