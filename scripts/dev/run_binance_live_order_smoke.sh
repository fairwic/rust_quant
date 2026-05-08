#!/usr/bin/env bash
# run_binance_live_order_smoke.sh
#
# Binance 实盘下单 smoke 脚本。
#
# 功能：
#   1. 检查必要 env var
#   2. 向 quant_web.user_api_credentials 插入（或更新）Binance 凭证
#   3. 验证一个明确指定的 Web pending_close 任务
#   4. 以 EXECUTION_WORKER_DRY_RUN=false 运行 execution worker
#   5. 验证 exchange_order_results 最新记录 order_status != 'dry_run'
#
# 必须设置的 env var：
#   BINANCE_LIVE_API_KEY        Binance 实盘 API Key
#   BINANCE_LIVE_API_SECRET     Binance 实盘 Secret Key
#   BINANCE_LIVE_PENDING_CLOSE_TASK_ID  Web 已生成的 pending_close close task id
#   WEB_DATABASE_URL 或 DATABASE_URL  quant_web 数据库连接串
#
# 可选 env var：
#   API_CREDENTIAL_SECRET       凭证加密密钥（与 Web 后端保持一致，默认 alpha-pulse-local-credential-key）
#   BINANCE_PROXY_URL           SOCKS5 代理（默认 socks5h://127.0.0.1:7897）
#   RUST_QUAN_WEB_BASE_URL      Web 后端地址（默认 http://127.0.0.1:8000）
#   EXECUTION_EVENT_SECRET      内部事件密钥（默认 local-dev-secret）
#   BINANCE_LIVE_BUYER_EMAIL    凭证关联邮箱（默认 demo-exec-worker@example.com）
#   BINANCE_LIVE_STRATEGY_SLUG  策略 slug（默认 vegas）
#   BINANCE_LIVE_SYMBOL         交易对（默认 ETH-USDT-SWAP）
#   POSTGRES_CONTAINER          podman/docker 容器名（默认 postgres）
#   POSTGRES_USER               psql 用户（默认 postgres）
#   WEB_POSTGRES_DB             psql 数据库名（默认 quant_web）
#   EXECUTION_WORKER_USE_EXISTING_BINARY  是否使用已有二进制（默认 auto）
#   RUSTUP_TOOLCHAIN            Rust 工具链版本（默认 1.91.1）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# ---------------------------------------------------------------------------
# Step 0: 检查必要 env var
# ---------------------------------------------------------------------------
echo
echo "=== Step 0: 检查必要 env var ==="

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
if [[ -z "${BINANCE_LIVE_PENDING_CLOSE_TASK_ID:-}" ]]; then
    missing_vars+=("BINANCE_LIVE_PENDING_CLOSE_TASK_ID")
fi

# 支持 WEB_DATABASE_URL 或 DATABASE_URL
if [[ -n "${WEB_DATABASE_URL:-}" ]]; then
    _DB_URL="${WEB_DATABASE_URL}"
elif [[ -n "${DATABASE_URL:-}" ]]; then
    _DB_URL="${DATABASE_URL}"
else
    missing_vars+=("WEB_DATABASE_URL (or DATABASE_URL)")
fi

if [[ ${#missing_vars[@]} -gt 0 ]]; then
    echo "ERROR: 以下必要 env var 未设置，脚本退出：" >&2
    for var in "${missing_vars[@]}"; do
        echo "  - ${var}" >&2
    done
    echo >&2
    echo "使用示例：" >&2
    echo "  export BINANCE_LIVE_API_KEY=<your_api_key>" >&2
    echo "  export BINANCE_LIVE_API_SECRET=<your_api_secret>" >&2
    echo "  export BINANCE_LIVE_PENDING_CLOSE_TASK_ID=<web_pending_close_task_id>" >&2
    echo "  export WEB_DATABASE_URL=postgres://postgres:postgres123@localhost:5432/quant_web" >&2
    echo "  ${BASH_SOURCE[0]}" >&2
    exit 1
fi

if [[ ! "${BINANCE_LIVE_PENDING_CLOSE_TASK_ID}" =~ ^[0-9]+$ ]]; then
    echo "ERROR: BINANCE_LIVE_PENDING_CLOSE_TASK_ID must be a numeric execution_tasks.id" >&2
    exit 1
fi

# 默认值
: "${API_CREDENTIAL_SECRET:="alpha-pulse-local-credential-key"}"
: "${BINANCE_PROXY_URL:="socks5h://127.0.0.1:7897"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${BINANCE_LIVE_BUYER_EMAIL:="demo-exec-worker@example.com"}"
: "${BINANCE_LIVE_STRATEGY_SLUG:="vegas"}"
: "${BINANCE_LIVE_SYMBOL:="ETH-USDT-SWAP"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${WEB_POSTGRES_DB:="quant_web"}"
: "${EXECUTION_WORKER_USE_EXISTING_BINARY:="auto"}"
: "${RUSTUP_TOOLCHAIN:="1.91.1"}"

WEB_DATABASE_URL="${_DB_URL}"

echo "  BINANCE_LIVE_API_KEY    = <set>"
echo "  BINANCE_LIVE_API_SECRET = <set>"
echo "  pending_close_task_id   = ${BINANCE_LIVE_PENDING_CLOSE_TASK_ID}"
echo "  WEB_DATABASE_URL        = ${WEB_DATABASE_URL}"
echo "  BINANCE_PROXY_URL       = ${BINANCE_PROXY_URL}"
echo "  RUST_QUAN_WEB_BASE_URL  = ${RUST_QUAN_WEB_BASE_URL}"
echo "  buyer_email             = ${BINANCE_LIVE_BUYER_EMAIL}"
echo "  strategy_slug           = ${BINANCE_LIVE_STRATEGY_SLUG}"
echo "  symbol                  = ${BINANCE_LIVE_SYMBOL}"

# ---------------------------------------------------------------------------
# psql helper（与其他 seed 脚本保持一致）
# ---------------------------------------------------------------------------
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

    echo "psql was not found, and podman container '${POSTGRES_CONTAINER}' is unavailable" >&2
    exit 1
}

query_web_scalar() {
    run_web_sql -Atc "$1"
}

preflight_binance_signed_account() {
    for cmd in curl openssl xxd; do
        if ! command -v "${cmd}" >/dev/null 2>&1; then
            echo "ERROR: missing required command for Binance signed preflight: ${cmd}" >&2
            exit 1
        fi
    done

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
    http_status="$(curl "${curl_args[@]}" "https://fapi.binance.com/fapi/v2/account?${query}&signature=${signature}" || true)"

    if [[ "${http_status}" != "200" ]]; then
        echo "ERROR: Binance signed account preflight failed before Web state changes." >&2
        echo "  http_status=${http_status}" >&2
        echo "  Check API key/secret pairing, futures permission, and IP whitelist for the current outbound IP." >&2
        if [[ -s "${body_file}" ]]; then
            echo "  exchange_response:" >&2
            sed 's/[[:cntrl:]]//g' "${body_file}" >&2
            echo >&2
        fi
        rm -f "${body_file}"
        exit 1
    fi

    rm -f "${body_file}"
    echo "  signed_account_preflight = ok"
}

# ---------------------------------------------------------------------------
# XOR 凭证加密（与 Web 后端 seal_credential 逻辑一致）
# 输入：raw_value  seal_key
# 输出：hex 字符串（stdout）
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Step 1: 向 user_api_credentials 插入 Binance 实盘凭证
# ---------------------------------------------------------------------------
echo
echo "=== Step 1: Binance signed account preflight ==="
preflight_binance_signed_account

# ---------------------------------------------------------------------------
# Step 2: 验证明确指定的 Web pending_close 任务
# ---------------------------------------------------------------------------
echo
echo "=== Step 2: 验证 Web pending_close 任务 ==="

NEW_TASK_ID="${BINANCE_LIVE_PENDING_CLOSE_TASK_ID}"

TASK_ROW="$(
    run_web_sql -At -F $'\t' -c "
        SELECT
            id,
            buyer_email,
            strategy_slug,
            symbol,
            task_type,
            task_status,
            COALESCE((request_payload_json::json -> 'close_order' ->> 'side'), ''),
            COALESCE((request_payload_json::json -> 'close_order' ->> 'position_side'), ''),
            COALESCE((request_payload_json::json -> 'close_order' ->> 'reduce_only'), '')
        FROM execution_tasks
        WHERE id = ${NEW_TASK_ID}
          AND buyer_email = '${BINANCE_LIVE_BUYER_EMAIL}'
          AND strategy_slug = '${BINANCE_LIVE_STRATEGY_SLUG}'
          AND symbol = '${BINANCE_LIVE_SYMBOL}'
          AND task_type = 'risk_control_close_candidate'
          AND task_status = 'pending_close'
          AND request_payload_json::jsonb ? 'close_order'
        LIMIT 1;
    "
)"

if [[ -z "${TASK_ROW}" ]]; then
    echo "ERROR: 指定任务不是可执行的 Web pending_close close task: id=${NEW_TASK_ID}" >&2
    echo "  必须满足：" >&2
    echo "    - buyer_email/strategy_slug/symbol 与脚本参数一致" >&2
    echo "    - task_type = 'risk_control_close_candidate'" >&2
    echo "    - task_status = 'pending_close'" >&2
    echo "    - request_payload_json 包含 close_order" >&2
    echo >&2
    echo "  诊断查询：" >&2
    run_web_sql -P pager=off -c "
        SELECT
            id,
            buyer_email,
            strategy_slug,
            symbol,
            task_type,
            task_status,
            request_payload_json::json -> 'close_order' AS close_order,
            updated_at
        FROM execution_tasks
        WHERE id = ${NEW_TASK_ID};
    " >&2 || true
    exit 1
fi

IFS=$'\t' read -r TASK_ID TASK_BUYER TASK_STRATEGY TASK_SYMBOL TASK_TYPE TASK_STATUS CLOSE_SIDE CLOSE_POSITION_SIDE CLOSE_REDUCE_ONLY <<<"${TASK_ROW}"

OTHER_PENDING_CLOSE_COUNT="$(query_web_scalar "
    SELECT COUNT(*)
    FROM execution_tasks
    WHERE id <> ${NEW_TASK_ID}
      AND task_type = 'risk_control_close_candidate'
      AND task_status = 'pending_close';
")"

if [[ ! "${OTHER_PENDING_CLOSE_COUNT}" =~ ^[0-9]+$ ]]; then
    echo "ERROR: pending_close task count query returned invalid value: ${OTHER_PENDING_CLOSE_COUNT}" >&2
    exit 1
fi
if (( OTHER_PENDING_CLOSE_COUNT > 0 )); then
    echo "ERROR: 发现其他 pending_close close tasks，拒绝实盘 smoke，避免 worker 租到非目标任务。" >&2
    echo "  target_task_id=${NEW_TASK_ID} other_pending_close_count=${OTHER_PENDING_CLOSE_COUNT}" >&2
    exit 1
fi

echo "  task_id        = ${TASK_ID}"
echo "  buyer_email    = ${TASK_BUYER}"
echo "  strategy_slug  = ${TASK_STRATEGY}"
echo "  symbol         = ${TASK_SYMBOL}"
echo "  task_type      = ${TASK_TYPE}"
echo "  task_status    = ${TASK_STATUS}"
echo "  close_side     = ${CLOSE_SIDE:-<derived>}"
echo "  position_side  = ${CLOSE_POSITION_SIDE:-<empty>}"
echo "  reduce_only    = ${CLOSE_REDUCE_ONLY:-<empty>}"

# ---------------------------------------------------------------------------
# Step 3: 向 user_api_credentials 插入 Binance 实盘凭证
# ---------------------------------------------------------------------------
echo
echo "=== Step 3: 插入 Binance 实盘凭证到 user_api_credentials ==="

API_KEY_CIPHER="$(seal_credential "${BINANCE_LIVE_API_KEY}" "${API_CREDENTIAL_SECRET}")"
API_SECRET_CIPHER="$(seal_credential "${BINANCE_LIVE_API_SECRET}" "${API_CREDENTIAL_SECRET}")"

# 生成 api_key_mask（保留前4后4，中间替换为 ****）
_key="${BINANCE_LIVE_API_KEY}"
_key_len="${#_key}"
if (( _key_len <= 4 )); then
    API_KEY_MASK="$(printf '%*s' "${_key_len}" '' | tr ' ' '*')"
elif (( _key_len < 8 )); then
    API_KEY_MASK="${_key:0:2}****${_key: -2}"
else
    API_KEY_MASK="${_key:0:4}****${_key: -4}"
fi

echo "  exchange        = 币安"
echo "  credential_mask = <stored>"
echo "  api_key_cipher  = <sealed:${#API_KEY_CIPHER} hex chars>"
echo "  api_secret_cipher = <sealed:${#API_SECRET_CIPHER} hex chars>"
echo "  seal_key_source = API_CREDENTIAL_SECRET"

run_web_sql \
    -v buyer_email="${BINANCE_LIVE_BUYER_EMAIL}" \
    -v api_key_cipher="${API_KEY_CIPHER}" \
    -v api_secret_cipher="${API_SECRET_CIPHER}" \
    -v api_key_mask="${API_KEY_MASK}" <<'SQL'
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
    'binance_live_smoke',
    'Binance live smoke credential. Inserted by run_binance_live_order_smoke.sh.',
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
    last_check_code    = 'binance_live_smoke',
    last_check_message = EXCLUDED.last_check_message,
    updated_at         = NOW();

SELECT
    id,
    buyer_email,
    exchange,
    permission_scope,
    status,
    last_check_code
FROM user_api_credentials
WHERE buyer_email = :'buyer_email'
  AND exchange = '币安';
SQL

echo "凭证插入/更新完成。"

# ---------------------------------------------------------------------------
# Step 4: 运行 execution worker（实盘模式）
# ---------------------------------------------------------------------------
echo
echo "=== Step 4: 运行 execution worker（EXECUTION_WORKER_DRY_RUN=false）==="
echo "  EXECUTION_WORKER_LIVE_ORDER_CONFIRM = I_UNDERSTAND_LIVE_ORDERS"
echo "  EXECUTION_WORKER_DEFAULT_EXCHANGE   = binance"
echo "  BINANCE_PROXY_URL                   = ${BINANCE_PROXY_URL}"
echo "  task_id                             = ${NEW_TASK_ID}"
echo
echo "  *** 注意：此步骤将向 Binance 实盘发送真实订单 ***"
echo

BASE_ORDER_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM exchange_order_results WHERE execution_task_id = ${NEW_TASK_ID};")"

export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export EXECUTION_WORKER_DRY_RUN=false
export EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS
export EXECUTION_WORKER_DEFAULT_EXCHANGE=binance
export EXECUTION_WORKER_RUN_ONCE=true
export EXECUTION_WORKER_ONLY=true
export EXECUTION_WORKER_LEASE_LIMIT=1
export EXECUTION_WORKER_TASK_TYPES=risk_control_close_candidate
export EXECUTION_WORKER_TASK_STATUSES=pending_close
export RUSTUP_TOOLCHAIN
export BINANCE_PROXY_URL
export IS_RUN_EXECUTION_WORKER=true
export IS_BACK_TEST=false
export IS_OPEN_SOCKET=false
export IS_RUN_REAL_STRATEGY=false
export IS_RUN_SYNC_DATA_JOB=false
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

if command -v rustup >/dev/null 2>&1; then
    _RUSTC="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc 2>/dev/null || true)"
    if [[ -n "${_RUSTC}" ]]; then
        export RUSTC="${_RUSTC}"
    fi
fi

cd "${REPO_ROOT}"

_TARGET_BINARY="${REPO_ROOT}/target/debug/rust_quant"
if [[ "${EXECUTION_WORKER_USE_EXISTING_BINARY}" =~ ^(true|TRUE|1|yes|YES)$ ]] ||
   { [[ "${EXECUTION_WORKER_USE_EXISTING_BINARY}" =~ ^(auto|AUTO)$ ]] && [[ -x "${_TARGET_BINARY}" ]]; }; then
    if [[ ! -x "${_TARGET_BINARY}" ]]; then
        echo "ERROR: target/debug/rust_quant is not executable; build it first." >&2
        exit 2
    fi
    echo "  Using existing binary: ${_TARGET_BINARY}"
    "${_TARGET_BINARY}" "$@" || {
        echo "WARNING: execution worker 退出码非零，继续验证结果..." >&2
    }
elif command -v rustup >/dev/null 2>&1; then
    rustup run "${RUSTUP_TOOLCHAIN}" cargo run --bin rust_quant "$@" || {
        echo "WARNING: execution worker 退出码非零，继续验证结果..." >&2
    }
else
    cargo run --bin rust_quant "$@" || {
        echo "WARNING: execution worker 退出码非零，继续验证结果..." >&2
    }
fi

# ---------------------------------------------------------------------------
# Step 5: 验证 exchange_order_results
# ---------------------------------------------------------------------------
echo
echo "=== Step 5: 验证 exchange_order_results ==="

ORDER_RESULT_ROW="$(
    run_web_sql -Atc "
        SELECT
            o.id,
            o.order_status,
            o.order_side,
            o.exchange,
            o.external_order_id,
            t.task_status
        FROM exchange_order_results o
        JOIN execution_tasks t ON t.id = o.execution_task_id
        WHERE o.execution_task_id = ${NEW_TASK_ID}
          AND o.id > ${BASE_ORDER_ID}
        ORDER BY o.id DESC
        LIMIT 1;
    " 2>/dev/null || true
)"

if [[ -z "${ORDER_RESULT_ROW}" ]]; then
    echo "FAIL: exchange_order_results 中未找到 task_id=${NEW_TASK_ID} 的新记录。" >&2
    echo "  可能原因：" >&2
    echo "    - execution worker 未成功处理该任务" >&2
    echo "    - Binance API 调用失败（检查 API key 权限、代理连通性）" >&2
    echo "    - 任务已被其他 worker 处理" >&2
    echo >&2
    echo "  诊断查询：" >&2
    run_web_sql -P pager=off -c "
        SELECT
            t.id AS task_id,
            t.task_type,
            t.task_status,
            t.updated_at,
            o.id AS order_result_id,
            o.order_status,
            o.order_side,
            o.exchange
        FROM execution_tasks t
        LEFT JOIN exchange_order_results o ON o.execution_task_id = t.id
        WHERE t.id = ${NEW_TASK_ID}
        ORDER BY o.id DESC NULLS LAST;
    " >&2 || true
    exit 1
fi

IFS='|' read -r ORDER_ID ORDER_STATUS ORDER_SIDE EXCHANGE EXTERNAL_ORDER_ID TASK_STATUS <<<"${ORDER_RESULT_ROW}"

echo "  order_result_id   = ${ORDER_ID}"
echo "  order_status      = ${ORDER_STATUS}"
echo "  order_side        = ${ORDER_SIDE}"
echo "  exchange          = ${EXCHANGE}"
echo "  external_order_id = ${EXTERNAL_ORDER_ID}"
echo "  task_status       = ${TASK_STATUS}"

if [[ "${ORDER_STATUS}" == "dry_run" ]]; then
    echo >&2
    echo "FAIL: order_status = 'dry_run'，实盘下单未生效。" >&2
    echo "  请检查：" >&2
    echo "    - EXECUTION_WORKER_DRY_RUN 是否被覆盖为 true" >&2
    echo "    - EXECUTION_WORKER_LIVE_ORDER_CONFIRM 是否正确设置" >&2
    exit 1
fi

echo
echo "=== Binance 实盘下单 smoke 验证通过 ==="
echo "  task_id=${NEW_TASK_ID} order_result_id=${ORDER_ID} order_status=${ORDER_STATUS} exchange=${EXCHANGE}"
echo "  external_order_id=${EXTERNAL_ORDER_ID}"
echo

# 打印完整结果行
run_web_sql -P pager=off -c "
    SELECT
        o.id AS order_result_id,
        o.execution_task_id,
        o.buyer_email,
        o.exchange,
        o.external_order_id,
        o.order_side,
        o.order_status,
        o.filled_qty,
        o.filled_quote,
        o.fee_amount,
        o.created_at
    FROM exchange_order_results o
    WHERE o.id = ${ORDER_ID};
"
