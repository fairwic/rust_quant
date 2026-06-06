#!/usr/bin/env bash
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
MARKET_VELOCITY_LIVE_COMBO_ID="${MARKET_VELOCITY_LIVE_COMBO_ID:-85}"
MARKET_VELOCITY_LIVE_TARGET_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"
MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE="${MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE:-false}"
MARKET_VELOCITY_POST_TRADE_INCLUDE_FILLS="${MARKET_VELOCITY_POST_TRADE_INCLUDE_FILLS:-true}"
EXECUTION_WORKER_USE_EXISTING_BINARY="${EXECUTION_WORKER_USE_EXISTING_BINARY:-auto}"
RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

blocker() {
    local code="$1"
    local detail="${2:-}"
    if [[ -n "${detail}" ]]; then
        echo "blocker=${code} detail=${detail}" >&2
    else
        echo "blocker=${code}" >&2
    fi
}

require_numeric_id() {
    local label="$1"
    local value="$2"
    if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
        blocker "${label}_invalid" "${value}"
        exit 2
    fi
}

require_optional_positive_integer() {
    local label="$1"
    local value="${2:-}"
    if [[ -n "${value}" && ! "${value}" =~ ^[1-9][0-9]*$ ]]; then
        blocker "${label}_invalid" "${value}"
        exit 2
    fi
}

require_okx_symbol() {
    local value="$1"
    if [[ ! "${value}" =~ ^[A-Z0-9]+(-[A-Z0-9]+)+$ ]]; then
        blocker "okx_symbol_invalid" "${value:-missing}"
        exit 2
    fi
}

is_enabled() {
    case "${1:-}" in
        true | TRUE | 1 | yes | YES)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

positive_decimal_text() {
    local value="$1"
    [[ "${value}" =~ ^[0-9]+([.][0-9]+)?$ ]] && [[ ! "${value}" =~ ^0+([.]0+)?$ ]]
}

shell_env_assign() {
    local key="$1"
    local value="$2"
    printf "%s=%q" "${key}" "${value}"
}

query_web() {
    local sql="$1"
    podman exec -i "${POSTGRES_CONTAINER}" psql \
        -U "${POSTGRES_USER}" \
        -d "${WEB_POSTGRES_DB}" \
        -XAt \
        -F $'\t' \
        -c "${sql}"
}

close_fill_writeback_apply_enabled() {
    is_enabled "${RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY:-false}"
}

require_close_fill_writeback_route_ready() {
    if ! close_fill_writeback_apply_enabled; then
        return 0
    fi

    local web_base_url="${RUST_QUAN_WEB_BASE_URL:-${QUANT_WEB_BASE_URL:-}}"
    if [[ -z "${web_base_url}" ]]; then
        blocker "close_fill_writeback_web_base_url_missing"
        return 2
    fi
    if ! command -v curl >/dev/null 2>&1; then
        blocker "curl_missing_for_close_fill_writeback_route_check"
        return 2
    fi

    local route_url="${web_base_url%/}/api/commerce/internal/exchange-close-fill-writeback"
    local http_code
    http_code="$(
        curl -sS -m 5 \
            -o /dev/null \
            -w '%{http_code}' \
            -X POST \
            -H 'content-type: application/json' \
            -H "x-alpha-execution-secret: ${EXECUTION_EVENT_SECRET:-${RUST_QUAN_WEB_INTERNAL_SECRET:-}}" \
            --data '{}' \
            "${route_url}" 2>/dev/null || true
    )"

    case "${http_code}" in
        404 | 000 | "")
            blocker "close_fill_writeback_route_missing" "url=${route_url},http_code=${http_code:-missing}"
            return 2
            ;;
    esac

    echo "close_fill_writeback_route=available url=${route_url} probe_status=${http_code}"
}

run_reconciliation_snapshot_once() {
    cd "${REPO_ROOT}"
    local target_binary="${REPO_ROOT}/target/debug/rust_quant"
    local snapshot_status=0
    case "${EXECUTION_WORKER_USE_EXISTING_BINARY}" in
        true | TRUE | 1 | yes | YES)
            if [[ ! -x "${target_binary}" ]]; then
                echo "target/debug/rust_quant is not executable; build it first or set EXECUTION_WORKER_USE_EXISTING_BINARY=auto/false." >&2
                return 2
            fi
            env IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=true \
                IS_RUN_EXECUTION_WORKER=false \
                IS_RUN_REAL_STRATEGY=false \
                IS_RUN_SYNC_DATA_JOB=false \
                IS_OPEN_SOCKET=false \
                "${target_binary}" || snapshot_status=$?
            return "${snapshot_status}"
            ;;
        auto | AUTO)
            if [[ -x "${target_binary}" ]]; then
                env IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=true \
                    IS_RUN_EXECUTION_WORKER=false \
                    IS_RUN_REAL_STRATEGY=false \
                    IS_RUN_SYNC_DATA_JOB=false \
                    IS_OPEN_SOCKET=false \
                    "${target_binary}" || snapshot_status=$?
                return "${snapshot_status}"
            fi
            ;;
    esac

    if command -v rustup >/dev/null 2>&1; then
        env IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=true \
            IS_RUN_EXECUTION_WORKER=false \
            IS_RUN_REAL_STRATEGY=false \
            IS_RUN_SYNC_DATA_JOB=false \
            IS_OPEN_SOCKET=false \
            rustup run "${RUSTUP_TOOLCHAIN}" cargo run -p rust-quant-cli --bin rust_quant || snapshot_status=$?
        return "${snapshot_status}"
    fi
    env IS_RUN_RECONCILIATION_SNAPSHOT_CHECK=true \
        IS_RUN_EXECUTION_WORKER=false \
        IS_RUN_REAL_STRATEGY=false \
        IS_RUN_SYNC_DATA_JOB=false \
        IS_OPEN_SOCKET=false \
        cargo run -p rust-quant-cli --bin rust_quant || snapshot_status=$?
    return "${snapshot_status}"
}

print_signed_snapshot_handoff() {
    local requirements
    requirements="$(shell_env_assign MARKET_VELOCITY_LIVE_COMBO_ID "${MARKET_VELOCITY_LIVE_COMBO_ID}")"
    requirements+=" $(shell_env_assign MARKET_VELOCITY_LIVE_TARGET_TASK_ID "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}")"
    requirements+=" $(shell_env_assign MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}")"
    requirements+=" $(shell_env_assign MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE true)"
    if [[ -n "${OKX_REQUEST_EXPIRATION_MS:-}" ]]; then
        requirements="$(shell_env_assign OKX_REQUEST_EXPIRATION_MS "${OKX_REQUEST_EXPIRATION_MS}") ${requirements}"
    fi

    echo "signed_snapshot_recheck=available"
    echo "signed_snapshot_recheck_scope=task_id:${task_id} symbol:${symbol} exchange:okx report:false include_fills:${MARKET_VELOCITY_POST_TRADE_INCLUDE_FILLS} mutation_allowed:false"
    echo "signed_snapshot_recheck_requirements=${requirements} bash scripts/dev/inspect_market_velocity_okx_post_trade.sh"
    echo "signed_snapshot_recheck_secret_required=RUST_QUAN_WEB_BASE_URL_and_EXECUTION_EVENT_SECRET_or_RUST_QUAN_WEB_INTERNAL_SECRET"
    local writeback_requirements
    writeback_requirements="${requirements}"
    writeback_requirements+=" $(shell_env_assign RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_APPLY true)"
    writeback_requirements+=" $(shell_env_assign RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_CONFIRM I_UNDERSTAND_THIS_WRITES_EXCHANGE_CLOSE_FILL_TO_WEB)"
    writeback_requirements+=" $(shell_env_assign RECONCILIATION_SNAPSHOT_CLOSE_FILL_WRITEBACK_INTENT "web-close-fill:combo=${combo_id}:task=${task_id}:symbol=${symbol}")"
    echo "close_fill_writeback_apply=available_after_signed_flat_candidate"
    echo "close_fill_writeback_apply_scope=task_id:${task_id} symbol:${symbol} exchange:${exchange} web_writeback_only:true exchange_mutation_allowed:false"
    echo "close_fill_writeback_apply_requirements=${writeback_requirements} bash scripts/dev/inspect_market_velocity_okx_post_trade.sh"
    echo "close_fill_writeback_apply_secret_required=RUST_QUAN_WEB_BASE_URL_and_EXECUTION_EVENT_SECRET_or_RUST_QUAN_WEB_INTERNAL_SECRET"
}

run_signed_snapshot_recheck() {
    if [[ -z "${RUST_QUAN_WEB_BASE_URL:-}${QUANT_WEB_BASE_URL:-}" ]]; then
        blocker "signed_snapshot_web_base_url_missing"
        return 2
    fi
    if [[ -z "${EXECUTION_EVENT_SECRET:-}${RUST_QUAN_WEB_INTERNAL_SECRET:-}" ]]; then
        blocker "signed_snapshot_internal_secret_missing"
        return 2
    fi
    require_close_fill_writeback_route_ready || return $?

    local reconciliation_row
    reconciliation_row="$(
        query_web "
-- signed_reconciliation_task_context
SELECT
  et.buyer_email,
  et.symbol,
  et.combo_id,
  COALESCE(et.request_payload_json::jsonb #>> '{api_credential_id}', '')
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  AND et.combo_id = ${MARKET_VELOCITY_LIVE_COMBO_ID};
"
    )"
    if [[ -z "${reconciliation_row}" ]]; then
        blocker "signed_snapshot_task_context_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi

    local reconciliation_buyer_email reconciliation_symbol reconciliation_combo_id reconciliation_credential_id
    IFS=$'\t' read -r reconciliation_buyer_email reconciliation_symbol reconciliation_combo_id reconciliation_credential_id <<<"${reconciliation_row}"
    if [[ -z "${reconciliation_buyer_email}" || -z "${reconciliation_symbol}" || ! "${reconciliation_combo_id}" =~ ^[0-9]+$ ]]; then
        blocker "signed_snapshot_task_context_invalid" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi
    require_okx_symbol "${reconciliation_symbol}"

    echo "signed_snapshot_recheck=okx_signed_readonly_reconciliation_snapshot"
    echo "signed_snapshot_recheck_scope=task_id:${MARKET_VELOCITY_LIVE_TARGET_TASK_ID} symbol:${reconciliation_symbol} exchange:okx report:false include_fills:${MARKET_VELOCITY_POST_TRADE_INCLUDE_FILLS} mutation_allowed:false"

    export RECONCILIATION_SNAPSHOT_CONFIRM=I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION
    export RECONCILIATION_SNAPSHOT_BUYER_EMAIL="${reconciliation_buyer_email}"
    export RECONCILIATION_SNAPSHOT_EXCHANGE=okx
    export RECONCILIATION_SNAPSHOT_SYMBOL="${reconciliation_symbol}"
    export RECONCILIATION_SNAPSHOT_COMBO_ID="${reconciliation_combo_id}"
    export RECONCILIATION_SNAPSHOT_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
    export RECONCILIATION_SNAPSHOT_REPORT=false
    export RECONCILIATION_SNAPSHOT_INCLUDE_FILLS="${MARKET_VELOCITY_POST_TRADE_INCLUDE_FILLS}"
    if [[ -n "${reconciliation_credential_id}" ]]; then
        export RECONCILIATION_SNAPSHOT_CREDENTIAL_REF="web_api_credential_id_${reconciliation_credential_id}"
    else
        unset RECONCILIATION_SNAPSHOT_CREDENTIAL_REF
    fi

    run_reconciliation_snapshot_once
}

echo "== Market Velocity OKX post-trade monitor =="
echo "combo_id=${MARKET_VELOCITY_LIVE_COMBO_ID}"
echo "target_task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-missing}"
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "mode=read_only"
echo "mutation_allowed=false"

require_numeric_id "combo_id" "${MARKET_VELOCITY_LIVE_COMBO_ID}"
require_numeric_id "target_task_id" "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
require_optional_positive_integer "okx_request_expiration_ms" "${OKX_REQUEST_EXPIRATION_MS:-}"

task_row="$(
    query_web "
-- post_trade_task_summary
SELECT
  et.id,
  et.combo_id,
  et.task_type,
  et.task_status,
  et.symbol,
  COALESCE(et.request_payload_json::jsonb #>> '{execution,exchange}', et.request_payload_json::jsonb #>> '{exchange}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{source_signal_type}', s.payload_json::jsonb #>> '{source_signal_type}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{execution,size_usdt}', et.request_payload_json::jsonb #>> '{size_usdt}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,selected_stop_loss_price}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,direction}', et.request_payload_json::jsonb #>> '{direction}', et.request_payload_json::jsonb #>> '{position_side}', et.request_payload_json::jsonb #>> '{side}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,entry_price}', et.request_payload_json::jsonb #>> '{risk_plan,entry_reference_price}', et.request_payload_json::jsonb #>> '{entry_price}', et.request_payload_json::jsonb #>> '{current_price}', ''),
  et.updated_at
FROM execution_tasks et
LEFT JOIN strategy_signal_inbox s ON s.id = et.strategy_signal_id
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  AND et.combo_id = ${MARKET_VELOCITY_LIVE_COMBO_ID};
"
)"
if [[ -z "${task_row}" ]]; then
    blocker "post_trade_task_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
    exit 2
fi

IFS=$'\t' read -r task_id combo_id task_type task_status symbol exchange source_signal_type size_usdt stop_loss direction entry_price task_updated_at <<<"${task_row}"
require_okx_symbol "${symbol}"
echo "post_trade_task=task_id:${task_id} combo_id:${combo_id} status:${task_status} symbol:${symbol} exchange:${exchange} source_signal_type:${source_signal_type} size_usdt:${size_usdt:-missing} stop_loss:${stop_loss:-missing} direction:${direction:-missing} entry_price:${entry_price:-missing} updated_at:${task_updated_at}"

if [[ "${combo_id}" != "${MARKET_VELOCITY_LIVE_COMBO_ID}" ||
    "${task_type}" != "execute_signal" ||
    "${exchange,,}" != "okx" ||
    "${source_signal_type}" != "market_velocity" ]]; then
    blocker "post_trade_task_scope_invalid" "combo_id=${combo_id:-missing},task_type=${task_type:-missing},exchange=${exchange:-missing},source_signal_type=${source_signal_type:-missing}"
    exit 2
fi
if [[ "${symbol^^}" == LINKUSDT* || "${symbol^^}" == LINK-USDT* ]]; then
    blocker "post_trade_link_task_forbidden" "${symbol}"
    exit 2
fi
case "${task_status}" in
    completed | pending_protection_sync)
        ;;
    *)
        blocker "post_trade_task_not_completed" "task_status=${task_status:-missing}"
        exit 2
        ;;
esac

order_row="$(
    query_web "
-- post_trade_order_summary
SELECT
  eor.id,
  eor.exchange,
  eor.external_order_id,
  eor.order_side,
  eor.order_status,
  COALESCE(eor.filled_qty::text, ''),
  COALESCE(eor.filled_quote::text, ''),
  COALESCE(eor.fee_amount::text, ''),
  COALESCE(eor.raw_payload_json::jsonb #>> '{protection_sync,protective_order_confirmed}', ''),
  COALESCE(eor.raw_payload_json::jsonb #>> '{protection_sync,protective_order_external_id}', ''),
  COALESCE(eor.raw_payload_json::jsonb #>> '{protection_sync,protective_order_mode}', ''),
  COALESCE(eor.raw_payload_json::jsonb #>> '{order_detail,attachAlgoOrds,0,slTriggerPx}', eor.raw_payload_json::jsonb #>> '{protection_sync,selected_stop_loss_price}', ''),
  eor.updated_at
FROM exchange_order_results eor
WHERE eor.execution_task_id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
ORDER BY eor.id DESC
LIMIT 1;
"
)"
if [[ -z "${order_row}" ]]; then
    blocker "post_trade_order_result_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
    exit 2
fi

IFS=$'\t' read -r order_result_id order_exchange external_order_id order_side order_status filled_qty filled_quote fee_amount protection_confirmed protection_external_id protection_mode protection_stop_loss order_updated_at <<<"${order_row}"
echo "post_trade_order=order_result_id:${order_result_id} exchange:${order_exchange} side:${order_side} status:${order_status} external_order_id:${external_order_id:-missing} filled_qty:${filled_qty:-missing} filled_quote:${filled_quote:-missing} fee:${fee_amount:-missing} updated_at:${order_updated_at}"

if [[ "${order_exchange,,}" != "okx" || "${order_status,,}" != "filled" || -z "${external_order_id}" ]]; then
    blocker "post_trade_order_not_filled" "exchange=${order_exchange:-missing},status=${order_status:-missing},external_order_id=${external_order_id:-missing}"
    exit 2
fi
if ! positive_decimal_text "${filled_qty}"; then
    blocker "post_trade_filled_qty_invalid" "${filled_qty:-missing}"
    exit 2
fi
if [[ "${protection_confirmed,,}" != "true" || -z "${protection_external_id}" ]]; then
    blocker "post_trade_protection_not_confirmed" "confirmed=${protection_confirmed:-missing},external_id=${protection_external_id:-missing}"
    exit 2
fi
echo "post_trade_protection=status:confirmed external_id:${protection_external_id} mode:${protection_mode:-missing} stop_loss:${protection_stop_loss:-missing}"

attempt_row="$(
    query_web "
-- post_trade_attempt_summary
SELECT
  eta.id,
  eta.attempt_no,
  eta.attempt_status,
  COALESCE(eta.executor, ''),
  COALESCE(NULLIF(eta.error_message, ''), 'none'),
  eta.updated_at
FROM execution_task_attempts eta
WHERE eta.execution_task_id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
ORDER BY eta.id DESC
LIMIT 1;
"
)"
if [[ -z "${attempt_row}" ]]; then
    blocker "post_trade_attempt_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
    exit 2
fi

IFS=$'\t' read -r attempt_id attempt_no attempt_status executor error_message attempt_updated_at <<<"${attempt_row}"
echo "post_trade_attempt=attempt_id:${attempt_id} attempt_no:${attempt_no} status:${attempt_status} executor:${executor:-missing} error:${error_message:-none} updated_at:${attempt_updated_at}"
if [[ "${attempt_status}" != "completed" ]]; then
    blocker "post_trade_attempt_not_completed" "attempt_status=${attempt_status:-missing}"
    exit 2
fi

echo "live_close_requires_separate_authorization=true"
echo "close_authorization_scope=task_id:${task_id} symbol:${symbol} filled_qty:${filled_qty} max_notional:${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "close_authorization_note=do_not_close_cancel_or_adjust_without_explicit_user_authorization"
print_signed_snapshot_handoff
if is_enabled "${MARKET_VELOCITY_POST_TRADE_SIGNED_RECONCILE}"; then
    run_signed_snapshot_recheck
fi
