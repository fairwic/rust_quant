#!/usr/bin/env bash
set -euo pipefail

: "${DEPLOY_SSH_USER:?DEPLOY_SSH_USER is required}"
: "${DEPLOY_SSH_HOST:?DEPLOY_SSH_HOST is required}"
: "${SERVER_APP_PATH:?SERVER_APP_PATH is required}"

compose_file="${DEPLOY_COMPOSE_FILE:-docker-compose.deploy.yml}"
services_csv="${DEPLOY_SERVICES:-quant-core-market-velocity-radar,quant-core-execution-worker}"
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

ssh -p "${ssh_port}" "${DEPLOY_SSH_USER}@${ssh_host}" \
  env \
  "DEPLOY_GHCR_USERNAME=${ghcr_username}" \
  "DEPLOY_GHCR_TOKEN=${ghcr_token}" \
  bash -s -- \
  "${SERVER_APP_PATH}" \
  "${compose_file}" \
  "${services_csv}" <<'REMOTE'
set -euo pipefail

server_app_path="$1"
compose_file="$2"
services_csv="$3"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"

cd "${server_app_path}"

if [ -n "${ghcr_username}" ] && [ -n "${ghcr_token}" ]; then
  printf '%s' "${ghcr_token}" | docker login ghcr.io -u "${ghcr_username}" --password-stdin > /dev/null
fi

IFS=',' read -r -a services <<< "${services_csv}"
override_file=".deploy/quant-core.rollback.override.yml"
{
  echo "services:"
  for service in "${services[@]}"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue
    snapshot_file=".deploy/${service}.previous_image"
    if [ ! -f "${snapshot_file}" ]; then
      echo "rollback snapshot missing: ${snapshot_file}" >&2
      exit 1
    fi
    echo "  ${service}:"
    echo "    image: $(cat "${snapshot_file}")"
    echo "    pull_policy: always"
  done
} > "${override_file}"

docker compose -f "${compose_file}" -f "${override_file}" pull "${services[@]}" || true
docker compose -f "${compose_file}" -f "${override_file}" up -d --no-build "${services[@]}"
docker compose -f "${compose_file}" -f "${override_file}" ps "${services[@]}"
REMOTE
