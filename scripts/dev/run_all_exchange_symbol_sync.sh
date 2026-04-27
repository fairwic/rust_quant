#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SINGLE_SYNC_SCRIPT="${REPO_ROOT}/scripts/dev/run_exchange_symbol_sync.sh"

: "${EXCHANGE_SYMBOL_SOURCES:=binance okx bitget gate kucoin}"

normalize_sources() {
    echo "$1" | tr ',' ' '
}

read -r -a SOURCES <<< "$(normalize_sources "${EXCHANGE_SYMBOL_SOURCES}")"

if [[ "${#SOURCES[@]}" -eq 0 ]]; then
    echo "EXCHANGE_SYMBOL_SOURCES must not be empty" >&2
    exit 1
fi

if [[ ! -f "${SINGLE_SYNC_SCRIPT}" ]]; then
    echo "single exchange sync script was not found: ${SINGLE_SYNC_SCRIPT}" >&2
    exit 1
fi

echo "==> syncing exchange symbols for ${#SOURCES[@]} sources"
echo "    sources: ${SOURCES[*]}"

for i in "${!SOURCES[@]}"; do
    source_name="${SOURCES[$i]}"
    echo "==> [$((i + 1))/${#SOURCES[@]}] source=${source_name}"

    set +e
    EXCHANGE_SYMBOL_SOURCE="${source_name}" bash "${SINGLE_SYNC_SCRIPT}"
    status=$?
    set -e

    if [[ "${status}" -eq 0 ]]; then
        echo "==> completed source=${source_name}"
        continue
    fi

    echo "==> failed source=${source_name} exit_code=${status}" >&2
    exit "${status}"
done

echo "==> completed exchange symbol sync for all sources"
