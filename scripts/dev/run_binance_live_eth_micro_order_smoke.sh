#!/usr/bin/env bash
# run_binance_live_eth_micro_order_smoke.sh
#
# Binance ETH 极小实盘 open/close smoke。
#
# 安全边界：
#   - 只允许 ETHUSDT / ETH-USDT-SWAP，拒绝任何非 ETH symbol。
#   - 需要脚本级确认 token: I_UNDERSTAND_TINY_ETH_LIVE_ORDER。
#   - 先做 Binance signed account preflight 和 ETHUSDT exchangeInfo filters 检查。
#   - 不打印 API key、secret、密文或原始数据库 URL。
#   - 不直接调用 Binance 下单接口；实盘下单必须经 Web execution_tasks -> worker -> exchange -> Web 回写。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${BINANCE_ETH_MICRO_SYMBOL:="ETH-USDT-SWAP"}"
: "${BINANCE_ETH_MICRO_QTY:="0.001"}"
: "${BINANCE_PROXY_URL:="socks5h://127.0.0.1:7897"}"
: "${API_CREDENTIAL_SECRET:="alpha-pulse-local-credential-key"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${BINANCE_ETH_MICRO_BUYER_EMAIL:="demo-exec-worker@example.com"}"
: "${BINANCE_ETH_MICRO_STRATEGY_SLUG:="vegas"}"
: "${BINANCE_ETH_MICRO_STRATEGY_KEY:="vegas_eth_micro_live_smoke"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${WEB_POSTGRES_DB:="quant_web"}"
: "${EXECUTION_WORKER_USE_EXISTING_BINARY:="auto"}"
: "${RUSTUP_TOOLCHAIN:="1.91.1"}"
: "${BINANCE_SIGNED_PREFLIGHT_RETRIES:="3"}"

EXPECTED_CONFIRMATION_TOKEN="I_UNDERSTAND_TINY_ETH_LIVE_ORDER"
ETH_NATIVE_SYMBOL="ETHUSDT"
ETH_WEB_SYMBOL="ETH-USDT-SWAP"

case "${BINANCE_ETH_MICRO_SYMBOL}" in
    ETHUSDT|ETH-USDT-SWAP)
        ;;
    *)
        echo "ERROR: Refusing non-ETH symbol: ${BINANCE_ETH_MICRO_SYMBOL}" >&2
        echo "Only ETHUSDT / ETH-USDT-SWAP are allowed for this micro smoke." >&2
        exit 1
        ;;
esac

BINANCE_ETH_MICRO_NATIVE_SYMBOL="${ETH_NATIVE_SYMBOL}"
BINANCE_ETH_MICRO_WEB_SYMBOL="${ETH_WEB_SYMBOL}"

if [[ -z "${BINANCE_LIVE_API_KEY:-}" && -n "${BINANCE_API_KEY:-}" ]]; then
    BINANCE_LIVE_API_KEY="${BINANCE_API_KEY}"
fi
if [[ -z "${BINANCE_LIVE_API_SECRET:-}" && -n "${BINANCE_API_SECRET:-}" ]]; then
    BINANCE_LIVE_API_SECRET="${BINANCE_API_SECRET}"
fi
if [[ -z "${BINANCE_LIVE_API_SECRET:-}" && -n "${binance_api_secret:-}" ]]; then
    BINANCE_LIVE_API_SECRET="${binance_api_secret}"
fi

missing_vars=()
if [[ -z "${BINANCE_LIVE_API_KEY:-}" ]]; then
    missing_vars+=("BINANCE_LIVE_API_KEY")
fi
if [[ -z "${BINANCE_LIVE_API_SECRET:-}" ]]; then
    missing_vars+=("BINANCE_LIVE_API_SECRET")
fi
if [[ -z "${BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM:-}" ]]; then
    missing_vars+=("BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM")
fi

if [[ -n "${WEB_DATABASE_URL:-}" ]]; then
    _DB_URL="${WEB_DATABASE_URL}"
elif [[ -n "${DATABASE_URL:-}" ]]; then
    _DB_URL="${DATABASE_URL}"
else
    missing_vars+=("WEB_DATABASE_URL (or DATABASE_URL)")
fi

if [[ ${#missing_vars[@]} -gt 0 ]]; then
    echo "ERROR: missing required env vars:" >&2
    for var in "${missing_vars[@]}"; do
        echo "  - ${var}" >&2
    done
    echo >&2
    echo "Required live confirmation:" >&2
    echo "  export BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM=${EXPECTED_CONFIRMATION_TOKEN}" >&2
    exit 1
fi

if [[ "${BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM}" != "${EXPECTED_CONFIRMATION_TOKEN}" ]]; then
    echo "ERROR: refusing live ETH micro smoke without exact confirmation token." >&2
    echo "Expected BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM=${EXPECTED_CONFIRMATION_TOKEN}" >&2
    exit 1
fi

if [[ ! "${BINANCE_ETH_MICRO_QTY}" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
    echo "ERROR: BINANCE_ETH_MICRO_QTY must be a positive decimal quantity." >&2
    exit 1
fi

if [[ -n "${BINANCE_ETH_MICRO_COMBO_ID:-}" && ! "${BINANCE_ETH_MICRO_COMBO_ID}" =~ ^[0-9]+$ ]]; then
    echo "ERROR: BINANCE_ETH_MICRO_COMBO_ID must be numeric when set." >&2
    exit 1
fi

WEB_DATABASE_URL="${_DB_URL}"

echo
echo "=== Binance ETH micro live smoke safety summary ==="
echo "  BINANCE_LIVE_API_KEY=<set>"
echo "  BINANCE_LIVE_API_SECRET=<set>"
echo "  WEB_DATABASE_URL=<set>"
echo "  native_symbol=${BINANCE_ETH_MICRO_NATIVE_SYMBOL}"
echo "  web_symbol=${BINANCE_ETH_MICRO_WEB_SYMBOL}"
echo "  quantity=${BINANCE_ETH_MICRO_QTY}"
echo "  buyer_email=${BINANCE_ETH_MICRO_BUYER_EMAIL}"
echo "  strategy_slug=${BINANCE_ETH_MICRO_STRATEGY_SLUG}"
echo "  proxy_configured=$(if [[ -n "${BINANCE_PROXY_URL:-}" ]]; then echo yes; else echo no; fi)"
echo "  live_confirmation_token=<accepted>"

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

    echo "ERROR: psql not found and podman container '${POSTGRES_CONTAINER}' is unavailable." >&2
    exit 1
}

query_web_scalar() {
    run_web_sql -Atc "$1"
}

curl_binance() {
    local output_file="$1"
    local url="$2"
    local -a curl_args=(-sS -m 15 -o "${output_file}" -w '%{http_code}')
    if [[ -n "${BINANCE_PROXY_URL:-}" ]]; then
        curl_args+=(--proxy "${BINANCE_PROXY_URL}")
    fi
    curl "${curl_args[@]}" "${url}" || true
}

require_binance_signed_tools() {
    for cmd in curl openssl xxd python3; do
        if ! command -v "${cmd}" >/dev/null 2>&1; then
            echo "ERROR: missing required command for Binance signed preflight: ${cmd}" >&2
            exit 1
        fi
    done
}

fetch_binance_signed_body() {
    local endpoint_path="$1"
    local context="$2"
    require_binance_signed_tools

    if [[ ! "${BINANCE_SIGNED_PREFLIGHT_RETRIES}" =~ ^[0-9]+$ ]] ||
       (( BINANCE_SIGNED_PREFLIGHT_RETRIES < 1 )); then
        echo "ERROR: BINANCE_SIGNED_PREFLIGHT_RETRIES must be a positive integer." >&2
        exit 1
    fi

    local attempt
    for (( attempt = 1; attempt <= BINANCE_SIGNED_PREFLIGHT_RETRIES; attempt++ )); do
        local query
        query="timestamp=$(($(date +%s) * 1000))&recvWindow=5000"
        local signature
        signature="$(printf '%s' "${query}" | openssl dgst -sha256 -hmac "${BINANCE_LIVE_API_SECRET}" -binary | xxd -p -c 256)"
        local body_file
        body_file="$(mktemp)"
        local -a curl_args=(
            -sS
            -m 15
            -o "${body_file}"
            -w '%{http_code}'
            -H "X-MBX-APIKEY: ${BINANCE_LIVE_API_KEY}"
        )
        if [[ -n "${BINANCE_PROXY_URL:-}" ]]; then
            curl_args+=(--proxy "${BINANCE_PROXY_URL}")
        fi

        local http_status
        http_status="$(curl "${curl_args[@]}" "https://fapi.binance.com${endpoint_path}?${query}&signature=${signature}" || true)"
        if [[ "${http_status}" == "200" ]]; then
            printf '%s\n' "${body_file}"
            return
        fi

        rm -f "${body_file}"
        if (( attempt < BINANCE_SIGNED_PREFLIGHT_RETRIES )); then
            echo "WARN: Binance signed ${context} preflight attempt ${attempt} failed; retrying." >&2
            sleep 1
        else
            echo "ERROR: Binance signed ${context} preflight failed before Web state changes." >&2
            echo "  http_status=${http_status}" >&2
            echo "  Check futures permission, API/IP whitelist, account mode, and local proxy connectivity." >&2
            exit 1
        fi
    done
}

fetch_binance_signed_account_body() {
    fetch_binance_signed_body "/fapi/v2/account" "account"
}

fetch_binance_position_mode_body() {
    fetch_binance_signed_body "/fapi/v1/positionSide/dual" "position-mode"
}

assert_eth_position_flat() {
    local body_file="$1"
    local context="$2"

    python3 - "${body_file}" "${context}" <<'PY'
import json
import sys
from decimal import Decimal, InvalidOperation

body_path, context = sys.argv[1], sys.argv[2]
with open(body_path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

positions = data.get("positions") or []
eth_position = next((item for item in positions if item.get("symbol") == "ETHUSDT"), None)
if eth_position is None:
    raise SystemExit(f"ERROR: ETHUSDT position not found during {context}")
try:
    position_amt = Decimal(str(eth_position.get("positionAmt", "0")))
except InvalidOperation as exc:
    raise SystemExit(f"ERROR: invalid ETHUSDT positionAmt during {context}") from exc
if position_amt != 0:
    raise SystemExit(
        f"ERROR: ETHUSDT position must be flat during {context}; positionAmt={position_amt}"
    )
PY
}

preflight_binance_signed_account() {
    local body_file
    body_file="$(fetch_binance_signed_account_body)"
    if ! python3 - "${body_file}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

if data.get("canTrade") is False:
    raise SystemExit("ERROR: Binance account preflight returned canTrade=false")
PY
    then
        rm -f "${body_file}"
        exit 1
    fi
    if ! assert_eth_position_flat "${body_file}" "preflight_eth_position_flat"; then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    echo "  signed_account_preflight=ok"
    echo "  preflight_eth_position_flat=ok"
}

preflight_binance_position_mode() {
    local body_file
    body_file="$(fetch_binance_position_mode_body)"
    if ! python3 - "${body_file}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

mode = data.get("dualSidePosition")
if mode is False or str(mode).lower() == "false":
    raise SystemExit(0)
if mode is True or str(mode).lower() == "true":
    raise SystemExit(
        "ERROR: refusing Binance Hedge Mode for this ETH reduce-only smoke; "
        "reduceOnly close orders require one-way mode in this script"
    )
raise SystemExit("ERROR: Binance position mode response missing dualSidePosition")
PY
    then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    echo "  binance_position_mode=one_way"
}

verify_final_eth_position_flat() {
    local body_file
    body_file="$(fetch_binance_signed_account_body)"
    if ! assert_eth_position_flat "${body_file}" "final_eth_position_flat"; then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    echo "  final_eth_position=flat"
}

preflight_binance_exchange_info_filters() {
    local exchange_info_file
    local premium_file
    exchange_info_file="$(mktemp)"
    premium_file="$(mktemp)"

    local exchange_status
    exchange_status="$(curl_binance "${exchange_info_file}" "https://fapi.binance.com/fapi/v1/exchangeInfo?symbol=ETHUSDT")"
    if [[ "${exchange_status}" != "200" ]]; then
        rm -f "${exchange_info_file}" "${premium_file}"
        echo "ERROR: Binance exchangeInfo preflight failed for ETHUSDT: http_status=${exchange_status}" >&2
        exit 1
    fi

    local premium_status
    premium_status="$(curl_binance "${premium_file}" "https://fapi.binance.com/fapi/v1/premiumIndex?symbol=ETHUSDT")"
    if [[ "${premium_status}" != "200" ]]; then
        rm -f "${exchange_info_file}" "${premium_file}"
        echo "ERROR: Binance mark price preflight failed for ETHUSDT: http_status=${premium_status}" >&2
        exit 1
    fi

    local filter_summary
    filter_summary="$(
        python3 - "${exchange_info_file}" "${premium_file}" "${BINANCE_ETH_MICRO_QTY}" <<'PY'
import json
import sys
from decimal import Decimal, InvalidOperation

exchange_path, premium_path, qty_raw = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    qty = Decimal(qty_raw)
except InvalidOperation as exc:
    raise SystemExit(f"ERROR: invalid ETH quantity: {qty_raw}") from exc
if qty <= 0:
    raise SystemExit("ERROR: ETH quantity must be positive")

with open(exchange_path, "r", encoding="utf-8") as fh:
    exchange_info = json.load(fh)
with open(premium_path, "r", encoding="utf-8") as fh:
    premium = json.load(fh)

symbols = exchange_info.get("symbols") or []
symbol = next((item for item in symbols if item.get("symbol") == "ETHUSDT"), None)
if not symbol:
    raise SystemExit("ERROR: ETHUSDT not found in exchangeInfo")
if symbol.get("status") != "TRADING":
    raise SystemExit(f"ERROR: ETHUSDT status is {symbol.get('status')}")

filters = {item.get("filterType"): item for item in symbol.get("filters", [])}
lot = filters.get("MARKET_LOT_SIZE") or filters.get("LOT_SIZE") or {}
min_qty = Decimal(lot.get("minQty", "0"))
max_qty = Decimal(lot.get("maxQty", "0"))
step_size = Decimal(lot.get("stepSize", "0"))

if qty < min_qty:
    raise SystemExit(f"ERROR: quantity {qty} is below minQty {min_qty}")
if max_qty > 0 and qty > max_qty:
    raise SystemExit(f"ERROR: quantity {qty} is above maxQty {max_qty}")
if step_size > 0 and qty % step_size != 0:
    raise SystemExit(f"ERROR: quantity {qty} is not aligned to stepSize {step_size}")

notional_filter = filters.get("MIN_NOTIONAL") or {}
min_notional = Decimal(str(notional_filter.get("notional", notional_filter.get("minNotional", "0"))))
mark_price = Decimal(str(premium.get("markPrice", "0")))
if mark_price <= 0:
    raise SystemExit("ERROR: markPrice is not positive")
notional = qty * mark_price
if min_notional > 0 and notional < min_notional:
    raise SystemExit(
        f"ERROR: quantity {qty} notional {notional} is below minNotional {min_notional}"
    )

print(f"minQty={min_qty} stepSize={step_size} minNotional={min_notional} markPrice={mark_price} notional={notional}")
PY
    )"

    rm -f "${exchange_info_file}" "${premium_file}"
    echo "  exchange_info_filters=ok ${filter_summary}"
}

seal_credential() {
    local raw="$1"
    local key="$2"
    local key_len="${#key}"
    local result=""
    local i
    for (( i = 0; i < ${#raw}; i++ )); do
        local raw_byte
        raw_byte=$(printf '%d' "'${raw:$i:1}")
        local key_byte
        key_byte=$(printf '%d' "'${key:$(( i % key_len )):1}")
        result+=$(printf '%02x' $(( raw_byte ^ key_byte )))
    done
    printf '%s' "${result}"
}

upsert_binance_credentials() {
    local api_key_cipher
    local api_secret_cipher
    api_key_cipher="$(seal_credential "${BINANCE_LIVE_API_KEY}" "${API_CREDENTIAL_SECRET}")"
    api_secret_cipher="$(seal_credential "${BINANCE_LIVE_API_SECRET}" "${API_CREDENTIAL_SECRET}")"

    local key_len
    local api_key_mask
    key_len="${#BINANCE_LIVE_API_KEY}"
    if (( key_len <= 4 )); then
        api_key_mask="$(printf '%*s' "${key_len}" '' | tr ' ' '*')"
    elif (( key_len < 8 )); then
        api_key_mask="${BINANCE_LIVE_API_KEY:0:2}****${BINANCE_LIVE_API_KEY: -2}"
    else
        api_key_mask="${BINANCE_LIVE_API_KEY:0:4}****${BINANCE_LIVE_API_KEY: -4}"
    fi

    run_web_sql \
        -v buyer_email="${BINANCE_ETH_MICRO_BUYER_EMAIL}" \
        -v api_key_cipher="${api_key_cipher}" \
        -v api_secret_cipher="${api_secret_cipher}" \
        -v api_key_mask="${api_key_mask}" <<'SQL'
INSERT INTO user_api_credentials (
    buyer_email,
    exchange,
    api_key_cipher,
    api_secret_cipher,
    passphrase_cipher,
    api_key_mask,
    permission_scope,
    status,
    last_check_at,
    last_check_code,
    last_check_message,
    created_at,
    updated_at
)
VALUES (
    :'buyer_email',
    '币安',
    :'api_key_cipher',
    :'api_secret_cipher',
    NULL,
    :'api_key_mask',
    '只读 + 下单',
    'active',
    NOW(),
    'binance_eth_micro_live_smoke',
    'Binance ETH micro live smoke credential.',
    NOW(),
    NOW()
)
ON CONFLICT (buyer_email, exchange) DO UPDATE
SET api_key_cipher     = EXCLUDED.api_key_cipher,
    api_secret_cipher  = EXCLUDED.api_secret_cipher,
    passphrase_cipher  = NULL,
    api_key_mask       = EXCLUDED.api_key_mask,
    permission_scope   = '只读 + 下单',
    status             = 'active',
    last_check_at      = NOW(),
    last_check_code    = 'binance_eth_micro_live_smoke',
    last_check_message = EXCLUDED.last_check_message,
    updated_at         = NOW();
SQL
    echo "  web_credentials=upserted"
}

resolve_combo_id() {
    if [[ -n "${BINANCE_ETH_MICRO_COMBO_ID:-}" ]]; then
        local row
        row="$(
            run_web_sql \
                -v combo_id="${BINANCE_ETH_MICRO_COMBO_ID}" \
                -v buyer_email="${BINANCE_ETH_MICRO_BUYER_EMAIL}" \
                -v strategy_slug="${BINANCE_ETH_MICRO_STRATEGY_SLUG}" \
                -v symbol="${BINANCE_ETH_MICRO_WEB_SYMBOL}" \
                -At <<'SQL'
SELECT id
FROM strategy_combo_subscriptions
WHERE id = :combo_id
  AND buyer_email = :'buyer_email'
  AND strategy_slug = :'strategy_slug'
  AND symbol = :'symbol'
LIMIT 1;
SQL
        )"
        if [[ -z "${row}" ]]; then
            echo "ERROR: BINANCE_ETH_MICRO_COMBO_ID does not match buyer/strategy/ETH symbol." >&2
            exit 1
        fi
        echo "${row}"
        return
    fi

    local rows
    rows="$(
        run_web_sql \
            -v buyer_email="${BINANCE_ETH_MICRO_BUYER_EMAIL}" \
            -v strategy_slug="${BINANCE_ETH_MICRO_STRATEGY_SLUG}" \
            -v symbol="${BINANCE_ETH_MICRO_WEB_SYMBOL}" \
            -At <<'SQL'
SELECT id
FROM strategy_combo_subscriptions
WHERE buyer_email = :'buyer_email'
  AND strategy_slug = :'strategy_slug'
  AND symbol = :'symbol'
  AND status = 'active'
ORDER BY id ASC
LIMIT 2;
SQL
    )"

    local count
    count="$(printf '%s\n' "${rows}" | sed '/^$/d' | wc -l | tr -d ' ')"
    if [[ "${count}" != "1" ]]; then
        echo "ERROR: expected exactly one active ETH combo subscription; found ${count}." >&2
        echo "Set BINANCE_ETH_MICRO_COMBO_ID after verifying the target buyer/strategy/symbol." >&2
        exit 1
    fi

    printf '%s\n' "${rows}" | sed -n '1p'
}

assert_no_other_pending_tasks() {
    local task_type="$1"
    local task_status="$2"
    local target_task_id="$3"
    # Lease guard contract: task_type = $1, task_status = $2, id <> $3.
    local other_count
    other_count="$(
        run_web_sql \
            -v task_type="${task_type}" \
            -v task_status="${task_status}" \
            -v target_task_id="${target_task_id}" \
            -At <<'SQL'
SELECT COUNT(*)
FROM execution_tasks
WHERE task_type = :'task_type'
  AND task_status = :'task_status'
  AND id <> :target_task_id;
SQL
    )"
    if [[ ! "${other_count}" =~ ^[0-9]+$ ]]; then
        echo "ERROR: pending task isolation query returned invalid count: ${other_count}" >&2
        exit 1
    fi
    if (( other_count > 0 )); then
        echo "ERROR: refusing live worker run because another matching task can be leased." >&2
        echo "  task_type=${task_type} task_status=${task_status} other_count=${other_count}" >&2
        exit 1
    fi
}

create_open_execution_task() {
    local combo_id="$1"
    local external_id
    local client_order_id
    external_id="binance-eth-micro-open-$(date +%s)-$$"
    client_order_id="rqethopen$(date +%s)$$"

    # Contract JSON keys: "trade_side", "open".
    run_web_sql \
        -v external_id="${external_id}" \
        -v strategy_slug="${BINANCE_ETH_MICRO_STRATEGY_SLUG}" \
        -v strategy_key="${BINANCE_ETH_MICRO_STRATEGY_KEY}" \
        -v symbol="${BINANCE_ETH_MICRO_WEB_SYMBOL}" \
        -v qty="${BINANCE_ETH_MICRO_QTY}" \
        -v combo_id="${combo_id}" \
        -v buyer_email="${BINANCE_ETH_MICRO_BUYER_EMAIL}" \
        -v client_order_id="${client_order_id}" \
        -At <<'SQL'
WITH signal AS (
    INSERT INTO strategy_signal_inbox (
        source,
        external_id,
        strategy_slug,
        strategy_key,
        symbol,
        signal_type,
        direction,
        title,
        summary,
        confidence,
        payload_json,
        generated_at,
        created_at,
        updated_at
    )
    VALUES (
        'rust_quant_eth_micro_live_smoke',
        :'external_id',
        :'strategy_slug',
        :'strategy_key',
        :'symbol',
        'entry',
        'long',
        'Binance ETH micro live open smoke',
        'Tiny ETH open order created by guarded live smoke script.',
        1.0,
        jsonb_build_object(
            'exchange', 'binance',
            'symbol', :'symbol',
            'side', 'buy',
            'order_type', 'market',
            'size', :'qty',
            'quantity', :'qty',
            'margin_mode', 'cross',
            'margin_coin', 'USDT',
            'trade_side', 'open',
            'client_order_id', :'client_order_id',
            'reduce_only', false
        )::text,
        NOW(),
        NOW(),
        NOW()
    )
    RETURNING id
), task AS (
    INSERT INTO execution_tasks (
        strategy_signal_id,
        combo_id,
        buyer_email,
        strategy_slug,
        symbol,
        task_type,
        task_status,
        priority,
        lease_owner,
        lease_until,
        scheduled_at,
        request_payload_json,
        created_at,
        updated_at
    )
    SELECT
        signal.id,
        :combo_id,
        :'buyer_email',
        :'strategy_slug',
        :'symbol',
        'execute_signal', 'pending',
        100,
        NULL,
        NULL,
        NOW(),
        jsonb_build_object(
            'source', 'rust_quant_eth_micro_live_smoke',
            'symbol', :'symbol',
            'signal_type', 'entry',
            'direction', 'long',
            'exchange', 'binance',
            'side', 'buy',
            'order_type', 'market',
            'size', :'qty',
            'quantity', :'qty',
            'margin_mode', 'cross',
            'margin_coin', 'USDT',
            'trade_side', 'open',
            'client_order_id', :'client_order_id',
            'reduce_only', false
        )::text,
        NOW(),
        NOW()
    FROM signal
    RETURNING id
)
SELECT id FROM task;
SQL
}

create_close_execution_task() {
    local combo_id="$1"
    local open_task_id="$2"
    local external_id
    local client_order_id
    external_id="binance-eth-micro-close-$(date +%s)-$$"
    client_order_id="rqethclose$(date +%s)$$"

    # Contract JSON keys: "close_order", "reduce_only", true, "trade_side", "close".
    run_web_sql \
        -v external_id="${external_id}" \
        -v strategy_slug="${BINANCE_ETH_MICRO_STRATEGY_SLUG}" \
        -v strategy_key="${BINANCE_ETH_MICRO_STRATEGY_KEY}" \
        -v symbol="${BINANCE_ETH_MICRO_WEB_SYMBOL}" \
        -v qty="${BINANCE_ETH_MICRO_QTY}" \
        -v combo_id="${combo_id}" \
        -v buyer_email="${BINANCE_ETH_MICRO_BUYER_EMAIL}" \
        -v client_order_id="${client_order_id}" \
        -v open_task_id="${open_task_id}" \
        -At <<'SQL'
WITH signal AS (
    INSERT INTO strategy_signal_inbox (
        source,
        external_id,
        strategy_slug,
        strategy_key,
        symbol,
        signal_type,
        direction,
        title,
        summary,
        confidence,
        payload_json,
        generated_at,
        created_at,
        updated_at
    )
    VALUES (
        'rust_quant_eth_micro_live_smoke',
        :'external_id',
        :'strategy_slug',
        :'strategy_key',
        :'symbol',
        'exit',
        'flat',
        'Binance ETH micro live reduce-only close smoke',
        'Immediate reduce-only close task created after open task.',
        1.0,
        jsonb_build_object(
            'exchange', 'binance',
            'symbol', :'symbol',
            'side', 'sell',
            'order_type', 'market',
            'size', :'qty',
            'quantity', :'qty',
            'trade_side', 'close',
            'client_order_id', :'client_order_id',
            'reduce_only', true,
            'open_task_id', :open_task_id
        )::text,
        NOW(),
        NOW(),
        NOW()
    )
    RETURNING id
), task AS (
    INSERT INTO execution_tasks (
        strategy_signal_id,
        combo_id,
        buyer_email,
        strategy_slug,
        symbol,
        task_type,
        task_status,
        priority,
        lease_owner,
        lease_until,
        scheduled_at,
        request_payload_json,
        created_at,
        updated_at
    )
    SELECT
        signal.id,
        :combo_id,
        :'buyer_email',
        :'strategy_slug',
        :'symbol',
        'risk_control_close_candidate', 'pending_close',
        100,
        NULL,
        NULL,
        NOW(),
        jsonb_build_object(
            'source', 'rust_quant_eth_micro_live_smoke',
            'symbol', :'symbol',
            'risk_control', jsonb_build_object('action', 'close_candidate'),
            'manual_review', jsonb_build_object('action', 'approved_micro_smoke_close'),
            'close_order', jsonb_build_object(
                'exchange', 'binance',
                'symbol', :'symbol',
                'side', 'sell',
                'order_type', 'market',
                'size', :'qty',
                'quantity', :'qty',
                'trade_side', 'close',
                'client_order_id', :'client_order_id',
                'reduce_only', true,
                'open_task_id', :open_task_id
            )
        )::text,
        NOW(),
        NOW()
    FROM signal
    RETURNING id
)
SELECT id FROM task;
SQL
}

run_execution_worker_once() {
    local stage="$1"
    local task_type="$2"
    local task_status="$3"
    local task_id="$4"

    assert_no_other_pending_tasks "${task_type}" "${task_status}" "${task_id}"

    export RUST_QUAN_WEB_BASE_URL
    export EXECUTION_EVENT_SECRET
    export EXECUTION_WORKER_DRY_RUN=false
    export EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS
    export EXECUTION_WORKER_DEFAULT_EXCHANGE=binance
    export EXECUTION_WORKER_RUN_ONCE=true
    export EXECUTION_WORKER_ONLY=true
    export EXECUTION_WORKER_LEASE_LIMIT=1
    export EXECUTION_WORKER_TASK_TYPES="${task_type}"
    export EXECUTION_WORKER_TASK_STATUSES="${task_status}"
    export EXECUTION_WORKER_TARGET_TASK_IDS="${task_id}"
    export BINANCE_PROXY_URL
    export RUSTUP_TOOLCHAIN
    export IS_RUN_EXECUTION_WORKER=true
    export IS_BACK_TEST=false
    export IS_OPEN_SOCKET=false
    export IS_RUN_REAL_STRATEGY=false
    export IS_RUN_SYNC_DATA_JOB=false
    export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

    if command -v rustup >/dev/null 2>&1; then
        local rustc_path
        rustc_path="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc 2>/dev/null || true)"
        if [[ -n "${rustc_path}" ]]; then
            export RUSTC="${rustc_path}"
            export PATH="$(dirname "${rustc_path}"):${PATH}"
        fi
    fi

    echo "  worker_stage=${stage} task_id=${task_id} task_type=${task_type} task_status=${task_status}"
    cd "${REPO_ROOT}"
    local target_binary
    local worker_status=0
    target_binary="${REPO_ROOT}/target/debug/rust_quant"
    if [[ "${EXECUTION_WORKER_USE_EXISTING_BINARY}" =~ ^(true|TRUE|1|yes|YES)$ ]] ||
       { [[ "${EXECUTION_WORKER_USE_EXISTING_BINARY}" =~ ^(auto|AUTO)$ ]] && [[ -x "${target_binary}" ]]; }; then
        if [[ ! -x "${target_binary}" ]]; then
            echo "ERROR: target/debug/rust_quant is not executable; build it first." >&2
            exit 2
        fi
        "${target_binary}" || worker_status=$?
    elif command -v rustup >/dev/null 2>&1; then
        rustup run "${RUSTUP_TOOLCHAIN}" cargo run --bin rust_quant || worker_status=$?
    else
        cargo run --bin rust_quant || worker_status=$?
    fi
    unset EXECUTION_WORKER_TARGET_TASK_IDS
    return "${worker_status}"
}

verify_order_result() {
    local task_id="$1"
    local expected_side="$2"
    local label="$3"
    local row
    row="$(
        run_web_sql \
            -v task_id="${task_id}" \
            -v expected_side="${expected_side}" \
            -At <<'SQL'
SELECT
    o.id,
    o.order_status,
    o.order_side,
    o.exchange,
    o.external_order_id,
    t.task_status
FROM exchange_order_results o
JOIN execution_tasks t ON t.id = o.execution_task_id
WHERE o.execution_task_id = :task_id
  AND o.order_side = :'expected_side'
ORDER BY o.id DESC
LIMIT 1;
SQL
    )"

    if [[ -z "${row}" ]]; then
        echo "ERROR: ${label} order result was not written back to Web." >&2
        exit 1
    fi

    IFS='|' read -r order_id order_status order_side exchange external_order_id task_status <<<"${row}"
    if [[ "${order_status}" == "dry_run" ]]; then
        echo "ERROR: ${label} order result is dry_run; refusing to claim live smoke success." >&2
        exit 1
    fi

    echo "  ${label}_order_result_id=${order_id}"
    echo "  ${label}_order_status=${order_status}"
    echo "  ${label}_order_side=${order_side}"
    echo "  ${label}_exchange=${exchange}"
    echo "  ${label}_external_order_id_present=$(if [[ -n "${external_order_id}" ]]; then echo yes; else echo no; fi)"
    echo "  ${label}_task_status=${task_status}"
}

verify_close_reduce_only_contract() {
    local close_task_id="$1"
    local reduce_only
    reduce_only="$(
        run_web_sql \
            -v task_id="${close_task_id}" \
            -At <<'SQL'
SELECT COALESCE((request_payload_json::jsonb -> 'close_order' ->> 'reduce_only'), '')
FROM execution_tasks
WHERE id = :task_id;
SQL
    )"
    if [[ "${reduce_only}" != "true" ]]; then
        echo "ERROR: close task does not carry close_order.reduce_only=true." >&2
        exit 1
    fi
    echo "  close_reduce_only=true"
}

echo
echo "=== Step 1: Binance preflights ==="
preflight_binance_signed_account
preflight_binance_position_mode
preflight_binance_exchange_info_filters

echo
echo "=== Step 2: Web credential and combo preflight ==="
upsert_binance_credentials
COMBO_ID="$(resolve_combo_id)"
echo "  combo_id=${COMBO_ID}"

echo
echo "=== Step 3: Create ETH open task and run worker once ==="
OPEN_TASK_ID="$(create_open_execution_task "${COMBO_ID}")"
echo "  open_task_id=${OPEN_TASK_ID}"
run_execution_worker_once "open" "execute_signal" "pending" "${OPEN_TASK_ID}"
verify_order_result "${OPEN_TASK_ID}" "buy" "open"

echo
echo "=== Step 4: Create immediate reduce-only close task and run worker once ==="
CLOSE_TASK_ID="$(create_close_execution_task "${COMBO_ID}" "${OPEN_TASK_ID}")"
echo "  close_task_id=${CLOSE_TASK_ID}"
verify_close_reduce_only_contract "${CLOSE_TASK_ID}"
run_execution_worker_once "close" "risk_control_close_candidate" "pending_close" "${CLOSE_TASK_ID}"
verify_order_result "${CLOSE_TASK_ID}" "sell" "close"
verify_final_eth_position_flat

echo
echo "=== Binance ETH micro live smoke completed ==="
echo "  open_task_id=${OPEN_TASK_ID}"
echo "  close_task_id=${CLOSE_TASK_ID}"
