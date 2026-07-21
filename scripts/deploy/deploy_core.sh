#!/usr/bin/env bash
set -euo pipefail

action="${1:-}"
case "${action}" in
  promote|rollback) ;;
  *)
    echo "usage: deploy_core.sh promote|rollback" >&2
    exit 2
    ;;
esac

: "${DEPLOY_SSH_USER:?DEPLOY_SSH_USER is required}"
: "${DEPLOY_SSH_HOST:?DEPLOY_SSH_HOST is required}"
: "${SERVER_APP_PATH:?SERVER_APP_PATH is required}"
if [ "${action}" = "promote" ]; then
  : "${DEPLOY_IMAGE:?DEPLOY_IMAGE is required}"
fi
if [ -n "${DEPLOY_SERVICES:-}" ]; then
  echo "DEPLOY_SERVICES is no longer supported; edit scripts/deploy/runtime-services.txt" >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
runtime_services_file="${script_dir}/runtime-services.txt"
remote_runner="${script_dir}/deploy_core_remote.sh"
compose_file="${DEPLOY_COMPOSE_FILE:-docker-compose.deploy.yml}"
compose_source_file="${DEPLOY_COMPOSE_SOURCE_FILE:-docker-compose.deploy.yml}"
retired_services_csv="${DEPLOY_RETIRED_SERVICES:-quant-core-internal-server,quant-core-exchange-symbol-sync-worker,quant-core-vegas-eth-4h-worker,quant-core-vegas-universal-4h-worker,quant-core-market-velocity-radar,quant-core-all-market-candle-volume-monitor,quant-core-market-velocity-candle-backfill-scheduler,quant-core-strategy-4h-candle-backfill-scheduler,quant-core-market-velocity-kline-scanner-scheduler,quant-core-market-velocity-paper-observation-scheduler,quant-core-market-velocity-kline15m-paper-observation-scheduler,quant-core-market-velocity-breakdown-short-paper-observation-scheduler,quant-core-market-velocity-live-handoff,quant-core-market-velocity-live-handoff-scheduler,quant-core-market-velocity-breakdown-short-live-handoff-scheduler,quant-core-execution-confirmation-worker,quant-core-execution-report-replay-worker}"
obsolete_services_csv="${DEPLOY_OBSOLETE_SERVICES:-quant-core-vegas-eth-4h-live}"
six_role_cutover_confirm="${DEPLOY_SIX_ROLE_CUTOVER_CONFIRM:-}"
target_image="${DEPLOY_IMAGE:-}"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"

for required_file in "${runtime_services_file}" "${remote_runner}" "${compose_source_file}"; do
  if [ ! -f "${required_file}" ]; then
    echo "deploy input missing: ${required_file}" >&2
    exit 1
  fi
done

# 运行角色属于版本化生产拓扑，不允许由 CI Secret 临时改写。
services_csv="$(awk 'NF && $1 !~ /^#/ { if (found) printf ","; printf "%s", $1; found=1 } END { if (!found) exit 1 }' "${runtime_services_file}")"

ssh_host_input="${DEPLOY_SSH_HOST}"
ssh_port="${DEPLOY_SSH_PORT:-}"
ssh_host="${ssh_host_input}"
if [[ "${ssh_host_input}" == *"@"* ]]; then
  echo "DEPLOY_SSH_HOST must not include a username. Set DEPLOY_SSH_USER separately." >&2
  exit 1
fi
if [[ "${ssh_host_input}" =~ ^\[([^]]+)\]:(.+)$ ]]; then
  [ -n "${ssh_port}" ] || ssh_port="${BASH_REMATCH[2]}"
  ssh_host="${BASH_REMATCH[1]}"
elif [[ "${ssh_host_input}" =~ ^([^:]+):([0-9]+)$ ]]; then
  [ -n "${ssh_port}" ] || ssh_port="${BASH_REMATCH[2]}"
  ssh_host="${BASH_REMATCH[1]}"
fi
ssh_port="${ssh_port:-22}"
remote_compose_file=".deploy/current.$(basename "${compose_file}")"

ssh -p "${ssh_port}" "${DEPLOY_SSH_USER}@${ssh_host}" "cd '${SERVER_APP_PATH}' && mkdir -p .deploy"
scp -P "${ssh_port}" "${compose_source_file}" "${DEPLOY_SSH_USER}@${ssh_host}:${SERVER_APP_PATH}/${remote_compose_file}"

# 远端实现从当前提交通过 stdin 执行，避免服务器残留旧部署脚本。
ssh -p "${ssh_port}" "${DEPLOY_SSH_USER}@${ssh_host}" \
  env \
  "DEPLOY_GHCR_USERNAME=${ghcr_username}" \
  "DEPLOY_GHCR_TOKEN=${ghcr_token}" \
  bash -s -- \
  "${action}" \
  "${SERVER_APP_PATH}" \
  "${remote_compose_file}" \
  "${services_csv}" \
  "${target_image}" \
  "${retired_services_csv}" \
  "${six_role_cutover_confirm}" \
  "${obsolete_services_csv}" < "${remote_runner}"
