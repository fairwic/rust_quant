#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
RUST_QUAN_WEB_BASE_URL="${RUST_QUAN_WEB_BASE_URL:-http://127.0.0.1:8000}"
EXECUTION_EVENT_SECRET="${EXECUTION_EVENT_SECRET:-local-dev-secret}"
MARKET_VELOCITY_LIVE_COMBO_ID="${MARKET_VELOCITY_LIVE_COMBO_ID:-85}"
MARKET_VELOCITY_LIVE_TARGET_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"
MARKET_VELOCITY_LIVE_REFRESH_READINESS="${MARKET_VELOCITY_LIVE_REFRESH_READINESS:-auto}"
MARKET_VELOCITY_LIVE_WORKER_APPLY="${MARKET_VELOCITY_LIVE_WORKER_APPLY:-false}"
MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY="${MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY:-false}"
MARKET_VELOCITY_LIVE_WORKER_CONFIRM="${MARKET_VELOCITY_LIVE_WORKER_CONFIRM:-}"
MARKET_VELOCITY_LIVE_WORKER_INTENT="${MARKET_VELOCITY_LIVE_WORKER_INTENT:-}"
MARKET_VELOCITY_LIVE_WORKER_CONFIRM_PHRASE="I_UNDERSTAND_THIS_RUNS_OKX_MARKET_VELOCITY_LIVE_WORKER"
READINESS_RECOVERY_CONFIRM_PHRASE="I_UNDERSTAND_THIS_REFRESHES_OKX_MARKET_VELOCITY_READINESS"
MARKET_VELOCITY_LIVE_REQUIRED_INTENT=""
MARKET_VELOCITY_LIVE_PRE_RUN_RECONCILE="${MARKET_VELOCITY_LIVE_PRE_RUN_RECONCILE:-true}"
MARKET_VELOCITY_LIVE_POST_RUN_RECONCILE="${MARKET_VELOCITY_LIVE_POST_RUN_RECONCILE:-true}"
MARKET_VELOCITY_LIVE_POST_RUN_INCLUDE_FILLS="${MARKET_VELOCITY_LIVE_POST_RUN_INCLUDE_FILLS:-false}"
RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.91.1}"
QUANT_CORE_DATABASE_URL="${QUANT_CORE_DATABASE_URL:-postgres://postgres:postgres123@localhost:5432/quant_core}"
QUANT_DATABASE_URL="${QUANT_DATABASE_URL:-${QUANT_CORE_DATABASE_URL}}"
SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
EXECUTION_WORKER_USE_EXISTING_BINARY="${EXECUTION_WORKER_USE_EXISTING_BINARY:-auto}"
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

is_enabled() {
    local value="$1"
    [[ "${value}" == "true" || "${value}" == "TRUE" || "${value}" == "1" || "${value}" == "yes" || "${value}" == "YES" ]]
}

readiness_refresh_enabled() {
    local value="${MARKET_VELOCITY_LIVE_REFRESH_READINESS}"
    case "${value}" in
        true | TRUE | 1 | yes | YES)
            return 0
            ;;
        false | FALSE | 0 | no | NO)
            return 1
            ;;
        auto | AUTO)
            [[ "${MARKET_VELOCITY_LIVE_WORKER_APPLY}" == "true" ]] || is_enabled "${MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY}"
            return $?
            ;;
        *)
            blocker "readiness_refresh_mode_invalid" "${value}"
            exit 2
            ;;
    esac
}

print_readiness_recovery_handoff() {
    local readiness_recovery_intent="okx-readiness:task=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}:max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
    local handoff_scope
    handoff_scope="$(common_handoff_scope)"
    echo "readiness_recovery_dry_run=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}${handoff_scope} scripts/dev/recover_market_velocity_okx_live_readiness.sh"
    echo "readiness_recovery_apply_requirements=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}${handoff_scope} $(shell_env_assign MARKET_VELOCITY_LIVE_READINESS_RECOVERY_APPLY true) $(shell_env_assign MARKET_VELOCITY_LIVE_READINESS_RECOVERY_CONFIRM "${READINESS_RECOVERY_CONFIRM_PHRASE}") $(shell_env_assign MARKET_VELOCITY_LIVE_READINESS_RECOVERY_INTENT "${readiness_recovery_intent}")"
}

refresh_expired_risk_context() {
    if ! command -v curl >/dev/null 2>&1; then
        blocker "curl_missing_for_readiness_refresh"
        exit 2
    fi

    echo "readiness_refresh=expired_risk_context"
    curl -fsS -m 10 \
        -H "x-alpha-execution-secret: ${EXECUTION_EVENT_SECRET}" \
        "${RUST_QUAN_WEB_BASE_URL%/}/api/commerce/internal/execution-tasks/lease?limit=1&task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}&task_type=execute_signal&task_status=pending" >/dev/null
    echo "readiness_refresh_lease=ok"

    local credential_id
    credential_id="$(
        query_web "
SELECT u.id
FROM user_api_credentials u
JOIN strategy_combo_subscriptions c
  ON c.buyer_email = u.buyer_email
WHERE c.id = ${MARKET_VELOCITY_LIVE_COMBO_ID}
  AND lower(u.exchange) = 'okx'
  AND u.status = 'active'
  AND u.last_check_code IN ('signed_exchange_preflight_passed','signed_exchange_check_passed')
  AND u.api_key_cipher LIKE 'v4:local_aes256gcm:%'
  AND u.api_secret_cipher LIKE 'v4:local_aes256gcm:%'
  AND (
    u.passphrase_cipher IS NULL
    OR BTRIM(u.passphrase_cipher) = ''
    OR u.passphrase_cipher LIKE 'v4:local_aes256gcm:%'
  )
ORDER BY u.updated_at DESC, u.id DESC
LIMIT 1;
"
    )"
    if [[ ! "${credential_id}" =~ ^[0-9]+$ ]]; then
        blocker "okx_credential_not_ready_for_readiness_refresh"
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
-- readiness_refresh_credential_status
SELECT
  COALESCE(status, ''),
  COALESCE(last_check_code, '')
FROM user_api_credentials
WHERE id = ${credential_id};
"
    )"
    if [[ -z "${refreshed_credential_row}" ]]; then
        blocker "readiness_refresh_credential_missing"
        exit 2
    fi

    local refreshed_credential_status refreshed_credential_check_code
    IFS=$'\t' read -r refreshed_credential_status refreshed_credential_check_code <<<"${refreshed_credential_row}"
    if [[ "${refreshed_credential_status}" != "active" ]] ||
        [[ ! "${refreshed_credential_check_code}" =~ ^(signed_exchange_preflight_passed|signed_exchange_check_passed)$ ]]; then
        blocker "readiness_refresh_credential_not_ready" "status=${refreshed_credential_status:-missing},last_check_code=${refreshed_credential_check_code:-missing}"
        exit 2
    fi

    echo "readiness_refresh_credential_check=ready"
}

run_live_preflight_with_recovery() {
    local preflight_output=""
    local preflight_status=0

    if preflight_output="$("${SCRIPT_DIR}/run_market_velocity_okx_live_preflight.sh" 2>&1)"; then
        printf '%s\n' "${preflight_output}"
        return 0
    else
        preflight_status=$?
    fi

    printf '%s\n' "${preflight_output}" >&2

    if [[ "${preflight_output}" == *"task_risk_context_expired"* ]]; then
        if readiness_refresh_enabled; then
            refresh_expired_risk_context
            "${SCRIPT_DIR}/run_market_velocity_okx_live_preflight.sh"
            return $?
        fi
        print_readiness_recovery_handoff
        blocker "readiness_refresh_disabled" "mode=${MARKET_VELOCITY_LIVE_REFRESH_READINESS},reason=task_risk_context_expired"
    fi

    return "${preflight_status}"
}

run_worker_once() {
    cd "${REPO_ROOT}"
    local target_binary="${REPO_ROOT}/target/debug/rust_quant"
    local worker_status=0
    case "${EXECUTION_WORKER_USE_EXISTING_BINARY}" in
        true | TRUE | 1 | yes | YES)
            if [[ ! -x "${target_binary}" ]]; then
                echo "target/debug/rust_quant is not executable; build it first or set EXECUTION_WORKER_USE_EXISTING_BINARY=auto/false." >&2
                exit 2
            fi
            "${target_binary}" || worker_status=$?
            return "${worker_status}"
            ;;
        auto | AUTO)
            if [[ -x "${target_binary}" ]]; then
                "${target_binary}" || worker_status=$?
                return "${worker_status}"
            fi
            ;;
    esac

    if command -v rustup >/dev/null 2>&1; then
        rustup run "${RUSTUP_TOOLCHAIN}" cargo run -p rust-quant-cli --bin rust_quant || worker_status=$?
        return "${worker_status}"
    fi
    cargo run -p rust-quant-cli --bin rust_quant || worker_status=$?
    return "${worker_status}"
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

assert_post_run_target_handled() {
    local summary_row
    summary_row="$(
        query_web "
-- post_run_target_handled_summary
SELECT
  et.task_status,
  (
    SELECT COUNT(*)
    FROM exchange_order_results eor
    WHERE eor.execution_task_id = et.id
  ),
  (
    SELECT COUNT(*)
    FROM execution_task_attempts eta
    WHERE eta.execution_task_id = et.id
  )
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID};
"
    )"
    if [[ -z "${summary_row}" ]]; then
        blocker "post_run_target_task_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi

    local summary_task_status summary_order_result_count summary_attempt_count
    IFS=$'\t' read -r summary_task_status summary_order_result_count summary_attempt_count <<<"${summary_row}"
    echo "post_run_target_summary=status:${summary_task_status} order_results:${summary_order_result_count} attempts:${summary_attempt_count}"
    if [[ ! "${summary_order_result_count}" =~ ^[0-9]+$ || ! "${summary_attempt_count}" =~ ^[0-9]+$ ]]; then
        blocker "post_run_target_task_summary_invalid" "order_results=${summary_order_result_count:-missing},attempts=${summary_attempt_count:-missing}"
        return 2
    fi
    if [[ "${summary_order_result_count}" == "0" && "${summary_attempt_count}" == "0" ]]; then
        blocker "post_run_target_task_unhandled" "task_status=${summary_task_status:-missing},order_results=0,attempts=0"
        return 2
    fi
    case "${summary_task_status}" in
        completed | pending_protection_sync)
            ;;
        pending | leased)
            blocker "post_run_target_task_not_consumed" "task_status=${summary_task_status},order_results=${summary_order_result_count},attempts=${summary_attempt_count}"
            return 2
            ;;
        failed | blocked | cancelled | canceled | protective_order_failed | manual_review)
            blocker "post_run_target_task_failed" "task_status=${summary_task_status},order_results=${summary_order_result_count},attempts=${summary_attempt_count}"
            return 2
            ;;
        *)
            blocker "post_run_target_task_status_unexpected" "task_status=${summary_task_status:-missing},order_results=${summary_order_result_count},attempts=${summary_attempt_count}"
            return 2
            ;;
    esac
}

collect_post_run_evidence() {
    echo
    echo "post_run_evidence=web_task_order_result"
    query_web "
SELECT
  et.id,
  et.symbol,
  et.task_status,
  et.lease_owner,
  COALESCE(et.lease_until::text, ''),
  COUNT(eor.id),
  COALESCE(string_agg(eor.order_status || ':' || eor.order_side || ':' || eor.exchange, ',' ORDER BY eor.id), 'none'),
  et.updated_at
FROM execution_tasks et
LEFT JOIN exchange_order_results eor
  ON eor.execution_task_id = et.id
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
GROUP BY et.id, et.symbol, et.task_status, et.lease_owner, et.lease_until, et.updated_at;
" | sed 's/^/  task\t/'

    echo "post_run_evidence=execution_task_attempts"
    query_web "
SELECT
  eta.id,
  eta.attempt_no,
  eta.attempt_status,
  COALESCE(eta.executor, ''),
  COALESCE(eta.error_message, ''),
  eta.updated_at
FROM execution_task_attempts eta
WHERE eta.execution_task_id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
ORDER BY eta.id DESC
LIMIT 5;
" | sed 's/^/  attempt\t/'

    echo "rollback_plan=manual_close_required_if_position_open"
    echo "rollback_check=inspect_okx_position_and_open_orders_for_task_symbol_before_any_close"
    echo "rollback_scope=target_task_id:${MARKET_VELOCITY_LIVE_TARGET_TASK_ID} max_notional:${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT} close_requires_separate_authorization"
    assert_post_run_target_handled
}

collect_pre_run_exchange_readonly_evidence() {
    echo
    if ! is_enabled "${MARKET_VELOCITY_LIVE_PRE_RUN_RECONCILE}"; then
        echo "pre_run_evidence=okx_signed_readonly_reconciliation_snapshot skipped=true"
        return 0
    fi

    local reconciliation_row
    reconciliation_row="$(
        query_web "
SELECT
  et.buyer_email,
  et.symbol,
  et.combo_id,
  COALESCE(et.request_payload_json::jsonb #>> '{api_credential_id}', '')
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID};
"
    )"
    if [[ -z "${reconciliation_row}" ]]; then
        blocker "pre_run_reconciliation_task_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi

    local reconciliation_buyer_email reconciliation_symbol reconciliation_combo_id reconciliation_credential_id
    IFS=$'\t' read -r reconciliation_buyer_email reconciliation_symbol reconciliation_combo_id reconciliation_credential_id <<<"${reconciliation_row}"
    if [[ -z "${reconciliation_buyer_email}" || -z "${reconciliation_symbol}" || ! "${reconciliation_combo_id}" =~ ^[0-9]+$ ]]; then
        blocker "pre_run_reconciliation_task_invalid" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi

    echo "pre_run_evidence=okx_signed_readonly_reconciliation_snapshot"
    echo "pre_run_exchange_readonly_scope=task_id:${MARKET_VELOCITY_LIVE_TARGET_TASK_ID} symbol:${reconciliation_symbol} exchange:okx report:false include_fills:false"

    export RECONCILIATION_SNAPSHOT_CONFIRM=I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION
    export RECONCILIATION_SNAPSHOT_BUYER_EMAIL="${reconciliation_buyer_email}"
    export RECONCILIATION_SNAPSHOT_EXCHANGE=okx
    export RECONCILIATION_SNAPSHOT_SYMBOL="${reconciliation_symbol}"
    export RECONCILIATION_SNAPSHOT_COMBO_ID="${reconciliation_combo_id}"
    export RECONCILIATION_SNAPSHOT_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
    export RECONCILIATION_SNAPSHOT_REPORT=false
    export RECONCILIATION_SNAPSHOT_INCLUDE_FILLS=false
    if [[ -n "${reconciliation_credential_id}" ]]; then
        export RECONCILIATION_SNAPSHOT_CREDENTIAL_REF="web_api_credential_id_${reconciliation_credential_id}"
    else
        unset RECONCILIATION_SNAPSHOT_CREDENTIAL_REF
    fi

    run_reconciliation_snapshot_once
}

collect_post_run_exchange_readonly_evidence() {
    echo
    if ! is_enabled "${MARKET_VELOCITY_LIVE_POST_RUN_RECONCILE}"; then
        echo "post_run_evidence=okx_signed_readonly_reconciliation_snapshot skipped=true"
        return 0
    fi

    local reconciliation_row
    reconciliation_row="$(
        query_web "
SELECT
  et.buyer_email,
  et.symbol,
  et.combo_id,
  COALESCE(et.request_payload_json::jsonb #>> '{api_credential_id}', '')
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID};
"
    )"
    if [[ -z "${reconciliation_row}" ]]; then
        blocker "post_run_reconciliation_task_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi

    local reconciliation_buyer_email reconciliation_symbol reconciliation_combo_id reconciliation_credential_id
    IFS=$'\t' read -r reconciliation_buyer_email reconciliation_symbol reconciliation_combo_id reconciliation_credential_id <<<"${reconciliation_row}"
    if [[ -z "${reconciliation_buyer_email}" || -z "${reconciliation_symbol}" || ! "${reconciliation_combo_id}" =~ ^[0-9]+$ ]]; then
        blocker "post_run_reconciliation_task_invalid" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        return 2
    fi

    echo "post_run_evidence=okx_signed_readonly_reconciliation_snapshot"
    echo "post_run_exchange_readonly_scope=task_id:${MARKET_VELOCITY_LIVE_TARGET_TASK_ID} symbol:${reconciliation_symbol} exchange:okx report:false include_fills:${MARKET_VELOCITY_LIVE_POST_RUN_INCLUDE_FILLS}"

    export RECONCILIATION_SNAPSHOT_CONFIRM=I_UNDERSTAND_SIGNED_READ_ONLY_RECONCILIATION
    export RECONCILIATION_SNAPSHOT_BUYER_EMAIL="${reconciliation_buyer_email}"
    export RECONCILIATION_SNAPSHOT_EXCHANGE=okx
    export RECONCILIATION_SNAPSHOT_SYMBOL="${reconciliation_symbol}"
    export RECONCILIATION_SNAPSHOT_COMBO_ID="${reconciliation_combo_id}"
    export RECONCILIATION_SNAPSHOT_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
    export RECONCILIATION_SNAPSHOT_REPORT=false
    export RECONCILIATION_SNAPSHOT_INCLUDE_FILLS="${MARKET_VELOCITY_LIVE_POST_RUN_INCLUDE_FILLS}"
    if [[ -n "${reconciliation_credential_id}" ]]; then
        export RECONCILIATION_SNAPSHOT_CREDENTIAL_REF="web_api_credential_id_${reconciliation_credential_id}"
    else
        unset RECONCILIATION_SNAPSHOT_CREDENTIAL_REF
    fi

    run_reconciliation_snapshot_once
}

print_live_apply_manifest() {
    local manifest_row
    manifest_row="$(
        query_web "
-- live_apply_manifest_source
SELECT
  et.id,
  et.combo_id,
  et.symbol,
  et.task_status,
  COALESCE(et.request_payload_json::jsonb #>> '{execution,exchange}', et.request_payload_json::jsonb #>> '{exchange}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{source_signal_type}', s.payload_json::jsonb #>> '{source_signal_type}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{rank_event_id}', s.payload_json::jsonb #>> '{rank_event_id}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{execution,size_usdt}', et.request_payload_json::jsonb #>> '{size_usdt}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,selected_stop_loss_price}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,protective_stop_loss_required}', et.request_payload_json::jsonb #>> '{protective_stop_loss_required}', et.request_payload_json::jsonb #>> '{execution,protective_stop_loss_required}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,direction}', et.request_payload_json::jsonb #>> '{direction}', et.request_payload_json::jsonb #>> '{position_side}', et.request_payload_json::jsonb #>> '{side}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,entry_price}', et.request_payload_json::jsonb #>> '{risk_plan,entry_reference_price}', et.request_payload_json::jsonb #>> '{entry_price}', et.request_payload_json::jsonb #>> '{current_price}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{risk_plan,stop_loss_source}', et.request_payload_json::jsonb #>> '{stop_loss_source}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}', ''),
  CASE WHEN et.request_payload_json::jsonb #>> '{api_credential_id}' IS NULL THEN 'missing' ELSE 'present' END
FROM execution_tasks et
LEFT JOIN strategy_signal_inbox s ON s.id = et.strategy_signal_id
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  AND et.combo_id = ${MARKET_VELOCITY_LIVE_COMBO_ID};
"
    )"
    if [[ -z "${manifest_row}" ]]; then
        blocker "pre_apply_manifest_task_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        exit 2
    fi

    local manifest_task_id manifest_combo_id manifest_symbol manifest_task_status manifest_exchange
    local manifest_source_signal_type manifest_rank_event_id manifest_size_usdt manifest_stop_loss
    local manifest_protection_required manifest_protection_direction manifest_protection_entry_price
    local manifest_protection_stop_loss_source manifest_risk_context_expires_at manifest_credential_ref
    IFS=$'\t' read -r \
        manifest_task_id \
        manifest_combo_id \
        manifest_symbol \
        manifest_task_status \
        manifest_exchange \
        manifest_source_signal_type \
        manifest_rank_event_id \
        manifest_size_usdt \
        manifest_stop_loss \
        manifest_protection_required \
        manifest_protection_direction \
        manifest_protection_entry_price \
        manifest_protection_stop_loss_source \
        manifest_risk_context_expires_at \
        manifest_credential_ref <<<"${manifest_row}"

    require_okx_symbol "${manifest_symbol}"

    echo
    echo "live_apply_manifest=market_velocity_okx_scoped_worker"
    echo "manifest_target_task_id=${manifest_task_id}"
    echo "manifest_combo_id=${manifest_combo_id}"
    echo "manifest_exchange=${manifest_exchange}"
    echo "manifest_symbol=${manifest_symbol}"
    echo "manifest_task_status=${manifest_task_status}"
    echo "manifest_source_signal_type=${manifest_source_signal_type}"
    echo "manifest_rank_event_id=${manifest_rank_event_id:-none}"
    echo "manifest_size_usdt=${manifest_size_usdt:-missing}"
    echo "manifest_max_notional_usdt=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
    echo "manifest_stop_loss=${manifest_stop_loss:-missing}"
    echo "manifest_protection_required=${manifest_protection_required:-missing}"
    echo "manifest_protection_direction=${manifest_protection_direction:-missing}"
    echo "manifest_protection_entry_price=${manifest_protection_entry_price:-missing}"
    echo "manifest_protection_stop_loss_source=${manifest_protection_stop_loss_source:-missing}"
    echo "manifest_risk_context_expires_at=${manifest_risk_context_expires_at:-missing}"
    echo "manifest_credential_ref=${manifest_credential_ref}"
    echo "manifest_web_base_url=${RUST_QUAN_WEB_BASE_URL}"
    if [[ -n "${OKX_REQUEST_EXPIRATION_MS:-}" ]]; then
        echo "manifest_okx_request_expiration_ms=explicit:${OKX_REQUEST_EXPIRATION_MS}"
    else
        echo "manifest_okx_request_expiration_ms=unset"
    fi
    echo "manifest_worker_scope=EXECUTION_WORKER_TARGET_TASK_IDS=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID} EXECUTION_WORKER_TASK_TYPES=execute_signal EXECUTION_WORKER_TASK_STATUSES=pending EXECUTION_WORKER_LEASE_LIMIT=1"
    echo "manifest_pre_run_evidence=okx_signed_readonly_reconciliation_snapshot"
    echo "manifest_post_run_evidence=web_task_order_result,execution_task_attempts,okx_signed_readonly_reconciliation_snapshot"
    MARKET_VELOCITY_LIVE_REQUIRED_INTENT="okx:task=${manifest_task_id}:symbol=${manifest_symbol}:max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
    echo "manifest_live_mutation_intent=${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}"
    echo "manifest_live_mutation_requires=MARKET_VELOCITY_LIVE_WORKER_APPLY=true MARKET_VELOCITY_LIVE_WORKER_CONFIRM=${MARKET_VELOCITY_LIVE_WORKER_CONFIRM_PHRASE} MARKET_VELOCITY_LIVE_WORKER_INTENT=${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}"
}

assert_no_prior_task_execution_history() {
    local history_row
    history_row="$(
        query_web "
-- pre_apply_task_execution_history
SELECT
  (
    SELECT COUNT(*)
    FROM exchange_order_results eor
    WHERE eor.execution_task_id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  ),
  (
    SELECT COUNT(*)
    FROM execution_task_attempts eta
    WHERE eta.execution_task_id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  );
"
    )"
    if [[ -z "${history_row}" ]]; then
        blocker "target_task_execution_history_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        exit 2
    fi

    local order_result_count attempt_count
    IFS=$'\t' read -r order_result_count attempt_count <<<"${history_row}"
    if [[ ! "${order_result_count}" =~ ^[0-9]+$ || ! "${attempt_count}" =~ ^[0-9]+$ ]]; then
        blocker "target_task_execution_history_invalid" "order_results=${order_result_count:-missing},attempts=${attempt_count:-missing}"
        exit 2
    fi

    echo "pre_apply_task_history=order_results:${order_result_count} attempts:${attempt_count}"
    if [[ "${order_result_count}" != "0" || "${attempt_count}" != "0" ]]; then
        blocker "target_task_execution_history_present" "order_results=${order_result_count},attempts=${attempt_count}"
        exit 2
    fi
}

assert_target_task_still_pending() {
    local state_row
    state_row="$(
        query_web "
-- pre_apply_task_state
SELECT
  et.id,
  et.combo_id,
  et.symbol,
  et.task_type,
  et.task_status
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID};
"
    )"
    if [[ -z "${state_row}" ]]; then
        blocker "target_task_state_missing" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        exit 2
    fi

    local state_task_id state_combo_id state_symbol state_task_type state_task_status
    IFS=$'\t' read -r state_task_id state_combo_id state_symbol state_task_type state_task_status <<<"${state_row}"
    echo "pre_apply_task_state=task_id:${state_task_id} status:${state_task_status} type:${state_task_type} combo_id:${state_combo_id} symbol:${state_symbol}"
    if [[ "${state_task_id}" != "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}" ||
        "${state_combo_id}" != "${MARKET_VELOCITY_LIVE_COMBO_ID}" ||
        "${state_task_type}" != "execute_signal" ||
        "${state_task_status}" != "pending" ||
        -z "${state_symbol}" ]]; then
        blocker "target_task_state_changed" "task_status=${state_task_status:-missing},task_type=${state_task_type:-missing},combo_id=${state_combo_id:-missing},symbol=${state_symbol:-missing}"
        exit 2
    fi
}

echo "== Market Velocity OKX scoped live worker =="
echo "combo_id=${MARKET_VELOCITY_LIVE_COMBO_ID}"
echo "target_task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-missing}"
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "refresh_readiness=${MARKET_VELOCITY_LIVE_REFRESH_READINESS}"
if [[ "${MARKET_VELOCITY_LIVE_WORKER_APPLY}" == "true" ]]; then
    echo "mode=apply"
elif is_enabled "${MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY}"; then
    echo "mode=apply_rehearsal"
else
    echo "mode=dry_run"
fi

require_optional_positive_integer "okx_request_expiration_ms" "${OKX_REQUEST_EXPIRATION_MS:-}"
require_numeric_id "combo_id" "${MARKET_VELOCITY_LIVE_COMBO_ID}"
require_numeric_id "target_task_id" "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"

if [[ "${MARKET_VELOCITY_LIVE_WORKER_APPLY}" == "true" ]] && is_enabled "${MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY}"; then
    blocker "live_worker_rehearsal_conflicts_with_apply" "set either MARKET_VELOCITY_LIVE_WORKER_APPLY=true or MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY=true, not both"
    exit 2
fi

run_live_preflight_with_recovery

protected_link_count="$(
    query_web "
SELECT COUNT(*)
FROM execution_tasks et
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  AND UPPER(REPLACE(et.symbol, '-', '')) LIKE 'LINKUSDT%';
"
)"
if [[ "${protected_link_count}" != "0" ]]; then
    echo "Refusing to run protected LINK task: task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}" >&2
    exit 2
fi

print_live_apply_manifest

if [[ "${MARKET_VELOCITY_LIVE_WORKER_APPLY}" != "true" ]] && ! is_enabled "${MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY}"; then
    echo
    echo "worker=dry_run"
    echo "apply_requirements=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}$(common_handoff_scope) $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_APPLY true) $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_CONFIRM "${MARKET_VELOCITY_LIVE_WORKER_CONFIRM_PHRASE}") $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_INTENT "${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}")"
    echo "apply_rehearsal_requirements=${OKX_REQUEST_EXPIRATION_HANDOFF_PREFIX}$(common_handoff_scope) $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY true) $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_CONFIRM "${MARKET_VELOCITY_LIVE_WORKER_CONFIRM_PHRASE}") $(shell_env_assign MARKET_VELOCITY_LIVE_WORKER_INTENT "${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}")"
    exit 0
fi

if [[ "${MARKET_VELOCITY_LIVE_WORKER_CONFIRM}" != "${MARKET_VELOCITY_LIVE_WORKER_CONFIRM_PHRASE}" ]]; then
    blocker "live_worker_confirmation_missing" "set MARKET_VELOCITY_LIVE_WORKER_CONFIRM=${MARKET_VELOCITY_LIVE_WORKER_CONFIRM_PHRASE}"
    exit 2
fi
if [[ -z "${MARKET_VELOCITY_LIVE_WORKER_INTENT}" ]]; then
    blocker "live_worker_intent_missing" "set MARKET_VELOCITY_LIVE_WORKER_INTENT=${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}"
    exit 2
fi
if [[ "${MARKET_VELOCITY_LIVE_WORKER_INTENT}" != "${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}" ]]; then
    blocker "live_worker_intent_mismatch" "expected=${MARKET_VELOCITY_LIVE_REQUIRED_INTENT}"
    exit 2
fi

assert_target_task_still_pending
assert_no_prior_task_execution_history

export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export QUANT_CORE_DATABASE_URL
export QUANT_DATABASE_URL
export SQLX_OFFLINE
if [[ -n "${OKX_REQUEST_EXPIRATION_MS:-}" ]]; then
    export OKX_REQUEST_EXPIRATION_MS
fi

pre_run_status=0
collect_pre_run_exchange_readonly_evidence || pre_run_status=$?
echo "pre_run_evidence_status=${pre_run_status}"
if [[ "${pre_run_status}" != "0" ]]; then
    echo "final_exit_status=${pre_run_status} reason=pre_run_evidence_failed"
    exit "${pre_run_status}"
fi

pre_worker_preflight_status=0
echo
echo "pre_worker_preflight=market_velocity_okx_live_preflight"
run_live_preflight_with_recovery || pre_worker_preflight_status=$?
echo "pre_worker_preflight_status=${pre_worker_preflight_status}"
if [[ "${pre_worker_preflight_status}" != "0" ]]; then
    echo "final_exit_status=${pre_worker_preflight_status} reason=pre_worker_preflight_failed"
    exit "${pre_worker_preflight_status}"
fi

if is_enabled "${MARKET_VELOCITY_LIVE_WORKER_REHEARSE_APPLY}"; then
    echo
    echo "worker=rehearsal_stop_before_apply"
    echo "final_exit_status=0 reason=pre_run_evidence_ok_rehearsal_no_worker"
    exit 0
fi

export RUSTUP_TOOLCHAIN
export IS_RUN_EXECUTION_WORKER=true
export IS_BACK_TEST=false
export IS_OPEN_SOCKET=false
export IS_RUN_REAL_STRATEGY=false
export IS_RUN_SYNC_DATA_JOB=false
export EXECUTION_WORKER_ID="market_velocity_okx_live_task_${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
export EXECUTION_WORKER_DRY_RUN=false
export EXECUTION_WORKER_LIVE_ORDER_CONFIRM=I_UNDERSTAND_LIVE_ORDERS
export EXECUTION_WORKER_DEFAULT_EXCHANGE=okx
export EXECUTION_WORKER_RUN_ONCE=true
export EXECUTION_WORKER_ONLY=true
export EXECUTION_WORKER_LEASE_LIMIT=1
export EXECUTION_WORKER_TASK_TYPES=execute_signal
export EXECUTION_WORKER_TASK_STATUSES=pending
export EXECUTION_WORKER_TARGET_TASK_IDS="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"

if command -v rustup >/dev/null 2>&1; then
    RUSTC="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc 2>/dev/null || true)"
    if [[ -n "${RUSTC}" ]]; then
        export RUSTC
        export PATH="$(dirname "${RUSTC}"):${PATH}"
    fi
fi

echo
echo "worker=apply"
echo "worker_scope=EXECUTION_WORKER_TARGET_TASK_IDS=${EXECUTION_WORKER_TARGET_TASK_IDS} EXECUTION_WORKER_TASK_TYPES=${EXECUTION_WORKER_TASK_TYPES} EXECUTION_WORKER_TASK_STATUSES=${EXECUTION_WORKER_TASK_STATUSES} EXECUTION_WORKER_LEASE_LIMIT=${EXECUTION_WORKER_LEASE_LIMIT}"
worker_status=0
post_run_status=0
run_worker_once || worker_status=$?
collect_post_run_evidence || post_run_status=$?
collect_post_run_exchange_readonly_evidence || post_run_status=$?
echo "worker_exit_status=${worker_status}"
echo "post_run_evidence_status=${post_run_status}"
if [[ "${worker_status}" != "0" ]]; then
    echo "final_exit_status=${worker_status} reason=worker_failed"
    exit "${worker_status}"
fi
if [[ "${post_run_status}" != "0" ]]; then
    echo "final_exit_status=${post_run_status} reason=post_run_evidence_failed"
    exit "${post_run_status}"
fi
echo "final_exit_status=0 reason=worker_and_post_run_evidence_ok"
exit "${post_run_status}"
