#!/usr/bin/env bash
set -euo pipefail

: "${DEPLOY_SSH_USER:?DEPLOY_SSH_USER is required}"
: "${DEPLOY_SSH_HOST:?DEPLOY_SSH_HOST is required}"
: "${SERVER_APP_PATH:?SERVER_APP_PATH is required}"
: "${DEPLOY_IMAGE:?DEPLOY_IMAGE is required}"

compose_file="${DEPLOY_COMPOSE_FILE:-docker-compose.deploy.yml}"
compose_source_file="${DEPLOY_COMPOSE_SOURCE_FILE:-docker-compose.deploy.yml}"
services_csv="${DEPLOY_SERVICES:-quant-core-market-velocity-radar,quant-core-market-velocity-paper-observation-scheduler,quant-core-market-velocity-live-handoff-scheduler,quant-core-execution-worker}"
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

  local service container_id running
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue

    container_id="$(compose -f "${override_file}" ps --all -q "${service}" | head -n 1 || true)"
    if [ -z "${container_id}" ]; then
      echo "deployment service container missing: ${service}" >&2
      compose -f "${override_file}" config --services >&2 || true
      compose -f "${override_file}" ps --all >&2 || true
      exit 1
    fi

    running="$(docker inspect --format '{{.State.Running}}' "${container_id}")"
    if [ "${running}" != "true" ]; then
      echo "deployment service is not running: ${service}" >&2
      compose -f "${override_file}" ps --all "${service}" >&2 || true
      compose -f "${override_file}" logs --tail=120 "${service}" >&2 || true
      exit 1
    fi
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

IFS=',' read -r -a services <<< "${services_csv}"
override_file=".deploy/quant-core.release.override.yml"
{
  echo "services:"
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

compose -f "${override_file}" pull "${services[@]}"
remove_conflicting_named_containers "${services[@]}"
compose -f "${override_file}" up -d --no-build "${services[@]}"
assert_services_running "${compose_file}" "${override_file}" "${services[@]}"
compose -f "${override_file}" ps --all "${services[@]}"
REMOTE
