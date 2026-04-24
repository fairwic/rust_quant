#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${MYSQL_CONTAINER:=mysql}"
: "${MYSQL_USER:=root}"
: "${MYSQL_PASSWORD:=example}"
: "${MYSQL_DATABASE:=test}"
: "${POSTGRES_CONTAINER:=postgres}"
: "${POSTGRES_USER:=postgres}"
: "${POSTGRES_DB:=quant_core}"
: "${QUANT_CORE_STRATEGY_EXCHANGE:=binance}"

if ! command -v podman >/dev/null 2>&1; then
    echo "podman is required" >&2
    exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required" >&2
    exit 1
fi

cd "${REPO_ROOT}"

echo "Preparing quant_core DDL"
./scripts/dev/ddl_smoke.sh >/dev/null

echo "Migrating legacy MySQL strategy_config rows into quant_core.strategy_configs"
echo "  mysql: ${MYSQL_CONTAINER}/${MYSQL_DATABASE}"
echo "  postgres: ${POSTGRES_CONTAINER}/${POSTGRES_DB}"
echo "  exchange: ${QUANT_CORE_STRATEGY_EXCHANGE}"

migrated=0
while IFS= read -r row_json; do
    legacy_id="$(jq -r '.legacy_id' <<<"${row_json}")"
    strategy_key="$(jq -r '.strategy_type | ascii_downcase' <<<"${row_json}")"
    symbol="$(jq -r '.inst_id' <<<"${row_json}")"
    timeframe="$(jq -r '.timeframe' <<<"${row_json}")"
    config_json="$(
        jq -c '(.value // {}) + {
          "_migration": {
            "source": "legacy_mysql.strategy_config",
            "legacy_id": .legacy_id,
            "kline_start_time": .kline_start_time,
            "kline_end_time": .kline_end_time,
            "final_fund": .final_fund
          }
        }' <<<"${row_json}"
    )"
    risk_json="$(jq -c '.risk_config // {}' <<<"${row_json}")"

    podman exec -i "${POSTGRES_CONTAINER}" psql \
        -U "${POSTGRES_USER}" \
        -d "${POSTGRES_DB}" \
        -v ON_ERROR_STOP=1 \
        -v legacy_id="${legacy_id}" \
        -v strategy_key="${strategy_key}" \
        -v strategy_name="${strategy_key}" \
        -v exchange="${QUANT_CORE_STRATEGY_EXCHANGE}" \
        -v symbol="${symbol}" \
        -v timeframe="${timeframe}" \
        -v config="${config_json}" \
        -v risk_config="${risk_json}" >/dev/null <<'SQL'
INSERT INTO strategy_configs (
    legacy_id,
    strategy_key,
    strategy_name,
    version,
    exchange,
    symbol,
    timeframe,
    enabled,
    config,
    risk_config
)
VALUES (
    :'legacy_id'::bigint,
    :'strategy_key',
    :'strategy_name',
    'legacy-mysql',
    :'exchange',
    :'symbol',
    :'timeframe',
    true,
    :'config'::jsonb,
    :'risk_config'::jsonb
)
ON CONFLICT (strategy_key, version, exchange, symbol, timeframe)
DO UPDATE SET
    legacy_id = EXCLUDED.legacy_id,
    strategy_name = EXCLUDED.strategy_name,
    enabled = EXCLUDED.enabled,
    config = EXCLUDED.config,
    risk_config = EXCLUDED.risk_config,
    updated_at = NOW();
SQL

    migrated=$((migrated + 1))
done < <(
    podman exec "${MYSQL_CONTAINER}" mysql \
        -u"${MYSQL_USER}" \
        -p"${MYSQL_PASSWORD}" \
        -D "${MYSQL_DATABASE}" \
        --batch \
        --raw \
        --skip-column-names \
        -e "
            SELECT JSON_OBJECT(
                'legacy_id', id,
                'strategy_type', strategy_type,
                'inst_id', inst_id,
                'timeframe', time,
                'value', COALESCE(CAST(value AS JSON), JSON_OBJECT()),
                'risk_config', COALESCE(CAST(risk_config AS JSON), JSON_OBJECT()),
                'kline_start_time', kline_start_time,
                'kline_end_time', kline_end_time,
                'final_fund', final_fund
            )
            FROM strategy_config
            WHERE is_deleted = 0
            ORDER BY id;
        " 2>/dev/null
)

echo "Migrated rows: ${migrated}"
echo
podman exec -i "${POSTGRES_CONTAINER}" psql \
    -U "${POSTGRES_USER}" \
    -d "${POSTGRES_DB}" \
    -v ON_ERROR_STOP=1 <<'SQL'
SELECT legacy_id, strategy_key, exchange, symbol, timeframe, enabled
FROM strategy_configs
WHERE legacy_id IS NOT NULL
ORDER BY legacy_id;
SQL
