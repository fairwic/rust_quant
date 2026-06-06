#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
WEB_REPO_ROOT="${REPO_ROOT}/../rust_quan_web"

: "${WEB_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_web"}"
: "${RUST_QUAN_WEB_BASE_URL:="http://127.0.0.1:8000"}"
: "${EXECUTION_EVENT_SECRET:="local-dev-secret"}"
: "${POSTGRES_CONTAINER:=""}"
: "${RUSTUP_TOOLCHAIN:="1.91.1"}"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "missing required command: $1" >&2
        exit 1
    fi
}

psql_args() {
    if command -v psql >/dev/null 2>&1; then
        printf '%s\0' psql "${WEB_DATABASE_URL}"
        return
    fi

    if command -v podman >/dev/null 2>&1; then
        local container_name=""
        if [[ -n "${POSTGRES_CONTAINER}" ]]; then
            container_name="${POSTGRES_CONTAINER}"
        else
            local candidate=""
            for candidate in quant_core_postgres postgres pgsql; do
                if podman container exists "${candidate}" >/dev/null 2>&1 &&
                    [[ "$(podman inspect -f '{{.State.Running}}' "${candidate}" 2>/dev/null)" == "true" ]]; then
                    container_name="${candidate}"
                    break
                fi
            done
            if [[ -z "${container_name}" ]]; then
                while IFS= read -r candidate; do
                    case "${candidate}" in
                        *postgres* | *pgsql* | *pg*)
                            if [[ "$(podman inspect -f '{{.State.Running}}' "${candidate}" 2>/dev/null)" == "true" ]]; then
                                container_name="${candidate}"
                                break
                            fi
                            ;;
                    esac
                done < <(podman ps --format '{{.Names}}' 2>/dev/null)
            fi
        fi

        if [[ -n "${container_name}" ]] &&
            podman container exists "${container_name}" >/dev/null 2>&1 &&
            [[ "$(podman inspect -f '{{.State.Running}}' "${container_name}" 2>/dev/null)" == "true" ]]; then
            printf '%s\0' podman exec "${container_name}" psql "${WEB_DATABASE_URL}"
            return
        fi
    fi

    echo "psql was not found, and no running podman Postgres container was available for ${WEB_DATABASE_URL}" >&2
    exit 1
}

run_web_sql_tsv() {
    local sql="$1"
    local -a cmd=()
    while IFS= read -r -d '' arg; do
        cmd+=("$arg")
    done < <(psql_args)

    "${cmd[@]}" -v ON_ERROR_STOP=1 -At -F $'\t' -c "$sql"
}

query_web_scalar() {
    run_web_sql_tsv "$1"
}

require_cmd curl

if [[ ! -d "${WEB_REPO_ROOT}/backend" ]]; then
    echo "missing rust_quan_web backend at ${WEB_REPO_ROOT}/backend" >&2
    exit 2
fi

if ! curl -fsS -m 3 "${RUST_QUAN_WEB_BASE_URL}/" >/dev/null; then
    echo "Web backend is not reachable at ${RUST_QUAN_WEB_BASE_URL}" >&2
    exit 2
fi

baseline_signal_id="$(
    query_web_scalar "
        SELECT COALESCE(MAX(id), 0)
        FROM strategy_signal_inbox
        WHERE source = 'rust_quant'
          AND strategy_slug = 'market_velocity'
          AND symbol = 'ETHUSDT';
    "
)"

echo "== Market Velocity dry-run e2e smoke =="
echo "web=${RUST_QUAN_WEB_BASE_URL}"
echo "web_db=${WEB_DATABASE_URL}"
echo "baseline_signal_id=${baseline_signal_id}"

echo
echo "Seeding Web Market Velocity runtime fixture"
(
    cd "${WEB_REPO_ROOT}"
    DATABASE_URL="${WEB_DATABASE_URL}" \
        RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN}" \
        cargo test -p backend seed_market_velocity_runtime_fixture_for_dry_run_worker -- --ignored --nocapture
)

echo
echo "Dispatching synthetic Core Market Velocity event to Web"
(
    cd "${REPO_ROOT}"
    MARKET_VELOCITY_SIGNAL_DISPATCH_MODE=web \
        MARKET_VELOCITY_STRATEGY_SLUG=market_velocity \
        RUST_QUAN_WEB_BASE_URL="${RUST_QUAN_WEB_BASE_URL}" \
        EXECUTION_EVENT_SECRET="${EXECUTION_EVENT_SECRET}" \
        RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN}" \
        cargo test -p rust-quant-services market_velocity_synthetic_event_dispatches_to_running_quant_web -- --ignored --nocapture
)

new_signal_id="$(
    query_web_scalar "
        SELECT COALESCE(MAX(id), 0)
        FROM strategy_signal_inbox
        WHERE source = 'rust_quant'
          AND strategy_slug = 'market_velocity'
          AND symbol = 'ETHUSDT'
          AND id > ${baseline_signal_id};
    "
)"
if [[ -z "${new_signal_id}" || "${new_signal_id}" == "0" ]]; then
    echo "Expected a fresh Market Velocity strategy signal after baseline id ${baseline_signal_id}, but none was found." >&2
    exit 1
fi

new_task_id="$(
    query_web_scalar "
        SELECT COALESCE(MAX(et.id), 0)
        FROM execution_tasks et
        WHERE et.strategy_signal_id = ${new_signal_id};
    "
)"
if [[ -z "${new_task_id}" || "${new_task_id}" == "0" ]]; then
    echo "Expected an execution task for Market Velocity strategy signal id ${new_signal_id}, but none was found." >&2
    exit 1
fi

source_signal_type="$(
    query_web_scalar "
        SELECT et.request_payload_json::jsonb #>> '{source_signal_type}'
        FROM execution_tasks et
        WHERE et.id = ${new_task_id};
    "
)"
if [[ "${source_signal_type}" != "market_velocity" ]]; then
    echo "Expected task ${new_task_id} source_signal_type=market_velocity, got '${source_signal_type}'." >&2
    exit 1
fi

echo
echo "Running Core execution worker dry-run for task ${new_task_id}"
(
    cd "${REPO_ROOT}"
    RUST_QUAN_WEB_BASE_URL="${RUST_QUAN_WEB_BASE_URL}" \
        EXECUTION_EVENT_SECRET="${EXECUTION_EVENT_SECRET}" \
        EXECUTION_WORKER_DRY_RUN=true \
        EXECUTION_WORKER_LEASE_LIMIT=1 \
        EXECUTION_WORKER_TASK_TYPES=execute_signal \
        EXECUTION_WORKER_TASK_STATUSES=pending \
        EXECUTION_WORKER_TARGET_TASK_IDS="${new_task_id}" \
        RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN}" \
        ./scripts/dev/run_execution_worker_dry_run.sh
)

task_status="$(
    query_web_scalar "
        SELECT task_status
        FROM execution_tasks
        WHERE id = ${new_task_id}
          AND task_status IN ('completed', 'pending_protection_sync');
    "
)"
if [[ -z "${task_status}" ]]; then
    echo "Expected task ${new_task_id} to be completed or pending_protection_sync after dry-run worker." >&2
    exit 1
fi

dry_run_order_count="$(
    query_web_scalar "
        SELECT COUNT(*)
        FROM exchange_order_results
        WHERE execution_task_id = ${new_task_id}
          AND order_status = 'dry_run';
    "
)"
if [[ "${dry_run_order_count}" != "1" ]]; then
    echo "Expected exactly one dry_run order result for task ${new_task_id}, got ${dry_run_order_count}." >&2
    exit 1
fi

echo
echo "Verified Market Velocity dry-run task"
run_web_sql_tsv "
    SELECT
      et.id,
      et.strategy_signal_id,
      et.strategy_slug,
      et.symbol,
      et.task_status,
      et.request_payload_json::jsonb #>> '{source_signal_type}' AS source_signal_type
    FROM execution_tasks et
    WHERE et.id = ${new_task_id};
"

echo "market_velocity dry-run e2e smoke completed"
