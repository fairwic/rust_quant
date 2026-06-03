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
MONOREPO_ROOT="$(cd "${REPO_ROOT}/.." && pwd)"

safe_assign_env() {
    local key="$1"
    local value="$2"
    if [[ -z "${!key:-}" ]]; then
        printf -v "${key}" '%s' "${value}"
        export "${key}"
    fi
}

load_env_file_safe() {
    local env_file="$1"
    [[ -f "${env_file}" ]] || return 0

    local line
    while IFS= read -r line || [[ -n "${line}" ]]; do
        line="${line%$'\r'}"
        line="${line#"${line%%[![:space:]]*}"}"
        [[ -z "${line}" || "${line}" == \#* ]] && continue
        if [[ "${line}" == export[[:space:]]* ]]; then
            line="${line#export }"
            line="${line#"${line%%[![:space:]]*}"}"
        fi
        [[ "${line}" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]] || continue

        local key="${line%%=*}"
        local value="${line#*=}"
        value="${value#"${value%%[![:space:]]*}"}"
        value="${value%"${value##*[![:space:]]}"}"
        if [[ "${value}" == \"*\" && "${value}" == *\" ]]; then
            value="${value:1:${#value}-2}"
        elif [[ "${value}" == \'*\' && "${value}" == *\' ]]; then
            value="${value:1:${#value}-2}"
        fi

        case "${key}" in
            BINANCE_API_KEY|BINANCE_API_SECRET|binance_api_secret|BINANCE_LIVE_API_KEY|BINANCE_LIVE_API_SECRET|BINANCE_PROXY_URL|RUST_QUAN_WEB_BASE_URL|EXECUTION_EVENT_SECRET|POSTGRES_CONTAINER|POSTGRES_USER|WEB_POSTGRES_DB|WEB_DATABASE_URL|BINANCE_ETH_MICRO_COMBO_ID|BINANCE_ETH_MICRO_QTY|BINANCE_ETH_MICRO_STOP_LOSS_PRICE|BINANCE_ETH_MICRO_BUYER_EMAIL|BINANCE_ETH_MICRO_STRATEGY_SLUG|BINANCE_ETH_MICRO_STRATEGY_KEY|RUSTUP_TOOLCHAIN|EXECUTION_WORKER_USE_EXISTING_BINARY)
                safe_assign_env "${key}" "${value}"
                ;;
            DATABASE_URL)
                if [[ "${env_file}" == *"/rust_quan_web/backend/.env" ]]; then
                    safe_assign_env WEB_DATABASE_URL "${value}"
                else
                    safe_assign_env DATABASE_URL "${value}"
                fi
                ;;
        esac
    done < "${env_file}"
}

load_env_file_safe "${REPO_ROOT}/.env"
load_env_file_safe "${MONOREPO_ROOT}/rust_quan_web/backend/.env"

: "${BINANCE_ETH_MICRO_SYMBOL:="ETH-USDT-SWAP"}"
: "${BINANCE_ETH_MICRO_QTY:="0.010"}"
: "${BINANCE_ETH_MICRO_STOP_LOSS_PRICE:=""}"
: "${BINANCE_PROXY_URL:="socks5h://127.0.0.1:7897"}"
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
BINANCE_ETH_MICRO_POSITION_MODE=""
BINANCE_ETH_MICRO_POSITION_SIDE=""
BINANCE_ETH_MICRO_OPEN_REDUCE_ONLY=""
BINANCE_ETH_MICRO_CLOSE_REDUCE_ONLY="true"
BINANCE_ETH_MICRO_NOTIONAL=""
BINANCE_ETH_MICRO_MARK_PRICE=""

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
    local extra_query="${3:-}"
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
        if [[ -n "${extra_query}" ]]; then
            query="${extra_query}&${query}"
        fi
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

fetch_binance_open_orders_body() {
    fetch_binance_signed_body "/fapi/v1/openOrders" "open-orders" "symbol=ETHUSDT"
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

eth_position_is_flat() {
    local body_file
    body_file="$(fetch_binance_signed_account_body)"
    if python3 - "${body_file}" <<'PY'
import json
import sys
from decimal import Decimal, InvalidOperation

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

positions = data.get("positions") or []
eth_position = next((item for item in positions if item.get("symbol") == "ETHUSDT"), None)
if eth_position is None:
    raise SystemExit(2)
try:
    position_amt = Decimal(str(eth_position.get("positionAmt", "0")))
except InvalidOperation:
    raise SystemExit(2)
raise SystemExit(0 if position_amt == 0 else 1)
PY
    then
        rm -f "${body_file}"
        return 0
    fi
    local status=$?
    rm -f "${body_file}"
    return "${status}"
}

assert_eth_open_orders_clear() {
    local body_file="$1"
    local context="$2"

    python3 - "${body_file}" "${context}" <<'PY'
import json
import sys

body_path, context = sys.argv[1], sys.argv[2]
with open(body_path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

if not isinstance(data, list):
    raise SystemExit(f"ERROR: ETHUSDT openOrders response is not a list during {context}")
if data:
    raise SystemExit(
        f"ERROR: ETHUSDT must have no open orders during {context}; open_order_count={len(data)}"
    )
PY
}

preflight_binance_open_orders_clear() {
    local body_file
    body_file="$(fetch_binance_open_orders_body)"
    if ! assert_eth_open_orders_clear "${body_file}" "preflight_eth_open_orders_clear"; then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    echo "  preflight_eth_open_orders_clear=ok"
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
    local position_mode
    position_mode="$(python3 - "${body_file}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)

mode = data.get("dualSidePosition")
if mode is False or str(mode).lower() == "false":
    print("one_way")
    raise SystemExit(0)
if mode is True or str(mode).lower() == "true":
    print("hedge")
    raise SystemExit(0)
raise SystemExit("ERROR: Binance position mode response missing dualSidePosition")
PY
    )"
    if [[ -z "${position_mode}" ]]; then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    if [[ "${position_mode}" == "hedge" ]]; then
        BINANCE_ETH_MICRO_POSITION_MODE=hedge
        BINANCE_ETH_MICRO_POSITION_SIDE=long
        BINANCE_ETH_MICRO_CLOSE_REDUCE_ONLY=""
        echo "  binance_position_mode=hedge"
        echo "  hedge_position_side=LONG"
    else
        BINANCE_ETH_MICRO_POSITION_MODE=""
        BINANCE_ETH_MICRO_POSITION_SIDE=""
        BINANCE_ETH_MICRO_CLOSE_REDUCE_ONLY="true"
        echo "  binance_position_mode=one_way"
    fi
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

verify_final_eth_open_orders_clear() {
    local body_file
    body_file="$(fetch_binance_open_orders_body)"
    if ! assert_eth_open_orders_clear "${body_file}" "final_eth_open_orders_clear"; then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    echo "  final_eth_open_orders_clear=ok"
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
        python3 - "${exchange_info_file}" "${premium_file}" "${BINANCE_ETH_MICRO_QTY}" "${BINANCE_ETH_MICRO_STOP_LOSS_PRICE}" <<'PY'
import json
import sys
from decimal import Decimal, InvalidOperation, ROUND_DOWN

exchange_path, premium_path, qty_raw, stop_loss_raw = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
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
price_filter = filters.get("PRICE_FILTER") or {}
tick_size = Decimal(price_filter.get("tickSize", "0"))

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

if stop_loss_raw:
    try:
        stop_loss_price = Decimal(stop_loss_raw)
    except InvalidOperation as exc:
        raise SystemExit(f"ERROR: invalid ETH stop-loss price: {stop_loss_raw}") from exc
else:
    stop_loss_price = mark_price * Decimal("0.98")

if tick_size > 0:
    stop_loss_price = (stop_loss_price / tick_size).to_integral_value(rounding=ROUND_DOWN) * tick_size
if stop_loss_price <= 0:
    raise SystemExit("ERROR: ETH stop-loss price must be positive")
if stop_loss_price >= mark_price:
    raise SystemExit(
        f"ERROR: long ETH stop-loss price {stop_loss_price} must be below markPrice {mark_price}"
    )

print(
    " ".join(
        [
            f"minQty={min_qty}",
            f"stepSize={step_size}",
            f"minNotional={min_notional}",
            f"markPrice={mark_price}",
            f"notional={notional}",
            f"stopLossPrice={stop_loss_price}",
        ]
    )
)
PY
    )"

    rm -f "${exchange_info_file}" "${premium_file}"
    BINANCE_ETH_MICRO_MARK_PRICE="$(printf '%s\n' "${filter_summary}" | sed -n 's/.*markPrice=\([^ ]*\).*/\1/p')"
    BINANCE_ETH_MICRO_NOTIONAL="$(printf '%s\n' "${filter_summary}" | sed -n 's/.*notional=\([^ ]*\).*/\1/p')"
    BINANCE_ETH_MICRO_STOP_LOSS_PRICE="$(printf '%s\n' "${filter_summary}" | sed -n 's/.*stopLossPrice=\([^ ]*\).*/\1/p')"
    if [[ -z "${BINANCE_ETH_MICRO_MARK_PRICE}" || -z "${BINANCE_ETH_MICRO_NOTIONAL}" || -z "${BINANCE_ETH_MICRO_STOP_LOSS_PRICE}" ]]; then
        echo "ERROR: failed to capture ETH micro mark/notional/stop-loss from Binance exchangeInfo preflight." >&2
        exit 1
    fi
    echo "  exchange_info_filters=ok ${filter_summary}"
}

preflight_binance_margin_available() {
    if [[ -z "${BINANCE_ETH_MICRO_NOTIONAL}" ]]; then
        echo "ERROR: ETH micro notional is missing before available-margin preflight." >&2
        exit 1
    fi

    local body_file
    body_file="$(fetch_binance_signed_account_body)"
    if ! python3 - "${body_file}" "${BINANCE_ETH_MICRO_NOTIONAL}" <<'PY'
import json
import sys
from decimal import Decimal, InvalidOperation

body_path, notional_raw = sys.argv[1], sys.argv[2]
try:
    estimated_notional = Decimal(str(notional_raw))
except InvalidOperation as exc:
    raise SystemExit("ERROR: invalid ETH micro notional before margin preflight") from exc

with open(body_path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

available_raw = data.get("availableBalance")
if available_raw is None:
    assets = data.get("assets") or []
    usdt_asset = next((item for item in assets if item.get("asset") == "USDT"), None)
    if usdt_asset:
        available_raw = usdt_asset.get("availableBalance")

if available_raw is None:
    raise SystemExit("ERROR: Binance account response missing USDT availableBalance")

try:
    available = Decimal(str(available_raw))
except InvalidOperation as exc:
    raise SystemExit("ERROR: invalid Binance USDT availableBalance") from exc

if estimated_notional <= 0:
    raise SystemExit("ERROR: ETH micro estimated notional must be positive")

if available < estimated_notional:
    raise SystemExit(
        "ERROR: Binance Futures available USDT margin is below the ETH micro notional preflight; "
        f"estimated_notional={estimated_notional}"
    )
PY
    then
        rm -f "${body_file}"
        exit 1
    fi
    rm -f "${body_file}"
    echo "  preflight_margin_available=ok"
}

verify_existing_binance_credential_ready() {
    local rows
    rows="$(
        run_web_sql \
            -v buyer_email="${BINANCE_ETH_MICRO_BUYER_EMAIL}" \
            -At <<'SQL'
SELECT
    id,
    COALESCE(api_key_mask, ''),
    COALESCE(last_check_code, '')
FROM user_api_credentials
WHERE buyer_email = :'buyer_email'
  AND exchange = '币安'
  AND status = 'active'
  AND last_check_code IN ('signed_exchange_preflight_passed', 'signed_exchange_check_passed')
  AND api_key_cipher LIKE 'v3:aes256gcm:%'
  AND api_secret_cipher LIKE 'v3:aes256gcm:%'
  AND (passphrase_cipher IS NULL OR passphrase_cipher LIKE 'v3:aes256gcm:%')
ORDER BY updated_at DESC, id DESC
LIMIT 2;
SQL
    )"

    local row_count
    row_count="$(printf '%s\n' "${rows}" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')"
    if [[ "${row_count}" != "1" ]]; then
        echo "ERROR: expected exactly one active Binance Web credential with v3 envelope and signed preflight code." >&2
        echo "  buyer_email=${BINANCE_ETH_MICRO_BUYER_EMAIL}" >&2
        echo "  matching_credential_count=${row_count}" >&2
        echo "  Re-save the Binance credential through rust_quan_web before running live validation." >&2
        exit 1
    fi

    local credential_id
    local api_key_mask
    local last_check_code
    IFS='|' read -r credential_id api_key_mask last_check_code <<<"${rows}"
    echo "  web_credential_ready=ok"
    echo "  web_credential_id=${credential_id}"
    echo "  web_credential_api_key_mask=${api_key_mask:-<masked>}"
    echo "  web_credential_last_check_code=${last_check_code}"
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

assert_target_is_next_leaseable_task() {
    local task_type="$1"
    local task_status="$2"
    local target_task_id="$3"
    local row
    row="$(
        run_web_sql \
            -v task_type="${task_type}" \
            -v task_status="${task_status}" \
            -v target_task_id="${target_task_id}" \
            -At <<'SQL'
WITH target AS (
    SELECT id, priority, scheduled_at
    FROM execution_tasks
    WHERE id = :target_task_id
      AND task_type = :'task_type'
      AND task_status = :'task_status'
      AND scheduled_at <= NOW()
), earlier_leaseable AS (
    SELECT other.id
    FROM execution_tasks other
    JOIN target ON TRUE
    WHERE other.task_type = :'task_type'
      AND other.id <> target.id
      AND other.scheduled_at <= NOW()
      AND (
        other.task_status = :'task_status'
        OR (other.task_status = 'leased' AND other.lease_until < NOW())
      )
      AND (
        other.priority > target.priority
        OR (
          other.priority = target.priority
          AND (
            other.scheduled_at < target.scheduled_at
            OR (other.scheduled_at = target.scheduled_at AND other.id < target.id)
          )
        )
      )
)
SELECT
    (SELECT COUNT(*) FROM target),
    (SELECT COUNT(*) FROM earlier_leaseable);
SQL
    )"
    local target_count
    local earlier_count
    IFS='|' read -r target_count earlier_count <<<"${row}"
    if [[ "${target_count}" != "1" ]]; then
        echo "ERROR: target task is not leaseable before live worker run." >&2
        echo "  task_type=${task_type} task_status=${task_status} target_task_id=${target_task_id}" >&2
        exit 1
    fi
    if [[ ! "${earlier_count}" =~ ^[0-9]+$ ]]; then
        echo "ERROR: lease order isolation query returned invalid count: ${earlier_count}" >&2
        exit 1
    fi
    if (( earlier_count > 0 )); then
        echo "ERROR: refusing live worker run because another task would be leased first." >&2
        echo "  task_type=${task_type} task_status=${task_status} target_task_id=${target_task_id} earlier_count=${earlier_count}" >&2
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
        -v entry_price="${BINANCE_ETH_MICRO_MARK_PRICE}" \
        -v stop_loss_price="${BINANCE_ETH_MICRO_STOP_LOSS_PRICE}" \
        -v position_mode="${BINANCE_ETH_MICRO_POSITION_MODE}" \
        -v position_side="${BINANCE_ETH_MICRO_POSITION_SIDE}" \
        -v open_reduce_only="${BINANCE_ETH_MICRO_OPEN_REDUCE_ONLY}" \
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
        jsonb_strip_nulls(jsonb_build_object(
            'exchange', 'binance',
            'symbol', :'symbol',
            'side', 'buy',
            'order_type', 'market',
            'size', :'qty',
            'quantity', :'qty',
            'margin_mode', 'cross',
            'margin_coin', 'USDT',
            'position_mode', NULLIF(:'position_mode', ''),
            'position_side', NULLIF(:'position_side', ''),
            'trade_side', 'open',
            'client_order_id', :'client_order_id',
            'reduce_only', NULLIF(:'open_reduce_only', '')::boolean,
            'protective_stop_loss_required', true,
            'risk_plan', jsonb_build_object(
                'entry_price', :'entry_price'::numeric,
                'selected_stop_loss_price', :'stop_loss_price'::numeric,
                'direction', 'long',
                'protective_stop_loss_required', true
            )
        ))::text,
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
        jsonb_strip_nulls(jsonb_build_object(
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
            'position_mode', NULLIF(:'position_mode', ''),
            'position_side', NULLIF(:'position_side', ''),
            'trade_side', 'open',
            'client_order_id', :'client_order_id',
            'reduce_only', NULLIF(:'open_reduce_only', '')::boolean,
            'protective_stop_loss_required', true,
            'risk_plan', jsonb_build_object(
                'entry_price', :'entry_price'::numeric,
                'selected_stop_loss_price', :'stop_loss_price'::numeric,
                'direction', 'long',
                'protective_stop_loss_required', true
            )
        ))::text,
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

    # Contract JSON keys: "close_order", mode-aware "reduce_only", "position_side", "trade_side", "close".
    run_web_sql \
        -v external_id="${external_id}" \
        -v strategy_slug="${BINANCE_ETH_MICRO_STRATEGY_SLUG}" \
        -v strategy_key="${BINANCE_ETH_MICRO_STRATEGY_KEY}" \
        -v symbol="${BINANCE_ETH_MICRO_WEB_SYMBOL}" \
        -v qty="${BINANCE_ETH_MICRO_QTY}" \
        -v position_mode="${BINANCE_ETH_MICRO_POSITION_MODE}" \
        -v position_side="${BINANCE_ETH_MICRO_POSITION_SIDE}" \
        -v close_reduce_only="${BINANCE_ETH_MICRO_CLOSE_REDUCE_ONLY}" \
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
        jsonb_strip_nulls(jsonb_build_object(
            'exchange', 'binance',
            'symbol', :'symbol',
            'side', 'sell',
            'order_type', 'market',
            'size', :'qty',
            'quantity', :'qty',
            'position_mode', NULLIF(:'position_mode', ''),
            'position_side', NULLIF(:'position_side', ''),
            'trade_side', 'close',
            'client_order_id', :'client_order_id',
            'reduce_only', NULLIF(:'close_reduce_only', '')::boolean,
            'open_task_id', :open_task_id
        ))::text,
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
        jsonb_strip_nulls(jsonb_build_object(
            'source', 'rust_quant_eth_micro_live_smoke',
            'symbol', :'symbol',
            'risk_control', jsonb_build_object('action', 'close_candidate'),
            'manual_review', jsonb_build_object('action', 'approved_micro_smoke_close'),
            'close_order', jsonb_strip_nulls(jsonb_build_object(
                'exchange', 'binance',
                'symbol', :'symbol',
                'side', 'sell',
                'order_type', 'market',
                'size', :'qty',
                'quantity', :'qty',
                'position_mode', NULLIF(:'position_mode', ''),
                'position_side', NULLIF(:'position_side', ''),
                'trade_side', 'close',
                'client_order_id', :'client_order_id',
                'reduce_only', NULLIF(:'close_reduce_only', '')::boolean,
                'open_task_id', :open_task_id
            ))
        ))::text,
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

    assert_target_is_next_leaseable_task "${task_type}" "${task_status}" "${task_id}"

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
        return 1
    fi

    IFS='|' read -r order_id order_status order_side exchange external_order_id task_status <<<"${row}"
    local order_status_upper
    order_status_upper="$(printf '%s' "${order_status}" | tr '[:lower:]' '[:upper:]')"
    if [[ "${order_status}" == "dry_run" ]]; then
        echo "ERROR: ${label} order result is dry_run; refusing to claim live smoke success." >&2
        return 1
    fi
    if [[ "${order_status}" == "failed" || "${task_status}" == "failed" ]]; then
        echo "ERROR: ${label} order result failed; refusing to claim live smoke success." >&2
        echo "  ${label}_order_status=${order_status}" >&2
        echo "  ${label}_task_status=${task_status}" >&2
        return 1
    fi
    if [[ "${order_status_upper}" != "FILLED" ]]; then
        echo "ERROR: ${label} order result is ${order_status}; live ETH market smoke requires FILLED." >&2
        echo "  ${label}_order_status=${order_status}" >&2
        echo "  ${label}_task_status=${task_status}" >&2
        return 1
    fi

    echo "  ${label}_order_result_id=${order_id}"
    echo "  ${label}_order_status=${order_status}"
    echo "  ${label}_order_side=${order_side}"
    echo "  ${label}_exchange=${exchange}"
    echo "  ${label}_external_order_id_present=$(if [[ -n "${external_order_id}" ]]; then echo yes; else echo no; fi)"
    echo "  ${label}_task_status=${task_status}"
}

verify_open_protection_sync() {
    local task_id="$1"
    local row
    row="$(
        run_web_sql \
            -v task_id="${task_id}" \
            -At <<'SQL'
SELECT
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,contract_version}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,status}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,exchange}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,protective_order_mode}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,source}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,protective_order_confirmed}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,protective_order_external_id}', ''),
    COALESCE(raw_payload_json::jsonb #>> '{protection_sync,exchange_protective_order_supported}', '')
FROM exchange_order_results
WHERE execution_task_id = :task_id
  AND order_side = 'buy'
ORDER BY id DESC
LIMIT 1;
SQL
    )"

    if [[ -z "${row}" ]]; then
        echo "ERROR: open protection_sync evidence was not written back to Web." >&2
        return 1
    fi

    local contract_version
    local status
    local exchange
    local protective_order_mode
    local source
    local protective_order_confirmed
    local protective_order_external_id
    local exchange_protective_order_supported
    IFS='|' read -r contract_version status exchange protective_order_mode source protective_order_confirmed protective_order_external_id exchange_protective_order_supported <<<"${row}"

    if [[ "${contract_version}" != "v2" ||
          "${status}" != "completed" ||
          "${exchange}" != "binance" ||
          "${protective_order_mode}" != "independent_stop_market" ||
          "${source}" != "query_protective_order" ||
          "${protective_order_confirmed}" != "true" ||
          -z "${protective_order_external_id}" ||
          "${exchange_protective_order_supported}" != "true" ]]; then
        echo "ERROR: open protection_sync is not confirmed by Binance active-order evidence." >&2
        echo "  protection_contract_version=${contract_version}" >&2
        echo "  protection_status=${status}" >&2
        echo "  protection_exchange=${exchange}" >&2
        echo "  protective_order_mode=${protective_order_mode}" >&2
        echo "  protection_source=${source}" >&2
        echo "  protective_order_confirmed=${protective_order_confirmed}" >&2
        echo "  protective_order_external_id_present=$(if [[ -n "${protective_order_external_id}" ]]; then echo yes; else echo no; fi)" >&2
        echo "  exchange_protective_order_supported=${exchange_protective_order_supported}" >&2
        return 1
    fi

    echo "  open_protection_sync=confirmed"
    echo "  open_protection_contract_version=${contract_version}"
    echo "  open_protective_order_mode=${protective_order_mode}"
    echo "  open_protective_order_external_id_present=yes"
}

emergency_close_eth_position_via_web() {
    local combo_id="$1"
    local open_task_id="$2"

    echo "  emergency_close_eth_position_via_web=attempting"
    local emergency_close_task_id
    emergency_close_task_id="$(create_close_execution_task "${combo_id}" "${open_task_id}")"
    echo "  emergency_close_task_id=${emergency_close_task_id}"
    verify_close_reduce_only_contract "${emergency_close_task_id}"
    if ! run_execution_worker_once "emergency-close" "risk_control_close_candidate" "pending_close" "${emergency_close_task_id}"; then
        echo "ERROR: emergency Web close worker failed; manual exchange review is required." >&2
        verify_final_eth_position_flat
        verify_final_eth_open_orders_clear
        exit 1
    fi
    verify_order_result "${emergency_close_task_id}" "sell" "emergency_close"
    verify_final_eth_position_flat
    verify_final_eth_open_orders_clear
}

handle_open_stage_failure() {
    local combo_id="$1"
    local open_task_id="$2"

    echo "ERROR: open stage failed after possible live order placement; checking ETH position before exit." >&2
    if eth_position_is_flat; then
        echo "  open_stage_failure_eth_position=flat"
        verify_final_eth_open_orders_clear
        exit 1
    fi

    echo "WARN: ETHUSDT is not flat after open-stage failure; attempting Web-path emergency close." >&2
    emergency_close_eth_position_via_web "${combo_id}" "${open_task_id}"
    exit 1
}

verify_close_reduce_only_contract() {
    local close_task_id="$1"
    local row
    row="$(
        run_web_sql \
            -v task_id="${close_task_id}" \
            -At <<'SQL'
SELECT
    COALESCE((request_payload_json::jsonb -> 'close_order' ->> 'reduce_only'), ''),
    COALESCE((request_payload_json::jsonb -> 'close_order' ->> 'position_side'), '')
FROM execution_tasks
WHERE id = :task_id;
SQL
    )"
    local reduce_only
    local position_side
    IFS='|' read -r reduce_only position_side <<<"${row}"
    if [[ -n "${BINANCE_ETH_MICRO_CLOSE_REDUCE_ONLY}" ]]; then
        if [[ "${reduce_only}" != "true" ]]; then
            echo "ERROR: one-way close task does not carry close_order.reduce_only=true." >&2
            exit 1
        fi
        echo "  close_reduce_only=true"
    else
        if [[ -n "${reduce_only}" ]]; then
            echo "ERROR: hedge close task must omit close_order.reduce_only." >&2
            exit 1
        fi
        if [[ "${position_side}" != "long" ]]; then
            echo "ERROR: hedge close task must carry close_order.position_side=long." >&2
            exit 1
        fi
        echo "  close_reduce_only=omitted_for_hedge"
        echo "  close_position_side=long"
    fi
}

echo
echo "=== Step 1: Binance preflights ==="
preflight_binance_signed_account
preflight_binance_position_mode
preflight_binance_open_orders_clear
preflight_binance_exchange_info_filters
preflight_binance_margin_available

echo
echo "=== Step 2: Web credential and combo preflight ==="
verify_existing_binance_credential_ready
COMBO_ID="$(resolve_combo_id)"
echo "  combo_id=${COMBO_ID}"

echo
echo "=== Step 3: Create ETH open task and run worker once ==="
OPEN_TASK_ID="$(create_open_execution_task "${COMBO_ID}")"
echo "  open_task_id=${OPEN_TASK_ID}"
if ! run_execution_worker_once "open" "execute_signal" "pending" "${OPEN_TASK_ID}"; then
    handle_open_stage_failure "${COMBO_ID}" "${OPEN_TASK_ID}"
fi
if ! verify_order_result "${OPEN_TASK_ID}" "buy" "open"; then
    handle_open_stage_failure "${COMBO_ID}" "${OPEN_TASK_ID}"
fi
if ! verify_open_protection_sync "${OPEN_TASK_ID}"; then
    handle_open_stage_failure "${COMBO_ID}" "${OPEN_TASK_ID}"
fi

echo
echo "=== Step 4: Create immediate reduce-only close task and run worker once ==="
CLOSE_TASK_ID="$(create_close_execution_task "${COMBO_ID}" "${OPEN_TASK_ID}")"
echo "  close_task_id=${CLOSE_TASK_ID}"
verify_close_reduce_only_contract "${CLOSE_TASK_ID}"
if ! run_execution_worker_once "close" "risk_control_close_candidate" "pending_close" "${CLOSE_TASK_ID}"; then
    echo "ERROR: close worker failed; checking final ETH state." >&2
    verify_final_eth_position_flat
    verify_final_eth_open_orders_clear
    exit 1
fi
if ! verify_order_result "${CLOSE_TASK_ID}" "sell" "close"; then
    echo "ERROR: close order result verification failed; checking final ETH state." >&2
    verify_final_eth_position_flat
    verify_final_eth_open_orders_clear
    exit 1
fi
verify_final_eth_position_flat
verify_final_eth_open_orders_clear

echo
echo "=== Binance ETH micro live smoke completed ==="
echo "  open_task_id=${OPEN_TASK_ID}"
echo "  close_task_id=${CLOSE_TASK_ID}"
