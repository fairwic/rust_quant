#!/usr/bin/env bash
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
CORE_POSTGRES_DB="${CORE_POSTGRES_DB:-quant_core}"
MARKET_VELOCITY_LIVE_COMBO_ID="${MARKET_VELOCITY_LIVE_COMBO_ID:-78}"
MARKET_VELOCITY_LIVE_TARGET_TASK_ID="${MARKET_VELOCITY_LIVE_TARGET_TASK_ID:-}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"
MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS="${MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS:-60}"

failures=0

blocker() {
    local code="$1"
    local detail="${2:-}"
    if [[ -n "${detail}" ]]; then
        echo "blocker=${code} detail=${detail}"
    else
        echo "blocker=${code}"
    fi
    failures=$((failures + 1))
}

require_numeric_id() {
    local label="$1"
    local value="$2"
    if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
        blocker "${label}_invalid" "${value}"
        return 1
    fi
}

require_positive_integer() {
    local label="$1"
    local value="$2"
    if [[ ! "${value}" =~ ^[0-9]+$ || "${value}" == "0" ]]; then
        blocker "${label}_invalid" "${value}"
        return 1
    fi
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

query_core() {
    local sql="$1"
    podman exec -i "${POSTGRES_CONTAINER}" psql \
        -U "${POSTGRES_USER}" \
        -d "${CORE_POSTGRES_DB}" \
        -XAt \
        -F $'\t' \
        -c "${sql}"
}

sql_escape_literal() {
    printf "%s" "$1" | sed "s/'/''/g"
}

require_positive_decimal_field() {
    local label="$1"
    local value="$2"
    if [[ -z "${value}" ]]; then
        blocker "${label}_missing"
        return
    fi

    if python3 - "${value}" <<'PY'
from decimal import Decimal, InvalidOperation
import sys

try:
    value = Decimal(sys.argv[1])
except InvalidOperation:
    sys.exit(2)

if value <= 0:
    sys.exit(1)
PY
    then
        return
    fi

    local status=$?
    if [[ "${status}" == "1" ]]; then
        blocker "${label}_not_positive" "${value}"
    else
        blocker "${label}_decimal_invalid" "${value}"
    fi
}

decimal_le() {
    local value="$1"
    local limit="$2"
    local label="$3"
    python3 - "${value}" "${limit}" "${label}" <<'PY'
from decimal import Decimal, InvalidOperation
import sys

value_raw, limit_raw, label = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    value = Decimal(value_raw)
    limit = Decimal(limit_raw)
except InvalidOperation:
    print(f"blocker={label}_decimal_invalid detail={value_raw}/{limit_raw}")
    sys.exit(2)

if value <= 0:
    print(f"blocker={label}_not_positive detail={value}")
    sys.exit(2)

if value > limit:
    print(f"blocker={label}_above_max_notional detail={value}>{limit}")
    sys.exit(1)
PY
}

validate_stop_loss_side() {
    local selected_stop_loss_price="$1"
    local risk_plan_entry_price="$2"
    local risk_plan_direction="$3"
    python3 - "${selected_stop_loss_price}" "${risk_plan_entry_price}" "${risk_plan_direction}" <<'PY'
from decimal import Decimal, InvalidOperation
import sys

stop_raw, entry_raw, direction_raw = sys.argv[1], sys.argv[2], sys.argv[3]
direction = direction_raw.strip().lower()
try:
    stop = Decimal(stop_raw)
    entry = Decimal(entry_raw)
except InvalidOperation:
    print(f"blocker=task_stop_loss_decimal_invalid detail={stop_raw}/{entry_raw}")
    sys.exit(2)

if stop <= 0 or entry <= 0:
    print(f"blocker=task_stop_loss_or_entry_not_positive detail={stop}/{entry}")
    sys.exit(2)

if direction in {"long", "buy", "open_long"}:
    if stop >= entry:
        print(f"blocker=task_stop_loss_not_below_entry_for_long detail={stop}>={entry}")
        sys.exit(1)
elif direction in {"short", "sell", "open_short"}:
    if stop <= entry:
        print(f"blocker=task_stop_loss_not_above_entry_for_short detail={stop}<={entry}")
        sys.exit(1)
else:
    print(f"blocker=task_risk_plan_direction_invalid detail={direction_raw}")
    sys.exit(2)
PY
}

validate_okx_symbol_filters() {
    local task_symbol="$1"
    local task_symbol_sql
    task_symbol_sql="$(sql_escape_literal "${task_symbol}")"

    local filter_row
    filter_row="$(
        query_core "
SELECT
  exchange_symbol,
  normalized_symbol,
  status,
  COALESCE(min_qty, ''),
  COALESCE(step_size, ''),
  COALESCE(tick_size, ''),
  COALESCE(raw_payload #>> '{ctVal}', ''),
  COALESCE(raw_payload #>> '{ctValCcy}', '')
FROM exchange_symbols
WHERE exchange = 'okx'
  AND (
    normalized_symbol = '${task_symbol_sql}'
    OR exchange_symbol = '${task_symbol_sql}'
  )
ORDER BY
  CASE WHEN lower(status) IN ('trading', 'live') THEN 0 ELSE 1 END,
  updated_at DESC
LIMIT 1;
"
    )"

    if [[ -z "${filter_row}" ]]; then
        blocker "okx_symbol_filters_missing" "${task_symbol}"
        return
    fi

    local exchange_symbol normalized_symbol symbol_status min_qty step_size tick_size contract_value contract_value_currency
    IFS=$'\t' read -r exchange_symbol normalized_symbol symbol_status min_qty step_size tick_size contract_value contract_value_currency <<<"${filter_row}"
    echo "okx_symbol_filters=ready symbol=${normalized_symbol:-missing} exchange_symbol=${exchange_symbol:-missing} status=${symbol_status:-missing} min_qty=${min_qty:-missing} step_size=${step_size:-missing} tick_size=${tick_size:-missing} contract_value=${contract_value:-missing} contract_value_currency=${contract_value_currency:-missing}"

    if [[ ! "${symbol_status,,}" =~ ^(trading|live)$ ]]; then
        blocker "okx_symbol_filters_not_live" "status=${symbol_status:-missing}"
    fi
    require_positive_decimal_field "okx_symbol_filter_min_qty" "${min_qty}"
    require_positive_decimal_field "okx_symbol_filter_step_size" "${step_size}"
    require_positive_decimal_field "okx_symbol_filter_tick_size" "${tick_size}"
    require_positive_decimal_field "okx_symbol_filter_contract_value" "${contract_value}"
    if [[ -z "${contract_value_currency}" ]]; then
        blocker "okx_symbol_filter_contract_value_currency_missing"
    fi
}

echo "== Market Velocity OKX live preflight =="
echo "combo_id=${MARKET_VELOCITY_LIVE_COMBO_ID}"
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "min_risk_ttl_seconds=${MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS}"

require_numeric_id "combo_id" "${MARKET_VELOCITY_LIVE_COMBO_ID}" || true
require_positive_integer "min_risk_ttl_seconds" "${MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS}" || true
if [[ "${failures}" -gt 0 ]]; then
    exit 2
fi

if ! decimal_le "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}" "999999999" "max_notional"; then
    failures=$((failures + 1))
fi

combo_row="$(
    query_web "
SELECT
  c.id,
  c.strategy_slug,
  c.symbol,
  c.status,
  c.service_mode,
  COALESCE(c.execution_exchange, ''),
  (c.expired_at >= NOW())::text,
  COALESCE(r.status, ''),
  COALESCE(r.risk_acknowledged, FALSE)::text,
  COALESCE(r.max_position_usdt::text, ''),
  COALESCE(r.max_daily_loss_usdt::text, ''),
  COALESCE(r.max_daily_trades::text, '')
FROM strategy_combo_subscriptions c
LEFT JOIN combo_risk_settings r
  ON r.combo_id = c.id
 AND r.buyer_email = c.buyer_email
WHERE c.id = ${MARKET_VELOCITY_LIVE_COMBO_ID};
"
)"

if [[ -z "${combo_row}" ]]; then
    blocker "market_velocity_combo_missing" "id=${MARKET_VELOCITY_LIVE_COMBO_ID}"
else
    IFS=$'\t' read -r combo_id strategy_slug subscription_symbol combo_status service_mode execution_exchange subscription_valid risk_status risk_ack max_position_usdt max_daily_loss_usdt max_daily_trades <<<"${combo_row}"
    echo "combo=${combo_id} strategy=${strategy_slug} symbol=${subscription_symbol} status=${combo_status} service_mode=${service_mode} execution_exchange=${execution_exchange:-none} subscription_valid=${subscription_valid} risk_status=${risk_status:-none} risk_ack=${risk_ack}"
    echo "risk=max_position_usdt:${max_position_usdt:-missing} max_daily_loss_usdt:${max_daily_loss_usdt:-missing} max_daily_trades:${max_daily_trades:-missing}"

    if [[ "${strategy_slug}" != "market_velocity" && "${strategy_slug}" != "market_velocity_radar" ]]; then
        blocker "strategy_slug_not_market_velocity" "${strategy_slug}"
    fi
    if [[ "${subscription_symbol}" != "MARKET-VELOCITY-ALL" ]]; then
        blocker "market_velocity_subscription_not_all_symbol" "${subscription_symbol}"
    fi
    if [[ "${combo_status}" != "active" ]]; then
        blocker "combo_not_active" "${combo_status}"
    fi
    if [[ "${service_mode}" != "api_trade_enabled" ]]; then
        blocker "combo_api_trade_not_enabled" "${service_mode}"
    fi
    if [[ "${execution_exchange,,}" != "okx" ]]; then
        blocker "combo_execution_exchange_not_okx" "${execution_exchange:-none}"
    fi
    if [[ "${subscription_valid}" != "true" ]]; then
        blocker "combo_subscription_expired"
    fi
    if [[ "${risk_status}" != "active" || "${risk_ack}" != "true" ]]; then
        blocker "combo_risk_not_acknowledged" "status=${risk_status:-none},ack=${risk_ack:-false}"
    fi
    if [[ -z "${max_position_usdt}" ]]; then
        blocker "combo_max_position_missing"
    elif ! decimal_le "${max_position_usdt}" "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}" "combo_max_position"; then
        failures=$((failures + 1))
    fi
    if [[ -z "${max_daily_loss_usdt}" ]]; then
        blocker "combo_max_daily_loss_missing"
    elif ! decimal_le "${max_daily_loss_usdt}" "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}" "combo_max_daily_loss"; then
        failures=$((failures + 1))
    fi
fi

credential_count="$(
    query_web "
SELECT COUNT(*)
FROM user_api_credentials
WHERE buyer_email = (
    SELECT buyer_email
    FROM strategy_combo_subscriptions
    WHERE id = ${MARKET_VELOCITY_LIVE_COMBO_ID}
)
  AND lower(exchange) = 'okx'
  AND status = 'active'
  AND last_check_code IN (
      'signed_exchange_preflight_passed',
      'signed_exchange_check_passed'
  )
  AND api_key_cipher LIKE 'v4:local_aes256gcm:%'
  AND api_secret_cipher LIKE 'v4:local_aes256gcm:%'
  AND (
      passphrase_cipher IS NULL
      OR BTRIM(passphrase_cipher) = ''
      OR passphrase_cipher LIKE 'v4:local_aes256gcm:%'
  );
"
)"

if [[ "${credential_count}" != "1" ]]; then
    blocker "okx_credential_not_ready" "matching_credentials=${credential_count}"
else
    echo "okx_credential=ready"
fi

if [[ -z "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}" ]]; then
    blocker "target_task_missing" "set MARKET_VELOCITY_LIVE_TARGET_TASK_ID"
else
    require_numeric_id "target_task_id" "${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}" || true
    if [[ "${failures}" -eq 0 ]]; then
        task_row="$(
            query_web "
SELECT
  et.id,
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
  COALESCE(et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}', '') AS risk_context_expires_at,
  COALESCE(
    FLOOR(EXTRACT(EPOCH FROM (
      NULLIF(et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}', '')::TIMESTAMP - NOW()
    )))::text,
    ''
  ) AS risk_context_seconds_remaining,
  CASE
    WHEN NULLIF(et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}', '') IS NULL THEN 'missing'
    WHEN (et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}')::TIMESTAMP <= NOW() THEN 'expired'
    WHEN (et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}')::TIMESTAMP < NOW() + ('${MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS} seconds')::interval THEN 'too_short'
    ELSE 'true'
  END AS risk_context_fresh,
  CASE WHEN et.request_payload_json::jsonb #>> '{api_credential_id}' IS NULL THEN 'missing' ELSE 'present' END
FROM execution_tasks et
LEFT JOIN strategy_signal_inbox s ON s.id = et.strategy_signal_id
WHERE et.id = ${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}
  AND et.combo_id = ${MARKET_VELOCITY_LIVE_COMBO_ID}
  AND et.task_type = 'execute_signal'
  AND et.task_status IN ('pending', 'leased')
  AND COALESCE(et.request_payload_json::jsonb #>> '{source_signal_type}', s.payload_json::jsonb #>> '{source_signal_type}', '') = 'market_velocity'
  AND lower(COALESCE(et.request_payload_json::jsonb #>> '{execution,exchange}', et.request_payload_json::jsonb #>> '{exchange}', '')) = 'okx'
  AND UPPER(REPLACE(et.symbol, '-', '')) NOT LIKE 'LINKUSDT%';
"
        )"
        if [[ -z "${task_row}" ]]; then
            blocker "target_task_not_okx_market_velocity_pending" "task_id=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID}"
        else
            IFS=$'\t' read -r task_id task_symbol task_status task_exchange source_signal_type rank_event_id size_usdt selected_stop_loss_price protective_stop_loss_required risk_plan_direction risk_plan_entry_price risk_plan_stop_loss_source risk_context_expires_at risk_context_seconds_remaining risk_context_fresh credential_ref <<<"${task_row}"
            echo "task=${task_id} symbol=${task_symbol} status=${task_status} exchange=${task_exchange} source_signal_type=${source_signal_type} rank_event_id=${rank_event_id:-none} size_usdt=${size_usdt:-missing} stop_loss=${selected_stop_loss_price:-missing} protection_required=${protective_stop_loss_required:-missing} risk_plan_direction=${risk_plan_direction:-missing} risk_plan_entry_price=${risk_plan_entry_price:-missing} risk_plan_stop_loss_source=${risk_plan_stop_loss_source:-missing} risk_context_expires_at=${risk_context_expires_at:-missing} risk_context_seconds_remaining=${risk_context_seconds_remaining:-missing} credential_ref=${credential_ref}"
            if [[ -z "${size_usdt}" ]]; then
                blocker "task_size_usdt_missing"
            elif ! decimal_le "${size_usdt}" "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}" "task_size_usdt"; then
                failures=$((failures + 1))
            fi
            if [[ -z "${selected_stop_loss_price}" ]]; then
                blocker "task_stop_loss_missing"
            fi
            case "${protective_stop_loss_required,,}" in
                true | 1 | yes) ;;
                *) blocker "task_protective_stop_loss_not_required" "${protective_stop_loss_required:-missing}" ;;
            esac
            if [[ -z "${risk_plan_direction}" ]]; then
                blocker "task_risk_plan_direction_missing"
            elif [[ ! "${risk_plan_direction,,}" =~ ^(long|short|buy|sell|open_long|open_short)$ ]]; then
                blocker "task_risk_plan_direction_invalid" "${risk_plan_direction}"
            fi
            if [[ -z "${risk_plan_entry_price}" ]]; then
                blocker "task_risk_plan_entry_price_missing"
            fi
            if [[ -z "${risk_plan_stop_loss_source}" ]]; then
                blocker "task_risk_plan_stop_loss_source_missing"
            fi
            if [[ -n "${selected_stop_loss_price}" && -n "${risk_plan_entry_price}" && "${risk_plan_direction,,}" =~ ^(long|short|buy|sell|open_long|open_short)$ ]]; then
                if ! validate_stop_loss_side "${selected_stop_loss_price}" "${risk_plan_entry_price}" "${risk_plan_direction}"; then
                    failures=$((failures + 1))
                fi
            fi
            if [[ "${risk_context_fresh}" == "missing" ]]; then
                blocker "task_risk_context_missing"
            elif [[ "${risk_context_fresh}" == "expired" ]]; then
                blocker "task_risk_context_expired" "${risk_context_expires_at:-missing}"
            elif [[ "${risk_context_fresh}" == "too_short" ]]; then
                blocker "task_risk_context_ttl_too_short" "${risk_context_seconds_remaining:-missing}s<${MARKET_VELOCITY_LIVE_MIN_RISK_TTL_SECONDS}s"
            elif [[ "${risk_context_fresh}" != "true" ]]; then
                blocker "task_risk_context_invalid" "${risk_context_fresh}"
            fi
            if [[ "${credential_ref}" != "present" ]]; then
                blocker "task_api_credential_ref_missing"
            fi
            validate_okx_symbol_filters "${task_symbol}"
        fi
    fi
fi

echo
echo "candidate_okx_tasks:"
query_web "
SELECT
  et.id,
  et.symbol,
  et.task_status,
  COALESCE(et.request_payload_json::jsonb #>> '{execution,size_usdt}', ''),
  COALESCE(et.request_payload_json::jsonb #>> '{user_execution_risk_context,expires_at}', ''),
  et.updated_at
FROM execution_tasks et
LEFT JOIN strategy_signal_inbox s ON s.id = et.strategy_signal_id
WHERE et.combo_id = ${MARKET_VELOCITY_LIVE_COMBO_ID}
  AND et.task_type = 'execute_signal'
  AND et.task_status IN ('pending', 'leased')
  AND COALESCE(et.request_payload_json::jsonb #>> '{source_signal_type}', s.payload_json::jsonb #>> '{source_signal_type}', '') = 'market_velocity'
  AND lower(COALESCE(et.request_payload_json::jsonb #>> '{execution,exchange}', et.request_payload_json::jsonb #>> '{exchange}', '')) = 'okx'
  AND UPPER(REPLACE(et.symbol, '-', '')) NOT LIKE 'LINKUSDT%'
ORDER BY et.updated_at DESC, et.id DESC
LIMIT 10;
" | sed 's/^/  /'

if [[ "${failures}" -gt 0 ]]; then
    echo
    echo "preflight=blocked failures=${failures}"
    exit 2
fi

echo
echo "preflight=ok"
echo "next_worker_scope=EXECUTION_WORKER_TARGET_TASK_IDS=${MARKET_VELOCITY_LIVE_TARGET_TASK_ID} EXECUTION_WORKER_TASK_TYPES=execute_signal EXECUTION_WORKER_TASK_STATUSES=pending EXECUTION_WORKER_LEASE_LIMIT=1"
