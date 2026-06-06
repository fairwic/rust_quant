#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
RUST_QUAN_WEB_BASE_URL="${RUST_QUAN_WEB_BASE_URL:-http://127.0.0.1:8000}"
EXECUTION_EVENT_SECRET="${EXECUTION_EVENT_SECRET:-local-dev-secret}"
MARKET_VELOCITY_LIVE_COMBO_ID="${MARKET_VELOCITY_LIVE_COMBO_ID:-85}"
MARKET_VELOCITY_LIVE_TARGET_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"
MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY="${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY:-false}"
MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM="${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM:-}"
MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT="${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT:-}"
READINESS_RECOVERY_CONFIRM_PHRASE="I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS"
READINESS_RECOVERY_REQUIRED_INTENT=""
RECOVERY_TARGET_STATUS=""
RECOVERY_TARGET_LEASE_OWNER=""
RECOVERY_TARGET_RISK_CONTEXT_EXPIRES_AT=""
RECOVERY_TARGET_SYMBOL=""
RECOVERY_REASON=""
WORKER_LIVE_CONFIRM_PHRASE="I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER"
OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX=""
if [[ -n "${OKX_REQUEST_EXPIRATION_MS:-}" ]]; then
    OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX="OKX_REQUEST_EXPIRATION_MS=${OKX_REQUEST_EXPIRATION_MS} "
fi

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
    if [[ -z "${value}" ]]; then
        return
    fi
    if [[ ! "${value}" =~ ^[0-9]+$ || "${value}" =~ ^0+$ ]]; then
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

shell_env_assign() {
    local key="$1"
    local value="$2"
    local quoted_value
    printf -v quoted_value '%q' "${value}"
    printf '%s=%s' "${key}" "${quoted_value}"
}

common_handoff_scope() {
    printf '%s %s %s %s' \
        "$(shell_env_assign RUST_QUAN_WEB_BASE_URL "${RUST_QUAN_WEB_BASE_URL}")" \
        "$(shell_env_assign MARKET_VELOCITY_LIVE_COMBO_ID "${MARKET_VELOCITY_LIVE_COMBO_ID}")" \
        "$(shell_env_assign MARKET_VELOCITY_LIVE_TARGET_TASK_ID "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}")" \
        "$(shell_env_assign MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}")"
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

run_live_preflight_capture() {
    local output_var="$1"
    local status_var="$2"
    local output=""
    local status=0

    if output="$("${SCRIPT_DIR}/run_market_velocity_okx_live_preflight.sh" 2>&1)"; then
        status=0
    else
        status=$?
    fi

    printf -v "${output_var}" '%s' "${output}"
    printf -v "${status_var}" '%s' "${status}"
}

print_next_worker_handoff() {
    local worker_live_required_intent="okx:task=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}:symbol=${RECOVERY_TARGET_SYMBOL}:max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
    local handoff_scope
    handoff_scope="$(common_handoff_scope)"
    echo "next_worker_dry_run=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}${handoff_scope} scripts/dev/run_market_velocity_okx_scoped_live_worker.sh"
    echo "next_worker_live_apply_requirements=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}${handoff_scope} $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_APPLY true) $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_CONFIRM "${WORKER_LIVE_CONFIRM_PHRASE}") $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_INTENT "${worker_live_required_intent}")"
}

validate_recovery_target_task() {
    local state_row
    state_row="$(
        query_web "
-- readiness_recovery_target_task
SELECT
  et.id,
  et.combo_id,
  et.task_type,
  et.task_status,
  et.symbol,
  COALESCE(et.lease_owner, ''),
  COALESCE(et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}', '')
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID};
"
    )"
    if [[ -z "${state_row}" ]]; then
        blocker "readiness_recovery_target_task_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        exit 2
    fi

    local task_id combo_id task_type task_status symbol lease_owner risk_context_expires_at
    local field_delimiter=$'\037'
    local state_row_delimited="${state_row//$'\t'/${field_delimiter}}"
    IFS="${field_delimiter}" read -r task_id combo_id task_type task_status symbol lease_owner risk_context_expires_at <<<"${state_row_delimited}"
    if [[ "${combo_id}" != "${MARKET_VELOCITY_LIVE_COMBO_ID}" ||
        "${task_type}" != "execute_signal" ||
        -z "${symbol}" ]]; then
        blocker "readiness_recovery_target_task_invalid" "combo_id=${combo_id:-missing},task_type=${task_type:-missing},task_status=${task_status:-missing},symbol=${symbol:-missing}"
        exit 2
    fi
    require_okx_symbol "${symbol}"
    RECOVERY_TARGET_STATUS="${task_status}"
    RECOVERY_TARGET_LEASE_OWNER="${lease_owner}"
    RECOVERY_TARGET_RISK_CONTEXT_EXPIRES_AT="${risk_context_expires_at}"
    RECOVERY_TARGET_SYMBOL="${symbol}"
    echo "readiness_recovery_target=task_id:${task_id} combo_id:${combo_id} status:${task_status} type:${task_type} symbol:${symbol} lease_owner:${lease_owner:-none} risk_context_expires_at:${risk_context_expires_at:-missing}"
    case "${task_status}" in
    pending | leased)
        ;;
    blocked)
        if [[ "${lease_owner}" != "lease_time_risk_snapshot_stale" ]]; then
            blocker "readiness_recovery_blocked_task_not_stale_risk_context" "lease_owner=${lease_owner:-missing}"
            exit 2
        fi
        ;;
    *)
        blocker "readiness_recovery_target_task_invalid" "combo_id=${combo_id:-missing},task_type=${task_type:-missing},task_status=${task_status:-missing},symbol=${symbol:-missing}"
        exit 2
        ;;
    esac
    if [[ "${symbol^^}" == LINKUSDT* || "${symbol^^}" == LINK-USDT* ]]; then
        blocker "readiness_recovery_link_task_forbidden" "${symbol}"
        exit 2
    fi
}

assert_recovery_target_pending_for_worker_handoff() {
    if [[ "${RECOVERY_TARGET_STATUS}" != "pending" || -n "${RECOVERY_TARGET_LEASE_OWNER}" ]]; then
        blocker "readiness_recovery_target_not_pending_for_worker_handoff" "task_status=${RECOVERY_TARGET_STATUS:-missing},lease_owner=${RECOVERY_TARGET_LEASE_OWNER:-none}"
        exit 2
    fi
}

lookup_recoverable_okx_credential_id() {
    query_web "
SELECT u.id
FROM user_api_credentials u
JOIN strategy_combo_subscriptions c
  ON c.buyer_email = u.buyer_email
WHERE c.id = ${MARKET_VELOCITY_LIVE_COMBO_ID}
  AND lower(u.exchange) = 'okx'
  AND (
    (
      u.status = 'active'
      AND u.last_check_code IN ('signed_exchange_preflight_passed','signed_exchange_check_passed')
    )
    OR (
      u.status = 'error'
      AND u.last_check_code = 'okx_preflight_network_error'
    )
  )
  AND u.api_key_cipher LIKE 'v4:local_aes256gcm:%'
  AND u.api_secret_cipher LIKE 'v4:local_aes256gcm:%'
  AND (
    u.passphrase_cipher IS NULL
    OR BTRIM(u.passphrase_cipher) = ''
    OR u.passphrase_cipher LIKE 'v4:local_aes256gcm:%'
  )
ORDER BY
  CASE
    WHEN u.status = 'active'
      AND u.last_check_code IN ('signed_exchange_preflight_passed','signed_exchange_check_passed')
      THEN 0
    ELSE 1
  END,
  u.updated_at DESC,
  u.id DESC
LIMIT 1;
"
}

refresh_expired_risk_context() {
    if ! command -v curl >/dev/null 2>&1; then
        blocker "curl_missing_for_readiness_recovery"
        exit 2
    fi

    echo "readiness_refresh=${RECOVERY_REASON:-expired_risk_context}"
    if [[ "${RECOVERY_TARGET_STATUS}" == "blocked" && "${RECOVERY_TARGET_LEASE_OWNER}" == "lease_time_risk_snapshot_stale" ]]; then
        echo "readiness_recovery_lease=skipped_already_blocked_stale_task"
    else
        curl -fsS -m 10 \
            -H "x-alpha-execution-secret: ${EXECUTION_EVENT_SECRET}" \
            "${RUST_QUAN_WEB_BASE_URL%/}/api/commerce/internal/execution-tasks/lease?limit=1&task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}&task_type=execute_signal&task_status=pending" >/dev/null
        echo "readiness_recovery_lease=ok"
    fi

    local credential_id
    credential_id="$(lookup_recoverable_okx_credential_id)"
    if [[ ! "${credential_id}" =~ ^[0-9]+$ ]]; then
        blocker "okx_credential_not_ready_for_readiness_recovery"
        exit 2
    fi

    curl -fsS -m 20 \
        -X POST \
        -H "content-type: application/json" \
        -H "x-alpha-execution-secret: ${EXECUTION_EVENT_SECRET}" \
        --data '{}' \
        "${RUST_QUAN_WEB_BASE_URL%/}/api/commerce/internal/api-credentials/${credential_id}/check" >/dev/null

    local refreshed_credential_row
    refreshed_credential_row="$(
        query_web "
-- readiness_recovery_credential_status
SELECT
  COALESCE(status, ''),
  COALESCE(last_check_code, '')
FROM user_api_credentials
WHERE id = ${credential_id};
"
    )"
    if [[ -z "${refreshed_credential_row}" ]]; then
        blocker "readiness_recovery_credential_missing"
        exit 2
    fi

    local refreshed_credential_status refreshed_credential_check_code
    IFS=$'\t' read -r refreshed_credential_status refreshed_credential_check_code <<<"${refreshed_credential_row}"
    if [[ "${refreshed_credential_status}" != "active" ]] ||
        [[ ! "${refreshed_credential_check_code}" =~ ^(signed_exchange_preflight_passed|signed_exchange_check_passed)$ ]]; then
        blocker "readiness_recovery_credential_not_ready" "status=${refreshed_credential_status:-missing},last_check_code=${refreshed_credential_check_code:-missing}"
        exit 2
    fi

    echo "readiness_recovery_credential_check=ready"
}

echo "== Market Velocity OKX live readiness recovery =="
echo "combo_id=${MARKET_VELOCITY_LIVE_COMBO_ID}"
echo "target_task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-missing}"
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
if [[ "${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY}" == "true" ]]; then
    echo "mode=apply"
else
    echo "mode=dry_run"
fi

require_optional_positive_integer "okx_request_expiration_ms" "${OKX_REQUEST_EXPIRATION_MS:-}"
require_numeric_id "combo_id" "${MARKET_VELOCITY_LIVE_COMBO_ID}"
require_numeric_id "target_task_id" "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"

READINESS_RECOVERY_REQUIRED_INTENT="okx-readiness:task=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}:max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"

preflight_output=""
preflight_status=0
run_live_preflight_capture preflight_output preflight_status
printf '%s\n' "${preflight_output}"

if [[ "${preflight_status}" == "0" ]]; then
    validate_recovery_target_task
    assert_recovery_target_pending_for_worker_handoff
    echo "recovery=not_needed"
    print_next_worker_handoff
    exit 0
fi

validate_recovery_target_task

if [[ "${preflight_output}" == *"task_risk_context_expired"* ]]; then
    RECOVERY_REASON="expired_risk_context"
elif [[ "${RECOVERY_TARGET_STATUS}" == "blocked" && "${RECOVERY_TARGET_LEASE_OWNER}" == "lease_time_risk_snapshot_stale" ]]; then
    RECOVERY_REASON="blocked_stale_risk_context"
else
    echo "recovery=blocked reason=preflight_not_expired_risk_context status=${preflight_status}"
    exit "${preflight_status}"
fi
echo "recovery_reason=${RECOVERY_REASON}"

if [[ "${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY}" != "true" ]]; then
    echo
    echo "recovery=dry_run"
    echo "recovery_apply_requirements=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}$(common_handoff_scope) $(shell_env_assign MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY true) $(shell_env_assign MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM "${READINESS_RECOVERY_CONFIRM_PHRASE}") $(shell_env_assign MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT "${READINESS_RECOVERY_REQUIRED_INTENT}")"
    exit 0
fi

if [[ "${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM}" != "${READINESS_RECOVERY_CONFIRM_PHRASE}" ]]; then
    blocker "readiness_recovery_confirmation_missing" "set MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM=${READINESS_RECOVERY_CONFIRM_PHRASE}"
    exit 2
fi
if [[ -z "${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT}" ]]; then
    blocker "readiness_recovery_intent_missing" "set MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT=${READINESS_RECOVERY_REQUIRED_INTENT}"
    exit 2
fi
if [[ "${MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT}" != "${READINESS_RECOVERY_REQUIRED_INTENT}" ]]; then
    blocker "readiness_recovery_intent_mismatch" "expected=${READINESS_RECOVERY_REQUIRED_INTENT}"
    exit 2
fi
if [[ "${RECOVERY_TARGET_STATUS}" == "leased" ]]; then
    blocker "readiness_recovery_leased_task_not_safe_for_apply" "lease_owner=${RECOVERY_TARGET_LEASE_OWNER:-missing}"
    exit 2
fi

refresh_expired_risk_context

post_preflight_output=""
post_preflight_status=0
echo
echo "post_recovery_preflight=market_velocity_okx_live_preflight"
run_live_preflight_capture post_preflight_output post_preflight_status
printf '%s\n' "${post_preflight_output}"
echo "post_recovery_preflight_status=${post_preflight_status}"
if [[ "${post_preflight_status}" != "0" ]]; then
    echo "recovery=blocked reason=post_recovery_preflight_failed"
    exit "${post_preflight_status}"
fi

validate_recovery_target_task
assert_recovery_target_pending_for_worker_handoff
echo "recovery=applied"
print_next_worker_handoff
