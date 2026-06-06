#!/usr/bin/env bash
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"
MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID="${MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID:-78}"
MARKET_VELOCITY_LIVE_READY_BUYER_HASH="${MARKET_VELOCITY_LIVE_READY_BUYER_HASH:-}"
MARKET_VELOCITY_LIVE_CONFIG_APPLY="${MARKET_VELOCITY_LIVE_CONFIG_APPLY:-false}"
MARKET_VELOCITY_LIVE_CONFIG_CONFIRM="${MARKET_VELOCITY_LIVE_CONFIG_CONFIRM:-}"
CONFIG_CONFIRM_PHRASE="I_UNDERSTAND_THIS_CHANGES_LIVE_CONFIG"

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

require_buyer_hash() {
    local value="$1"
    if [[ -n "${value}" && ! "${value}" =~ ^[0-9a-f]{32}$ ]]; then
        blocker "ready_buyer_hash_invalid" "${value}"
        return 1
    fi
}

require_authorized_notional() {
    local value="$1"
    python3 - "${value}" <<'PY'
from decimal import Decimal, InvalidOperation
import sys

raw = sys.argv[1]
try:
    value = Decimal(raw)
except InvalidOperation:
    print(f"blocker=max_notional_decimal_invalid detail={raw}")
    sys.exit(2)

if value <= 0:
    print(f"blocker=max_notional_not_positive detail={value}")
    sys.exit(2)

if value > Decimal("5"):
    print(f"blocker=max_notional_above_user_authorization detail={value}>5")
    sys.exit(1)
PY
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

ready_okx_credential_predicate() {
    cat <<SQL
lower(exchange) = 'okx'
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
  )
SQL
}

echo "== Market Velocity OKX live config prepare =="
echo "source_combo_id=${MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID}"
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
if [[ "${MARKET_VELOCITY_LIVE_CONFIG_APPLY}" == "true" ]]; then
    echo "mode=apply"
else
    echo "mode=dry_run"
fi

require_numeric_id "source_combo_id" "${MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID}" || true
require_buyer_hash "${MARKET_VELOCITY_LIVE_READY_BUYER_HASH}" || true
if ! require_authorized_notional "${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"; then
    failures=$((failures + 1))
fi
if [[ "${failures}" -gt 0 ]]; then
    exit 2
fi

buyer_hash_clause=""
if [[ -n "${MARKET_VELOCITY_LIVE_READY_BUYER_HASH}" ]]; then
    buyer_hash_clause="AND md5(buyer_email) = '${MARKET_VELOCITY_LIVE_READY_BUYER_HASH}'"
fi

ready_predicate="$(ready_okx_credential_predicate)"

target_summary="$(
    query_web "
WITH ready_okx_buyers AS (
  SELECT buyer_email, md5(buyer_email) AS buyer_hash, COUNT(*) AS credential_count
  FROM user_api_credentials
  WHERE ${ready_predicate}
    ${buyer_hash_clause}
  GROUP BY buyer_email
)
SELECT
  COUNT(*)::text,
  COALESCE(MAX(buyer_hash), ''),
  COALESCE(SUM(credential_count), 0)::text
FROM ready_okx_buyers;
"
)"
IFS=$'\t' read -r ready_buyer_count target_buyer_hash ready_credential_count <<<"${target_summary}"

echo "ready_okx_buyer_count=${ready_buyer_count}"
echo "ready_okx_credential_count=${ready_credential_count}"
if [[ -n "${target_buyer_hash}" ]]; then
    echo "target_buyer_hash=${target_buyer_hash}"
fi

if [[ "${ready_buyer_count}" == "0" ]]; then
    blocker "ready_okx_buyer_missing" "v4 signed OKX credential required"
elif [[ "${ready_buyer_count}" != "1" ]]; then
    blocker "ready_okx_buyer_ambiguous" "set MARKET_VELOCITY_LIVE_READY_BUYER_HASH"
fi

source_combo_row="$(
    query_web "
SELECT
  c.id,
  c.product_id,
  c.strategy_slug,
  c.strategy_title,
  c.symbol
FROM strategy_combo_subscriptions c
WHERE c.id = ${MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID};
"
)"
if [[ -z "${source_combo_row}" ]]; then
    blocker "source_combo_missing" "id=${MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID}"
else
    IFS=$'\t' read -r source_combo_id source_product_id source_strategy_slug source_strategy_title source_symbol <<<"${source_combo_row}"
    echo "source_combo=${source_combo_id} product_id=${source_product_id} strategy=${source_strategy_slug} symbol=${source_symbol}"
    if [[ "${source_strategy_slug}" != "market_velocity" && "${source_strategy_slug}" != "market_velocity_radar" ]]; then
        blocker "source_combo_not_market_velocity" "${source_strategy_slug}"
    fi
    if [[ "${source_symbol}" != "MARKET-VELOCITY-ALL" ]]; then
        blocker "source_combo_not_market_velocity_all_symbol" "${source_symbol}"
    fi
fi

if [[ "${ready_buyer_count}" == "1" ]]; then
    existing_combo_row="$(
        query_web "
WITH ready_okx_buyers AS (
  SELECT buyer_email, md5(buyer_email) AS buyer_hash
  FROM user_api_credentials
  WHERE ${ready_predicate}
    ${buyer_hash_clause}
  GROUP BY buyer_email
)
SELECT
  c.id,
  COALESCE(c.execution_exchange, 'none'),
  c.service_mode,
  c.status,
  COALESCE(r.max_position_usdt::text, 'missing'),
  COALESCE(r.max_daily_loss_usdt::text, 'missing'),
  COALESCE(r.max_daily_trades::text, 'missing'),
  COALESCE(r.risk_acknowledged::text, 'false'),
  COALESCE(r.status, 'missing')
FROM strategy_combo_subscriptions c
JOIN ready_okx_buyers b
  ON b.buyer_email = c.buyer_email
LEFT JOIN combo_risk_settings r
  ON r.combo_id = c.id
 AND r.buyer_email = c.buyer_email
WHERE c.strategy_slug IN ('market_velocity', 'market_velocity_radar')
   OR c.symbol = 'MARKET-VELOCITY-ALL'
ORDER BY c.id
LIMIT 1;
"
    )"
    echo
    echo "target_market_velocity_combo:"
    if [[ -z "${existing_combo_row}" ]]; then
        echo "  action=insert_market_velocity_combo"
    else
        IFS=$'\t' read -r existing_combo_id existing_exchange existing_service_mode existing_status existing_max_position existing_max_loss existing_max_trades existing_risk_ack existing_risk_status <<<"${existing_combo_row}"
        echo "  action=update_existing_combo"
        echo "  combo_id=${existing_combo_id}"
        echo "  execution_exchange=${existing_exchange:-none}"
        echo "  service_mode=${existing_service_mode}"
        echo "  status=${existing_status}"
        echo "  max_position_usdt=${existing_max_position:-missing}"
        echo "  max_daily_loss_usdt=${existing_max_loss:-missing}"
        echo "  max_daily_trades=${existing_max_trades:-missing}"
        echo "  risk_acknowledged=${existing_risk_ack}"
        echo "  risk_status=${existing_risk_status:-missing}"
    fi
fi

if [[ "${failures}" -gt 0 ]]; then
    echo
    echo "prepare=blocked failures=${failures}"
    exit 2
fi

echo
echo "planned_alignment:"
echo "  strategy_slug=market_velocity"
echo "  symbol=MARKET-VELOCITY-ALL"
echo "  execution_exchange=okx"
echo "  service_mode=api_trade_enabled"
echo "  max_position_usdt=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "  max_daily_loss_usdt=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "  max_daily_trades=1"
echo "  risk_acknowledged=true"

if [[ "${MARKET_VELOCITY_LIVE_CONFIG_APPLY}" != "true" ]]; then
    echo
    echo "prepare=dry_run"
    echo "apply_requirements=MARKET_VELOCITY_LIVE_CONFIG_APPLY=true MARKET_VELOCITY_LIVE_CONFIG_CONFIRM=${CONFIG_CONFIRM_PHRASE}"
    exit 0
fi

if [[ "${MARKET_VELOCITY_LIVE_CONFIG_CONFIRM}" != "${CONFIG_CONFIRM_PHRASE}" ]]; then
    blocker "apply_confirmation_missing" "set MARKET_VELOCITY_LIVE_CONFIG_CONFIRM=${CONFIG_CONFIRM_PHRASE}"
    echo
    echo "prepare=blocked failures=${failures}"
    exit 2
fi

apply_result="$(
    query_web "
BEGIN;
WITH ready_okx_buyers AS (
  SELECT buyer_email, md5(buyer_email) AS buyer_hash
  FROM user_api_credentials
  WHERE ${ready_predicate}
    ${buyer_hash_clause}
  GROUP BY buyer_email
),
target_buyer AS (
  SELECT buyer_email, buyer_hash
  FROM ready_okx_buyers
  WHERE (SELECT COUNT(*) FROM ready_okx_buyers) = 1
),
source_combo AS (
  SELECT product_id, strategy_title
  FROM strategy_combo_subscriptions
  WHERE id = ${MARKET_VELOCITY_LIVE_SOURCE_COMBO_ID}
    AND strategy_slug IN ('market_velocity', 'market_velocity_radar')
    AND symbol = 'MARKET-VELOCITY-ALL'
),
existing_combo AS (
  SELECT c.id, c.buyer_email
  FROM strategy_combo_subscriptions c
  JOIN target_buyer b
    ON b.buyer_email = c.buyer_email
  WHERE c.strategy_slug IN ('market_velocity', 'market_velocity_radar')
     OR c.symbol = 'MARKET-VELOCITY-ALL'
  ORDER BY c.id
  LIMIT 1
),
inserted_combo AS (
  INSERT INTO strategy_combo_subscriptions (
    product_id,
    buyer_email,
    strategy_slug,
    strategy_title,
    symbol,
    execution_exchange,
    service_mode,
    status,
    source,
    started_at,
    expired_at,
    created_at,
    updated_at
  )
  SELECT
    s.product_id,
    b.buyer_email,
    'market_velocity',
    s.strategy_title,
    'MARKET-VELOCITY-ALL',
    'okx',
    'api_trade_enabled',
    'active',
    'live_validation',
    NOW() AT TIME ZONE 'UTC',
    (NOW() AT TIME ZONE 'UTC') + INTERVAL '1 day',
    NOW() AT TIME ZONE 'UTC',
    NOW() AT TIME ZONE 'UTC'
  FROM target_buyer b
  CROSS JOIN source_combo s
  WHERE NOT EXISTS (SELECT 1 FROM existing_combo)
  RETURNING
    id,
    buyer_email,
    md5(buyer_email) AS buyer_hash,
    execution_exchange,
    service_mode,
    status
),
selected_combo AS (
  SELECT id, buyer_email
  FROM inserted_combo
  UNION ALL
  SELECT id, buyer_email
  FROM existing_combo
  WHERE NOT EXISTS (SELECT 1 FROM inserted_combo)
),
updated_existing_combo AS (
  UPDATE strategy_combo_subscriptions c
  SET
    strategy_slug = 'market_velocity',
    symbol = 'MARKET-VELOCITY-ALL',
    execution_exchange = 'okx',
    service_mode = 'api_trade_enabled',
    status = 'active',
    expired_at = GREATEST(c.expired_at, (NOW() AT TIME ZONE 'UTC') + INTERVAL '1 day'),
    updated_at = NOW() AT TIME ZONE 'UTC'
  FROM selected_combo selected
  WHERE c.id = selected.id
  RETURNING
    c.id,
    c.buyer_email,
    md5(c.buyer_email) AS buyer_hash,
    c.execution_exchange,
    c.service_mode,
    c.status
),
aligned_combo AS (
  SELECT
    id,
    buyer_email,
    buyer_hash,
    execution_exchange,
    service_mode,
    status
  FROM inserted_combo
  UNION ALL
  SELECT
    id,
    buyer_email,
    buyer_hash,
    execution_exchange,
    service_mode,
    status
  FROM updated_existing_combo
),
upserted_risk AS (
  INSERT INTO combo_risk_settings (
    combo_id,
    buyer_email,
    max_position_usdt,
    max_daily_trades,
    max_daily_loss_usdt,
    emergency_stop_enabled,
    risk_acknowledged,
    status,
    created_at,
    updated_at
  )
  SELECT
    id,
    buyer_email,
    ${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}::numeric AS max_position_usdt,
    1 AS max_daily_trades,
    ${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}::numeric AS max_daily_loss_usdt,
    TRUE,
    TRUE,
    'active',
    NOW() AT TIME ZONE 'UTC',
    NOW() AT TIME ZONE 'UTC'
  FROM aligned_combo
  ON CONFLICT (combo_id) DO UPDATE SET
    max_position_usdt = EXCLUDED.max_position_usdt,
    max_daily_trades = EXCLUDED.max_daily_trades,
    max_daily_loss_usdt = EXCLUDED.max_daily_loss_usdt,
    emergency_stop_enabled = EXCLUDED.emergency_stop_enabled,
    risk_acknowledged = EXCLUDED.risk_acknowledged,
    status = EXCLUDED.status,
    updated_at = EXCLUDED.updated_at
  RETURNING combo_id, max_position_usdt, max_daily_trades, max_daily_loss_usdt, risk_acknowledged, status
)
SELECT
  c.id,
  c.buyer_hash,
  c.execution_exchange,
  c.service_mode,
  c.status,
  r.max_position_usdt,
  r.max_daily_loss_usdt,
  r.max_daily_trades,
  r.risk_acknowledged,
  r.status
FROM aligned_combo c
JOIN upserted_risk r
  ON r.combo_id = c.id;
COMMIT;
"
)"

apply_result_row="$(printf '%s\n' "${apply_result}" | awk -F $'\t' 'NF >= 10 && $1 ~ /^[0-9]+$/ && $2 ~ /^[0-9a-f]{32}$/ { print; exit }')"
if [[ -z "${apply_result_row}" ]]; then
    blocker "apply_result_missing" "no combo was inserted or updated"
    echo
    echo "prepare=blocked failures=${failures}"
    exit 2
fi

IFS=$'\t' read -r applied_combo_id _applied_buyer_hash _applied_exchange _applied_service_mode _applied_status _applied_max_position _applied_max_loss _applied_max_trades _applied_risk_ack _applied_risk_status <<<"${apply_result_row}"

echo
echo "applied_alignment:"
echo "${apply_result_row}" | sed 's/^/  /'
echo
echo "prepare=applied"
echo "next_preflight=MARKET_VELOCITY_LIVE_COMBO_ID=${applied_combo_id} scripts/dev/run_market_velocity_okx_live_preflight.sh"
