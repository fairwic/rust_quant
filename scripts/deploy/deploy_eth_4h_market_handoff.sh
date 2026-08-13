#!/usr/bin/env bash
set -euo pipefail

action="${1:-}"
: "${DEPLOY_SSH_USER:?DEPLOY_SSH_USER is required}"
: "${DEPLOY_SSH_HOST:?DEPLOY_SSH_HOST is required}"
: "${SERVER_APP_PATH:?SERVER_APP_PATH is required}"
: "${DEPLOY_ETH_4H_HANDOFF_CONFIRM:?DEPLOY_ETH_4H_HANDOFF_CONFIRM is required}"

case "${action}" in
  prepare)
    expected_confirmation="prepare-legacy-eth-usdt-swap-4h-handoff"
    : "${DEPLOY_IMAGE:?DEPLOY_IMAGE is required for prepare}"
    : "${DEPLOY_EXPECTED_REVISION:?DEPLOY_EXPECTED_REVISION is required for prepare}"
    ;;
  verify)
    expected_confirmation="verify-legacy-eth-usdt-swap-4h-handoff"
    ;;
  rollback)
    expected_confirmation="rollback-legacy-eth-usdt-swap-4h-before-ownership"
    ;;
  *)
    echo "usage: deploy_eth_4h_market_handoff.sh prepare|verify|rollback" >&2
    exit 2
    ;;
esac
if [[ "${DEPLOY_ETH_4H_HANDOFF_CONFIRM}" != "${expected_confirmation}" ]]; then
  echo "invalid DEPLOY_ETH_4H_HANDOFF_CONFIRM for ${action}" >&2
  exit 1
fi
if [[ ! "${DEPLOY_SSH_USER}" =~ ^[A-Za-z0-9._-]+$ \
  || ! "${DEPLOY_SSH_HOST}" =~ ^[A-Za-z0-9.:-]+$ \
  || ! "${SERVER_APP_PATH}" =~ ^/[A-Za-z0-9._/-]+$ \
  || "${SERVER_APP_PATH}" == *".."* ]]; then
  echo "invalid scoped handoff SSH target" >&2
  exit 1
fi

target_image="${DEPLOY_IMAGE:--}"
expected_revision="${DEPLOY_EXPECTED_REVISION:--}"
if [[ "${action}" == "prepare" ]]; then
  [[ "${target_image}" =~ ^ghcr\.io/[a-z0-9._/-]+:sha-[0-9a-f]{40}$ ]] \
    || { echo "DEPLOY_IMAGE must be the exact source revision tag" >&2; exit 1; }
  [[ "${expected_revision}" =~ ^[0-9a-f]{40}$ ]] \
    || { echo "invalid DEPLOY_EXPECTED_REVISION" >&2; exit 1; }
fi

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repository_root="$(CDPATH= cd -- "${script_dir}/../.." && pwd)"
remote_root="${SERVER_APP_PATH}/.deploy/eth-4h-market-handoff"
ssh_port="${DEPLOY_SSH_PORT:-22}"
ssh_target="${DEPLOY_SSH_USER}@${DEPLOY_SSH_HOST}"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"

ssh -p "${ssh_port}" "${ssh_target}" "mkdir -p '${remote_root}'"
scp -P "${ssh_port}" \
  "${repository_root}/docker-compose.deploy.yml" \
  "${script_dir}/deploy_eth_4h_market_handoff_remote.sh" \
  "${ssh_target}:${remote_root}/"

ssh -p "${ssh_port}" "${ssh_target}" \
  env \
  "DEPLOY_GHCR_USERNAME=${ghcr_username}" \
  "DEPLOY_GHCR_TOKEN=${ghcr_token}" \
  "DEPLOY_ETH_4H_HANDOFF_CONFIRM=${DEPLOY_ETH_4H_HANDOFF_CONFIRM}" \
  bash -s -- \
  "${action}" \
  "${SERVER_APP_PATH}" \
  ".deploy/eth-4h-market-handoff" \
  "${target_image}" \
  "${expected_revision}" < "${script_dir}/deploy_eth_4h_market_handoff_remote.sh"
