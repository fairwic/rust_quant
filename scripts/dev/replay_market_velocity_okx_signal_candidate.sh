#!/usr/bin/env bash
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
QUANT_CORE_POSTGRES_DB="${QUANT_CORE_POSTGRES_DB:-quant_core}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
RUST_QUAN_WEB_BASE_URL="${RUST_QUAN_WEB_BASE_URL:-http://127.0.0.1:8000}"
EXECUTION_EVENT_SECRET="${EXECUTION_EVENT_SECRET:-local-dev-secret}"
MARKET_VELOCITY_SIGNAL_EVENT_ID="${MARKET_VELOCITY_SIGNAL_EVENT_ID:-}"
MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS="${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS:-24}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"
MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT="${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT:-0.02}"
MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX="${MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX:-}"
MARKET_VELOCITY_SIGNAL_REPLAY_APPLY="${MARKET_VELOCITY_SIGNAL_REPLAY_APPLY:-false}"
MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM="${MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM:-}"
REPLAY_CONFIRM_PHRASE="I_UNDERSTAND_THIS_CREATES_WEB_EXECUTION_TASK"

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

require_numeric_id_if_set() {
    local label="$1"
    local value="$2"
    if [[ -n "${value}" && ! "${value}" =~ ^[0-9]+$ ]]; then
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

require_stop_loss_pct() {
    local value="$1"
    python3 - "${value}" <<'PY'
from decimal import Decimal, InvalidOperation
import sys

raw = sys.argv[1]
try:
    value = Decimal(raw)
except InvalidOperation:
    print(f"blocker=stop_loss_pct_decimal_invalid detail={raw}")
    sys.exit(2)

if value <= 0 or value >= 1:
    print(f"blocker=stop_loss_pct_out_of_range detail={value}")
    sys.exit(2)
PY
}

require_safe_external_id_suffix() {
    local value="$1"
    if [[ -z "${value}" ]]; then
        return 0
    fi
    if [[ ! "${value}" =~ ^[A-Za-z0-9._:-]{1,80}$ ]]; then
        blocker "external_id_suffix_invalid" "${value}"
        return 1
    fi
}

query_core() {
    local sql="$1"
    podman exec -i "${POSTGRES_CONTAINER}" psql \
        -U "${POSTGRES_USER}" \
        -d "${QUANT_CORE_POSTGRES_DB}" \
        -XAt \
        -F $'\t' \
        -c "${sql}"
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

pretty_json() {
    python3 -m json.tool
}

json_field() {
    local payload="$1"
    local field="$2"
    python3 - "${field}" "${payload}" <<'PY'
import json
import sys

field = sys.argv[1]
payload = json.loads(sys.argv[2])
value = payload
for part in field.split("."):
    if isinstance(value, str):
        value = json.loads(value)
    value = value.get(part)
    if value is None:
        print("")
        sys.exit(0)
print(value)
PY
}

echo "== Market Velocity OKX signal candidate replay =="
echo "core_db=${QUANT_CORE_POSTGRES_DB}"
echo "web_db=${WEB_POSTGRES_DB}"
echo "web=${RUST_QUAN_WEB_BASE_URL}"
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "lookback_hours=${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS}"
echo "stop_loss_pct=${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT}"
if [[ -n "${MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX}" ]]; then
    echo "replay_external_id_suffix=${MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX}"
else
    echo "replay_external_id_suffix=none"
fi
if [[ "${MARKET_VELOCITY_SIGNAL_REPLAY_APPLY}" == "true" ]]; then
    echo "mode=apply"
else
    echo "mode=dry_run"
fi

require_numeric_id_if_set "signal_event_id" "${MARKET_VELOCITY_SIGNAL_EVENT_ID}" || true
require_positive_integer "lookback_hours" "${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS}" || true
if ! require_stop_loss_pct "${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT}"; then
    failures=$((failures + 1))
fi
require_safe_external_id_suffix "${MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX}" || true
if [[ "${failures}" -gt 0 ]]; then
    exit 2
fi

event_id_clause=""
if [[ -n "${MARKET_VELOCITY_SIGNAL_EVENT_ID}" ]]; then
    event_id_clause="AND id = ${MARKET_VELOCITY_SIGNAL_EVENT_ID}"
fi
replay_external_id_suffix_sql=""
if [[ -n "${MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX}" ]]; then
    replay_external_id_suffix_sql=":${MARKET_VELOCITY_SIGNAL_REPLAY_EXTERNAL_ID_SUFFIX}"
fi

# payload contract: "source_signal_type": "market_velocity", "side": "buy",
# "position_side": "long", "trade_side": "open", "order_type": "market",
# "protective_stop_loss_required": true.
candidate_payload="$(
    query_core "
WITH candidate AS (
  SELECT
    id,
    lower(exchange) AS exchange,
    upper(symbol) AS symbol,
    event_type,
    timeframe,
    old_rank,
    new_rank,
    delta_rank,
    volume_24h_quote,
    current_price,
    previous_price,
    price_change_pct,
    price_direction,
    technical_timeframe,
    technical_period,
    technical_close_price,
    technical_ma_value,
    technical_ema_value,
    technical_ma_distance_pct,
    technical_ema_distance_pct,
    technical_ma_state,
    technical_ema_state,
    technical_candle_count,
    technical_snapshot_at,
    technical_snapshot_status,
    detected_at
  FROM market_rank_events
  WHERE event_type IN ('rank_velocity', 'top_entry')
    AND delta_rank >= 3
    AND new_rank > 0
    AND new_rank <= 50
    AND lower(price_direction) = 'up'
    AND current_price IS NOT NULL
    AND lower(exchange) = 'okx'
    AND UPPER(REPLACE(symbol, '-', '')) NOT LIKE 'LINKUSDT%'
    AND detected_at >= NOW() - ('${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS} hours')::interval
    ${event_id_clause}
  ORDER BY detected_at DESC, id DESC
  LIMIT 1
),
payload AS (
  SELECT
    id,
    jsonb_build_object(
      'source',
      'rust_quant',
      'external_id',
      'rust_quant:market_velocity:' || id::text || '${replay_external_id_suffix_sql}',
      'strategy_slug',
      'market_velocity',
      'strategy_key',
      'market_velocity:' || exchange || ':' || symbol,
      'symbol',
      symbol,
      'signal_type',
      'entry',
      'direction',
      'long',
      'title',
      'Market Velocity long signal ' || symbol,
      'summary',
      symbol || ' ranking improved from ' || COALESCE(old_rank::text, 'none') || ' to ' || COALESCE(new_rank::text, 'none') || ', delta ' || COALESCE(delta_rank::text, 'none') || ', price direction ' || price_direction,
      'confidence',
      ROUND(LEAST(0.95, 0.55 + (LEAST(GREATEST(COALESCE(delta_rank, 0), 0), 20) * 0.01) + (LEAST(GREATEST(COALESCE(price_change_pct, 0), 0), 10) * 0.005) + 0.05)::numeric, 2)::double precision,
      'payload_json',
      jsonb_build_object(
        'source',
        'rust_quant',
        'source_signal_type',
        'market_velocity',
        'rank_event_id',
        id,
        'event_type',
        event_type,
        'strategy_slug',
        'market_velocity',
        'strategy_key',
        'market_velocity:' || exchange || ':' || symbol,
        'exchange',
        exchange,
        'symbol',
        symbol,
        'timeframe',
        timeframe,
        'old_rank',
        old_rank,
        'new_rank',
        new_rank,
        'delta_rank',
        delta_rank,
        'volume_24h_quote',
        volume_24h_quote,
        'current_price',
        current_price,
        'previous_price',
        previous_price,
        'price_change_pct',
        price_change_pct,
        'price_direction',
        price_direction,
        'technical_snapshot_status',
        technical_snapshot_status,
        'technical_snapshot',
        CASE
          WHEN technical_snapshot_status = 'captured' THEN jsonb_build_object(
            'timeframe',
            technical_timeframe,
            'period',
            technical_period,
            'close_price',
            technical_close_price,
            'ma_value',
            technical_ma_value,
            'ema_value',
            technical_ema_value,
            'ma_distance_pct',
            technical_ma_distance_pct,
            'ema_distance_pct',
            technical_ema_distance_pct,
            'ma_state',
            technical_ma_state,
            'ema_state',
            technical_ema_state,
            'candle_count',
            technical_candle_count,
            'snapshot_at',
            to_char(technical_snapshot_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
          )
          ELSE NULL
        END,
        'side',
        'buy',
        'position_side',
        'long',
        'trade_side',
        'open',
        'order_type',
        'market',
        'risk_plan',
        jsonb_build_object(
          'entry_price',
          current_price,
          'selected_stop_loss_price',
          ROUND((current_price * (1 - ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT}::numeric))::numeric, 6),
          'direction',
          'long',
          'protective_stop_loss_required',
          true,
          'stop_loss_source',
          'market_velocity_default_stop_loss_pct',
          'stop_loss_percent',
          ${MARKET_VELOCITY_SIGNAL_STOP_LOSS_PCT}::numeric
        ),
        'detected_at',
        to_char(detected_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
      )::text,
      'generated_at',
      to_char(detected_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
    )::text AS request_json
  FROM candidate
)
SELECT request_json
FROM payload;
"
)"

if [[ -z "${candidate_payload}" ]]; then
    blocker "eligible_okx_market_velocity_event_missing" "set MARKET_VELOCITY_SIGNAL_EVENT_ID or extend lookback"
    echo
    echo "replay=blocked failures=${failures}"
    exit 2
fi

echo
echo "candidate_request_json:"
printf '%s\n' "${candidate_payload}" | pretty_json | sed 's/^/  /'

candidate_symbol="$(json_field "${candidate_payload}" "symbol")"
candidate_exchange="$(json_field "${candidate_payload}" "payload_json.exchange")"

web_preflight_failures=0
web_blocker() {
    local code="$1"
    local detail="${2:-}"
    if [[ -n "${detail}" ]]; then
        echo "  blocker=${code} detail=${detail}"
    else
        echo "  blocker=${code}"
    fi
    web_preflight_failures=$((web_preflight_failures + 1))
}

echo
echo "web_task_generation_preflight:"
web_preflight_row="$(
    query_web "
WITH ready_okx_buyers AS (
  SELECT buyer_email, md5(buyer_email) AS buyer_hash
  FROM user_api_credentials
  WHERE lower(exchange) = 'okx'
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
  GROUP BY buyer_email
),
eligible_combos AS (
  SELECT
    c.id,
    c.buyer_email,
    md5(c.buyer_email) AS buyer_hash,
    c.strategy_slug,
    c.symbol,
    COALESCE(c.execution_exchange, '') AS execution_exchange,
    c.service_mode,
    c.status AS combo_status,
    c.expired_at,
    r.status AS risk_status,
    r.risk_acknowledged,
    r.max_position_usdt,
    r.max_daily_loss_usdt,
    r.max_daily_trades
  FROM strategy_combo_subscriptions c
  JOIN ready_okx_buyers b
    ON b.buyer_email = c.buyer_email
  JOIN combo_risk_settings r
    ON r.combo_id = c.id
   AND r.buyer_email = c.buyer_email
  WHERE (c.strategy_slug IN ('market_velocity', 'market_velocity_radar')
     OR c.symbol = 'MARKET-VELOCITY-ALL')
    AND c.symbol = 'MARKET-VELOCITY-ALL'
    AND lower(COALESCE(c.execution_exchange, '')) = 'okx'
    AND c.service_mode = 'api_trade_enabled'
    AND c.status = 'active'
    AND c.expired_at >= NOW()
    AND r.status = 'active'
    AND r.risk_acknowledged = TRUE
    AND r.max_position_usdt > 0
    AND r.max_position_usdt <= ${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}
    AND r.max_daily_loss_usdt > 0
    AND r.max_daily_loss_usdt <= ${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}
    AND r.max_daily_trades = 1
),
fresh_risk_snapshots AS (
  SELECT DISTINCT s.buyer_email
  FROM user_execution_risk_snapshots s
  JOIN eligible_combos c
    ON c.buyer_email = s.buyer_email
  WHERE lower(s.exchange) = lower('${candidate_exchange}')
    AND (lower(s.symbol) = lower('${candidate_symbol}') OR s.symbol = '*')
    AND s.status = 'active'
    AND s.expires_at >= NOW()
),
active_task_conflicts AS (
  SELECT et.id
  FROM execution_tasks et
  JOIN eligible_combos c
    ON c.id = et.combo_id
   AND c.buyer_email = et.buyer_email
  WHERE et.strategy_slug = c.strategy_slug
    AND lower(et.symbol) = lower('${candidate_symbol}')
    AND et.task_status NOT IN (
      'completed',
      'failed',
      'cancelled',
      'canceled',
      'expired',
      'blocked'
    )
),
position_conflicts AS (
  SELECT p.id
  FROM user_position_snapshots p
  JOIN eligible_combos c
    ON c.id = p.combo_id
   AND c.buyer_email = p.buyer_email
  WHERE lower(p.symbol) = lower('${candidate_symbol}')
    AND p.quantity > 0
)
SELECT
  COUNT(DISTINCT c.id)::text AS eligible_combo_count,
  COALESCE(MAX(c.id)::text, 'none') AS combo_id,
  COALESCE(MAX(c.buyer_hash), 'none') AS buyer_hash,
  COUNT(DISTINCT s.buyer_email)::text AS fresh_risk_snapshot_count,
  (SELECT COUNT(*) FROM active_task_conflicts)::text AS active_task_conflict_count,
  (SELECT COUNT(*) FROM position_conflicts)::text AS position_conflict_count
FROM eligible_combos c
LEFT JOIN fresh_risk_snapshots s
  ON s.buyer_email = c.buyer_email;
"
)"
IFS=$'\t' read -r eligible_combo_count preflight_combo_id preflight_buyer_hash fresh_risk_snapshot_count active_task_conflict_count position_conflict_count <<<"${web_preflight_row}"

echo "  candidate=${candidate_exchange}:${candidate_symbol}"
echo "  eligible_combo_count=${eligible_combo_count}"
if [[ "${preflight_combo_id}" != "none" ]]; then
    echo "  combo_id=${preflight_combo_id}"
fi
if [[ "${preflight_buyer_hash}" != "none" ]]; then
    echo "  buyer_hash=${preflight_buyer_hash}"
fi
echo "  fresh_risk_snapshot_count=${fresh_risk_snapshot_count}"
echo "  active_task_conflict_count=${active_task_conflict_count}"
echo "  position_conflict_count=${position_conflict_count}"

if [[ "${eligible_combo_count}" != "1" ]]; then
    web_blocker "eligible_okx_market_velocity_combo_not_unique" "${eligible_combo_count}"
fi
if [[ "${fresh_risk_snapshot_count}" != "1" ]]; then
    web_blocker "fresh_user_execution_risk_snapshot_missing" "${fresh_risk_snapshot_count}"
fi
if [[ "${active_task_conflict_count}" != "0" ]]; then
    web_blocker "active_execution_task_conflict" "${active_task_conflict_count}"
fi
if [[ "${position_conflict_count}" != "0" ]]; then
    web_blocker "active_position_conflict" "${position_conflict_count}"
fi

if [[ "${web_preflight_failures}" -gt 0 ]]; then
    echo "  web_task_generation_preflight=blocked failures=${web_preflight_failures}"
else
    echo "  web_task_generation_preflight=ok"
fi

if [[ "${MARKET_VELOCITY_SIGNAL_REPLAY_APPLY}" != "true" ]]; then
    echo
    echo "replay=dry_run"
    echo "apply_requirements=MARKET_VELOCITY_SIGNAL_REPLAY_APPLY=true MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM=${REPLAY_CONFIRM_PHRASE}"
    exit 0
fi

if [[ "${MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM}" != "${REPLAY_CONFIRM_PHRASE}" ]]; then
    blocker "replay_confirmation_missing" "set MARKET_VELOCITY_SIGNAL_REPLAY_CONFIRM=${REPLAY_CONFIRM_PHRASE}"
    echo
    echo "replay=blocked failures=${failures}"
    exit 2
fi

if [[ "${web_preflight_failures}" -gt 0 ]]; then
    blocker "web_task_generation_preflight_blocked" "failures=${web_preflight_failures}"
    echo
    echo "replay=blocked failures=${failures}"
    exit 2
fi

if ! command -v curl >/dev/null 2>&1; then
    blocker "curl_missing"
    echo
    echo "replay=blocked failures=${failures}"
    exit 2
fi

response="$(
    curl -fsS -m 10 \
        -H "content-type: application/json" \
        -H "x-alpha-execution-secret: ${EXECUTION_EVENT_SECRET}" \
        --data-binary "${candidate_payload}" \
        "${RUST_QUAN_WEB_BASE_URL%/}/api/commerce/internal/strategy-signals"
)"

echo
echo "web_response:"
printf '%s\n' "${response}" | pretty_json | sed 's/^/  /'
echo
echo "replay=applied"
