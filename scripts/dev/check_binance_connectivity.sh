#!/usr/bin/env bash
set -euo pipefail

: "${BINANCE_API_URL:=https://fapi.binance.com}"
: "${BINANCE_WS_STREAM_URL:=https://fstream.binance.com}"
: "${BINANCE_REST_ENDPOINTS:=${BINANCE_API_URL}}"
: "${BINANCE_WS_ENDPOINTS:=${BINANCE_WS_STREAM_URL}}"
: "${BINANCE_TEST_SYMBOL:=BCHUSDT}"
: "${BINANCE_TEST_INTERVAL:=1h}"
: "${BINANCE_CONNECT_TIMEOUT_SECS:=10}"
: "${BINANCE_CONNECTIVITY_RETRIES:=2}"
: "${BINANCE_CONNECTIVITY_RETRY_DELAY_SECS:=2}"

PROXY_URL=""
PROXY_SOURCE=""
DIAGNOSTIC_FLAGS=""

select_proxy_url() {
    if [[ -n "${BINANCE_PROXY_URL:-}" ]]; then
        PROXY_SOURCE="BINANCE_PROXY_URL"
        printf '%s' "${BINANCE_PROXY_URL}"
        return
    fi
    if [[ -n "${ALL_PROXY:-}" ]]; then
        PROXY_SOURCE="ALL_PROXY"
        printf '%s' "${ALL_PROXY}"
        return
    fi
    if [[ -n "${all_proxy:-}" ]]; then
        PROXY_SOURCE="all_proxy"
        printf '%s' "${all_proxy}"
        return
    fi
    if [[ -n "${HTTPS_PROXY:-}" ]]; then
        PROXY_SOURCE="HTTPS_PROXY"
        printf '%s' "${HTTPS_PROXY}"
        return
    fi
    if [[ -n "${https_proxy:-}" ]]; then
        PROXY_SOURCE="https_proxy"
        printf '%s' "${https_proxy}"
        return
    fi
}

normalize_proxy_url() {
    local proxy_url="$1"
    if [[ "${proxy_url}" == socks5://* ]]; then
        printf 'socks5h://%s' "${proxy_url#socks5://}"
        return
    fi
    printf '%s' "${proxy_url}"
}

proxy_host_port() {
    local proxy_url="$1"
    printf '%s' "${proxy_url}" \
        | sed -E 's#^[a-zA-Z0-9+.-]+://##; s#^([^/@]+@)?##; s#/.*$##'
}

parse_endpoints() {
    local raw="$1"
    raw="${raw//,/ }"
    read -r -a PARSED_ENDPOINTS <<<"${raw}"
}

record_diagnostics() {
    local output="$1"
    case "${output}" in
        *SSL_ERROR_SYSCALL*|*OpenSSL\ SSL_connect*|*TLSv1*|*unexpected\ eof*)
            DIAGNOSTIC_FLAGS="${DIAGNOSTIC_FLAGS} tls"
            ;;
    esac
    case "${output}" in
        *Failed\ to\ connect\ to*|*Connection\ refused*|*SOCKS*|*proxy\ connect\ aborted*|*Proxy\ CONNECT\ aborted*)
            DIAGNOSTIC_FLAGS="${DIAGNOSTIC_FLAGS} proxy"
            ;;
    esac
}

probe_once() {
    local url="$1"
    local expect_http="$2"
    local proxy_arg=()
    local output curl_exit http_status
    if [[ -n "${PROXY_URL}" ]]; then
        proxy_arg=(-x "${PROXY_URL}")
    fi

    set +e
    output="$(
        curl -sS \
            --max-time "${BINANCE_CONNECT_TIMEOUT_SECS}" \
            "${proxy_arg[@]}" \
            -w '\nHTTP_STATUS=%{http_code}\nERR=%{errormsg}\n' \
            "${url}" 2>&1
    )"
    curl_exit=$?
    set -e

    http_status="$(printf '%s\n' "${output}" | sed -n 's/^HTTP_STATUS=//p' | tail -n 1)"
    [[ -n "${http_status}" ]] || http_status="000"

    printf '%s\n' "${output}" | head -n 20
    echo "CURL_EXIT=${curl_exit}"

    record_diagnostics "${output}"

    if [[ "${curl_exit}" -ne 0 ]]; then
        return 1
    fi
    if [[ "${expect_http}" == "strict-200" && "${http_status}" != "200" ]]; then
        return 1
    fi
    if [[ "${expect_http}" == "any-http" && "${http_status}" == "000" ]]; then
        return 1
    fi
    return 0
}

probe_with_retry() {
    local label="$1"
    local url="$2"
    local expect_http="$3"
    local attempts=0

    echo
    echo "${label}"
    while (( attempts < BINANCE_CONNECTIVITY_RETRIES )); do
        attempts=$((attempts + 1))
        echo "Attempt ${attempts}/${BINANCE_CONNECTIVITY_RETRIES}: ${url}"
        if probe_once "${url}" "${expect_http}"; then
            echo "RESULT=ok"
            return 0
        fi
        if (( attempts < BINANCE_CONNECTIVITY_RETRIES )); then
            sleep "${BINANCE_CONNECTIVITY_RETRY_DELAY_SECS}"
        fi
    done

    echo "RESULT=failed"
    return 1
}

probe_rest_endpoint() {
    local endpoint="$1"
    probe_with_retry "REST /fapi/v1/time [${endpoint}]" "${endpoint%/}/fapi/v1/time" "strict-200" &&
        probe_with_retry \
            "REST /fapi/v1/klines [${endpoint}]" \
            "${endpoint%/}/fapi/v1/klines?symbol=${BINANCE_TEST_SYMBOL}&interval=${BINANCE_TEST_INTERVAL}&limit=1" \
            "strict-200"
}

probe_ws_endpoint() {
    local endpoint="$1"
    probe_with_retry \
        "STREAM TLS reachability [${endpoint}]" \
        "${endpoint%/}/market/stream?streams=$(printf '%s' "${BINANCE_TEST_SYMBOL}" | tr '[:upper:]' '[:lower:]')@kline_${BINANCE_TEST_INTERVAL}" \
        "any-http"
}

print_diagnostic_hints() {
    echo
    echo "Diagnostic hints"
    if [[ "${DIAGNOSTIC_FLAGS}" == *"tls"* ]]; then
        echo "  - TLS handshake failed. If curl shows SSL_ERROR_SYSCALL, try another endpoint or verify your proxy can tunnel TLS to Binance futures."
    fi
    if [[ "${DIAGNOSTIC_FLAGS}" == *"proxy"* ]]; then
        echo "  - Proxy connectivity failed. Confirm the local listener is reachable and prefer socks5h://127.0.0.1:7897 over socks5:// when you need remote DNS resolution."
    fi
    if [[ "${DIAGNOSTIC_FLAGS}" != *"tls"* && "${DIAGNOSTIC_FLAGS}" != *"proxy"* ]]; then
        echo "  - No specific TLS/proxy signature detected. Review curl exit codes, HTTP_STATUS, and endpoint reachability above."
    fi
    echo "  - Retry example: BINANCE_PROXY_URL=socks5h://127.0.0.1:7897 BINANCE_CONNECTIVITY_RETRIES=3 ${0}"
}

PROXY_URL="$(select_proxy_url || true)"
if [[ -n "${PROXY_URL}" ]]; then
    PROXY_URL="$(normalize_proxy_url "${PROXY_URL}")"
fi

parse_endpoints "${BINANCE_REST_ENDPOINTS}"
REST_ENDPOINT_LIST=("${PARSED_ENDPOINTS[@]}")
parse_endpoints "${BINANCE_WS_ENDPOINTS}"
WS_ENDPOINT_LIST=("${PARSED_ENDPOINTS[@]}")

echo "Binance connectivity check"
echo "  rest_endpoints: ${BINANCE_REST_ENDPOINTS}"
echo "  ws_endpoints: ${BINANCE_WS_ENDPOINTS}"
echo "  test_symbol: ${BINANCE_TEST_SYMBOL}"
echo "  test_interval: ${BINANCE_TEST_INTERVAL}"
echo "  connect_timeout_secs: ${BINANCE_CONNECT_TIMEOUT_SECS}"
echo "  retries: ${BINANCE_CONNECTIVITY_RETRIES}"
echo "  retry_delay_secs: ${BINANCE_CONNECTIVITY_RETRY_DELAY_SECS}"
echo "  proxy: ${PROXY_URL:-<none>}"
echo "  proxy_source: ${PROXY_SOURCE:-<none>}"

if [[ -n "${PROXY_URL}" ]]; then
    host_port="$(proxy_host_port "${PROXY_URL}")"
    echo "  proxy_host_port: ${host_port}"
    if command -v nc >/dev/null 2>&1; then
        host="${host_port%:*}"
        port="${host_port##*:}"
        if nc -z -G 2 "${host}" "${port}" >/dev/null 2>&1; then
            echo "  proxy_listener: ok"
        else
            echo "  proxy_listener: unavailable"
        fi
    fi
fi

rest_ok=1
for endpoint in "${REST_ENDPOINT_LIST[@]}"; do
    if probe_rest_endpoint "${endpoint}"; then
        rest_ok=0
        break
    fi
done

ws_ok=1
for endpoint in "${WS_ENDPOINT_LIST[@]}"; do
    if probe_ws_endpoint "${endpoint}"; then
        ws_ok=0
        break
    fi
done

echo
echo "Connectivity summary"
echo "  rest_reachable=$([[ ${rest_ok} -eq 0 ]] && echo true || echo false)"
echo "  ws_reachable=$([[ ${ws_ok} -eq 0 ]] && echo true || echo false)"

if [[ "${rest_ok}" -ne 0 || "${ws_ok}" -ne 0 ]]; then
    print_diagnostic_hints
    exit 1
fi

