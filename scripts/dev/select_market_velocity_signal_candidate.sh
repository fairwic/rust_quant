#!/usr/bin/env bash
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
QUANT_CORE_POSTGRES_DB="${QUANT_CORE_POSTGRES_DB:-quant_core}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS="${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS:-24}"
MARKET_VELOCITY_SIGNAL_LIMIT="${MARKET_VELOCITY_SIGNAL_LIMIT:-20}"

query_db() {
    local database="$1"
    local sql="$2"
    podman exec -i "${POSTGRES_CONTAINER}" psql \
        -U "${POSTGRES_USER}" \
        -d "${database}" \
        -XAt \
        -F $'\t' \
        -c "${sql}"
}

echo "== Market Velocity real signal candidate selector =="
echo "quant_core=${QUANT_CORE_POSTGRES_DB}"
echo "quant_web=${WEB_POSTGRES_DB}"
echo "lookback_hours=${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS}"
echo "limit=${MARKET_VELOCITY_SIGNAL_LIMIT}"
echo "mode=read_only"

echo
echo "eligible_core_rank_events:"
query_db "${QUANT_CORE_POSTGRES_DB}" "
SELECT
  id,
  exchange,
  symbol,
  event_type,
  old_rank,
  new_rank,
  delta_rank,
  current_price,
  previous_price,
  price_direction,
  detected_at
FROM market_rank_events
WHERE event_type IN ('rank_velocity', 'top_entry')
  AND delta_rank >= 3
  AND new_rank > 0
  AND new_rank <= 50
  AND lower(price_direction) = 'up'
  AND current_price IS NOT NULL
  AND UPPER(REPLACE(symbol, '-', '')) NOT LIKE 'LINKUSDT%'
  AND detected_at >= NOW() - ('${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS} hours')::interval
ORDER BY detected_at DESC, id DESC
LIMIT ${MARKET_VELOCITY_SIGNAL_LIMIT};
" | sed 's/^/  /'

echo
echo "recent_web_market_velocity_dispatches:"
query_db "${WEB_POSTGRES_DB}" "
SELECT
  s.id,
  s.external_id,
  s.symbol,
  s.generated_at,
  et.id AS task_id,
  et.task_status,
  COALESCE(et.request_payload_json::jsonb #>> '{execution,exchange}', et.request_payload_json::jsonb #>> '{exchange}', '') AS task_exchange,
  COALESCE(et.request_payload_json::jsonb #>> '{execution,size_usdt}', '') AS size_usdt
FROM strategy_signal_inbox s
LEFT JOIN execution_tasks et
  ON et.strategy_signal_id = s.id
WHERE s.external_id LIKE 'rust_quant:market_velocity:%'
  AND s.strategy_slug IN ('market_velocity', 'market_velocity_radar')
  AND s.generated_at >= NOW() - ('${MARKET_VELOCITY_SIGNAL_LOOKBACK_HOURS} hours')::interval
ORDER BY s.generated_at DESC, s.id DESC, et.id DESC
LIMIT ${MARKET_VELOCITY_SIGNAL_LIMIT};
" | sed 's/^/  /'

echo
echo "candidate_usage:"
echo "  If a core event is eligible but absent from recent_web_market_velocity_dispatches, it can be replayed through the Web internal strategy-signal API after config preflight is aligned."
echo "  This script does not submit or replay signals."
