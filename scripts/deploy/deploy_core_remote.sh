#!/usr/bin/env bash
set -euo pipefail

action="$1"
server_app_path="$2"
compose_file="$3"
services_csv="$4"
target_image="$5"
retired_services_csv="$6"
six_role_cutover_confirm="$7"
obsolete_services_csv="$8"
ghcr_username="${DEPLOY_GHCR_USERNAME:-}"
ghcr_token="${DEPLOY_GHCR_TOKEN:-}"

cd "${server_app_path}"
mkdir -p .deploy
compose_project_name="${DEPLOY_COMPOSE_PROJECT_NAME:-$(basename "$(pwd)")}"

# 固定 project directory/name，避免上传到 .deploy 后被 Compose 识别成另一个项目。
compose() {
  docker compose \
    --project-directory "${server_app_path}" \
    --project-name "${compose_project_name}" \
    --profile schema-ensure \
    -f "${compose_file}" \
    "$@"
}

# 这里只证明进程在稳定窗口内未退出；依赖级 readiness 由发布后的只读验收负责。
assert_services_process_stable() {
  local _compose_file="$1"
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
      echo "deployment services failed process stability within ${timeout_secs}s" >&2
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

# 只删除与固定 container_name 精确冲突且不属于当前 Compose 项目的残留容器。
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

# 清理列表是显式 allowlist，禁止用广域 orphan 清理波及共享 Redis 等外部依赖。
remove_retired_deployment_containers() {
  local service existing_container_id
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue
    existing_container_id="$(docker ps -aq --filter "name=^/${service}$" | head -n 1 || true)"
    [ -z "${existing_container_id}" ] && continue
    echo "removing retired deployment container: ${service} (${existing_container_id})" >&2
    docker rm -f "${existing_container_id}"
  done
}

# 统一处理 dotenv 的引号、空白和行尾注释，避免安全检查被表示形式绕过。
normalize_live_mutation_env_value() {
  local value="${1:-}"
  value="$(printf '%s' "${value}" | sed -e 's/[[:space:]]#.*$//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  case "${value}" in
    \"*\") value="${value#\"}"; value="${value%\"}" ;;
    \'*\') value="${value#\'}"; value="${value%\'}" ;;
  esac
  printf '%s' "${value}"
}

# 这些确认字段必须是单次人工动作，任何持久非空值都会扩大实盘 mutation 权限。
live_mutation_env_value_is_dangerous() {
  local key="$1"
  local value
  value="$(normalize_live_mutation_env_value "${2:-}")"
  case "${key}" in
    LEGACY_DIRECT_LIVE_ORDER_CONFIRM|LEGACY_SIGNED_READ_ONLY_CONFIRM|RISK_BALANCE_LIVE_MUTATION_CONFIRM|PROTECTIVE_OUTCOME_CONFIRM)
      [ -n "${value}" ]
      ;;
    *) return 1 ;;
  esac
}

# 读取 dotenv 中最后一次赋值，保持与 shell 后赋值覆盖前赋值的习惯一致。
read_dotenv_value() {
  local key="$1"
  local env_file="$2"
  local line
  line="$(grep -E "^[[:space:]]*(export[[:space:]]+)?${key}=" "${env_file}" | tail -n 1 || true)"
  [ -n "${line}" ] || return 1
  printf '%s' "${line#*=}"
}

# Docker 容器 IP 会在重建后漂移，只允许 DNS 名或显式管理的外部 Redis 地址。
redis_host_value_is_pinned_ip() {
  local value
  value="$(normalize_live_mutation_env_value "${1:-}")"
  [[ "${value}" =~ ^redis://[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+(:[0-9]+)?/?$ ]]
}

# 在删除旧容器前阻断不可恢复的 Redis 地址配置。
assert_no_pinned_redis_host_env() {
  local found=0
  local value env_file
  value="${REDIS_HOST:-}"
  if [ -n "${value}" ] && redis_host_value_is_pinned_ip "${value}"; then
    echo "refusing deployment with pinned Redis container IP in process env: REDIS_HOST" >&2
    found=1
  fi
  for env_file in .env .env.deploy; do
    [ -f "${env_file}" ] || continue
    value="$(read_dotenv_value "REDIS_HOST" "${env_file}" || true)"
    if [ -n "${value}" ] && redis_host_value_is_pinned_ip "${value}"; then
      echo "refusing deployment with pinned Redis container IP in ${env_file}: REDIS_HOST" >&2
      found=1
    fi
  done
  if [ "${found}" = "1" ]; then
    echo "use a stable Docker DNS name such as redis://redis:6379/ or an explicitly managed external Redis host" >&2
    exit 1
  fi
}

# 发布配置不得长期携带一次性实盘确认字段。
assert_no_persistent_live_mutation_env_flags() {
  local env_file=".env"
  local found=0
  local key value
  local live_mutation_keys=(
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
    echo "remove persistent live mutation flags from env/.env; use reviewed deploy compose for authorized live execution" >&2
    exit 1
  fi
}

# 旧的直接 Web dispatch 会与 signal-worker handoff 重复发出执行任务。
market_velocity_dispatch_mode_is_legacy_web_override() {
  local value
  value="$(normalize_live_mutation_env_value "${1:-}")"
  case "${value}" in
    web|quant_web|execution_tasks|enabled|true|1) return 0 ;;
    *) return 1 ;;
  esac
}

# 在启动 signal-worker 前拒绝持久化的 legacy dispatch override。
assert_no_legacy_market_velocity_dispatch_mode_override() {
  local found=0
  local value env_file
  value="${MARKET_VELOCITY_SIGNAL_DISPATCH_MODE:-}"
  if [ -n "${value}" ] && market_velocity_dispatch_mode_is_legacy_web_override "${value}"; then
    echo "refusing deployment with legacy Market Velocity direct dispatch mode in process env: MARKET_VELOCITY_SIGNAL_DISPATCH_MODE=${value}" >&2
    found=1
  fi
  for env_file in .env .env.deploy; do
    [ -f "${env_file}" ] || continue
    value="$(read_dotenv_value "MARKET_VELOCITY_SIGNAL_DISPATCH_MODE" "${env_file}" || true)"
    if [ -n "${value}" ] && market_velocity_dispatch_mode_is_legacy_web_override "${value}"; then
      echo "refusing deployment with legacy Market Velocity direct dispatch mode in ${env_file}: MARKET_VELOCITY_SIGNAL_DISPATCH_MODE=${value}" >&2
      found=1
    fi
  done
  if [ "${found}" = "1" ]; then
    echo "remove persistent MARKET_VELOCITY_SIGNAL_DISPATCH_MODE web override from env/.env.deploy; hybrid live handoff owns signal emission" >&2
    exit 1
  fi
}

# 只输出非敏感安全开关；密钥和数据库连接不得进入 CI 日志。
print_runtime_safety_flags() {
  local override_file="$1"
  shift
  local service container_id
  local flags_pattern='^(MARKET_VELOCITY_ENTRY_CANDLE_ON_DEMAND_REFRESH|LEGACY_DIRECT_LIVE_ORDER_CONFIRM|LEGACY_SIGNED_READ_ONLY_CONFIRM|RISK_BALANCE_LIVE_MUTATION_CONFIRM|PROTECTIVE_OUTCOME_CONFIRM)='
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

# Web 依赖 control-api；即使角色清单被误改也必须 fail-closed。
require_control_api_deploy_service() {
  local service
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ "${service}" = "quant-core-control-api" ] && return 0
  done
  echo "runtime-services.txt must include quant-core-control-api; quant-web depends on the Core internal API" >&2
  exit 1
}

# 生产拓扑固定为六角色，清单只能通过受评审的仓库变更更新。
require_exact_six_role_services() {
  local expected service candidate found
  local expected_services=(
    quant-core-control-api
    quant-core-market-worker
    quant-core-signal-worker
    quant-core-account-worker
    quant-core-execution-worker
    quant-core-reconciliation-worker
  )
  if [ "$#" -ne "${#expected_services[@]}" ]; then
    echo "runtime-services.txt must contain exactly the six Core runtime roles" >&2
    exit 1
  fi
  for expected in "${expected_services[@]}"; do
    found=0
    for candidate in "$@"; do
      service="$(printf '%s' "${candidate}" | xargs)"
      if [ "${service}" = "${expected}" ]; then
        found=1
        break
      fi
    done
    if [ "${found}" != "1" ]; then
      echo "runtime-services.txt is missing required six-role service ${expected}" >&2
      exit 1
    fi
  done
}

# Promote 与 rollback 共用同一远端安全前置条件，避免两份脚本日后漂移。
run_runtime_preflight() {
  assert_no_persistent_live_mutation_env_flags
  assert_no_pinned_redis_host_env
  assert_no_legacy_market_velocity_dispatch_mode_override
}

# 首次迁移必须先保存 legacy 服务与镜像；完成回滚窗口后整段应按迁移计划删除。
prepare_first_six_role_cutover() {
  local confirmation="$1"
  shift
  local service container_id image cutover_required=0
  for service in "$@"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue
    container_id="$(docker ps -aq --filter "name=^/${service}$" | head -n 1 || true)"
    if [ -n "${container_id}" ]; then
      cutover_required=1
      break
    fi
  done
  [ "${cutover_required}" = "1" ] || return 0
  if [ "${confirmation}" != "replace-legacy-runtime-with-six-roles" ]; then
    echo "legacy Core runtime containers are still present; set DEPLOY_SIX_ROLE_CUTOVER_CONFIRM=replace-legacy-runtime-with-six-roles for the one-time six-role cutover" >&2
    exit 1
  fi

  : > .deploy/six-role-cutover.previous_topology
  for service in "$@" quant-core-execution-worker; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue
    container_id="$(docker ps -aq --filter "name=^/${service}$" | head -n 1 || true)"
    [ -n "${container_id}" ] || continue
    image="$(docker inspect --format '{{.Config.Image}}' "${container_id}")"
    printf '%s|%s\n' "${service}" "${image}" >> .deploy/six-role-cutover.previous_topology
  done
  if [ ! -s .deploy/six-role-cutover.previous_topology ]; then
    echo "six-role cutover could not snapshot the legacy topology" >&2
    exit 1
  fi
  : > .deploy/six-role-cutover.rollback_to_legacy
  first_six_role_cutover=1
  echo "six-role cutover confirmed; legacy topology rollback snapshot saved" >&2
}

# 发布顺序保持为安全检查、拉镜像、schema、快照、清退、启动和稳定性验证。
promote_runtime() {
  [ -n "${target_image}" ] || {
    echo "DEPLOY_IMAGE is required for promote" >&2
    exit 1
  }
  first_six_role_cutover=0
  prepare_first_six_role_cutover "${six_role_cutover_confirm}" "${retired_services[@]}"
  local override_file=".deploy/quant-core.release.override.yml"
  local schema_service="quant-core-schema-ensure"
  local service container_id
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

  run_runtime_preflight
  compose -f "${override_file}" pull "${schema_service}" "${services[@]}"
  docker rm -f "${schema_service}" >/dev/null 2>&1 || true
  compose -f "${override_file}" run --rm --no-deps -T "${schema_service}" </dev/null
  remove_conflicting_named_containers "${services[@]}"
  remove_retired_deployment_containers "${retired_services[@]}"
  remove_retired_deployment_containers "${obsolete_services[@]}"
  compose -f "${override_file}" up -d --no-build --pull never "${services[@]}"
  assert_services_process_stable "${compose_file}" "${override_file}" "${services[@]}"
  if [ "${first_six_role_cutover}" = "0" ]; then
    rm -f .deploy/six-role-cutover.rollback_to_legacy
  fi
  print_runtime_safety_flags "${override_file}" "${services[@]}"
  compose -f "${override_file}" ps --all "${services[@]}"
}

# 回滚优先恢复首次 cutover 的 legacy 拓扑，否则恢复六角色逐服务 previous image。
rollback_runtime() {
  local override_file=".deploy/quant-core.rollback.override.yml"
  local missing_six_role_snapshot=0
  local service image snapshot_file legacy_snapshot_file
  if [ -f ".deploy/six-role-cutover.rollback_to_legacy" ]; then
    missing_six_role_snapshot=1
  fi
  for service in "${services[@]}"; do
    service="$(printf '%s' "${service}" | xargs)"
    [ -z "${service}" ] && continue
    if [ ! -f ".deploy/${service}.previous_image" ]; then
      missing_six_role_snapshot=1
      break
    fi
  done

  if [ "${missing_six_role_snapshot}" = "1" ]; then
    legacy_snapshot_file=".deploy/six-role-cutover.previous_topology"
    if [ ! -s "${legacy_snapshot_file}" ]; then
      echo "rollback snapshots for the six-role topology are incomplete and the legacy topology snapshot is missing" >&2
      exit 1
    fi
    restore_services=()
    {
      echo "services:"
      while IFS='|' read -r service image; do
        [ -n "${service}" ] && [ -n "${image}" ] || continue
        if [[ ! "${service}" =~ ^[a-zA-Z0-9._-]+$ ]]; then
          echo "invalid service name in legacy topology snapshot: ${service}" >&2
          exit 1
        fi
        restore_services+=("${service}")
        echo "  ${service}:"
        echo "    image: ${image}"
        echo "    pull_policy: always"
        if [ "${service}" = "quant-core-execution-worker" ]; then
          echo "    command: [rust_quant]"
        fi
      done < "${legacy_snapshot_file}"
    } > "${override_file}"
    if [ "${#restore_services[@]}" -eq 0 ]; then
      echo "legacy topology snapshot contains no services" >&2
      exit 1
    fi

    run_runtime_preflight
    compose -f "${override_file}" pull "${restore_services[@]}" || true
    remove_retired_deployment_containers "${services[@]}"
    remove_retired_deployment_containers "${obsolete_services[@]}"
    remove_conflicting_named_containers "${restore_services[@]}"
    compose -f "${override_file}" up -d --no-build --pull never "${restore_services[@]}"
    assert_services_process_stable "${compose_file}" "${override_file}" "${restore_services[@]}"
    print_runtime_safety_flags "${override_file}" "${restore_services[@]}"
    compose -f "${override_file}" ps --all "${restore_services[@]}"
    return 0
  fi

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

  run_runtime_preflight
  compose -f "${override_file}" pull "${services[@]}" || true
  remove_conflicting_named_containers "${services[@]}"
  remove_retired_deployment_containers "${retired_services[@]}"
  remove_retired_deployment_containers "${obsolete_services[@]}"
  compose -f "${override_file}" up -d --no-build --pull never "${services[@]}"
  assert_services_process_stable "${compose_file}" "${override_file}" "${services[@]}"
  print_runtime_safety_flags "${override_file}" "${services[@]}"
  compose -f "${override_file}" ps --all "${services[@]}"
}

if [ -n "${ghcr_username}" ] && [ -n "${ghcr_token}" ]; then
  printf '%s' "${ghcr_token}" | docker login ghcr.io -u "${ghcr_username}" --password-stdin > /dev/null
fi

IFS=',' read -r -a services <<< "${services_csv}"
IFS=',' read -r -a retired_services <<< "${retired_services_csv}"
IFS=',' read -r -a obsolete_services <<< "${obsolete_services_csv}"
require_control_api_deploy_service "${services[@]}"
require_exact_six_role_services "${services[@]}"

case "${action}" in
  promote) promote_runtime ;;
  rollback) rollback_runtime ;;
  *)
    echo "unsupported deployment action: ${action}" >&2
    exit 2
    ;;
esac
