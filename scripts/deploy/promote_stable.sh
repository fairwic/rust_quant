#!/usr/bin/env bash
set -euo pipefail

: "${DEPLOY_SSH_USER:?DEPLOY_SSH_USER is required}"
: "${DEPLOY_SSH_HOST:?DEPLOY_SSH_HOST is required}"
: "${SERVER_APP_PATH:?SERVER_APP_PATH is required}"
: "${DEPLOY_IMAGE:?DEPLOY_IMAGE is required}"

compose_file="${DEPLOY_COMPOSE_FILE:-docker-compose.deploy.yml}"
compose_source_file="${DEPLOY_COMPOSE_SOURCE_FILE:-docker-compose.deploy.yml}"
services_csv="${DEPLOY_SERVICES:-quant-core-internal-server,quant-core-market-velocity-radar,quant-core-market-velocity-paper-observation-scheduler,quant-core-market-velocity-live-handoff-scheduler,quant-core-execution-worker}"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"
ssh_host_input="${DEPLOY_SSH_HOST}"
ssh_port="${DEPLOY_SSH_PORT:-}"
ssh_host="${ssh_host_input}"

if [[ "${ssh_host_input}" == *"@"* ]]; then
  echo "DEPLOY_SSH_HOST must not include a username. Set DEPLOY_SSH_USER separately." >&2
  exit 1
fi

if [[ "${ssh_host_input}" =~ ^\[([^]]+)\]:(.+)$ ]]; then
  if [[ -z "${ssh_port}" ]]; then
    ssh_port="${BASH_REMATCH[2]}"
  fi
  ssh_host="${BASH_REMATCH[1]}"
elif [[ "${ssh_host_input}" =~ ^([^:]+):([0-9]+)$ ]]; then
  if [[ -z "${ssh_port}" ]]; then
    ssh_port="${BASH_REMATCH[2]}"
  fi
  ssh_host="${BASH_REMATCH[1]}"
fi

ssh_port="${ssh_port:-22}"
remote_compose_file=".deploy/current.$(basename "${compose_file}")"

if [ ! -f "${compose_source_file}" ]; then
  echo "deploy compose source missing: ${compose_source_file}" >&2
  exit 1
fi

ssh -p "${ssh_port}" "${DEPLOY_SSH_USER}@${ssh_host}" "cd '${SERVER_APP_PATH}' && mkdir -p .deploy"
scp -P "${ssh_port}" "${compose_source_file}" "${DEPLOY_SSH_USER}@${ssh_host}:${SERVER_APP_PATH}/${remote_compose_file}"

ssh -p "${ssh_port}" "${DEPLOY_SSH_USER}@${ssh_host}" \
  env \
  "DEPLOY_GHCR_USERNAME=${ghcr_username}" \
  "DEPLOY_GHCR_TOKEN=${ghcr_token}" \
  bash -s -- \
  "${SERVER_APP_PATH}" \
  "${remote_compose_file}" \
  "${services_csv}" \
  "${DEPLOY_IMAGE}" <<'REMOTE'
set -euo pipefail

server_app_path="$1"
compose_file="$2"
services_csv="$3"
target_image="$4"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"

cd "${server_app_path}"
mkdir -p .deploy
compose_project_name="${DEPLOY_COMPOSE_PROJECT_NAME:-$(basename "$(pwd)")}"
compose() {
  docker compose \
    --project-directory "${server_app_path}" \
    --project-name "${compose_project_name}" \
    --profile schema-ensure \
    --profile observation-scheduler \
    --profile live-handoff-scheduler \
    -f "${compose_file}" \
    "$@"
}

if [ -n "${ghcr_username}" ] && [ -n "${ghcr_token}" ]; then
  printf '%s' "${ghcr_token}" | docker login ghcr.io -u "${ghcr_username}" --password-stdin > /dev/null
fi

assert_services_running() {
  local compose_file="$1"
  local override_file="$2"
  shift 2

  local timeout_secs="${DEPLOY_HEALTH_TIMEOUT_SECS:-45}"
  local stable_secs="${DEPLOY_HEALTH_STABLE_SECS:-10}"
  local poll_secs="${DEPLOY_HEALTH_POLL_SECS:-2}"
  local allowed_restarts="${DEPLOY_HEALTH_ALLOWED_RESTARTS:-0}"
  local deadline stable_since now all_ready service container_id running restarting restart_count status exit_code

  deadline="$(($(date +%s) + timeout_secs))"
  stable_since=0

  while true; do
    now="$(date +%s)"
    all_ready=1

    for service in "$@"; do
      service="$(printf '%s' "${service}" | xargs)"
      [ -z "${service}" ] && continue

      container_id="$(compose -f "${override_file}" ps --all -q "${service}" | head -n 1 || true)"
      if [ -z "${container_id}" ]; then
        all_ready=0
        continue
      fi

      running="$(docker inspect --format '{{.State.Running}}' "${container_id}")"
      restarting="$(docker inspect --format '{{.State.Restarting}}' "${container_id}")"
      restart_count="$(docker inspect --format '{{.RestartCount}}' "${container_id}")"
      status="$(docker inspect --format '{{.State.Status}}' "${container_id}")"
      exit_code="$(docker inspect --format '{{.State.ExitCode}}' "${container_id}")"

      if [ "${running}" != "true" ] ||
        [ "${restarting}" = "true" ] ||
        [ "${restart_count:-0}" -gt "${allowed_restarts}" ]; then
        all_ready=0
        echo "deployment service not stable yet: ${service} status=${status} running=${running} restarting=${restarting} restart_count=${restart_count} exit_code=${exit_code}" >&2
      fi
    done

    if [ "${all_ready}" = "1" ]; then
      if [ "${stable_since}" -eq 0 ]; then
        stable_since="${now}"
      fi
      if [ "$((now - stable_since))" -ge "${stable_secs}" ]; then
        return 0
      fi
    else
      stable_since=0
    fi

    if [ "${now}" -ge "${deadline}" ]; then
      echo "deployment services failed readiness within ${timeout_secs}s" >&2
      compose -f "${override_file}" config --services >&2 || true
      compose -f "${override_file}" ps --all >&2 || true
      for service in "$@"; do
        service="$(printf '%s' "${service}" | xargs)"
        [ -z "${service}" ] && continue
        compose -f "${override_file}" logs --tail=120 "${service}" >&2 || true
      done
      exit 1
    fi

    sleep "${poll_secs}"
  done
}

remove_conflicting_named_containers() {
  local service existing_container_id compose_container_id
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue

    existing_container_id="$(docker ps -aq --filter "name=^/${service}$" | head -n 1 || true)"
    [ -z "${existing_container_id}" ] && continue

    compose_container_id="$(compose ps --all -q "${service}" | head -n 1 || true)"
    if [ -n "${compose_container_id}" ] && [ "${existing_container_id}" = "${compose_container_id}" ]; then
      continue
    fi

    echo "removing stale deployment container name conflict: ${service} (${existing_container_id})" >&2
    docker rm -f "${existing_container_id}"
  done
}

normalize_live_mutation_env_value() {
  local value="${1:-}"
  value="$(printf '%s' "${value}" | sed -e 's/[[:space:]]#.*$//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  case "${value}" in
    \"*\") value="${value#\"}"; value="${value%\"}" ;;
    \'*\') value="${value#\'}"; value="${value%\'}" ;;
  esac
  printf '%s' "${value}"
}

live_mutation_env_value_is_dangerous() {
  local key="$1"
  local value
  value="$(normalize_live_mutation_env_value "${2:-}")"

  case "${key}" in
    MARKET_VELOCITY_CREATE_TASK_APPLY|MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY|MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED)
      [ "$(printf '%s' "${value}" | tr '[:upper:]' '[:lower:]')" = "true" ]
      ;;
    MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED|EXECUTION_WORKER_DRY_RUN)
      [ "$(printf '%s' "${value}" | tr '[:upper:]' '[:lower:]')" = "false" ]
      ;;
    MARKET_VELOCITY_CREATE_TASK_CONFIRM|MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM|EXECUTION_WORKER_TARGET_TASK_IDS|EXECUTION_WORKER_LIVE_ORDER_CONFIRM|LEGACY_DIRECT_LIVE_ORDER_CONFIRM|LEGACY_SIGNED_READ_ONLY_CONFIRM|RISK_BALANCE_LIVE_MUTATION_CONFIRM|PROTECTIVE_OUTCOME_CONFIRM)
      [ -n "${value}" ]
      ;;
    *)
      return 1
      ;;
  esac
}

read_dotenv_value() {
  local key="$1"
  local env_file="$2"
  local line
  line="$(grep -E "^[[:space:]]*(export[[:space:]]+)?${key}=" "${env_file}" | tail -n 1 || true)"
  [ -n "${line}" ] || return 1
  printf '%s' "${line#*=}"
}

assert_no_persistent_live_mutation_env_flags() {
  local env_file=".env"
  local found=0
  local key value
  local live_mutation_keys=(
    MARKET_VELOCITY_CREATE_TASK_APPLY
    MARKET_VELOCITY_CREATE_TASK_CONFIRM
    MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY
    MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM
    MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED
    MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED
    EXECUTION_WORKER_DRY_RUN
    EXECUTION_WORKER_TARGET_TASK_IDS
    EXECUTION_WORKER_LIVE_ORDER_CONFIRM
    LEGACY_DIRECT_LIVE_ORDER_CONFIRM
    LEGACY_SIGNED_READ_ONLY_CONFIRM
    RISK_BALANCE_LIVE_MUTATION_CONFIRM
    PROTECTIVE_OUTCOME_CONFIRM
  )

  for key in "${live_mutation_keys[@]}"; do
    value="${!key:-}"
    if live_mutation_env_value_is_dangerous "${key}" "${value}"; then
      echo "refusing deployment with persistent live mutation flag in process env: ${key}" >&2
      found=1
    fi

    if [ -f "${env_file}" ]; then
      value="$(read_dotenv_value "${key}" "${env_file}" || true)"
      if [ -n "${value}" ] && live_mutation_env_value_is_dangerous "${key}" "${value}"; then
        echo "refusing deployment with persistent live mutation flag in .env: ${key}" >&2
        found=1
      fi
    fi
  done

  if [ "${found}" = "1" ]; then
    echo "remove persistent live mutation flags from env/.env; use scoped run-once handoff for reviewed live execution" >&2
    exit 1
  fi
}

print_runtime_safety_flags() {
  local override_file="$1"
  shift

  local service container_id
  local flags_pattern='^(MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH|MARKET_VELOCITY_CREATE_TASK_APPLY|MARKET_VELOCITY_CREATE_TASK_CONFIRM|MARKET_VELOCITY_RUN_SCOPED_WORKER_APPLY|MARKET_VELOCITY_RUN_SCOPED_WORKER_CONFIRM|MARKET_VELOCITY_SIGNAL_LIVE_ORDER_ALLOWED|MARKET_VELOCITY_SIGNAL_PAPER_TRADE_REQUIRED|EXECUTION_WORKER_DRY_RUN|EXECUTION_WORKER_TARGET_TASK_IDS|EXECUTION_WORKER_LIVE_ORDER_CONFIRM|LEGACY_DIRECT_LIVE_ORDER_CONFIRM|LEGACY_SIGNED_READ_ONLY_CONFIRM|RISK_BALANCE_LIVE_MUTATION_CONFIRM|PROTECTIVE_OUTCOME_CONFIRM)='
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue

    container_id="$(compose -f "${override_file}" ps --all -q "${service}" | head -n 1 || true)"
    [ -z "${container_id}" ] && continue

    echo "deployment runtime safety flags: ${service}" >&2
    docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "${container_id}" |
      grep -E "${flags_pattern}" >&2 || true
  done
}

require_internal_server_deploy_service() {
  local service
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    if [ "${service}" = "quant-core-internal-server" ]; then
      return 0
    fi
  done

  echo "DEPLOY_SERVICES must include quant-core-internal-server; quant-web depends on the Core internal API for market radar and asset snapshot refresh" >&2
  exit 1
}

IFS=',' read -r -a services <<< "${services_csv}"
require_internal_server_deploy_service "${services[@]}"
override_file=".deploy/quant-core.release.override.yml"
schema_service="quant-core-schema-ensure"
{
  echo "services:"
  echo "  ${schema_service}:"
  echo "    image: ${target_image}"
  echo "    pull_policy: always"
  for service in "${services[@]}"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue
    container_id="$(compose ps --all -q "${service}" | head -n 1 || true)"
    if [ -n "${container_id}" ]; then
      docker inspect --format '{{.Config.Image}}' "${container_id}" > ".deploy/${service}.previous_image"
    fi
    echo "  ${service}:"
    echo "    image: ${target_image}"
    echo "    pull_policy: always"
  done
} > "${override_file}"

run_schema_ensure() {
  docker rm -f "${schema_service}" >/dev/null 2>&1 || true
  compose -f "${override_file}" run --rm --no-deps -T "${schema_service}" </dev/null
}

assert_no_persistent_live_mutation_env_flags
compose -f "${override_file}" pull "${schema_service}" "${services[@]}"
run_schema_ensure
remove_conflicting_named_containers "${services[@]}"
compose -f "${override_file}" up -d --no-build --remove-orphans "${services[@]}"
assert_services_running "${compose_file}" "${override_file}" "${services[@]}"
print_runtime_safety_flags "${override_file}" "${services[@]}"
compose -f "${override_file}" ps --all "${services[@]}"
REMOTE
