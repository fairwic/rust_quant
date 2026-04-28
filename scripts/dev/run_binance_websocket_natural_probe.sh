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
: "${SMOKE_PERIOD:="1m"}"
: "${SMOKE_STRATEGY_KEY:="vegas"}"
: "${SMOKE_STRATEGY_VERSION:="smoke-binance-websocket-natural-eth-1m"}"
: "${SMOKE_SOURCE_STRATEGY_VERSION:=""}"
: "${SMOKE_STRATEGY_NAME:="Vegas Binance WebSocket Natural Probe"}"
: "${SMOKE_MIN_K_LINE_NUM:="60"}"
: "${SMOKE_SYNC_TIMEOUT_SECS:="180"}"
: "${SMOKE_LIVE_TIMEOUT_SECS:="150"}"
: "${BINANCE_CONNECTIVITY_PREFLIGHT:="true"}"
: "${BINANCE_CONNECTIVITY_ALLOW_FAILURE:="false"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${QUANT_CORE_POSTGRES_DB:="quant_core"}"
: "${WEB_POSTGRES_DB:="quant_web"}"

WEB_SEED_SCRIPT="${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh"
CONNECTIVITY_SCRIPT="${REPO_ROOT}/scripts/dev/check_binance_connectivity.sh"
LOG_FILE="${TMPDIR:-/tmp}/rust_quant_binance_websocket_natural_probe.$$.log"
RUST_QUANT_PID=""
WEB_READY="false"
CREATED_TEMP_STRATEGY_CONFIG="false"

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

    return 1
}

query_quant_scalar() {
    run_quant_sql -Atc "$1"
}

query_web_scalar() {
    run_web_sql -Atc "$1"
}

normalize_probe_slug() {
    printf '%s' "$1" \
        | tr '[:upper:]' '[:lower:]' \
        | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//'
}

normalize_table_suffix() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

derive_runtime_strategy_version() {
    local symbol_slug
    local period_slug
    symbol_slug="$(normalize_probe_slug "$1")"
    period_slug="$(normalize_probe_slug "$2")"
    printf 'smoke-binance-websocket-natural-%s-%s' "${symbol_slug}" "${period_slug}"
}

if [[ -z "${SMOKE_SOURCE_STRATEGY_VERSION}" ]]; then
    SMOKE_SOURCE_STRATEGY_VERSION="${SMOKE_STRATEGY_VERSION}"
fi

cleanup() {
    if [[ -n "${RUST_QUANT_PID}" ]] && kill -0 "${RUST_QUANT_PID}" >/dev/null 2>&1; then
        kill "${RUST_QUANT_PID}" >/dev/null 2>&1 || true
        wait "${RUST_QUANT_PID}" >/dev/null 2>&1 || true
    fi

    if [[ "${CREATED_TEMP_STRATEGY_CONFIG}" == "true" ]]; then
        run_quant_sql <<SQL >/dev/null 2>&1 || true
DELETE FROM strategy_configs
WHERE version = '${SMOKE_STRATEGY_VERSION}'
  AND exchange = 'binance'
  AND symbol = '${SMOKE_SYMBOL}'
  AND timeframe = '${SMOKE_PERIOD}';
SQL
    fi
}

trap cleanup EXIT

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

if [[ "${BINANCE_CONNECTIVITY_PREFLIGHT}" == "true" ]]; then
    echo
    echo "Running Binance connectivity preflight"
    echo "  script: ${CONNECTIVITY_SCRIPT}"
    if ! "${CONNECTIVITY_SCRIPT}"; then
        if [[ "${BINANCE_CONNECTIVITY_ALLOW_FAILURE}" == "true" ]]; then
            echo "Binance connectivity preflight failed, but continuing because BINANCE_CONNECTIVITY_ALLOW_FAILURE=true." >&2
        else
            echo "Binance connectivity preflight failed. Refusing natural probe blind run." >&2
            exit 2
        fi
    fi
else
    echo
    echo "Skipping Binance connectivity preflight"
fi

if [[ -x "${WEB_SEED_SCRIPT}" ]] && run_web_sql -Atc "SELECT 1" >/dev/null 2>&1; then
    WEB_READY="true"
    echo
    echo "Seeding Web demo combo for natural websocket probe"
    DATABASE_URL="${WEB_DATABASE_URL}" \
    TRADE_SIGNAL_SMOKE_STRATEGY_SLUG="${SMOKE_STRATEGY_KEY}" \
    EXECUTION_DEMO_STRATEGY_TITLE="Vegas Binance WebSocket Natural Probe" \
    TRADE_SIGNAL_SMOKE_SYMBOL="${SMOKE_SYMBOL}" \
        "${WEB_SEED_SCRIPT}"
else
    echo
    echo "Web backend seed skipped: missing script or quant_web DB access."
fi

BASE_SIGNAL_ID="0"
BASE_TASK_ID="0"
if [[ "${WEB_READY}" == "true" ]]; then
    BASE_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = '${SMOKE_STRATEGY_KEY}' AND symbol = '${SMOKE_SYMBOL}';")"
    BASE_TASK_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM execution_tasks;")"
fi

SMOKE_TABLE="$(printf '%s' "${SMOKE_SYMBOL}" | tr '[:upper:]' '[:lower:]')_candles_$(normalize_table_suffix "${SMOKE_PERIOD}")"

echo
EXISTING_STRATEGY_CONFIG_COUNT="$(query_quant_scalar "SELECT COUNT(*) FROM strategy_configs WHERE strategy_key = '${SMOKE_STRATEGY_KEY}' AND version = '${SMOKE_STRATEGY_VERSION}' AND exchange = 'binance' AND symbol = '${SMOKE_SYMBOL}' AND timeframe = '${SMOKE_PERIOD}' AND enabled = true;")"
if [[ "${EXISTING_STRATEGY_CONFIG_COUNT}" != "0" ]]; then
    echo "Using existing runtime strategy config"
    echo "  strategy_key: ${SMOKE_STRATEGY_KEY}"
    echo "  version: ${SMOKE_STRATEGY_VERSION}"
    echo "  source_version: ${SMOKE_SOURCE_STRATEGY_VERSION}"
else
    echo "Preparing temporary runtime strategy config"
    run_quant_sql <<SQL >/dev/null
WITH preferred_seed AS (
    SELECT config, risk_config
    FROM strategy_configs
    WHERE strategy_key = '${SMOKE_STRATEGY_KEY}'
      AND exchange = 'binance'
      AND symbol = '${SMOKE_SYMBOL}'
      AND enabled = true
      AND '${SMOKE_SOURCE_STRATEGY_VERSION}' <> ''
      AND version = '${SMOKE_SOURCE_STRATEGY_VERSION}'
    ORDER BY
      CASE WHEN timeframe = '${SMOKE_PERIOD}' THEN 0 ELSE 1 END,
      created_at DESC
    LIMIT 1
),
fallback_seed AS (
    SELECT config, risk_config
    FROM strategy_configs
    WHERE strategy_key = '${SMOKE_STRATEGY_KEY}'
      AND exchange = 'binance'
      AND symbol = '${SMOKE_SYMBOL}'
      AND enabled = true
      AND version NOT LIKE 'smoke-binance-websocket-natural-%'
    ORDER BY
      CASE
        WHEN timeframe = '${SMOKE_PERIOD}' THEN 0
        WHEN timeframe = '4H' THEN 1
        ELSE 2
      END,
      created_at DESC
    LIMIT 1
),
seed AS (
    SELECT config, risk_config FROM preferred_seed
    UNION ALL
    SELECT config, risk_config FROM fallback_seed
    WHERE NOT EXISTS (SELECT 1 FROM preferred_seed)
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
    CREATED_TEMP_STRATEGY_CONFIG="$(
        query_quant_scalar "SELECT CASE WHEN COUNT(*) > 0 THEN 'true' ELSE 'false' END FROM strategy_configs WHERE strategy_key = '${SMOKE_STRATEGY_KEY}' AND version = '${SMOKE_STRATEGY_VERSION}' AND exchange = 'binance' AND symbol = '${SMOKE_SYMBOL}' AND timeframe = '${SMOKE_PERIOD}';"
    )"
    if [[ "${CREATED_TEMP_STRATEGY_CONFIG}" != "true" ]]; then
        echo "Refusing to continue: no existing strategy config and temporary config could not be created." >&2
        exit 2
    fi
    echo "  runtime_version: ${SMOKE_STRATEGY_VERSION}"
    echo "  source_version: ${SMOKE_SOURCE_STRATEGY_VERSION}"
fi

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
BASE_CONFIRMED_TS="$(query_quant_scalar "SELECT COALESCE(MAX(ts), 0) FROM \"${SMOKE_TABLE}\" WHERE confirm = '1';")"

if [[ "${CONFIRMED_COUNT}" -lt "${SMOKE_MIN_K_LINE_NUM}" ]]; then
    echo "Refusing to continue: ${SMOKE_TABLE} confirmed candles=${CONFIRMED_COUNT}, expected at least ${SMOKE_MIN_K_LINE_NUM}." >&2
    exit 2
fi

echo
echo "Starting Binance websocket natural probe"
echo "  symbol: ${SMOKE_SYMBOL}"
echo "  period: ${SMOKE_PERIOD}"
echo "  confirmed_candles: ${CONFIRMED_COUNT}"
echo "  baseline_confirmed_ts: ${BASE_CONFIRMED_TS}"
echo "  baseline_signal_id: ${BASE_SIGNAL_ID}"
echo "  baseline_task_id: ${BASE_TASK_ID}"
echo "  web_ready: ${WEB_READY}"
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
        LIVE_STRATEGY_ONLY_INST_IDS="${SMOKE_SYMBOL}" \
        LIVE_STRATEGY_ONLY_PERIODS="${SMOKE_PERIOD}" \
        STRATEGY_SIGNAL_DISPATCH_MODE=web \
        EXECUTION_WORKER_DRY_RUN=true \
        RUST_LOG="${RUST_LOG:-info}" \
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
        LIVE_STRATEGY_ONLY_INST_IDS="${SMOKE_SYMBOL}" \
        LIVE_STRATEGY_ONLY_PERIODS="${SMOKE_PERIOD}" \
        STRATEGY_SIGNAL_DISPATCH_MODE=web \
        EXECUTION_WORKER_DRY_RUN=true \
        RUST_LOG="${RUST_LOG:-info}" \
        cargo run --bin rust_quant >"${LOG_FILE}" 2>&1 &
fi
RUST_QUANT_PID=$!

WS_CONNECTED="false"
CONFIRMED_TRIGGERED="false"
HANDLER_STARTED="false"
STRATEGY_EXECUTED="false"
SIGNAL_DISPATCHED="false"
NEW_SIGNAL_ID="0"
NEW_TASK_ID="0"

DEADLINE=$((SECONDS + SMOKE_LIVE_TIMEOUT_SECS))
while (( SECONDS < DEADLINE )); do
    if ! kill -0 "${RUST_QUANT_PID}" >/dev/null 2>&1; then
        echo "rust_quant exited before natural websocket probe reached a terminal observation." >&2
        break
    fi

    if grep -q "Binance public websocket启动成功" "${LOG_FILE}" 2>/dev/null; then
        WS_CONNECTED="true"
    fi
    if grep -q "Binance K线确认，触发策略执行" "${LOG_FILE}" 2>/dev/null; then
        CONFIRMED_TRIGGERED="true"
    fi
    if grep -q "K线确认触发策略检查" "${LOG_FILE}" 2>/dev/null; then
        HANDLER_STARTED="true"
    fi
    if grep -q "策略执行完成" "${LOG_FILE}" 2>/dev/null; then
        STRATEGY_EXECUTED="true"
    fi
    if grep -q "已提交策略信号到 rust_quan_web" "${LOG_FILE}" 2>/dev/null; then
        SIGNAL_DISPATCHED="true"
    fi

    if [[ "${WEB_READY}" == "true" ]]; then
        NEW_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = '${SMOKE_STRATEGY_KEY}' AND symbol = '${SMOKE_SYMBOL}' AND id > ${BASE_SIGNAL_ID};")"
        if [[ -n "${NEW_SIGNAL_ID}" && "${NEW_SIGNAL_ID}" != "0" ]]; then
            SIGNAL_DISPATCHED="true"
            NEW_TASK_ID="$(query_web_scalar "SELECT COALESCE(MAX(et.id), 0) FROM execution_tasks et JOIN strategy_signal_inbox ssi ON ssi.id = et.strategy_signal_id WHERE ssi.id = ${NEW_SIGNAL_ID};")"
        fi
    fi

    if [[ "${SIGNAL_DISPATCHED}" == "true" ]]; then
        break
    fi

    sleep 5
done

POST_CONFIRMED_TS="$(query_quant_scalar "SELECT COALESCE(MAX(ts), 0) FROM \"${SMOKE_TABLE}\" WHERE confirm = '1';")"

echo
echo "Natural websocket probe summary"
echo "  websocket_connected=${WS_CONNECTED}"
echo "  confirmed_kline_triggered=${CONFIRMED_TRIGGERED}"
echo "  handler_started=${HANDLER_STARTED}"
echo "  strategy_executed=${STRATEGY_EXECUTED}"
echo "  signal_dispatched=${SIGNAL_DISPATCHED}"
echo "  baseline_confirmed_ts=${BASE_CONFIRMED_TS}"
echo "  latest_confirmed_ts=${POST_CONFIRMED_TS}"
echo "  new_signal_id=${NEW_SIGNAL_ID}"
echo "  new_task_id=${NEW_TASK_ID}"

echo
echo "Relevant rust_quant log excerpts"
grep -E "Binance public websocket启动成功|Binance K线确认|K线确认触发策略检查|找到 .* 个策略配置|策略执行完成|已提交策略信号到 rust_quan_web|策略信号！" "${LOG_FILE}" | tail -n 40 || true

if [[ "${WEB_READY}" == "true" && "${NEW_SIGNAL_ID}" != "0" ]]; then
    echo
    echo "New natural strategy signal"
    run_web_sql <<SQL
SELECT id, source, strategy_slug, symbol, generated_at
FROM strategy_signal_inbox
WHERE id = ${NEW_SIGNAL_ID};
SQL
fi

if [[ "${WEB_READY}" == "true" && "${NEW_TASK_ID}" != "0" ]]; then
    echo
    echo "New execution task created from natural strategy signal"
    run_web_sql <<SQL
SELECT id, strategy_signal_id, combo_id, buyer_email, strategy_slug, symbol, task_status, scheduled_at, updated_at
FROM execution_tasks
WHERE id = ${NEW_TASK_ID};
SQL
fi

if [[ "${SIGNAL_DISPATCHED}" == "true" ]]; then
    echo
    echo "Natural websocket probe observed a full websocket -> strategy -> rust_quan_web signal path."
    exit 0
fi

if [[ "${STRATEGY_EXECUTED}" == "true" || "${HANDLER_STARTED}" == "true" || "${CONFIRMED_TRIGGERED}" == "true" ]]; then
    echo
    echo "Natural websocket probe reached strategy execution, but no natural signal was emitted within ${SMOKE_LIVE_TIMEOUT_SECS}s."
    echo "This usually means the observed confirmed Binance candle did not satisfy the strategy entry conditions."
    exit 1
fi

if [[ "${WS_CONNECTED}" == "true" ]]; then
    echo
    echo "Natural websocket probe connected to Binance, but did not observe a confirmed ${SMOKE_PERIOD} candle before timeout."
    exit 1
fi

echo
echo "Natural websocket probe failed before Binance websocket produced usable runtime evidence."
exit 1
