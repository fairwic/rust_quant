#!/usr/bin/env bash
set -euo pipefail

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-quant_core_postgres}"
POSTGRES_USER="${POSTGRES_USER:-postgres}"
WEB_POSTGRES_DB="${WEB_POSTGRES_DB:-quant_web}"
MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT="${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT:-5}"

query_web() {
    local sql="$1"
    podman exec -i "${POSTGRES_CONTAINER}" psql \
        -U "${POSTGRES_USER}" \
        -d "${WEB_POSTGRES_DB}" \
        -XAt \
        -F $'\t' \
        -c "${sql}"
}

echo "== Market Velocity OKX live config plan =="
echo "max_notional=${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}"
echo "mode=read_only"

echo
echo "ready_okx_credentials:"
query_web "
WITH ready_okx_credentials AS (
  SELECT
    md5(buyer_email) AS buyer_hash,
    COUNT(*) AS credential_count,
    MAX(last_check_at) AS last_check_at
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
  GROUP BY md5(buyer_email)
)
SELECT buyer_hash, credential_count, last_check_at
FROM ready_okx_credentials
ORDER BY last_check_at DESC NULLS LAST, buyer_hash;
" | sed 's/^/  /'

echo
echo "market_velocity_combos:"
query_web "
WITH market_velocity_combos AS (
  SELECT
    c.id AS combo_id,
    md5(c.buyer_email) AS buyer_hash,
    c.strategy_slug,
    c.symbol,
    c.status AS combo_status,
    c.service_mode,
    COALESCE(c.execution_exchange, '') AS execution_exchange,
    (c.expired_at >= NOW()) AS subscription_valid,
    COALESCE(r.status, '') AS risk_status,
    COALESCE(r.risk_acknowledged, FALSE) AS risk_acknowledged,
    COALESCE(r.max_position_usdt::text, '') AS max_position_usdt,
    COALESCE(r.max_daily_loss_usdt::text, '') AS max_daily_loss_usdt,
    COALESCE(r.max_daily_trades::text, '') AS max_daily_trades
  FROM strategy_combo_subscriptions c
  LEFT JOIN combo_risk_settings r
    ON r.combo_id = c.id
   AND r.buyer_email = c.buyer_email
  WHERE c.strategy_slug IN ('market_velocity', 'market_velocity_radar')
     OR c.symbol = 'MARKET-VELOCITY-ALL'
)
SELECT
  combo_id,
  buyer_hash,
  strategy_slug,
  symbol,
  combo_status,
  service_mode,
  execution_exchange,
  subscription_valid,
  risk_status,
  risk_acknowledged,
  max_position_usdt,
  max_daily_loss_usdt,
  max_daily_trades
FROM market_velocity_combos
ORDER BY combo_id;
" | sed 's/^/  /'

echo
echo "matching_ready_buyers:"
query_web "
WITH ready_okx_credentials AS (
  SELECT
    md5(buyer_email) AS buyer_hash,
    COUNT(*) AS credential_count
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
  GROUP BY md5(buyer_email)
),
market_velocity_combos AS (
  SELECT
    c.id AS combo_id,
    md5(c.buyer_email) AS buyer_hash,
    c.strategy_slug,
    c.symbol,
    c.status AS combo_status,
    c.service_mode,
    COALESCE(c.execution_exchange, '') AS execution_exchange,
    (c.expired_at >= NOW()) AS subscription_valid,
    COALESCE(r.status, '') AS risk_status,
    COALESCE(r.risk_acknowledged, FALSE) AS risk_acknowledged,
    r.max_position_usdt,
    r.max_daily_loss_usdt,
    r.max_daily_trades
  FROM strategy_combo_subscriptions c
  LEFT JOIN combo_risk_settings r
    ON r.combo_id = c.id
   AND r.buyer_email = c.buyer_email
  WHERE c.strategy_slug IN ('market_velocity', 'market_velocity_radar')
     OR c.symbol = 'MARKET-VELOCITY-ALL'
),
matching_ready_buyers AS (
  SELECT
    c.combo_id,
    c.buyer_hash,
    c.strategy_slug,
    c.symbol,
    c.combo_status,
    c.service_mode,
    c.execution_exchange,
    c.subscription_valid,
    c.risk_status,
    c.risk_acknowledged,
    c.max_position_usdt,
    c.max_daily_loss_usdt,
    c.max_daily_trades,
    r.credential_count
  FROM market_velocity_combos c
  JOIN ready_okx_credentials r
    ON r.buyer_hash = c.buyer_hash
)
SELECT
  combo_id,
  buyer_hash,
  credential_count,
  CASE
    WHEN lower(execution_exchange) = 'okx'
     AND combo_status = 'active'
     AND service_mode = 'api_trade_enabled'
     AND subscription_valid
     AND risk_status = 'active'
     AND risk_acknowledged
     AND max_position_usdt > 0
     AND max_position_usdt <= ${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}
     AND max_daily_loss_usdt > 0
     AND max_daily_loss_usdt <= ${MARKET_VELOCITY_LIVE_MAX_NOTIONAL_USDT}
     AND max_daily_trades = 1
    THEN 'ready_for_signal_task_generation'
    ELSE 'needs_config_alignment'
  END AS configuration_plan,
  execution_exchange,
  max_position_usdt,
  max_daily_loss_usdt,
  max_daily_trades
FROM matching_ready_buyers
ORDER BY combo_id;
" | sed 's/^/  /'

echo
echo "gap_summary:"
query_web "
WITH ready_okx_credentials AS (
  SELECT md5(buyer_email) AS buyer_hash
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
  GROUP BY md5(buyer_email)
),
market_velocity_combos AS (
  SELECT
    c.id AS combo_id,
    md5(c.buyer_email) AS buyer_hash
  FROM strategy_combo_subscriptions c
  WHERE c.strategy_slug IN ('market_velocity', 'market_velocity_radar')
     OR c.symbol = 'MARKET-VELOCITY-ALL'
)
SELECT
  'ready_okx_buyers_without_market_velocity_combo' AS gap,
  COUNT(*)
FROM ready_okx_credentials r
LEFT JOIN market_velocity_combos c
  ON c.buyer_hash = r.buyer_hash
WHERE c.combo_id IS NULL
UNION ALL
SELECT
  'market_velocity_combos_without_ready_okx_credential' AS gap,
  COUNT(*)
FROM market_velocity_combos c
LEFT JOIN ready_okx_credentials r
  ON r.buyer_hash = c.buyer_hash
WHERE r.buyer_hash IS NULL;
" | sed 's/^/  /'
