#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
UMBRELLA_ROOT="$(cd "${REPO_ROOT}/.." && pwd)"

: "${RUST_QUAN_WEB_BASE_URL:=http://127.0.0.1:8000}"
: "${NEWS_SERVER_BASE_URL:=http://127.0.0.1:3011}"
: "${QUANT_CORE_DATABASE_URL:=postgres://postgres:postgres123@localhost:5432/quant_core}"
: "${WEB_DATABASE_URL:=postgres://postgres:postgres123@localhost:5432/quant_web}"
: "${NEWS_DATABASE_URL:=postgres://postgres:postgres123@localhost:5432/quant_news}"
: "${POSTGRES_CONTAINER:=postgres}"
: "${POSTGRES_USER:=postgres}"
: "${QUANT_CORE_POSTGRES_DB:=quant_core}"
: "${WEB_POSTGRES_DB:=quant_web}"
: "${NEWS_POSTGRES_DB:=quant_news}"
: "${HEALTH_CHECK_DATABASES:=true}"
: "${HEALTH_CHECK_BINANCE:=false}"
: "${HEALTH_CHECK_EXECUTION_AUDIT:=false}"
: "${HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS:=24}"
: "${HEALTH_CHECK_STRICT:=false}"
: "${HEALTH_CHECK_TIMEOUT_SECS:=3}"
: "${HEALTH_CHECK_OUTPUT:=human}"
: "${HEALTH_CHECK_WORKER_STALE_SECS:=900}"
: "${HEALTH_CHECK_WORKER_STALE_LEVEL:=warn}"
: "${HEALTH_CHECK_WORKER_MODE:=all}"
: "${HEALTH_CHECK_EXPECTED_WORKERS:=}"

WARNINGS=0
FAILURES=0
EXPECTED_WORKER_WARNINGS=0
EXPECTED_WORKER_FAILURES=0
IGNORED_WORKER_COUNT=0
IGNORED_STALE_WORKER_COUNT=0
EXECUTION_AUDIT_RECENT_FAILURES=0
EXECUTION_AUDIT_STALE_LEASED_WORKERS=0
CURRENT_SECTION=""
CHECK_LEVELS=()
CHECK_SECTIONS=()
CHECK_MESSAGES=()
ALERT_SEVERITIES=()
ALERT_CODES=()
ALERT_SECTIONS=()
ALERT_MESSAGES=()

redact_value() {
    local value="${1:-}"
    if [[ -z "${value}" ]]; then
        printf '<unset>'
        return
    fi
    printf '<set>'
}

is_enabled() {
    case "${1:-}" in
        1 | true | TRUE | yes | YES) return 0 ;;
        *) return 1 ;;
    esac
}

is_json_output() {
    [[ "${HEALTH_CHECK_OUTPUT}" == "json" ]]
}

is_non_negative_integer() {
    [[ "${1:-}" =~ ^[0-9]+$ ]]
}

record_check() {
    local level="$1"
    local message="$2"
    CHECK_LEVELS+=("${level}")
    CHECK_SECTIONS+=("${CURRENT_SECTION}")
    CHECK_MESSAGES+=("${message}")
    record_alert_for_check "${level}" "${CURRENT_SECTION}" "${message}"
    if ! is_json_output; then
        printf '%-4s %s\n' "${level}" "${message}"
    fi
}

record_alert() {
    local severity="$1"
    local code="$2"
    local section_name="$3"
    local message="$4"
    ALERT_SEVERITIES+=("${severity}")
    ALERT_CODES+=("${code}")
    ALERT_SECTIONS+=("${section_name}")
    ALERT_MESSAGES+=("${message}")
}

record_alert_for_check() {
    local level="$1"
    local section_name="$2"
    local message="$3"
    local severity=""
    local code=""

    if [[ "${message}" == *"expected_stale_worker_id="* ]]; then
        code="EXPECTED_WORKER_STALE"
        if [[ "${level}" == "FAIL" ]]; then
            severity="P0"
        else
            severity="P1"
        fi
    elif [[ "${message}" == *"ignored_stale_worker_id="* ]]; then
        severity="INFO"
        code="IGNORED_STALE_WORKER"
    elif [[ "${message}" == *"ignored_worker_id="* ]]; then
        severity="P1"
        code="UNEXPECTED_WORKER"
    elif [[ "${level}" == "WARN" && "${message}" == *"exchange_request_audit_logs:"* && "${message}" == *"recent_failures="* ]]; then
        severity="P1"
        code="EXCHANGE_REQUEST_AUDIT_FAILURES"
    elif [[ "${level}" == "WARN" && "${message}" == *"stale_leased_workers="* ]]; then
        severity="P1"
        code="WORKER_LEASE_STALE"
    elif [[ "${message}" == *"exchange_request_audit_logs: missing table"* ]]; then
        severity="INFO"
        code="EXECUTION_AUDIT_TABLE_MISSING"
    elif [[ "${level}" == "FAIL" ]]; then
        severity="P0"
        code="HEALTH_CHECK_FAIL"
    elif [[ "${level}" == "WARN" ]]; then
        severity="P1"
        code="HEALTH_CHECK_WARN"
    else
        return 0
    fi

    record_alert "${severity}" "${code}" "${section_name}" "${message}"
}

info() {
    record_check "INFO" "$1"
}

ok() {
    record_check "OK" "$1"
}

warn() {
    WARNINGS=$((WARNINGS + 1))
    record_check "WARN" "$1"
}

fail() {
    FAILURES=$((FAILURES + 1))
    record_check "FAIL" "$1"
}

section() {
    CURRENT_SECTION="$1"
    if ! is_json_output; then
        printf '\n== %s ==\n' "$1"
    fi
}

json_escape() {
    local value="${1:-}"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    value="${value//$'\n'/\\n}"
    value="${value//$'\r'/\\r}"
    value="${value//$'\t'/\\t}"
    printf '%s' "${value}"
}

json_string() {
    printf '"%s"' "$(json_escape "$1")"
}

has_expected_workers() {
    local compact="${HEALTH_CHECK_EXPECTED_WORKERS//,/}"
    compact="${compact// /}"
    [[ -n "${compact}" ]]
}

effective_worker_mode() {
    if [[ "${HEALTH_CHECK_WORKER_MODE}" == "expected" ]]; then
        printf 'expected'
        return
    fi
    if has_expected_workers; then
        printf 'expected'
        return
    fi
    printf 'all'
}

is_expected_worker() {
    local worker_id="$1"
    local expected_workers="${HEALTH_CHECK_EXPECTED_WORKERS//,/ }"
    local expected_worker
    for expected_worker in ${expected_workers}; do
        if [[ "${expected_worker}" == "${worker_id}" ]]; then
            return 0
        fi
    done
    return 1
}

render_json() {
    local status="ok"
    if (( FAILURES > 0 )); then
        status="fail"
    elif (( WARNINGS > 0 )); then
        status="warn"
    fi

    printf '{\n'
    printf '  "output": '; json_string "json"; printf ',\n'
    printf '  "status": '; json_string "${status}"; printf ',\n'
    printf '  "warnings": %s,\n' "${WARNINGS}"
    printf '  "failures": %s,\n' "${FAILURES}"
    printf '  "repo": '; json_string "${REPO_ROOT}"; printf ',\n'
    printf '  "umbrella": '; json_string "${UMBRELLA_ROOT}"; printf ',\n'
    printf '  "quant_core_database_url": '; json_string "${quant_core_display}"; printf ',\n'
    printf '  "web_database_url": '; json_string "${web_database_display}"; printf ',\n'
    printf '  "news_database_url": '; json_string "${news_database_display}"; printf ',\n'
    printf '  "execution_event_secret": '; json_string "${execution_secret_display}"; printf ',\n'
    printf '  "database_checks": '; json_string "${HEALTH_CHECK_DATABASES}"; printf ',\n'
    printf '  "binance_public_check": '; json_string "${HEALTH_CHECK_BINANCE}"; printf ',\n'
    printf '  "execution_audit_check": '; json_string "${HEALTH_CHECK_EXECUTION_AUDIT}"; printf ',\n'
    printf '  "worker_stale_secs": '; json_string "${HEALTH_CHECK_WORKER_STALE_SECS}"; printf ',\n'
    printf '  "worker_stale_level": '; json_string "${HEALTH_CHECK_WORKER_STALE_LEVEL}"; printf ',\n'
    printf '  "worker_mode": '; json_string "${EFFECTIVE_WORKER_MODE}"; printf ',\n'
    printf '  "expected_workers": '; json_string "${HEALTH_CHECK_EXPECTED_WORKERS}"; printf ',\n'
    printf '  "summary": {\n'
    printf '    "expected_worker_failures": %s,\n' "${EXPECTED_WORKER_FAILURES}"
    printf '    "expected_worker_warnings": %s,\n' "${EXPECTED_WORKER_WARNINGS}"
    printf '    "ignored_worker_count": %s,\n' "${IGNORED_WORKER_COUNT}"
    printf '    "ignored_stale_worker_count": %s,\n' "${IGNORED_STALE_WORKER_COUNT}"
    printf '    "execution_audit_recent_failures": %s,\n' "${EXECUTION_AUDIT_RECENT_FAILURES}"
    printf '    "execution_audit_stale_leased_workers": %s\n' "${EXECUTION_AUDIT_STALE_LEASED_WORKERS}"
    printf '  },\n'
    printf '  "checks": [\n'
    local index
    for index in "${!CHECK_LEVELS[@]}"; do
        if (( index > 0 )); then
            printf ',\n'
        fi
        printf '    {"level": '; json_string "${CHECK_LEVELS[index]}"; printf ', '
        printf '"section": '; json_string "${CHECK_SECTIONS[index]}"; printf ', '
        printf '"message": '; json_string "${CHECK_MESSAGES[index]}"; printf '}'
    done
    printf '\n  ],\n'
    printf '  "alerts": [\n'
    for index in "${!ALERT_SEVERITIES[@]}"; do
        if (( index > 0 )); then
            printf ',\n'
        fi
        printf '    {"severity": '; json_string "${ALERT_SEVERITIES[index]}"; printf ', '
        printf '"code": '; json_string "${ALERT_CODES[index]}"; printf ', '
        printf '"section": '; json_string "${ALERT_SECTIONS[index]}"; printf ', '
        printf '"message": '; json_string "${ALERT_MESSAGES[index]}"; printf '}'
    done
    printf '\n  ]\n'
    printf '}\n'
}

check_repo_root() {
    local label="$1"
    local path="$2"
    if [[ ! -d "${path}" ]]; then
        warn "${label}: missing path"
        return
    fi
    if git -C "${path}" rev-parse --show-toplevel >/dev/null 2>&1; then
        ok "${label}: git repo present"
    else
        warn "${label}: not a git repo"
    fi
}

check_script_syntax() {
    local relative_path="$1"
    local script_path="${REPO_ROOT}/${relative_path}"
    if [[ ! -f "${script_path}" ]]; then
        warn "${relative_path}: missing"
        return
    fi
    if bash -n "${script_path}" >/dev/null 2>&1; then
        ok "${relative_path}: bash syntax ok"
    else
        fail "${relative_path}: bash syntax failed"
    fi
}

check_http_endpoint() {
    local label="$1"
    local url="$2"
    if ! command -v curl >/dev/null 2>&1; then
        warn "${label}: curl not found"
        return
    fi

    local output_file
    output_file="$(mktemp)"
    local http_status
    http_status="$(
        curl -sS \
            -m "${HEALTH_CHECK_TIMEOUT_SECS}" \
            -o "${output_file}" \
            -w '%{http_code}' \
            "${url}" 2>/dev/null || true
    )"
    rm -f "${output_file}"

    if [[ "${http_status}" =~ ^[0-9]{3}$ && "${http_status}" != "000" ]]; then
        ok "${label}: http_status=${http_status}"
    else
        warn "${label}: unavailable"
    fi
}

psql_read() {
    local database_url="$1"
    local database_name="$2"
    local sql="$3"
    if command -v psql >/dev/null 2>&1; then
        psql "${database_url}" -v ON_ERROR_STOP=1 -Atc "${sql}"
        return
    fi
    if command -v podman >/dev/null 2>&1 &&
        podman container exists "${POSTGRES_CONTAINER}" >/dev/null 2>&1; then
        podman exec -i "${POSTGRES_CONTAINER}" psql \
            -U "${POSTGRES_USER}" \
            -d "${database_name}" \
            -v ON_ERROR_STOP=1 \
            -Atc "${sql}"
        return
    fi
    return 2
}

check_database_select_one() {
    local label="$1"
    local database_url="$2"
    local database_name="$3"
    local result
    result="$(psql_read "${database_url}" "${database_name}" "SELECT 1;" 2>/dev/null)" || {
        local status=$?
        if [[ "${status}" == "2" ]]; then
            warn "${label}: no local psql or postgres container"
        else
            warn "${label}: SELECT 1 failed"
        fi
        return
    }
    if [[ "${result}" == "1" ]]; then
        ok "${label}: SELECT 1 ok"
    else
        warn "${label}: unexpected SELECT 1 result"
    fi
}

check_worker_checkpoints() {
    local rows
    rows="$(
        psql_read "${QUANT_CORE_DATABASE_URL}" \
            "${QUANT_CORE_POSTGRES_DB}" \
            "SELECT worker_id, worker_status, COALESCE(last_task_id::text, ''), COALESCE(last_heartbeat_at::text, ''), COALESCE(EXTRACT(EPOCH FROM (now() - last_heartbeat_at))::bigint::text, '') FROM execution_worker_checkpoints ORDER BY updated_at DESC NULLS LAST LIMIT 5;" \
            2>/dev/null
    )" || {
        warn "quant_core execution_worker_checkpoints: unavailable"
        return
    }

    if [[ -z "${rows}" ]]; then
        warn "quant_core execution_worker_checkpoints: no rows"
        return
    fi

    ok "quant_core execution_worker_checkpoints: recent rows"
    ok "quant_core execution_worker_checkpoints: worker_mode=${EFFECTIVE_WORKER_MODE} expected_workers=$(redact_value "${HEALTH_CHECK_EXPECTED_WORKERS}")"
    if [[ -n "${HEALTH_CHECK_WORKER_STALE_SECS}" ]] &&
        [[ "${HEALTH_CHECK_WORKER_STALE_SECS}" != "0" ]] &&
        ! is_non_negative_integer "${HEALTH_CHECK_WORKER_STALE_SECS}"; then
        warn "quant_core execution_worker_checkpoints: invalid HEALTH_CHECK_WORKER_STALE_SECS=${HEALTH_CHECK_WORKER_STALE_SECS}"
    fi
    while IFS='|' read -r worker_id worker_status last_task_id last_heartbeat_at heartbeat_age_secs; do
        if ! is_json_output; then
            printf '  worker_id=%s worker_status=%s last_task_id=%s last_heartbeat_at=%s heartbeat_age_secs=%s\n' \
                "${worker_id:-<empty>}" \
                "${worker_status:-<empty>}" \
                "${last_task_id:-<empty>}" \
                "${last_heartbeat_at:-<empty>}" \
                "${heartbeat_age_secs:-<empty>}"
        fi
        if [[ -n "${HEALTH_CHECK_WORKER_STALE_SECS}" ]] &&
            [[ "${HEALTH_CHECK_WORKER_STALE_SECS}" != "0" ]] &&
            is_non_negative_integer "${HEALTH_CHECK_WORKER_STALE_SECS}" &&
            is_non_negative_integer "${heartbeat_age_secs}" &&
            (( heartbeat_age_secs > HEALTH_CHECK_WORKER_STALE_SECS )); then
            if [[ "${EFFECTIVE_WORKER_MODE}" == "expected" ]]; then
                if is_expected_worker "${worker_id}"; then
                    local stale_message="quant_core execution_worker_checkpoints: expected_stale_worker_id=${worker_id:-<empty>} heartbeat_age_secs=${heartbeat_age_secs} threshold_secs=${HEALTH_CHECK_WORKER_STALE_SECS} last_heartbeat_at=${last_heartbeat_at:-<empty>}"
                    if [[ "${HEALTH_CHECK_WORKER_STALE_LEVEL}" == "fail" ]]; then
                        EXPECTED_WORKER_FAILURES=$((EXPECTED_WORKER_FAILURES + 1))
                        fail "${stale_message}"
                    else
                        EXPECTED_WORKER_WARNINGS=$((EXPECTED_WORKER_WARNINGS + 1))
                        warn "${stale_message}"
                    fi
                else
                    IGNORED_WORKER_COUNT=$((IGNORED_WORKER_COUNT + 1))
                    info "quant_core execution_worker_checkpoints: ignored_worker_id=${worker_id:-<empty>} reason=not_expected heartbeat_age_secs=${heartbeat_age_secs} last_heartbeat_at=${last_heartbeat_at:-<empty>}"
                fi
            else
                IGNORED_STALE_WORKER_COUNT=$((IGNORED_STALE_WORKER_COUNT + 1))
                info "quant_core execution_worker_checkpoints: ignored_stale_worker_id=${worker_id:-<empty>} reason=worker_mode_all heartbeat_age_secs=${heartbeat_age_secs} threshold_secs=${HEALTH_CHECK_WORKER_STALE_SECS} last_heartbeat_at=${last_heartbeat_at:-<empty>}"
            fi
        elif [[ "${EFFECTIVE_WORKER_MODE}" == "expected" ]]; then
            if is_expected_worker "${worker_id}"; then
                ok "quant_core execution_worker_checkpoints: expected_worker_id=${worker_id:-<empty>} heartbeat_age_secs=${heartbeat_age_secs:-<empty>} last_heartbeat_at=${last_heartbeat_at:-<empty>}"
            else
                IGNORED_WORKER_COUNT=$((IGNORED_WORKER_COUNT + 1))
                info "quant_core execution_worker_checkpoints: ignored_worker_id=${worker_id:-<empty>} reason=not_expected heartbeat_age_secs=${heartbeat_age_secs:-<empty>} last_heartbeat_at=${last_heartbeat_at:-<empty>}"
            fi
        fi
    done <<< "${rows}"
}

check_execution_audit() {
    if ! is_enabled "${HEALTH_CHECK_EXECUTION_AUDIT}"; then
        ok "execution audit: skipped"
        return
    fi
    if ! is_enabled "${HEALTH_CHECK_DATABASES}"; then
        warn "execution audit: skipped because HEALTH_CHECK_DATABASES=false"
        return
    fi
    if ! is_non_negative_integer "${HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS}"; then
        warn "quant_core execution audit: invalid HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS=${HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS}"
        return
    fi

    local audit_table
    audit_table="$(
        psql_read "${QUANT_CORE_DATABASE_URL}" \
            "${QUANT_CORE_POSTGRES_DB}" \
            "SELECT COALESCE(to_regclass('public.exchange_request_audit_logs')::text, '');" \
            2>/dev/null
    )" || {
        warn "quant_core exchange_request_audit_logs: table check unavailable"
        return
    }
    if [[ "${audit_table}" != "public.exchange_request_audit_logs" ]]; then
        info "quant_core exchange_request_audit_logs: missing table"
        return
    fi

    ok "quant_core exchange_request_audit_logs: table present"
    local request_summary
    request_summary="$(
        psql_read "${QUANT_CORE_DATABASE_URL}" \
            "${QUANT_CORE_POSTGRES_DB}" \
            "SELECT COUNT(*)::text || '|' || COUNT(*) FILTER (WHERE request_status <> 'completed')::text || '|' || COALESCE(MAX(latency_ms), 0)::text FROM exchange_request_audit_logs WHERE created_at >= NOW() - make_interval(hours => ${HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS});" \
            2>/dev/null
    )" || {
        warn "quant_core exchange_request_audit_logs: recent summary unavailable"
        return
    }
    local recent_total recent_failures max_latency_ms
    IFS='|' read -r recent_total recent_failures max_latency_ms <<< "${request_summary}"
    recent_total="${recent_total:-0}"
    recent_failures="${recent_failures:-0}"
    max_latency_ms="${max_latency_ms:-0}"
    EXECUTION_AUDIT_RECENT_FAILURES="${recent_failures}"
    if is_non_negative_integer "${recent_failures}" && (( recent_failures > 0 )); then
        warn "quant_core exchange_request_audit_logs: recent_total=${recent_total} recent_failures=${recent_failures} max_latency_ms=${max_latency_ms} lookback_hours=${HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS}"
    else
        ok "quant_core exchange_request_audit_logs: recent_total=${recent_total} recent_failures=${recent_failures} max_latency_ms=${max_latency_ms} lookback_hours=${HEALTH_CHECK_EXECUTION_AUDIT_LOOKBACK_HOURS}"
    fi

    if ! is_non_negative_integer "${HEALTH_CHECK_WORKER_STALE_SECS}" || [[ "${HEALTH_CHECK_WORKER_STALE_SECS}" == "0" ]]; then
        info "quant_core execution_worker_checkpoints: lease audit skipped because worker stale threshold is disabled"
        return
    fi

    local lease_summary
    lease_summary="$(
        psql_read "${QUANT_CORE_DATABASE_URL}" \
            "${QUANT_CORE_POSTGRES_DB}" \
            "SELECT COUNT(*) FILTER (WHERE worker_status IN ('leased', 'processing'))::text || '|' || COUNT(*) FILTER (WHERE worker_status IN ('leased', 'processing') AND last_heartbeat_at IS NOT NULL AND EXTRACT(EPOCH FROM (now() - last_heartbeat_at))::bigint > ${HEALTH_CHECK_WORKER_STALE_SECS})::text AS stale_leased_workers FROM execution_worker_checkpoints;" \
            2>/dev/null
    )" || {
        warn "quant_core execution_worker_checkpoints: lease audit summary unavailable"
        return
    }
    local leased_workers stale_leased_workers
    IFS='|' read -r leased_workers stale_leased_workers <<< "${lease_summary}"
    leased_workers="${leased_workers:-0}"
    stale_leased_workers="${stale_leased_workers:-0}"
    EXECUTION_AUDIT_STALE_LEASED_WORKERS="${stale_leased_workers}"
    if is_non_negative_integer "${stale_leased_workers}" && (( stale_leased_workers > 0 )); then
        warn "quant_core execution_worker_checkpoints: leased_workers=${leased_workers} stale_leased_workers=${stale_leased_workers} threshold_secs=${HEALTH_CHECK_WORKER_STALE_SECS}"
    else
        ok "quant_core execution_worker_checkpoints: leased_workers=${leased_workers} stale_leased_workers=${stale_leased_workers} threshold_secs=${HEALTH_CHECK_WORKER_STALE_SECS}"
    fi
}

check_public_binance_connectivity() {
    if ! is_enabled "${HEALTH_CHECK_BINANCE}"; then
        ok "binance public connectivity: skipped"
        return
    fi
    if [[ ! -x "${SCRIPT_DIR}/check_binance_connectivity.sh" ]]; then
        warn "binance public connectivity: script not executable"
        return
    fi
    if "${SCRIPT_DIR}/check_binance_connectivity.sh" >/dev/null 2>&1; then
        ok "binance public connectivity: public probe ok"
    else
        warn "binance public connectivity: public probe failed"
    fi
}

quant_core_display="$(redact_value "${QUANT_CORE_DATABASE_URL:-}")"
web_database_display="$(redact_value "${WEB_DATABASE_URL:-}")"
news_database_display="$(redact_value "${NEWS_DATABASE_URL:-}")"
execution_secret_display="$(redact_value "${EXECUTION_EVENT_SECRET:-}")"
EFFECTIVE_WORKER_MODE="$(effective_worker_mode)"

if ! is_json_output; then
    echo "Local service health check"
    printf '  repo: %s\n' "${REPO_ROOT}"
    printf '  umbrella: %s\n' "${UMBRELLA_ROOT}"
    printf '  quant_core_database_url: %s\n' "${quant_core_display}"
    printf '  web_database_url: %s\n' "${web_database_display}"
    printf '  news_database_url: %s\n' "${news_database_display}"
    printf '  execution_event_secret: %s\n' "${execution_secret_display}"
    printf '  database_checks: %s\n' "${HEALTH_CHECK_DATABASES}"
    printf '  binance_public_check: %s\n' "${HEALTH_CHECK_BINANCE}"
    printf '  execution_audit_check: %s\n' "${HEALTH_CHECK_EXECUTION_AUDIT}"
    printf '  worker_stale_secs: %s\n' "${HEALTH_CHECK_WORKER_STALE_SECS}"
    printf '  worker_stale_level: %s\n' "${HEALTH_CHECK_WORKER_STALE_LEVEL}"
    printf '  worker_mode: %s\n' "${EFFECTIVE_WORKER_MODE}"
    printf '  expected_workers: %s\n' "$(redact_value "${HEALTH_CHECK_EXPECTED_WORKERS}")"
fi

section "Repo Roots"
check_repo_root "rust_quant" "${REPO_ROOT}"
check_repo_root "rust_quan_web" "${UMBRELLA_ROOT}/rust_quan_web"
check_repo_root "rust_quant_news" "${UMBRELLA_ROOT}/rust_quant_news"
check_repo_root "crypto_exc_all" "${UMBRELLA_ROOT}/crypto_exc_all"

section "Script Syntax"
check_script_syntax "scripts/dev/run_execution_worker_dry_run.sh"
check_script_syntax "scripts/dev/run_execution_worker_local_preflight.sh"
check_script_syntax "scripts/dev/check_binance_connectivity.sh"

section "Local HTTP"
check_http_endpoint "rust_quan_web" "${RUST_QUAN_WEB_BASE_URL}/"
check_http_endpoint "rust_quant_news" "${NEWS_SERVER_BASE_URL}/"

section "Databases"
if is_enabled "${HEALTH_CHECK_DATABASES}"; then
    check_database_select_one "quant_core" "${QUANT_CORE_DATABASE_URL}" "${QUANT_CORE_POSTGRES_DB}"
    check_database_select_one "quant_web" "${WEB_DATABASE_URL}" "${WEB_POSTGRES_DB}"
    check_database_select_one "quant_news" "${NEWS_DATABASE_URL}" "${NEWS_POSTGRES_DB}"
    check_worker_checkpoints
else
    ok "database checks: skipped"
fi

section "Execution Audit"
check_execution_audit

section "Exchange Connectivity"
check_public_binance_connectivity

section "Summary"
if is_json_output; then
    render_json
else
    printf 'warnings=%s failures=%s\n' "${WARNINGS}" "${FAILURES}"
fi
if (( FAILURES > 0 )); then
    exit 1
fi
if (( WARNINGS > 0 )) && is_enabled "${HEALTH_CHECK_STRICT}"; then
    exit 1
fi
