#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WRAPPER_SOURCE="${REPO_ROOT}/scripts/dev/run_all_exchange_symbol_sync.sh"

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

assert_file_equals() {
    local expected_file="$1"
    local actual_file="$2"

    if ! diff -u "${expected_file}" "${actual_file}"; then
        fail "expected ${actual_file} to match ${expected_file}"
    fi
}

assert_contains() {
    local pattern="$1"
    local file="$2"

    if ! grep -Fq -- "${pattern}" "${file}"; then
        fail "expected ${file} to contain: ${pattern}"
    fi
}

make_temp_repo() {
    local temp_dir
    temp_dir="$(mktemp -d)"
    mkdir -p "${temp_dir}/scripts/dev"
    cp "${WRAPPER_SOURCE}" "${temp_dir}/scripts/dev/run_all_exchange_symbol_sync.sh"
    cat > "${temp_dir}/scripts/dev/run_exchange_symbol_sync.sh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail

: "${EXCHANGE_SYMBOL_SOURCE:?EXCHANGE_SYMBOL_SOURCE is required}"
: "${CALL_LOG_FILE:?CALL_LOG_FILE is required}"

echo "${EXCHANGE_SYMBOL_SOURCE}" >> "${CALL_LOG_FILE}"
echo "stub sync ${EXCHANGE_SYMBOL_SOURCE}"

if [[ "${FAIL_ON_SOURCE:-}" == "${EXCHANGE_SYMBOL_SOURCE}" ]]; then
    echo "stub failure ${EXCHANGE_SYMBOL_SOURCE}" >&2
    exit 23
fi
STUB
    chmod +x "${temp_dir}/scripts/dev/run_all_exchange_symbol_sync.sh"
    chmod -x "${temp_dir}/scripts/dev/run_exchange_symbol_sync.sh"
    echo "${temp_dir}"
}

run_wrapper() {
    local temp_repo="$1"
    local stdout_file="$2"
    local stderr_file="$3"
    local status
    shift 3

    set +e
    (
        cd "${temp_repo}"
        "$@"
    ) >"${stdout_file}" 2>"${stderr_file}"
    status=$?
    set -e
    return "${status}"
}

test_default_order() {
    local temp_repo stdout_file stderr_file actual_file expected_file
    temp_repo="$(make_temp_repo)"
    trap 'rm -rf "${temp_repo}"' RETURN
    stdout_file="${temp_repo}/stdout.log"
    stderr_file="${temp_repo}/stderr.log"
    actual_file="${temp_repo}/calls.log"
    expected_file="${temp_repo}/expected.log"

    run_wrapper \
        "${temp_repo}" \
        "${stdout_file}" \
        "${stderr_file}" \
        env CALL_LOG_FILE="${actual_file}" ./scripts/dev/run_all_exchange_symbol_sync.sh

    cat > "${expected_file}" <<'EOF'
binance
okx
bitget
gate
kucoin
EOF

    assert_file_equals "${expected_file}" "${actual_file}"
    assert_contains "==> syncing exchange symbols for 5 sources" "${stdout_file}"
    assert_contains "[1/5] source=binance" "${stdout_file}"
    assert_contains "[5/5] source=kucoin" "${stdout_file}"
    assert_contains "completed exchange symbol sync for all sources" "${stdout_file}"
}

test_override_order() {
    local temp_repo stdout_file stderr_file actual_file expected_file
    temp_repo="$(make_temp_repo)"
    trap 'rm -rf "${temp_repo}"' RETURN
    stdout_file="${temp_repo}/stdout.log"
    stderr_file="${temp_repo}/stderr.log"
    actual_file="${temp_repo}/calls.log"
    expected_file="${temp_repo}/expected.log"

    run_wrapper \
        "${temp_repo}" \
        "${stdout_file}" \
        "${stderr_file}" \
        env CALL_LOG_FILE="${actual_file}" EXCHANGE_SYMBOL_SOURCES="gate kucoin" ./scripts/dev/run_all_exchange_symbol_sync.sh

    cat > "${expected_file}" <<'EOF'
gate
kucoin
EOF

    assert_file_equals "${expected_file}" "${actual_file}"
    assert_contains "sources: gate kucoin" "${stdout_file}"
    assert_contains "[1/2] source=gate" "${stdout_file}"
    assert_contains "[2/2] source=kucoin" "${stdout_file}"
}

test_fail_fast() {
    local temp_repo stdout_file stderr_file actual_file expected_file status
    temp_repo="$(make_temp_repo)"
    trap 'rm -rf "${temp_repo}"' RETURN
    stdout_file="${temp_repo}/stdout.log"
    stderr_file="${temp_repo}/stderr.log"
    actual_file="${temp_repo}/calls.log"
    expected_file="${temp_repo}/expected.log"

    status=0
    if run_wrapper \
        "${temp_repo}" \
        "${stdout_file}" \
        "${stderr_file}" \
        env CALL_LOG_FILE="${actual_file}" FAIL_ON_SOURCE="bitget" ./scripts/dev/run_all_exchange_symbol_sync.sh; then
        fail "expected fail-fast scenario to return non-zero"
    else
        status=$?
    fi

    if [[ "${status}" -ne 23 ]]; then
        fail "expected fail-fast scenario to return 23, got ${status}"
    fi

    cat > "${expected_file}" <<'EOF'
binance
okx
bitget
EOF

    assert_file_equals "${expected_file}" "${actual_file}"
    assert_contains "[3/5] source=bitget" "${stdout_file}"
    assert_contains "failed source=bitget exit_code=23" "${stderr_file}"
    assert_contains "stub failure bitget" "${stderr_file}"
}

test_default_order
test_override_order
test_fail_fast

echo "PASS: run_all_exchange_symbol_sync_contract"
