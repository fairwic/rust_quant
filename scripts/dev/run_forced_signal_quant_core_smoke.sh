#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${RUST_QUANT_SMOKE_FORCE_SIGNAL:="buy"}"
: "${WEB_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_web"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${POSTGRES_CONTAINER:="quant_core_postgres"}"
: "${POSTGRES_USER:="postgres"}"
: "${WEB_POSTGRES_DB:="quant_web"}"
: "${EXECUTION_DEMO_BUYER_EMAIL:="demo-exec-worker@example.com"}"

WEB_SEED_SCRIPT="${REPO_ROOT}/../rust_quan_web/backend/scripts/dev/seed_execution_demo_combo.sh"
WEB_BACKEND_ENV="${REPO_ROOT}/../rust_quan_web/backend/.env"
RESTORE_DEMO_CREDENTIAL=false
RESTORE_DEMO_RISK_SNAPSHOT=false
RESTORE_DEMO_POSITION=false

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

load_web_backend_credential_encryption_env() {
    if [[ ! -f "${WEB_BACKEND_ENV}" ]]; then
        return 0
    fi

    local line key value
    while IFS= read -r line; do
        line="${line%%$'\r'}"
        [[ -z "${line}" || "${line}" == \#* || "${line}" != *=* ]] && continue

        key="${line%%=*}"
        value="${line#*=}"
        case "${key}" in
            API_CREDENTIAL_ENCRYPTION_KEY|API_CREDENTIAL_ENCRYPTION_KEY_ID|API_CREDENTIAL_ENCRYPTION_PREVIOUS_KEYS)
                value="${value%\"}"
                value="${value#\"}"
                value="${value%\'}"
                value="${value#\'}"
                export "${key}=${value}"
                ;;
        esac
    done <"${WEB_BACKEND_ENV}"
}

seal_forced_signal_fixture_cipher() {
    local raw_value="$1"
    local field="$2"

    if ! command -v node >/dev/null 2>&1; then
        echo "Refusing to run: node is required to seal local v4 API credential fixtures." >&2
        return 1
    fi

    node - "${raw_value}" "${field}" "${EXECUTION_DEMO_BUYER_EMAIL}" <<'NODE'
const crypto = require('crypto');

const rawValue = process.argv[2];
const field = process.argv[3];
const buyerEmail = process.argv[4];
const exchange = '币安';
const provider = 'local_aes256gcm';
const key = (process.env.API_CREDENTIAL_ENCRYPTION_KEY || '').trim();
const keyId = (process.env.API_CREDENTIAL_ENCRYPTION_KEY_ID || '').trim();

if (key.length < 32) {
  console.error('API_CREDENTIAL_ENCRYPTION_KEY must be configured before sealing fixture credentials.');
  process.exit(2);
}
if (!keyId) {
  console.error('API_CREDENTIAL_ENCRYPTION_KEY_ID must be configured before sealing fixture credentials.');
  process.exit(2);
}

function base64url(value) {
  return Buffer.from(value).toString('base64url');
}

function shortSha256Ref(prefix, value) {
  return `${prefix}_sha256_${crypto.createHash('sha256').update(value).digest('hex').slice(0, 16)}`;
}

function sortedObject(entries) {
  return Object.fromEntries(Object.entries(entries).sort(([left], [right]) => left.localeCompare(right)));
}

// Mirrors rust_quan_web CredentialEnvelopeContext for local forced-signal fixtures.
const encryptionContext = sortedObject({
  app: 'rust_quan_web',
  purpose: 'api_credential',
  context_version: 'v1',
  buyer_email_ref: shortSha256Ref('email', buyerEmail.trim().toLowerCase()),
  exchange,
  field,
});
const keyIdRef = shortSha256Ref('local_key', keyId);
const metadata = {
  contract_version: 'v4',
  provider,
  algorithm: 'aes-256-gcm',
  key_id_ref: keyIdRef,
  encryption_context: encryptionContext,
  created_at: new Date().toISOString(),
  rotation: {
    active_key_id_ref: keyIdRef,
    previous_key_id_refs: [],
    rollback_supported: true,
  },
};
const keyMaterial = crypto.createHash('sha256').update(key).digest();
const nonce = crypto.randomBytes(12);
const cipher = crypto.createCipheriv('aes-256-gcm', keyMaterial, nonce);
cipher.setAAD(Buffer.from(JSON.stringify(encryptionContext)));
const ciphertext = Buffer.concat([cipher.update(rawValue, 'utf8'), cipher.final(), cipher.getAuthTag()]);

process.stdout.write([
  'v4',
  provider,
  base64url(Buffer.from(keyId, 'utf8')),
  base64url(nonce),
  base64url(Buffer.from(JSON.stringify(metadata), 'utf8')),
  base64url(ciphertext),
].join(':'));
NODE
}

restore_demo_credential() {
    if [[ "${RESTORE_DEMO_CREDENTIAL}" != "true" ]]; then
        return
    fi

    run_web_sql <<SQL >/dev/null 2>&1 || true
UPDATE user_api_credentials
SET api_key_cipher = '0d0313090d00121c02120b4e09420211050008171c',
    api_secret_cipher = '0d0313090d00121c02120b4e09420211050010170616001a',
    passphrase_cipher = NULL,
    api_key_mask = 'loca****-key',
    permission_scope = '只读 + 下单',
    status = 'active',
    last_check_at = NOW(),
    last_check_code = 'local_smoke',
    last_check_message = 'Local dry-run seed only. Do not use for real exchange orders.',
    updated_at = NOW()
WHERE buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
  AND exchange = '币安';
SQL
}

restore_demo_risk_snapshot() {
    if [[ "${RESTORE_DEMO_RISK_SNAPSHOT}" != "true" ]]; then
        return
    fi

    run_web_sql <<SQL >/dev/null 2>&1 || true
DELETE FROM user_execution_risk_snapshots
WHERE buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
  AND exchange = 'binance'
  AND symbol = 'ETH-USDT-SWAP'
  AND snapshot_source = 'fixture_signed_read_only_preflight';
SQL
}

restore_demo_position() {
    if [[ "${RESTORE_DEMO_POSITION}" != "true" ]]; then
        return
    fi

    run_web_sql <<SQL >/dev/null 2>&1 || true
WITH combo AS (
  SELECT id
  FROM strategy_combo_subscriptions
  WHERE buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
    AND strategy_slug = 'vegas'
    AND symbol = 'ETH-USDT-SWAP'
    AND source = 'local_smoke'
  ORDER BY id DESC
  LIMIT 1
)
UPDATE user_position_snapshots snapshot
SET exchange = '币安',
    side = 'buy',
    quantity = 0.01000000,
    quote_amount = 35.00000000,
    snapshot_source = 'execution_result',
    snapshot_at = NOW() - INTERVAL '6 minutes',
    updated_at = NOW()
FROM combo
WHERE snapshot.buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
  AND snapshot.combo_id = combo.id
  AND snapshot.symbol = 'ETH-USDT-SWAP';
SQL
}

restore_demo_fixtures() {
    restore_demo_position
    restore_demo_risk_snapshot
    restore_demo_credential
}

trap restore_demo_fixtures EXIT

reset_forced_signal_open_tasks() {
    run_web_sql <<SQL >/dev/null
WITH target_tasks AS (
  SELECT et.id
  FROM execution_tasks et
  JOIN strategy_signal_inbox signal ON signal.id = et.strategy_signal_id
  WHERE et.buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
    AND et.strategy_slug = 'vegas'
    AND LOWER(et.symbol) = LOWER('ETH-USDT-SWAP')
    AND signal.source = 'rust_quant'
    AND signal.strategy_slug = 'vegas'
    AND LOWER(signal.symbol) = LOWER('ETH-USDT-SWAP')
    AND et.task_status NOT IN (
      'completed',
      'failed',
      'cancelled',
      'canceled',
      'expired',
      'blocked'
    )
)
UPDATE execution_tasks et
SET task_status = 'blocked',
    lease_owner = 'forced_signal_smoke_reset',
    lease_until = NULL,
    updated_at = NOW()
FROM target_tasks
WHERE et.id = target_tasks.id;
SQL
}

cd "${REPO_ROOT}"

if [[ ! -x "${WEB_SEED_SCRIPT}" ]]; then
    echo "Refusing to run: missing executable Web seed script: ${WEB_SEED_SCRIPT}" >&2
    exit 2
fi

echo
echo "Seeding Web demo combo for forced Vegas signal"
DATABASE_URL="${WEB_DATABASE_URL}" \
POSTGRES_CONTAINER="${POSTGRES_CONTAINER}" \
POSTGRES_USER="${POSTGRES_USER}" \
POSTGRES_DB="${WEB_POSTGRES_DB}" \
EXECUTION_DEMO_BUYER_EMAIL="${EXECUTION_DEMO_BUYER_EMAIL}" \
TRADE_SIGNAL_SMOKE_STRATEGY_SLUG=vegas \
EXECUTION_DEMO_STRATEGY_TITLE="Vegas Strategy Smoke" \
TRADE_SIGNAL_SMOKE_SYMBOL=ETH-USDT-SWAP \
    "${WEB_SEED_SCRIPT}"

echo
echo "Temporarily enabling signed-preflight fixture credential for dry-run task generation"
echo "Loading Web backend credential encryption config for local fixture sealing"
load_web_backend_credential_encryption_env
FORCED_SIGNAL_API_KEY_CIPHER="$(seal_forced_signal_fixture_cipher "forced-signal-fixture-api-key" "api_key")"
FORCED_SIGNAL_API_SECRET_CIPHER="$(seal_forced_signal_fixture_cipher "forced-signal-fixture-api-secret" "api_secret")"
run_web_sql <<SQL >/dev/null
UPDATE user_api_credentials
SET api_key_cipher = '${FORCED_SIGNAL_API_KEY_CIPHER}',
    api_secret_cipher = '${FORCED_SIGNAL_API_SECRET_CIPHER}',
    passphrase_cipher = NULL,
    api_key_mask = 'BIN****TURE',
    permission_scope = 'read,trade',
    status = 'active',
    last_check_at = NOW(),
    last_check_code = 'signed_exchange_preflight_passed',
    last_check_message = 'Local forced-signal dry-run fixture. Restored to local_smoke after script exit.',
    updated_at = NOW()
WHERE buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
  AND exchange = '币安';
SQL
RESTORE_DEMO_CREDENTIAL=true

echo
echo "Temporarily seeding signed-preflight risk snapshot for dry-run task generation"
run_web_sql <<SQL >/dev/null
INSERT INTO user_execution_risk_snapshots (
    buyer_email, exchange, symbol, account_equity_usdt,
    available_margin_usdt, remaining_daily_loss_usdt,
    strategy_max_drawdown_percent, risk_per_trade_percent, max_leverage,
    status, snapshot_source, snapshot_at, expires_at, created_at, updated_at
) VALUES (
    '${EXECUTION_DEMO_BUYER_EMAIL}', 'binance', 'ETH-USDT-SWAP', 10000.0,
    4000.0, 250.0,
    0.18, 0.01, 3.0,
    'active', 'fixture_signed_read_only_preflight', NOW(), NOW() + INTERVAL '30 minutes', NOW(), NOW()
)
ON CONFLICT (buyer_email, exchange, symbol) DO UPDATE SET
    account_equity_usdt = EXCLUDED.account_equity_usdt,
    available_margin_usdt = EXCLUDED.available_margin_usdt,
    remaining_daily_loss_usdt = EXCLUDED.remaining_daily_loss_usdt,
    strategy_max_drawdown_percent = EXCLUDED.strategy_max_drawdown_percent,
    risk_per_trade_percent = EXCLUDED.risk_per_trade_percent,
    max_leverage = EXCLUDED.max_leverage,
    status = EXCLUDED.status,
    snapshot_source = EXCLUDED.snapshot_source,
    snapshot_at = EXCLUDED.snapshot_at,
    expires_at = EXCLUDED.expires_at,
    updated_at = EXCLUDED.updated_at;
SQL
RESTORE_DEMO_RISK_SNAPSHOT=true

echo
echo "Temporarily clearing demo position snapshot for forced dry-run entry"
run_web_sql <<SQL >/dev/null
WITH combo AS (
  SELECT id
  FROM strategy_combo_subscriptions
  WHERE buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
    AND strategy_slug = 'vegas'
    AND symbol = 'ETH-USDT-SWAP'
    AND source = 'local_smoke'
  ORDER BY id DESC
  LIMIT 1
)
UPDATE user_position_snapshots snapshot
SET quantity = 0,
    quote_amount = 0,
    snapshot_source = 'forced_signal_position_clear',
    snapshot_at = NOW(),
    updated_at = NOW()
FROM combo
WHERE snapshot.buyer_email = '${EXECUTION_DEMO_BUYER_EMAIL}'
  AND snapshot.combo_id = combo.id
  AND snapshot.symbol = 'ETH-USDT-SWAP';
SQL
RESTORE_DEMO_POSITION=true

echo
echo "Resetting stale forced-signal open execution tasks for repeatable dry-run smoke"
reset_forced_signal_open_tasks

BASE_SIGNAL_ID="$(query_web_scalar "SELECT COALESCE(MAX(id), 0) FROM strategy_signal_inbox WHERE source = 'rust_quant' AND strategy_slug = 'vegas' AND symbol = 'ETH-USDT-SWAP';")"

export RUST_QUANT_SMOKE_FORCE_SIGNAL
export RUST_QUANT_SMOKE_EXTERNAL_ID_SUFFIX
export RUST_QUAN_WEB_BASE_URL
export EXECUTION_EVENT_SECRET
export POSTGRES_CONTAINER
export POSTGRES_USER
export WEB_POSTGRES_DB
export LIVE_STRATEGY_ONLY_INST_IDS=ETH-USDT-SWAP
export LIVE_STRATEGY_ONLY_PERIODS=4H
export STRATEGY_SIGNAL_DISPATCH_MODE=web
export EXECUTION_WORKER_DRY_RUN=true
export RUN_EXECUTION_WORKER_AFTER_STRATEGY=false

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
echo "Running execution worker dry-run for forced task ${NEW_TASK_ID}"
EXECUTION_WORKER_DRY_RUN=true \
EXECUTION_WORKER_LEASE_LIMIT=1 \
EXECUTION_WORKER_TASK_TYPES=execute_signal \
EXECUTION_WORKER_TASK_STATUSES=pending \
EXECUTION_WORKER_TARGET_TASK_IDS="${NEW_TASK_ID}" \
RUST_QUAN_WEB_BASE_URL="${RUST_QUAN_WEB_BASE_URL}" \
EXECUTION_EVENT_SECRET="${EXECUTION_EVENT_SECRET}" \
    ./scripts/dev/run_execution_worker_dry_run.sh

TASK_STATUS="$(query_web_scalar "SELECT task_status FROM execution_tasks WHERE id = ${NEW_TASK_ID} AND task_status IN ('completed', 'pending_protection_sync');")"
if [[ -z "${TASK_STATUS}" ]]; then
    echo "Expected task ${NEW_TASK_ID} to be completed or pending_protection_sync after dry-run worker." >&2
    run_web_sql <<SQL >&2 || true
SELECT id, task_status, lease_owner, lease_until, updated_at
FROM execution_tasks
WHERE id = ${NEW_TASK_ID};

SELECT id, attempt_no, attempt_status, executor, error_message, created_at, updated_at
FROM execution_task_attempts
WHERE execution_task_id = ${NEW_TASK_ID}
ORDER BY id;

SELECT id, exchange, external_order_id, order_side, order_status, filled_qty, filled_quote, created_at, updated_at
FROM exchange_order_results
WHERE execution_task_id = ${NEW_TASK_ID}
ORDER BY id;
SQL
    exit 1
fi

ATTEMPT_COUNT="$(query_web_scalar "SELECT COUNT(*) FROM execution_task_attempts WHERE execution_task_id = ${NEW_TASK_ID};")"
if (( ATTEMPT_COUNT < 1 )); then
    echo "Expected at least one execution_task_attempt for task ${NEW_TASK_ID}, got ${ATTEMPT_COUNT}." >&2
    exit 1
fi

DRY_RUN_ORDER_COUNT="$(query_web_scalar "SELECT COUNT(*) FROM exchange_order_results WHERE execution_task_id = ${NEW_TASK_ID} AND order_status = 'dry_run';")"
if [[ "${DRY_RUN_ORDER_COUNT}" != "1" ]]; then
    echo "Expected exactly one dry_run exchange_order_result for task ${NEW_TASK_ID}, got ${DRY_RUN_ORDER_COUNT}." >&2
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

SELECT
  eta.id,
  eta.execution_task_id,
  eta.attempt_no,
  eta.attempt_status,
  eta.executor,
  eta.error_message,
  eta.created_at,
  eta.updated_at
FROM execution_task_attempts eta
WHERE eta.execution_task_id = ${NEW_TASK_ID}
ORDER BY eta.id;

SELECT
  eor.id,
  eor.execution_task_id,
  eor.exchange,
  eor.external_order_id,
  eor.order_side,
  eor.order_status,
  eor.filled_qty,
  eor.filled_quote,
  eor.created_at,
  eor.updated_at
FROM exchange_order_results eor
WHERE eor.execution_task_id = ${NEW_TASK_ID}
ORDER BY eor.id;
SQL

echo
echo "Forced strategy smoke complete."
echo "Verified a fresh forced strategy signal, execution task, execution attempt, and dry-run order result after baseline signal id ${BASE_SIGNAL_ID}."
