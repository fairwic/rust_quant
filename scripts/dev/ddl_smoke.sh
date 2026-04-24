#!/usr/bin/env bash
set -euo pipefail

CONTAINER="${POSTGRES_CONTAINER:-postgres}"
DB_USER="${POSTGRES_USER:-postgres}"
DB_NAME="${POSTGRES_DB:-quant_core}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DDL_FILE="${QUANT_CORE_DDL_FILE:-$REPO_DIR/sql/postgres_quant_core.sql}"

if ! command -v podman >/dev/null 2>&1; then
  echo "podman is required" >&2
  exit 1
fi

if [[ ! -f "$DDL_FILE" ]]; then
  echo "DDL file not found: $DDL_FILE" >&2
  exit 1
fi

echo "Applying quant_core DDL"
echo "  container: $CONTAINER"
echo "  database: $DB_NAME"
echo "  ddl: $DDL_FILE"

podman exec -i "$CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 < "$DDL_FILE"

echo
echo "Key quant_core tables in public schema:"
podman exec -i "$CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 <<'SQL'
SELECT table_name
FROM information_schema.tables
WHERE table_schema = 'public'
  AND table_name IN (
    'strategy_configs',
    'risk_configs',
    'market_candles',
    'market_snapshots',
    'indicator_snapshots',
    'strategy_signals',
    'strategy_run_states',
    'backtest_runs',
    'backtest_results',
    'backtest_trades',
    'execution_worker_checkpoints',
    'exchange_request_audit_logs'
    ,'strategy_config'
    ,'back_test_log'
    ,'back_test_detail'
    ,'back_test_analysis'
    ,'filtered_signal_log'
    ,'dynamic_config_log'
    ,'strategy_run'
    ,'signal_snapshot_log'
    ,'risk_decision_log'
    ,'order_decision_log'
    ,'strategy_job_signal_log'
    ,'funding_rates'
    ,'tickers_data'
    ,'tickers_volume'
    ,'external_market_snapshots'
    ,'exchange_symbols'
    ,'exchange_symbol_listing_events'
  )
ORDER BY table_name;
SQL

echo
echo "quant_core DDL smoke complete."
