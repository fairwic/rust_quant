#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${RUST_QUANT_SMOKE_FORCE_SIGNAL:="buy"}"
: "${WEB_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_web"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${POSTGRES_CONTAINER:="postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${WEB_POSTGRES_DB:="quant_web"}"

WEB_SEED_SCRIPT="${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh"

if [[ -z "${RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX:-}" ]]; then
    RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX="$(date -u +%Y%m%dT%H%M%SZ)-$$"
fi

run_web_sql() {
    if command -v psql >/dev/null 2>&1; then
        psql "${WEB_DATABASE_URL}" -v ON_ERROR_STOP=1 "$@"
        return
    fi

    if command -v podman >/dev/null 2>&1 &&
        podman container exists "${POSTGRES_CONTAINER}" >/dev/null 2>&1; then
        podman exec -i "${POSTGRES_CONTAINER}" psql \
            -U "${POSTGRES_USER}" \
            -d "${WEB_POSTGRES_DB}" \
            -v ON_ERROR_STOP=1 \
            "$@"
        return
    fi

    echo "Skipping Web verification: neither psql nor podman container '${POSTGRES_CONTAINER}' is available." >&2
    return 1
}

query_web_scalar() {
    run_web_sql -Atc "$1"
}

cd "${REPO_ROOT}"

if [[ ! -x "${WEB_SEED_SCRIPT}" ]]; then
    echo "Refusing to run: missing executable Web seed script: ${WEB_SEED_SCRIPT}" >&2
    exit 2
fi

echo
echo "Seeding Web demo combo for forced Vegas signal"
DATABASE_URL="${WEB_DATABASE_URL}" \
TRADE_SIGNAL_SMOKE_STRATEGY_SLUG=vegas \
EXECUTION_DEMO_STRATEGY_TITLE="Vegas Strategy Smoke" \
TRADE_SIGNAL_SMOKE_SYMBOL=ETH-USDT-SWAP \
    "${WEB_SEED_SCRIPT}"

BASE_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = 'vegas' AND symbol = 'ETH-USDT-SWAP';")"

export RUST_QUANT_SMOKE_FORCE_SIGNAL
export RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX
export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export STRATEGY_SIGNAL_DISPATCH_MODE=web
export EXECUTION_WORKER_DRY_RUN=true
export RUN_EXECUTION_WORKER_AFTER_STRATEGY=true

echo
echo "Running forced live strategy quant_core smoke"
echo "  force_signal: ${RUST_QUANT_SMOKE_FORCE_SIGNAL}"
echo "  external_id_suffix: ${RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX}"
echo "  baseline_signal_id: ${BASE_SIGNAL_ID}"
echo "  web: ${RUST_QUAN_WEB_BASE_URL}"
echo "  web_db: ${WEB_DATABASE_URL}"

./scripts/dev/run_live_strategy_quant_core_smoke.sh "$@"

NEW_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = 'vegas' AND symbol = 'ETH-USDT-SWAP' AND id > ${BASE_SIGNAL_ID};")"
if [[ -z "${NEW_SIGNAL_ID}" || "${NEW_SIGNAL_ID}" == "0" ]]; then
    echo "Expected a fresh rust_quant forced strategy signal after baseline id ${BASE_SIGNAL_ID}, but none was found." >&2
    exit 1
fi

NEW_TASK_ID="$(query_web_scalar "SELECT COALESCE(MAX(et.id), 0) FROM execution_tasks et JOIN strategy_signal_inbox ssi ON ssi.id = et.strategy_signal_id WHERE ssi.id = ${NEW_SIGNAL_ID};")"
if [[ -z "${NEW_TASK_ID}" || "${NEW_TASK_ID}" == "0" ]]; then
    echo "Expected an execution task for forced strategy signal id ${NEW_SIGNAL_ID}, but none was found." >&2
    exit 1
fi

echo
echo "Verifying latest rust_quant -> rust_quan_web forced strategy records"
run_web_sql <<SQL
SELECT
  id,
  source,
  external_id,
  strategy_slug,
  strategy_key,
  symbol,
  signal_type,
  generated_at
FROM strategy_signal_inbox
WHERE id = ${NEW_SIGNAL_ID};

SELECT
  et.id,
  et.strategy_signal_id,
  et.combo_id,
  et.buyer_email,
  et.strategy_slug,
  et.symbol,
  et.task_status,
  et.scheduled_at,
  et.updated_at
FROM execution_tasks et
JOIN strategy_signal_inbox ssi ON ssi.id = et.strategy_signal_id
WHERE et.id = ${NEW_TASK_ID};
SQL

echo
echo "Forced strategy smoke complete."
echo "Verified a fresh forced strategy signal and execution task after baseline signal id ${BASE_SIGNAL_ID}."
