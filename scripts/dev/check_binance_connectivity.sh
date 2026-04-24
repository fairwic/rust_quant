#!/usr/bin/env bash
set -euo pipefail

: "${BINANCE_API_URL:="https://fapi.binance.com"}"
: "${BINANCE_WS_STREAM_URL:="https://fstream.binance.com"}"
: "${BINANCE_TEST_SYMBOL:="BCHUSDT"}"
: "${BINANCE_TEST_INTERVAL:="1h"}"
: "${BINANCE_CONNECT_TIMEOUT_SECS:="10"}"

select_proxy_url() {
    if [[ -n "${BINANCE_PROXY_URL:-}" ]]; then
        printf '%s' "${BINANCE_PROXY_URL}"
        return
    fi
    if [[ -n "${ALL_PROXY:-}" ]]; then
        printf '%s' "${ALL_PROXY}"
        return
    fi
    if [[ -n "${all_proxy:-}" ]]; then
        printf '%s' "${all_proxy}"
        return
    fi
    if [[ -n "${HTTPS_PROXY:-}" ]]; then
        printf '%s' "${HTTPS_PROXY}"
        return
    fi
    if [[ -n "${https_proxy:-}" ]]; then
        printf '%s' "${https_proxy}"
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

probe_url() {
    local label="$1"
    local url="$2"
    local proxy_arg=()
    if [[ -n "${PROXY_URL}" ]]; then
        proxy_arg=(-x "${PROXY_URL}")
    fi

    echo
    echo "${label}"
    set +e
    output="$(curl -sS \
        --max-time "${BINANCE_CONNECT_TIMEOUT_SECS}" \
        "${proxy_arg[@]}" \
        -w '\nHTTP_STATUS=%{http_code}\nERR=%{errormsg}\n' \
        "${url}" 2>&1)"
    curl_exit=$?
    set -e
    printf '%s\n' "${output}" | head -n 20
    echo "CURL_EXIT=${curl_exit}"
}

PROXY_URL="$(select_proxy_url || true)"
if [[ -n "${PROXY_URL}" ]]; then
    PROXY_URL="$(normalize_proxy_url "${PROXY_URL}")"
fi

echo "Binance connectivity check"
echo "  rest_base: ${BINANCE_API_URL}"
echo "  stream_base: ${BINANCE_WS_STREAM_URL}"
echo "  test_symbol: ${BINANCE_TEST_SYMBOL}"
echo "  test_interval: ${BINANCE_TEST_INTERVAL}"
echo "  proxy: ${PROXY_URL:-<none>}"

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

probe_url "REST /fapi/v1/time" "${BINANCE_API_URL%/}/fapi/v1/time"
probe_url \
    "REST /fapi/v1/klines" \
    "${BINANCE_API_URL%/}/fapi/v1/klines?symbol=${BINANCE_TEST_SYMBOL}&interval=${BINANCE_TEST_INTERVAL}&limit=1"
probe_url \
    "STREAM TLS reachability" \
    "${BINANCE_WS_STREAM_URL%/}/market/stream?streams=$(printf '%s' "${BINANCE_TEST_SYMBOL}" | tr '[:upper:]' '[:lower:]')@kline_${BINANCE_TEST_INTERVAL}"
