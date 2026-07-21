#!/usr/bin/env bash
set -euo pipefail

: "${DEPLOY_SSH_USER:?DEPLOY_SSH_USER is required}"
: "${DEPLOY_SSH_HOST:?DEPLOY_SSH_HOST is required}"

expected_revision="${1:-${VERIFY_EXPECTED_REVISION:-}}"
ssh_port="${DEPLOY_SSH_PORT:-22}"
log_window="${VERIFY_LOG_WINDOW:-10m}"
checkpoint_sample_secs="${VERIFY_CHECKPOINT_SAMPLE_SECS:-12}"

if [[ ! "${expected_revision}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "expected revision must be a 40-character lowercase git SHA" >&2
  exit 1
fi
if [[ ! "${log_window}" =~ ^[0-9]+[smhd]$ ]]; then
  echo "VERIFY_LOG_WINDOW must use a Docker duration such as 10m or 1h" >&2
  exit 1
fi
if [[ ! "${checkpoint_sample_secs}" =~ ^[0-9]+$ ]] || [ "${checkpoint_sample_secs}" -lt 1 ] || [ "${checkpoint_sample_secs}" -gt 60 ]; then
  echo "VERIFY_CHECKPOINT_SAMPLE_SECS must be between 1 and 60" >&2
  exit 1
fi

ssh_args=(-o BatchMode=yes -o ConnectTimeout=15 -p "${ssh_port}")
if [ -n "${VERIFY_SSH_KEY_PATH:-}" ]; then
  ssh_args+=(-i "${VERIFY_SSH_KEY_PATH}")
fi

ssh "${ssh_args[@]}" "${DEPLOY_SSH_USER}@${DEPLOY_SSH_HOST}" \
  bash -s -- "${expected_revision}" "${log_window}" "${checkpoint_sample_secs}" <<'REMOTE'
set -euo pipefail

expected_revision="$1"
log_window="$2"
checkpoint_sample_secs="$3"
failures=0
lease_events=0
containers=(
  quant-core-control-api
  quant-core-market-worker
  quant-core-signal-worker
  quant-core-account-worker
  quant-core-execution-worker
  quant-core-reconciliation-worker
)

# 只读取指定的非敏感运行参数，避免把容器内的密钥和数据库连接打印到验收日志。
read_container_env() {
  local container="$1"
  local key="$2"
  docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "${container}" |
    awk -F= -v target="${key}" '$1 == target {sub(/^[^=]*=/, ""); print; exit}'
}

# revision、运行状态和重启次数共同证明当前容器确实稳定运行目标镜像。
for container in "${containers[@]}"; do
  if ! docker inspect "${container}" >/dev/null 2>&1; then
    echo "CONTAINER|${container}|missing"
    failures=$((failures + 1))
    continue
  fi
  revision="$(docker inspect -f '{{index .Config.Labels "org.opencontainers.image.revision"}}' "${container}")"
  status="$(docker inspect -f '{{.State.Status}}' "${container}")"
  restarts="$(docker inspect -f '{{.RestartCount}}' "${container}")"
  echo "CONTAINER|${container}|revision=${revision}|status=${status}|restarts=${restarts}"
  if [ "${revision}" != "${expected_revision}" ] || [ "${status}" != "running" ] || [ "${restarts}" != "0" ]; then
    failures=$((failures + 1))
  fi
done

# 这些值是发布契约的一部分；逐项比较比输出全部环境变量更安全、更紧凑。
assert_env() {
  local container="$1"
  local key="$2"
  local expected="$3"
  local actual
  actual="$(read_container_env "${container}" "${key}")"
  echo "CONFIG|${container}|${key}=${actual}"
  if [ "${actual}" != "${expected}" ]; then
    failures=$((failures + 1))
  fi
}

assert_env quant-core-execution-worker EXECUTION_WORKER_TASK_TYPES "execute_signal,risk_control_close_candidate"
assert_env quant-core-signal-worker LIVE_STRATEGY_ONLY_TYPES "vegas,vegas_universal_4h"
assert_env quant-core-signal-worker LIVE_STRATEGY_VEGAS_ONLY_INST_IDS "ETH-USDT-SWAP"
assert_env quant-core-execution-worker EXECUTION_WORKER_TASK_STATUSES "pending,pending_close"
assert_env quant-core-signal-worker MARKET_VELOCITY_LIVE_HANDOFF_INTERVAL_SECS "5"
assert_env quant-core-signal-worker MARKET_VELOCITY_LIVE_HANDOFF_SIGNAL_TTL_MS "10000"
assert_env quant-core-market-worker MARKET_VELOCITY_SIGNAL_DISPATCH_MODE "disabled"
assert_env quant-core-market-worker EXCHANGE_LISTING_SIGNAL_SUBMIT "0"

# 日志只汇总错误和租约事件数量，失败后再由操作者定向读取原始日志。
for container in \
  quant-core-market-worker \
  quant-core-signal-worker \
  quant-core-account-worker \
  quant-core-execution-worker \
  quant-core-reconciliation-worker; do
  logs="$(docker logs --since "${log_window}" "${container}" 2>&1 || true)"
  error_count="$(printf '%s' "${logs}" | grep -Eic 'panic|fatal|thread .* panicked|polling failed|worker.*failed|must not exceed signal TTL|boolean value' || true)"
  container_lease_events="$(printf '%s' "${logs}" | grep -Ec 'leased tasks from quant_web|starts leased task' || true)"
  lease_events=$((lease_events + container_lease_events))
  echo "LOGS|${container}|errors=${error_count}|lease_events=${container_lease_events}"
  if [ "${error_count}" != "0" ]; then
    failures=$((failures + 1))
  fi
done

# checkpoint 计数只用于展示短窗口写入速率；真实任务运行时可能产生合法的额外更新。
checkpoint_update_count() {
  docker exec postgres sh -lc 'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d quant_core -Atc "SELECT n_tup_upd FROM pg_stat_user_tables WHERE relname='\''execution_worker_checkpoints'\'';"'
}

if docker inspect postgres >/dev/null 2>&1; then
  checkpoint_rows="$(docker exec postgres sh -lc 'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d quant_core -Atc "SELECT count(*) FROM execution_worker_checkpoints WHERE worker_id IN ('\''quant-core-worker-prod'\'','\''quant-core-account-worker-prod'\'','\''quant-core-reconciliation-worker-prod'\'');"')"
  before="$(checkpoint_update_count)"
  sleep "${checkpoint_sample_secs}"
  after="$(checkpoint_update_count)"
  echo "CHECKPOINTS|worker_rows=${checkpoint_rows}|updates=${checkpoint_sample_secs}s:$((after - before))|lease_events=${lease_events}"
  if [ "${checkpoint_rows}" != "3" ]; then
    failures=$((failures + 1))
  fi
else
  echo "CHECKPOINTS|postgres_container_missing"
  failures=$((failures + 1))
fi

if [ "${failures}" -ne 0 ]; then
  echo "VERIFICATION=FAIL|failures=${failures}"
  exit 1
fi
echo "VERIFICATION=PASS"
REMOTE
