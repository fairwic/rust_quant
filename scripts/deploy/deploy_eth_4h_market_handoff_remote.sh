#!/usr/bin/env bash
set -euo pipefail

action="$1"
server_app_path="$2"
deploy_root="$3"
target_image="$4"
expected_revision="$5"
compose_file="${deploy_root}/docker-compose.deploy.yml"
current_release_file="${deploy_root}/current-release.env"
previous_compose_file="${deploy_root}/previous-compose.yml"
previous_images_file="${deploy_root}/previous-images.env"
ownership_marker="${deploy_root}/market-ownership-transferred"
project_name="rust_quant"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"
handoff_confirmation="${DEPLOY_ETH_4H_HANDOFF_CONFIRM:-}"

services=(
  quant-core-vegas-eth-4h-worker
  quant-core-vegas-universal-4h-worker
  quant-core-strategy-4h-candle-backfill-scheduler
)

cd "${server_app_path}"
mkdir -p "${deploy_root}"

case "${action}" in
  prepare) expected_confirmation="prepare-legacy-eth-usdt-swap-4h-handoff" ;;
  verify) expected_confirmation="verify-legacy-eth-usdt-swap-4h-handoff" ;;
  rollback) expected_confirmation="rollback-legacy-eth-usdt-swap-4h-before-ownership" ;;
  *) echo "invalid scoped handoff action" >&2; exit 2 ;;
esac
[[ "${handoff_confirmation}" == "${expected_confirmation}" ]] \
  || { echo "invalid remote scoped handoff confirmation" >&2; exit 1; }

compose() {
  LEGACY_CANDLE_PERSIST_EXCLUDED_TARGETS="ETH-USDT-SWAP:4H" \
  STRATEGY_4H_CANDLE_BACKFILL_EXCLUDED_SYMBOLS="ETH-USDT-SWAP" \
    docker compose \
      --project-directory "${server_app_path}" \
      --project-name "${project_name}" \
      --file "${compose_file}" \
      "$@"
}

validate_scope() {
  [[ -f "${compose_file}" ]] || { echo "scoped handoff compose is missing" >&2; return 1; }
  if docker inspect quant-core-signal-worker >/dev/null 2>&1; then
    echo "quant-core-signal-worker unexpectedly exists; reevaluate the writer set" >&2
    return 1
  fi
  local service project
  for service in "${services[@]}"; do
    docker inspect "${service}" >/dev/null 2>&1 \
      || { echo "required legacy writer is missing: ${service}" >&2; return 1; }
    project="$(docker inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "${service}")"
    [[ "${project}" == "${project_name}" ]] \
      || { echo "legacy writer project drift: ${service}" >&2; return 1; }
  done
}

resolved_image_for_tag() {
  local repository
  repository="${target_image%:*}"
  docker image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' "${target_image}" \
    | awk -v prefix="${repository}@sha256:" 'index($0, prefix) == 1 {print; exit}'
}

pull_and_verify_image() {
  [[ "${target_image}" =~ ^ghcr\.io/[a-z0-9._/-]+:sha-[0-9a-f]{40}$ \
    && "${expected_revision}" =~ ^[0-9a-f]{40}$ ]] \
    || { echo "invalid scoped handoff image request" >&2; return 1; }
  if [[ -n "${ghcr_token}" ]]; then
    [[ -n "${ghcr_username}" ]] || { echo "GHCR username is required with token" >&2; return 1; }
    printf '%s' "${ghcr_token}" \
      | docker login ghcr.io --username "${ghcr_username}" --password-stdin >/dev/null
  fi
  docker pull "${target_image}" >/dev/null
  local revision
  revision="$(docker image inspect --format '{{ index .Config.Labels "org.opencontainers.image.revision" }}' "${target_image}")"
  [[ "${revision}" == "${expected_revision}" ]] \
    || { echo "scoped handoff image revision mismatch" >&2; return 1; }
  RESOLVED_IMAGE="$(resolved_image_for_tag)"
  [[ "${RESOLVED_IMAGE}" =~ ^ghcr\.io/[a-z0-9._/-]+@sha256:[0-9a-f]{64}$ ]] \
    || { echo "scoped handoff image digest is missing" >&2; return 1; }
}

write_release_file() {
  local temporary_file
  temporary_file="$(mktemp "${deploy_root}/release.XXXXXX")"
  chmod 600 "${temporary_file}"
  printf 'image=%s\nrevision=%s\n' "${RESOLVED_IMAGE}" "${expected_revision}" > "${temporary_file}"
  mv "${temporary_file}" "${current_release_file}"
}

read_release_file() {
  [[ -f "${current_release_file}" ]] || { echo "scoped handoff release is missing" >&2; return 1; }
  RESOLVED_IMAGE="$(sed -n 's/^image=//p' "${current_release_file}")"
  expected_revision="$(sed -n 's/^revision=//p' "${current_release_file}")"
  [[ "${RESOLVED_IMAGE}" =~ ^ghcr\.io/[a-z0-9._/-]+@sha256:[0-9a-f]{64}$ \
    && "${expected_revision}" =~ ^[0-9a-f]{40}$ ]] \
    || { echo "scoped handoff release file is invalid" >&2; return 1; }
}

write_image_override() {
  local override_file="$1" service
  {
    echo "services:"
    for service in "${services[@]}"; do
      printf '  %s:\n    image: %s\n    pull_policy: never\n' "${service}" "${RESOLVED_IMAGE}"
    done
  } > "${override_file}"
}

verify_services_once() {
  local service running restart_count revision persistence command
  for service in "${services[@]}"; do
    running="$(docker inspect --format '{{.State.Running}}' "${service}")"
    restart_count="$(docker inspect --format '{{.RestartCount}}' "${service}")"
    revision="$(docker inspect --format '{{ index .Config.Labels "org.opencontainers.image.revision" }}' "${service}")"
    [[ "${running}" == "true" && "${restart_count}" == "0" && "${revision}" == "${expected_revision}" ]] \
      || return 1
  done
  for service in quant-core-vegas-eth-4h-worker quant-core-vegas-universal-4h-worker; do
    persistence="$(docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "${service}" \
      | awk -F= '$1 == "WEBSOCKET_CANDLE_PERSIST_EXCLUDED_TARGETS" {print $2}')"
    [[ "${persistence}" == "ETH-USDT-SWAP:4H" ]] || return 1
  done
  command="$(docker inspect --format '{{json .Config.Cmd}}' quant-core-strategy-4h-candle-backfill-scheduler)"
  [[ "${command}" == *'"--exclude-symbols","ETH-USDT-SWAP"'* ]] || return 1
}

verify_services() {
  local attempt
  for attempt in $(seq 1 12); do
    if verify_services_once; then
      sleep 10
      verify_services_once
      echo "legacy ETH-USDT-SWAP/4H handoff verified at ${expected_revision} ${RESOLVED_IMAGE}"
      return 0
    fi
    sleep 5
  done
  echo "legacy ETH-USDT-SWAP/4H handoff verification failed" >&2
  return 1
}

restore_previous_services() {
  [[ ! -e "${ownership_marker}" ]] \
    || { echo "market ownership already transferred; scoped legacy rollback is blocked" >&2; return 1; }
  [[ -f "${previous_compose_file}" && -f "${previous_images_file}" ]] \
    || { echo "scoped handoff rollback snapshot is incomplete" >&2; return 1; }
  local rollback_override="${deploy_root}/rollback.override.yml" service image
  {
    echo "services:"
    for service in "${services[@]}"; do
      image="$(sed -n "s|^${service}=||p" "${previous_images_file}")"
      [[ -n "${image}" ]] || { echo "previous image is missing: ${service}" >&2; return 1; }
      printf '  %s:\n    image: %s\n    pull_policy: never\n' "${service}" "${image}"
    done
  } > "${rollback_override}"
  docker compose \
    --project-directory "${server_app_path}" \
    --project-name "${project_name}" \
    --file "${previous_compose_file}" \
    --file "${rollback_override}" \
    up --detach --no-deps --no-build --pull never "${services[@]}"
  sleep 5
  for service in "${services[@]}"; do
    image="$(sed -n "s|^${service}=||p" "${previous_images_file}")"
    [[ "$(docker inspect --format '{{.State.Running}}' "${service}")" == "true" \
      && "$(docker inspect --format '{{.Config.Image}}' "${service}")" == "${image}" ]] \
      || { echo "legacy rollback verification failed: ${service}" >&2; return 1; }
  done
  echo "legacy ETH-USDT-SWAP/4H writers restored before Market ownership transfer"
}

prepare_handoff() {
  [[ ! -e "${ownership_marker}" ]] \
    || { echo "market ownership already transferred" >&2; return 1; }
  validate_scope
  pull_and_verify_image
  cp docker-compose.deploy.yml "${previous_compose_file}"
  local temporary_images="${previous_images_file}.next" service override_file
  : > "${temporary_images}"
  for service in "${services[@]}"; do
    printf '%s=%s\n' "${service}" "$(docker inspect --format '{{.Config.Image}}' "${service}")" \
      >> "${temporary_images}"
  done
  mv "${temporary_images}" "${previous_images_file}"
  override_file="${deploy_root}/prepare.override.yml"
  write_image_override "${override_file}"
  if ! compose --file "${override_file}" up --detach --no-deps --no-build --pull never "${services[@]}" \
    || ! verify_services \
    || ! write_release_file; then
    restore_previous_services
    return 1
  fi
}

case "${action}" in
  prepare)
    prepare_handoff
    ;;
  verify)
    validate_scope
    read_release_file
    verify_services
    ;;
  rollback)
    restore_previous_services
    ;;
  *)
    echo "usage: deploy_eth_4h_market_handoff_remote.sh prepare|verify|rollback" >&2
    exit 2
    ;;
esac
